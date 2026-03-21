//! Shared compliance checking helpers.
//!
//! This is an internal implementation module (`pub(crate)`). Many functions
//! have non-cached convenience variants alongside the `_cached` variants that
//! `pdfa.rs` actually calls. The non-cached variants are kept for completeness
//! and potential future use in tests or tooling.
#![allow(dead_code)]

use crate::{ComplianceIssue, ComplianceReport, PdfALevel, Severity};
use pdf_syntax::object::dict::keys;
use pdf_syntax::object::{Array, Dict, Name, ObjRef, Object, Stream};
use pdf_syntax::page::Resources;
use pdf_syntax::Pdf;

/// Maximum decompressed page content stream size (in bytes) to scan.
/// Checks that iterate content streams byte-by-byte skip streams larger than
/// this to avoid pathological O(n²) scan times on content-heavy pages.
const MAX_CONTENT_STREAM_SCAN_SIZE: usize = 1_000_000; // 1 MB

/// Pre-collected objects from a PDF, to avoid repeated expensive parsing.
/// Created once with `ObjectCache::new(pdf)` and shared across checks.
pub struct ObjectCache<'a> {
    objects: Vec<Object<'a>>,
}

impl<'a> ObjectCache<'a> {
    pub fn new(pdf: &'a Pdf) -> Self {
        Self {
            objects: pdf.objects().into_iter().collect(),
        }
    }

    /// Build cache only if estimated object count is below threshold.
    /// Returns empty cache for very large PDFs to avoid excessive parse time.
    pub fn new_bounded(pdf: &'a Pdf, max_objects: usize) -> Self {
        let estimated = pdf.len();
        if estimated > max_objects {
            return Self {
                objects: Vec::new(),
            };
        }
        Self::new(pdf)
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Object<'a>> {
        self.objects.iter()
    }
}

/// Helper to push an error into a report.
pub fn error(report: &mut ComplianceReport, rule: &str, message: impl Into<String>) {
    report.issues.push(ComplianceIssue {
        rule: rule.to_string(),
        severity: Severity::Error,
        message: message.into(),
        location: None,
    });
}

/// Helper to push a located error into a report.
pub fn error_at(
    report: &mut ComplianceReport,
    rule: &str,
    message: impl Into<String>,
    location: impl Into<String>,
) {
    report.issues.push(ComplianceIssue {
        rule: rule.to_string(),
        severity: Severity::Error,
        message: message.into(),
        location: Some(location.into()),
    });
}

/// Helper to push a warning into a report.
pub fn warning(report: &mut ComplianceReport, rule: &str, message: impl Into<String>) {
    report.issues.push(ComplianceIssue {
        rule: rule.to_string(),
        severity: Severity::Warning,
        message: message.into(),
        location: None,
    });
}

/// Helper to push info into a report.
#[allow(dead_code)]
pub fn info(report: &mut ComplianceReport, rule: &str, message: impl Into<String>) {
    report.issues.push(ComplianceIssue {
        rule: rule.to_string(),
        severity: Severity::Info,
        message: message.into(),
        location: None,
    });
}

/// Get the document catalog dictionary.
pub fn catalog<'a>(pdf: &'a Pdf) -> Option<Dict<'a>> {
    let xref = pdf.xref();
    xref.get(xref.root_id())
}

/// Check if the document is encrypted.
///
/// The /Encrypt entry lives in the trailer dictionary (or xref stream dict),
/// not in the catalog. We scan the raw PDF bytes for it.
pub fn is_encrypted(pdf: &Pdf) -> bool {
    let cache = ObjectCache::new(pdf);
    is_encrypted_cached(pdf, &cache)
}

/// Cached version of is_encrypted.
pub fn is_encrypted_cached(pdf: &Pdf, cache: &ObjectCache<'_>) -> bool {
    let data = pdf.data().as_ref();

    // Look for /Encrypt in the trailer dictionary section.
    if let Some(trailer_pos) = data.windows(7).rposition(|w| w == b"trailer") {
        let end = data.len().min(trailer_pos + 2000);
        let trailer_region = &data[trailer_pos..end];
        if trailer_region.windows(8).any(|w| w == b"/Encrypt") {
            return true;
        }
    }

    // Also check xref stream dictionaries (PDF 1.5+).
    for obj in cache.iter() {
        if let Object::Stream(s) = obj {
            let dict = s.dict();
            if let Some(t) = dict.get::<Name>(keys::TYPE) {
                if t.as_ref() == keys::XREF && dict.contains_key(keys::ENCRYPT) {
                    return true;
                }
            }
        }
    }

    false
}

/// Get XMP metadata as bytes from the catalog Metadata stream.
pub fn get_xmp_metadata(pdf: &Pdf) -> Option<Vec<u8>> {
    let cat = catalog(pdf)?;
    // Attempt to resolve /Metadata via pdf-syntax and decode the stream.
    // cat.get() can return None when the stream keyword is malformed (e.g. 'stream '
    // with a space before the EOL — §6.1.7.1 violation), because pdf-syntax cannot
    // locate the stream body and the object fails to parse as a Stream. (#FP-6.7.11)
    if let Some(stream) = cat.get::<Stream<'_>>(keys::METADATA) {
        if let Ok(data) = stream.decoded() {
            if !data.is_empty() {
                return Some(data);
            }
        }
        let raw = stream.raw_data();
        if !raw.is_empty() {
            return Some(raw.to_vec());
        }
    }
    // Fallback: raw-byte scan for <?xpacket in the whole PDF.
    // Only activate when the catalog has a /Metadata entry; don't fabricate XMP for
    // documents that genuinely lack it.
    if !cat.contains_key(keys::METADATA) {
        return None;
    }
    let raw = pdf.data().as_ref();
    let needle = b"<?xpacket";
    let start = raw.windows(needle.len()).position(|w| w == needle)?;
    let end_needle = b"<?xpacket end";
    let end = raw[start..]
        .windows(end_needle.len())
        .position(|w| w == end_needle)
        .map(|off| {
            let rel = start + off + end_needle.len();
            raw[rel..]
                .windows(2)
                .position(|w| w == b"?>")
                .map_or(rel, |e| rel + e + 2)
        })
        .unwrap_or(raw.len());
    Some(raw[start..end].to_vec())
}

/// Returns `true` when the XMP text contains a closing tag `</ns:tag>` with no
/// corresponding opening tag anywhere in the document (case-sensitive).
/// Detects malformed XML like `<dc:creator>…</dc:Creator>` (wrong-case close tag).
/// veraPDF's strict XML parser throws on this; our regex would silently succeed.
pub(crate) fn xmp_has_mismatched_close_tags(text: &str) -> bool {
    let mut pos = 0;
    while let Some(close_rel) = text[pos..].find("</") {
        let name_start = pos + close_rel + 2;
        let Some(gt_rel) = text[name_start..].find('>') else {
            break;
        };
        let tag = text[name_start..name_start + gt_rel].trim();
        // Skip XML comments (<!--...) and processing instructions (<?...).
        if !tag.is_empty() && !tag.starts_with('!') && !tag.starts_with('?') {
            // A matching opening tag must be present: <tag>, <tag >, <tag\t>, or <tag\n>.
            let open = format!("<{tag}");
            let has_open = text.contains(&format!("{open}>"))
                || text.find(&open).is_some_and(|i| {
                    text.as_bytes()
                        .get(i + open.len())
                        .is_some_and(|&b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
                });
            if !has_open {
                return true;
            }
        }
        pos = name_start + gt_rel + 1;
    }
    false
}

/// Parse XMP metadata to find pdfaid:part and pdfaid:conformance.
///
/// PDF/A-4 (ISO 19005-4) may omit pdfaid:conformance entirely;
/// in that case, conformance defaults to an empty string.
pub fn parse_xmp_pdfa(xmp: &[u8]) -> Option<(u8, String)> {
    let text = std::str::from_utf8(xmp).ok()?;

    // Malformed XML (e.g. mismatched close tag) makes the XMP untrustworthy.
    // Return None so callers fall back to the PDF/A-1B default and fire §6.7.11,
    // matching veraPDF's behaviour. Fixes §6.7.11 FN on 6-7-2-1-t01-fail-d.pdf.
    if xmp_has_mismatched_close_tags(text) {
        return None;
    }

    // Wrong pdfaid namespace URI makes the identification schema untrustworthy.
    // Return None so callers fall back to PDF/A-1B default and fire §6.7.11.
    // Fixes §6.7.11 FN on cs-veraPDF test suite 6-7-3-t01-fail-a.pdf
    // (xmlns:pdfaid="http://www.aiim.org/pdfa/" — missing "/ns/id/").
    const CORRECT_PDFAID_NS: &str = "http://www.aiim.org/pdfa/ns/id/";
    if text.contains("xmlns:pdfaid") && !text.contains(CORRECT_PDFAID_NS) {
        return None;
    }

    let part = extract_xmp_value(text, "pdfaid:part")
        .or_else(|| extract_xmp_attr(text, "pdfaid:part"))?
        .parse::<u8>()
        .ok()?;

    let conformance = extract_xmp_value(text, "pdfaid:conformance")
        .or_else(|| extract_xmp_attr(text, "pdfaid:conformance"))
        .unwrap_or_default();

    Some((part, conformance))
}

/// Parse XMP metadata to find pdfuaid:part.
pub fn parse_xmp_pdfua(xmp: &[u8]) -> Option<u8> {
    let text = std::str::from_utf8(xmp).ok()?;
    extract_xmp_value(text, "pdfuaid:part")
        .or_else(|| extract_xmp_attr(text, "pdfuaid:part"))?
        .parse::<u8>()
        .ok()
}

/// Check XMP metadata contains PDF/A Identification Schema (§6.7.11).
///
/// Note: "missing XMP stream" is §6.7.2 (PDF/A-1), fired by check_xmp_metadata in pdfa.rs.
/// This function only fires §6.7.11 when XMP exists but is missing the pdfaid identification.
pub fn check_xmp_pdfa_identification(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(xmp) = get_xmp_metadata(pdf) else {
        // Missing XMP is handled by check_xmp_metadata with the correct per-part rule (§6.7.2 for PDF/A-1).
        return;
    };
    let text = String::from_utf8_lossy(&xmp);
    // Check for pdfaid:part presence
    if !text.contains("pdfaid:part") {
        error(
            report,
            "6.7.11",
            "XMP metadata missing PDF/A Identification Schema (pdfaid:part)",
        );
    }
}

/// Check PDF/A-4 conformance property is absent (§6.7.3).
/// For PDF/A-4, pdfaid:conformance must not be present.
pub fn check_pdfa4_conformance_absent(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(xmp) = get_xmp_metadata(pdf) else {
        return;
    };
    let text = String::from_utf8_lossy(&xmp);
    // Check if conformance is present
    if let Some(val) = extract_xmp_value(&text, "pdfaid:conformance") {
        if val.is_empty() || val.trim().is_empty() {
            error(
                report,
                "6.7.3",
                "PDF/A-4 must not have pdfaid:conformance (found empty value)",
            );
        } else {
            error(
                report,
                "6.7.3",
                format!("PDF/A-4 must not have pdfaid:conformance (found '{val}')"),
            );
        }
    } else if extract_xmp_attr(&text, "pdfaid:conformance").is_some() {
        error(
            report,
            "6.7.3",
            "PDF/A-4 must not have pdfaid:conformance attribute",
        );
    }
}

/// Check stream dicts for empty name keys (§6.1.7.1 t3, §6.1.6.1).
pub fn check_stream_empty_keys(pdf: &Pdf, report: &mut ComplianceReport) {
    let cache = ObjectCache::new(pdf);
    check_stream_empty_keys_cached(&cache, report);
}

/// Cached version using pre-collected objects.
pub fn check_stream_empty_keys_cached(cache: &ObjectCache<'_>, report: &mut ComplianceReport) {
    for obj in cache.iter() {
        if let Object::Stream(s) = obj {
            for (key, _) in s.dict().entries() {
                if key.as_ref().is_empty() {
                    error(report, "6.1.7.1", "Stream dictionary contains empty key");
                    return;
                }
            }
        }
    }
}

/// Check MarkInfo/Marked is present and true (§6.8.2.2).
pub fn check_mark_info(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    match cat.get::<Dict<'_>>(keys::MARK_INFO) {
        Some(mark_info) => match mark_info.get::<Object<'_>>(b"Marked" as &[u8]) {
            Some(Object::Boolean(true)) => {}
            _ => {
                error(report, "6.8.2.2", "MarkInfo /Marked is not set to true");
            }
        },
        None => {
            error(
                report,
                "6.8.2.2",
                "MarkInfo dictionary missing from catalog",
            );
        }
    }
}

/// Decode the five predefined XML entities in an XMP text value.
///
/// XMP text stored inside XML elements may use standard XML entities.
/// When comparing XMP values with PDF Info dict values (which have no
/// entity escaping), we must decode them first. (#FIX-6.7.3-xml-entities)
fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
}

/// Extract a value from an XMP element like `<ns:key>value</ns:key>`.
fn extract_xmp_value(text: &str, key: &str) -> Option<String> {
    let open = format!("<{key}>");
    let close = format!("</{key}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(decode_xml_entities(text[start..end].trim()))
}

/// Extract a value from an XMP attribute like `ns:key="value"` or `ns:key='value'`.
fn extract_xmp_attr(text: &str, key: &str) -> Option<String> {
    // Try double-quoted attribute first
    let pattern_dq = format!("{key}=\"");
    if let Some(start) = text.find(&pattern_dq) {
        let val_start = start + pattern_dq.len();
        if let Some(end) = text[val_start..].find('"') {
            return Some(decode_xml_entities(text[val_start..val_start + end].trim()));
        }
    }
    // Fall back to single-quoted attribute (e.g. pdfaid:part='2')
    let pattern_sq = format!("{key}='");
    if let Some(start) = text.find(&pattern_sq) {
        let val_start = start + pattern_sq.len();
        if let Some(end) = text[val_start..].find('\'') {
            return Some(decode_xml_entities(text[val_start..val_start + end].trim()));
        }
    }
    None
}

/// Extract the first value from an rdf:Alt container (e.g., dc:title).
fn extract_rdf_alt_value(text: &str, key: &str) -> Option<String> {
    let open = format!("<{key}>");
    let start = text.find(&open)?;
    let close = format!("</{key}>");
    let end = text.find(&close)?;
    let region = &text[start..end];
    // Find first <rdf:li ...>value</rdf:li>
    let li_start = region.find("<rdf:li")?;
    let content_start = region[li_start..].find('>')? + li_start + 1;
    let content_end = region[content_start..].find("</rdf:li>")? + content_start;
    Some(decode_xml_entities(
        region[content_start..content_end].trim(),
    ))
}

/// Extract all values from an rdf:Seq container and count entries.
fn extract_rdf_seq_values(text: &str, key: &str) -> (Vec<String>, usize) {
    let open = format!("<{key}>");
    let close = format!("</{key}>");
    let Some(start) = text.find(&open) else {
        return (vec![], 0);
    };
    let Some(end) = text[start..].find(&close) else {
        return (vec![], 0);
    };
    let region = &text[start..start + end];
    let mut values = Vec::new();
    let mut search = 0;
    while let Some(li_start) = region[search..].find("<rdf:li") {
        let abs_start = search + li_start;
        if let Some(gt) = region[abs_start..].find('>') {
            let content_start = abs_start + gt + 1;
            if let Some(li_end) = region[content_start..].find("</rdf:li>") {
                let content_end = content_start + li_end;
                values.push(region[content_start..content_end].trim().to_string());
                search = content_end;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    let count = values.len();
    (values, count)
}

/// Parse an XMP ISO 8601 datetime string into components.
///
/// Format: `YYYY-MM-DDThh:mm:ss[±hh:mm]`
fn parse_xmp_datetime(s: &str) -> Option<(u16, u8, u8, u8, u8, u8)> {
    // Remove timezone suffix for component parsing
    let s = s.trim();
    let base = if let Some(idx) = s.rfind('+') {
        if idx > 10 {
            &s[..idx]
        } else {
            s
        }
    } else if let Some(idx) = s.rfind('-') {
        if idx > 10 {
            &s[..idx]
        } else {
            s
        }
    } else {
        s.trim_end_matches('Z')
    };

    let parts: Vec<&str> = base.split('T').collect();
    let date_parts: Vec<&str> = parts.first()?.split('-').collect();
    let year: u16 = date_parts.first()?.parse().ok()?;
    let month: u8 = date_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let day: u8 = date_parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);

    let (hour, minute, second) = if let Some(time) = parts.get(1) {
        let time_parts: Vec<&str> = time.split(':').collect();
        let h: u8 = time_parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let m: u8 = time_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let s: u8 = time_parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        (h, m, s)
    } else {
        (0, 0, 0)
    };

    Some((year, month, day, hour, minute, second))
}

/// Check date value equivalence between Info dict and XMP.
///
/// Uses specific §6.7.3.X subclauses matching veraPDF numbering:
/// - §6.7.3.1 = CreationDate / xmp:CreateDate
/// - §6.7.3.8 = ModDate / xmp:ModifyDate
fn check_date_equivalence(
    pdf_date: &Option<pdf_syntax::object::DateTime>,
    info_key: &str,
    xmp_key: &str,
    xmp_text: &str,
    report: &mut ComplianceReport,
) {
    let Some(dt) = pdf_date else { return };
    let xmp_val =
        extract_xmp_value(xmp_text, xmp_key).or_else(|| extract_xmp_attr(xmp_text, xmp_key));
    let Some(xmp_str) = xmp_val else { return };

    // Map info_key to the appropriate §6.7.3.X subclause (#467)
    let rule = if info_key.contains("Mod") || xmp_key.contains("Modify") {
        "6.7.3.8"
    } else {
        "6.7.3.1"
    };

    if let Some((y, mo, d, _h, _mi, _s)) = parse_xmp_datetime(&xmp_str) {
        // Compare only the date portion (year/month/day).
        // Timezone differences between /Info and XMP can cause hour/minute mismatches
        // for identical timestamps — full UTC normalization would add complexity without
        // meaningful benefit. Fixes false positives on §6.7.3. Fixes #454.
        if dt.year != y || dt.month != mo || dt.day != d {
            error(
                report,
                rule,
                format!(
                    "{info_key} date mismatch: Info={:04}-{:02}-{:02} vs XMP={xmp_str}",
                    dt.year, dt.month, dt.day,
                ),
            );
        }
    }
}

/// Count the number of OutputIntent entries with S=GTS_PDFA1.
/// ISO 19005-1 §6.2.2 allows at most one; more than one is a violation.
pub fn count_gts_pdfa1_intents(pdf: &Pdf) -> usize {
    let Some(cat) = catalog(pdf) else { return 0 };
    let Some(intents) = cat.get::<Array<'_>>(keys::OUTPUT_INTENTS) else {
        return 0;
    };
    intents
        .iter::<Dict<'_>>()
        .filter(|d| {
            d.get::<Name>(keys::S)
                .is_some_and(|s| s.as_ref() == b"GTS_PDFA1")
        })
        .count()
}

/// Check if the catalog has an OutputIntents array with GTS_PDFA1 subtype.
pub fn has_output_intent(pdf: &Pdf) -> bool {
    let Some(cat) = catalog(pdf) else {
        return false;
    };
    let Some(intents) = cat.get::<Array<'_>>(keys::OUTPUT_INTENTS) else {
        return false;
    };
    for dict in intents.iter::<Dict<'_>>() {
        if let Some(s) = dict.get::<Name>(keys::S) {
            if s.as_ref() == b"GTS_PDFA1" {
                return true;
            }
        }
    }
    false
}

/// Determine the number of components in the OutputIntent's DestOutputProfile ICC profile.
/// Returns None if no GTS_PDFA1 OutputIntent or no parseable profile.
pub fn output_intent_profile_components(pdf: &Pdf) -> Option<u32> {
    let cat = catalog(pdf)?;
    let intents = cat.get::<Array<'_>>(keys::OUTPUT_INTENTS)?;
    for dict in intents.iter::<Dict<'_>>() {
        if let Some(s) = dict.get::<Name>(keys::S) {
            if s.as_ref() == b"GTS_PDFA1" {
                let stream = dict.get::<Stream<'_>>(keys::DEST_OUTPUT_PROFILE)?;
                let data = stream.decoded().ok()?;
                if data.len() < 20 {
                    return None;
                }
                // ICC profile header: bytes 16-19 = color space signature
                let cs_sig = &data[16..20];
                return Some(match cs_sig {
                    b"RGB " => 3,
                    b"CMYK" => 4,
                    b"GRAY" => 1,
                    _ => 0, // unknown
                });
            }
        }
    }
    None
}

/// Check device color usage matches OutputIntent profile color space (§6.2.3.3).
///
/// Even with an OutputIntent, device colors may only be used if the profile's
/// color space matches (e.g., DeviceCMYK only with CMYK OutputIntent).
/// Exception: if a Default* colour space is defined in the page's Resources
/// (DefaultCMYK/DefaultRGB/DefaultGray), the corresponding device colour space
/// may be used in content streams regardless of the OutputIntent's component
/// count (PDF/A-2 §6.2.3.3, PDF Reference §4.5.4).
/// Note: Default* does NOT apply to Image XObject /ColorSpace entries.
pub fn check_device_color_vs_output_intent(pdf: &Pdf, report: &mut ComplianceReport) {
    // 0 = no OutputIntent; device color spaces are forbidden without a matching profile.
    // Using 0 here causes all device-CS checks below to fire (since 0 ≠ 1/3/4). (#467)
    let profile_components = output_intent_profile_components(pdf).unwrap_or(0);

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let loc = format!("page {}", page_idx + 1);

        let page_dict = page.raw();
        let res_dict = page_dict.get::<Dict<'_>>(keys::RESOURCES);

        // Determine which device CS are covered by Default* resources on this page.
        // When DefaultCMYK/DefaultRGB/DefaultGray is present, the corresponding
        // device CS operators in content streams are always valid (the Default* ICC
        // profile provides the rendering intent regardless of OutputIntent components).
        let (default_cmyk, default_rgb, default_gray) = res_dict
            .as_ref()
            .map(|rd| page_default_color_spaces(rd))
            .unwrap_or((false, false, false));

        // Effective profile components, adjusted for Default* resources:
        // treat each covered device CS as if the profile matches exactly.
        let eff_cmyk = if default_cmyk { 4 } else { profile_components };
        let eff_rgb = if default_rgb { 3 } else { profile_components };
        let eff_gray = if default_gray { 1 } else { profile_components };

        // Scan page content stream
        if let Some(content) = page.page_stream() {
            let ops = detect_device_color_ops(content);
            report_color_vs_profile_eff(&ops, eff_cmyk, eff_rgb, eff_gray, &loc, report);
            // §6.2.4.3: implicit DeviceGray — painting operators with no prior color command
            // use DeviceGray as the default. Fire when gray has no matching profile/Default*.
            if eff_gray != 1
                && eff_gray != 3
                && eff_gray != 4
                && !ops.has_rgb
                && !ops.has_cmyk
                && !ops.has_gray
                && content_has_implicit_gray(content)
            {
                error_at(
                    report,
                    "6.2.4.3",
                    "Implicit DeviceGray (painting without explicit color) in page content",
                    loc.clone(),
                );
            }
        }

        // Scan Form XObject content streams
        if let Some(ref rd) = res_dict {
            if let Some(xobj_dict) = rd.get::<Dict<'_>>(keys::XOBJECT) {
                for (xname, _) in xobj_dict.entries() {
                    let Some(stream) = xobj_dict.get::<Stream<'_>>(xname.as_ref()) else {
                        continue;
                    };
                    let dict = stream.dict();
                    let is_form = dict
                        .get::<Name>(keys::SUBTYPE)
                        .is_some_and(|s| s.as_ref() == b"Form");
                    if !is_form {
                        continue;
                    }
                    if let Ok(decoded) = stream.decoded() {
                        let xname_str = std::str::from_utf8(xname.as_ref()).unwrap_or("?");
                        let xloc = format!("{loc} XObject {xname_str}");
                        let ops = detect_device_color_ops(&decoded);
                        report_color_vs_profile_eff(
                            &ops, eff_cmyk, eff_rgb, eff_gray, &xloc, report,
                        );
                    }
                }
            }
        }

        // Scan annotation appearance streams
        if let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) {
            for annot in annots.iter::<Dict<'_>>() {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    for key in [b"N" as &[u8], b"R", b"D"] {
                        let kstr = std::str::from_utf8(key).unwrap_or("?");
                        if let Some(stream) = ap.get::<Stream<'_>>(key) {
                            // Direct appearance stream (most common case)
                            if let Ok(decoded) = stream.decoded() {
                                let ops = detect_device_color_ops(&decoded);
                                let kloc = format!("{loc} AP/{kstr}");
                                report_color_vs_profile_eff(
                                    &ops, eff_cmyk, eff_rgb, eff_gray, &kloc, report,
                                );
                            }
                        } else if let Some(sub_dict) = ap.get::<Dict<'_>>(key) {
                            // §6.2.4.3: AP sub-state dict (e.g. /N << /True 13 0 R /False 14 0 R >>)
                            // Each sub-state value is an appearance stream; scan each for device colors.
                            for (state_name, _) in sub_dict.entries() {
                                if let Some(state_stream) =
                                    sub_dict.get::<Stream<'_>>(state_name.as_ref())
                                {
                                    if let Ok(decoded) = state_stream.decoded() {
                                        let ops = detect_device_color_ops(&decoded);
                                        let sstr =
                                            std::str::from_utf8(state_name.as_ref()).unwrap_or("?");
                                        let kloc = format!("{loc} AP/{kstr}/{sstr}");
                                        report_color_vs_profile_eff(
                                            &ops, eff_cmyk, eff_rgb, eff_gray, &kloc, report,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Scan Shading/Pattern resources for device CS vs profile mismatch.
        // Image XObjects use profile_components directly (Default* does not apply
        // to image XObject /ColorSpace entries per PDF spec §4.5.4).
        if let Some(ref rd) = res_dict {
            scan_shading_cs_vs_profile(rd, profile_components, &loc, report);
            scan_pattern_cs_vs_profile(rd, profile_components, &loc, report);
            scan_image_cs_vs_profile(rd, profile_components, &loc, report);
            scan_type3_charprocs_vs_profile(rd, eff_cmyk, eff_rgb, eff_gray, &loc, report);
            scan_smask_cs_vs_profile(rd, profile_components, &loc, report);
        }
    }
}

/// Detect which Default* colour spaces are defined in a Resources dictionary.
/// Returns (has_default_cmyk, has_default_rgb, has_default_gray).
fn page_default_color_spaces(res_dict: &Dict<'_>) -> (bool, bool, bool) {
    let Some(cs_dict) = res_dict.get::<Dict<'_>>(keys::COLORSPACE) else {
        return (false, false, false);
    };
    let has_cmyk = cs_dict.contains_key(b"DefaultCMYK" as &[u8]);
    let has_rgb = cs_dict.contains_key(b"DefaultRGB" as &[u8]);
    let has_gray = cs_dict.contains_key(b"DefaultGray" as &[u8]);
    (has_cmyk, has_rgb, has_gray)
}

/// Scan Shading resources for device CS vs OutputIntent profile (§6.2.3.3).
fn scan_shading_cs_vs_profile(
    res_dict: &Dict<'_>,
    profile_components: u32,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    let Some(shading_dict) = res_dict.get::<Dict<'_>>(b"Shading" as &[u8]) else {
        return;
    };
    for (name, _) in shading_dict.entries() {
        let sname = std::str::from_utf8(name.as_ref()).unwrap_or("?");
        let loc = format!("{base_loc} Shading {sname}");
        if let Some(sh) = shading_dict.get::<Dict<'_>>(name.as_ref()) {
            if let Some(cs) = sh.get::<Name>(keys::COLORSPACE) {
                report_cs_name_vs_profile(cs.as_ref(), profile_components, &loc, report);
            }
        } else if let Some(sh_stream) = shading_dict.get::<Stream<'_>>(name.as_ref()) {
            if let Some(cs) = sh_stream.dict().get::<Name>(keys::COLORSPACE) {
                report_cs_name_vs_profile(cs.as_ref(), profile_components, &loc, report);
            }
        }
    }
}

/// Scan Pattern resources for device CS vs OutputIntent profile (§6.2.3.3).
fn scan_pattern_cs_vs_profile(
    res_dict: &Dict<'_>,
    profile_components: u32,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    let Some(pat_dict) = res_dict.get::<Dict<'_>>(b"Pattern" as &[u8]) else {
        return;
    };
    for (name, _) in pat_dict.entries() {
        let pname = std::str::from_utf8(name.as_ref()).unwrap_or("?");
        if let Some(pat) = pat_dict.get::<Dict<'_>>(name.as_ref()) {
            if let Some(shading) = pat.get::<Dict<'_>>(b"Shading" as &[u8]) {
                if let Some(cs) = shading.get::<Name>(keys::COLORSPACE) {
                    let loc = format!("{base_loc} Pattern {pname}");
                    report_cs_name_vs_profile(cs.as_ref(), profile_components, &loc, report);
                }
            }
        }
        if let Some(pat_stream) = pat_dict.get::<Stream<'_>>(name.as_ref()) {
            let loc = format!("{base_loc} Pattern {pname}");
            if let Ok(decoded) = pat_stream.decoded() {
                let ops = detect_device_color_ops(&decoded);
                report_color_vs_profile(&ops, profile_components, &loc, report);
            }
            // Also check the tiling pattern's own Resources/ColorSpace dict.
            // Inline images inside the pattern can reference named color spaces
            // (e.g., /CS0 cs where CS0 => DeviceRGB) defined here. (#467)
            if let Some(pat_res) = pat_stream.dict().get::<Dict<'_>>(keys::RESOURCES) {
                if let Some(cs_dict) = pat_res.get::<Dict<'_>>(keys::COLORSPACE) {
                    for (csname, _) in cs_dict.entries() {
                        if let Some(cs_val) = cs_dict.get::<Name>(csname.as_ref()) {
                            report_cs_name_vs_profile(
                                cs_val.as_ref(),
                                profile_components,
                                &loc,
                                report,
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Scan Image XObject color spaces vs OutputIntent profile (§6.2.3.3).
fn scan_image_cs_vs_profile(
    res_dict: &Dict<'_>,
    profile_components: u32,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) else {
        return;
    };
    for (name, _) in xobj_dict.entries() {
        let Some(stream) = xobj_dict.get::<Stream<'_>>(name.as_ref()) else {
            continue;
        };
        let dict = stream.dict();
        let is_image = dict
            .get::<Name>(keys::SUBTYPE)
            .is_some_and(|s| s.as_ref() == keys::IMAGE);
        if !is_image {
            continue;
        }
        if let Some(cs) = dict.get::<Name>(keys::COLORSPACE) {
            let iname = std::str::from_utf8(name.as_ref()).unwrap_or("?");
            let loc = format!("{base_loc} Image {iname}");
            report_cs_name_vs_profile(cs.as_ref(), profile_components, &loc, report);
        }
    }
}

/// Scan Type 3 font CharProcs for device color vs OutputIntent profile (§6.2.3.3).
fn scan_type3_charprocs_vs_profile(
    res_dict: &Dict<'_>,
    eff_cmyk: u32,
    eff_rgb: u32,
    eff_gray: u32,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    let Some(font_dict) = res_dict.get::<Dict<'_>>(keys::FONT) else {
        return;
    };
    let rgb_ok = eff_rgb == 3;
    let cmyk_ok = eff_cmyk == 4;
    let gray_ok = eff_gray == 1 || eff_gray == 3 || eff_gray == 4;
    for (fname, _) in font_dict.entries() {
        let Some(font) = font_dict.get::<Dict<'_>>(fname.as_ref()) else {
            continue;
        };
        let is_type3 = font
            .get::<Name>(keys::SUBTYPE)
            .is_some_and(|s| s.as_ref() == b"Type3");
        if !is_type3 {
            continue;
        }
        let fstr = std::str::from_utf8(fname.as_ref()).unwrap_or("?");
        if let Some(charprocs) = font.get::<Dict<'_>>(b"CharProcs" as &[u8]) {
            for (cname, _) in charprocs.entries() {
                if let Some(stream) = charprocs.get::<Stream<'_>>(cname.as_ref()) {
                    if let Ok(decoded) = stream.decoded() {
                        let cstr = std::str::from_utf8(cname.as_ref()).unwrap_or("?");
                        let ops = detect_device_color_ops(&decoded);
                        let loc = format!("{base_loc} Type3Font {fstr} CharProc {cstr}");
                        report_color_vs_profile_eff(
                            &ops, eff_cmyk, eff_rgb, eff_gray, &loc, report,
                        );
                    }
                }
            }
        }
        // §6.2.4.3: Form XObjects in the Type3 font's /Resources may carry a
        // /Group with /S /Transparency and /CS <device-cs>, which counts as
        // implicit device-color use even if the CharProc streams are colorless.
        if let Some(font_res) = font.get::<Dict<'_>>(keys::RESOURCES) {
            if let Some(xobj_dict) = font_res.get::<Dict<'_>>(keys::XOBJECT) {
                for (xname, _) in xobj_dict.entries() {
                    let Some(xobj_stream) = xobj_dict.get::<Stream<'_>>(xname.as_ref()) else {
                        continue;
                    };
                    if let Some(group) = xobj_stream.dict().get::<Dict<'_>>(b"Group" as &[u8]) {
                        if let Some(cs) = group.get::<Name>(keys::CS) {
                            let xstr = std::str::from_utf8(xname.as_ref()).unwrap_or("?");
                            let xloc =
                                format!("{base_loc} Type3Font {fstr} XObject {xstr} Group /CS");
                            report_device_cs_name(
                                cs.as_ref(),
                                rgb_ok,
                                cmyk_ok,
                                gray_ok,
                                &xloc,
                                report,
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Scan SMask Form XObjects for device color vs OutputIntent profile (§6.2.3.3).
fn scan_smask_cs_vs_profile(
    res_dict: &Dict<'_>,
    profile_components: u32,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    let Some(gs_dict) = res_dict.get::<Dict<'_>>(keys::EXT_G_STATE) else {
        return;
    };
    for (gs_name, _) in gs_dict.entries() {
        let Some(gs) = gs_dict.get::<Dict<'_>>(gs_name.as_ref()) else {
            continue;
        };
        let Some(smask) = gs.get::<Dict<'_>>(keys::SMASK) else {
            continue;
        };
        if let Some(g_stream) = smask.get::<Stream<'_>>(b"G" as &[u8]) {
            if let Ok(decoded) = g_stream.decoded() {
                let gs_str = std::str::from_utf8(gs_name.as_ref()).unwrap_or("?");
                let sloc = format!("{base_loc} SMask {gs_str}");
                let ops = detect_device_color_ops(&decoded);
                report_color_vs_profile(&ops, profile_components, &sloc, report);
            }
        }
    }
}

/// Report device CS name vs OutputIntent profile mismatch (§6.2.3.3).
fn report_cs_name_vs_profile(
    cs_bytes: &[u8],
    profile_components: u32,
    location: &str,
    report: &mut ComplianceReport,
) {
    if cs_bytes == keys::DEVICE_RGB && profile_components != 3 {
        error_at(
            report,
            "6.2.3.3",
            "DeviceRGB used but OutputIntent profile is not RGB",
            location.to_string(),
        );
    }
    if cs_bytes == b"DeviceCMYK" && profile_components != 4 {
        error_at(
            report,
            "6.2.3.3",
            "DeviceCMYK used but OutputIntent profile is not CMYK",
            location.to_string(),
        );
    }
    if cs_bytes == b"DeviceGray"
        && profile_components != 1
        && profile_components != 3
        && profile_components != 4
    {
        error_at(
            report,
            "6.2.3.3",
            "DeviceGray used but OutputIntent profile is incompatible",
            location.to_string(),
        );
    }
}

fn report_color_vs_profile(
    ops: &DeviceColorOps,
    profile_components: u32,
    location: &str,
    report: &mut ComplianceReport,
) {
    report_color_vs_profile_eff(
        ops,
        profile_components,
        profile_components,
        profile_components,
        location,
        report,
    );
}

/// Like `report_color_vs_profile` but with per-device-CS effective component
/// counts that already account for Default* colour space substitutions.
fn report_color_vs_profile_eff(
    ops: &DeviceColorOps,
    eff_cmyk: u32,
    eff_rgb: u32,
    eff_gray: u32,
    location: &str,
    report: &mut ComplianceReport,
) {
    if ops.has_rgb && eff_rgb != 3 {
        error_at(
            report,
            "6.2.3.3",
            "DeviceRGB used but OutputIntent profile is not RGB",
            location.to_string(),
        );
    }
    if ops.has_cmyk && eff_cmyk != 4 {
        error_at(
            report,
            "6.2.3.3",
            "DeviceCMYK used but OutputIntent profile is not CMYK",
            location.to_string(),
        );
    }
    if ops.has_gray && eff_gray != 1 && eff_gray != 3 && eff_gray != 4 {
        // DeviceGray is implicitly compatible with RGB and CMYK profiles
        error_at(
            report,
            "6.2.3.3",
            "DeviceGray used but OutputIntent profile is incompatible",
            location.to_string(),
        );
    }
}

/// Check if a font descriptor has embedded font data.
pub fn font_has_embedding(desc: &Dict<'_>) -> bool {
    desc.get::<Stream<'_>>(keys::FONT_FILE).is_some()
        || desc.get::<Stream<'_>>(keys::FONT_FILE2).is_some()
        || desc.get::<Stream<'_>>(keys::FONT_FILE3).is_some()
}

/// Check if a font dictionary has a ToUnicode CMap.
pub fn font_has_tounicode(font_dict: &Dict<'_>) -> bool {
    font_dict.get::<Stream<'_>>(keys::TO_UNICODE).is_some()
}

/// Iterate over all font dictionaries in all page resources.
///
/// Uses `page.resources().fonts` which handles inherited /Resources
/// from parent Pages nodes, rather than only checking the page's own dict.
pub fn for_each_font<'a>(pdf: &'a Pdf, mut callback: impl FnMut(&str, &Dict<'a>, usize)) {
    let xref = pdf.xref();
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        // Helper: iterate fonts in a resource dict
        let mut visit_fonts = |fonts: &Dict<'a>| {
            for (name, _) in fonts.entries() {
                let name_str = std::str::from_utf8(name.as_ref()).unwrap_or("<invalid>");
                let font_dict_opt: Option<Dict<'a>> =
                    fonts.get::<Dict<'_>>(name.as_ref()).or_else(|| {
                        fonts
                            .get_ref(name.as_ref())
                            .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                    });
                if let Some(font_dict) = font_dict_opt {
                    callback(name_str, &font_dict, page_idx);
                }
            }
        };

        // Page-level fonts
        visit_fonts(&page.resources().fonts);

        // Fonts in Form XObject resources (catches fonts used via Do operator)
        let page_dict = page.raw();
        if let Some(res) = page_dict.get::<Dict<'_>>(keys::RESOURCES) {
            if let Some(xobjs) = res.get::<Dict<'_>>(keys::XOBJECT) {
                for (xname, _) in xobjs.entries() {
                    // Resolve indirect references via xref
                    let xobj_stream = xobjs.get::<Stream<'_>>(xname.as_ref()).or_else(|| {
                        xobjs
                            .get_ref(xname.as_ref())
                            .and_then(|r| xref.get::<Stream<'_>>(r.into()))
                    });
                    if let Some(stream) = xobj_stream {
                        let dict = stream.dict();
                        if dict
                            .get::<Name>(keys::SUBTYPE)
                            .is_some_and(|s| s.as_ref() == b"Form")
                        {
                            if let Some(xo_res) = dict.get::<Dict<'_>>(keys::RESOURCES) {
                                if let Some(xo_fonts) = xo_res.get::<Dict<'_>>(keys::FONT) {
                                    visit_fonts(&xo_fonts);
                                }
                            }
                        }
                    }
                }
            }
            // Fonts in tiling Pattern resources (isartor-6-3-4-t01-fail-h: font
            // only in Pattern, not in page /Font dict). Fixes §6.3.4 FN.
            if let Some(patterns) = res.get::<Dict<'_>>(keys::PATTERN) {
                for (pname, _) in patterns.entries() {
                    let pstream: Option<Stream<'_>> =
                        patterns.get::<Stream<'_>>(pname.as_ref()).or_else(|| {
                            patterns
                                .get_ref(pname.as_ref())
                                .and_then(|r| xref.get::<Stream<'_>>(r.into()))
                        });
                    if let Some(pstream) = pstream {
                        if let Some(p_res) = pstream.dict().get::<Dict<'_>>(keys::RESOURCES) {
                            if let Some(p_fonts) = p_res.get::<Dict<'_>>(keys::FONT) {
                                visit_fonts(&p_fonts);
                            }
                        }
                    }
                }
            }
            // Fonts in annotation appearance streams — §6.3.4 applies to fonts used
            // in widget/annotation AP (appearance) streams too.
            // isartor-6-3-4-t01-fail-f: ZapfDingbats in Widget annotation AP/N stream.
            // (#FN-6.3.4)
            for annot in page.annots() {
                let ap_opt: Option<Dict<'a>> = annot.get::<Dict<'_>>(keys::AP).or_else(|| {
                    annot
                        .get_ref(keys::AP)
                        .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                });
                if let Some(ap) = ap_opt {
                    for (ap_key, _) in ap.entries() {
                        let ap_stream: Option<Stream<'a>> =
                            ap.get::<Stream<'_>>(ap_key.as_ref()).or_else(|| {
                                ap.get_ref(ap_key.as_ref())
                                    .and_then(|r| xref.get::<Stream<'_>>(r.into()))
                            });
                        if let Some(stream) = ap_stream {
                            if let Some(ap_res) = stream.dict().get::<Dict<'_>>(keys::RESOURCES) {
                                if let Some(ap_fonts) = ap_res.get::<Dict<'_>>(keys::FONT) {
                                    visit_fonts(&ap_fonts);
                                }
                            }
                        }
                    }
                }
            }

            // Fonts in Type3 CharProcs /Resources — §6.3.4 applies to fonts used
            // inside Type3 glyph programs too.  isartor-6-3-4-t01-fail-g: page uses
            // a Type3 font whose CharProcs reference Helvetica via /Resources, and
            // Helvetica has no FontFile → not embedded.  Fixes §6.3.4 FN.
            if let Some(page_font_dict) = res.get::<Dict<'_>>(keys::FONT) {
                for (fname, _) in page_font_dict.entries() {
                    let fd: Option<Dict<'a>> =
                        page_font_dict.get::<Dict<'a>>(fname.as_ref()).or_else(|| {
                            page_font_dict
                                .get_ref(fname.as_ref())
                                .and_then(|r| xref.get::<Dict<'a>>(r.into()))
                        });
                    if let Some(fd) = fd {
                        if fd
                            .get::<Name>(keys::SUBTYPE)
                            .is_some_and(|s| s.as_ref() == b"Type3")
                        {
                            let t3_res: Option<Dict<'a>> =
                                fd.get::<Dict<'a>>(keys::RESOURCES).or_else(|| {
                                    fd.get_ref(keys::RESOURCES)
                                        .and_then(|r| xref.get::<Dict<'a>>(r.into()))
                                });
                            if let Some(t3_res) = t3_res {
                                if let Some(t3_fonts) =
                                    t3_res.get::<Dict<'a>>(keys::FONT).or_else(|| {
                                        t3_res
                                            .get_ref(keys::FONT)
                                            .and_then(|r| xref.get::<Dict<'a>>(r.into()))
                                    })
                                {
                                    visit_fonts(&t3_fonts);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Check if the document has embedded files.
///
/// Checks the Names/EmbeddedFiles tree, /AF array on the catalog, and also
/// scans all objects for file specification dicts containing an /EF key
/// (veraPDF 6.1.11 test 1).
pub fn has_embedded_files(pdf: &Pdf) -> bool {
    let Some(cat) = catalog(pdf) else {
        return false;
    };

    if let Some(names) = cat.get::<Dict<'_>>(keys::NAMES) {
        if names.get::<Object<'_>>(keys::EMBEDDED_FILES).is_some() {
            return true;
        }
    }

    if cat.get::<Array<'_>>(keys::AF).is_some() {
        return true;
    }

    // Scan all indirect objects for file spec dicts with /EF key
    for obj in pdf.objects() {
        match &obj {
            Object::Dict(dict) if dict.contains_key(keys::EF) => return true,
            Object::Stream(s) if s.dict().contains_key(keys::EF) => return true,
            _ => {}
        }
    }
    false
}

pub fn has_embedded_files_cached(pdf: &Pdf, cache: &ObjectCache<'_>) -> bool {
    let Some(cat) = catalog(pdf) else {
        return false;
    };

    if let Some(names) = cat.get::<Dict<'_>>(keys::NAMES) {
        if names.get::<Object<'_>>(keys::EMBEDDED_FILES).is_some() {
            return true;
        }
    }

    if cat.get::<Array<'_>>(keys::AF).is_some() {
        return true;
    }

    for obj in cache.iter() {
        match obj {
            Object::Dict(dict) if dict.contains_key(keys::EF) => return true,
            Object::Stream(s) if s.dict().contains_key(keys::EF) => return true,
            _ => {}
        }
    }
    false
}

/// Resolve the /Group dict from a page dict, handling indirect references.
///
/// Some PDF generators write `/Group 10 0 R` (indirect) rather than an inline
/// dict.  A plain `dict.get::<Dict<'_>>(keys::GROUP)` call only succeeds for
/// inline dicts; this helper also tries `get_ref` + xref lookup. (#467)
fn resolve_page_group_dict<'a>(
    page_dict: &Dict<'a>,
    xref: &'a pdf_syntax::xref::XRef,
) -> Option<Dict<'a>> {
    page_dict.get::<Dict<'_>>(keys::GROUP).or_else(|| {
        page_dict
            .get_ref(keys::GROUP)
            .and_then(|r| xref.get::<Dict<'_>>(r.into()))
    })
}

/// Check if any page has transparency (Group with /S /Transparency).
pub fn has_transparency(pdf: &Pdf) -> bool {
    let xref = pdf.xref();
    for page in pdf.pages().iter() {
        let page_dict = page.raw();
        if let Some(group) = resolve_page_group_dict(page_dict, xref) {
            if let Some(s) = group.get::<Name>(keys::S) {
                if s.as_ref() == keys::TRANSPARENCY {
                    return true;
                }
            }
        }
    }
    false
}

/// Get the StructTreeRoot dictionary if present.
pub fn struct_tree_root<'a>(pdf: &'a Pdf) -> Option<Dict<'a>> {
    let cat = catalog(pdf)?;
    cat.get::<Dict<'_>>(keys::STRUCT_TREE_ROOT)
}

/// Check if the document has a MarkInfo/Marked = true entry.
pub fn is_marked(pdf: &Pdf) -> bool {
    let Some(cat) = catalog(pdf) else {
        return false;
    };
    if let Some(mark_info) = cat.get::<Dict<'_>>(keys::MARK_INFO) {
        if let Some(Object::Boolean(marked)) = mark_info.get::<Object<'_>>(b"Marked" as &[u8]) {
            return marked;
        }
    }
    false
}

/// Get the document language from the catalog /Lang entry.
///
/// PDF strings may be encoded as UTF-16BE (hex string with `\xFE\xFF` BOM)
/// or as PDFDocEncoding/UTF-8 (literal string).  `from_utf8` fails on the
/// BOM bytes, causing a false "missing /Lang" report for perfectly valid
/// documents (e.g. veraPDF test suite `cs-7.1-t02-pass-a.pdf`).
pub fn document_lang(pdf: &Pdf) -> Option<String> {
    let cat = catalog(pdf)?;
    let lang = cat.get::<pdf_syntax::object::String>(keys::LANG)?;
    let bytes = lang.as_bytes();
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        // UTF-16BE with BOM
        let u16s: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        Some(std::string::String::from_utf16_lossy(&u16s))
    } else {
        // PDFDocEncoding / UTF-8 literal string
        std::string::String::from_utf8(bytes.to_vec())
            .ok()
            .or_else(|| Some(bytes.iter().map(|&b| b as char).collect()))
    }
}

/// Validate that a language tag follows basic BCP-47 format.
/// Primary subtag must be 2-3 ASCII letters.
fn is_valid_lang_tag(tag: &str) -> bool {
    if tag.is_empty() {
        return false;
    }
    let parts: Vec<&str> = tag.split('-').collect();
    // Primary subtag: 2-3 alpha
    let primary = parts[0];
    if !((primary.len() == 2 || primary.len() == 3)
        && primary.bytes().all(|b| b.is_ascii_alphabetic()))
    {
        return false;
    }
    // Validate subsequent subtags (simplified BCP 47)
    for sub in &parts[1..] {
        let len = sub.len();
        let all_alpha = sub.bytes().all(|b| b.is_ascii_alphabetic());
        let all_digit = sub.bytes().all(|b| b.is_ascii_digit());
        let all_alnum = sub.bytes().all(|b| b.is_ascii_alphanumeric());
        // Script: 4 alpha, Region: 2 alpha or 3 digit, Variant: 5-8 alnum or 4+ starting with digit
        let valid = ((len == 2 || len == 4) && all_alpha) // region (2 alpha) or script (4 alpha)
            || (len == 3 && all_digit)              // region (numeric)
            || ((5..=8).contains(&len) && all_alnum) // variant
            || (len >= 4 && sub.as_bytes()[0].is_ascii_digit() && all_alnum) // variant (digit start)
            || (len == 1 && sub.as_bytes()[0].is_ascii_alphanumeric()); // singleton
        if !valid {
            return false;
        }
    }
    true
}

/// Check /Lang entries in catalog and structure elements are valid BCP-47.
///
/// `rule` is the clause to emit: "6.7.4" for PDF/A-2/3 (veraPDF numbers it there),
/// "6.8.4" for PDF/A-1/4.
/// Decode a PDF string as text: try UTF-8 first, then UTF-16BE (with BOM `\xFE\xFF`).
/// Returns None if the bytes cannot be decoded as either.
fn decode_pdf_string(bytes: &[u8]) -> Option<String> {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Some(s.to_string());
    }
    // Try UTF-16BE with BOM (0xFE 0xFF)
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let pairs: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16(&pairs).ok();
    }
    None
}

pub fn check_lang_values(pdf: &Pdf, rule: &str, report: &mut ComplianceReport) {
    // Check catalog /Lang
    if let Some(cat) = catalog(pdf) {
        if let Some(lang_str) = cat.get::<pdf_syntax::object::String>(keys::LANG) {
            // /Lang may be a UTF-16BE string (hex-encoded with BOM 0xFE 0xFF).
            // (#FN-6.8.4 / #FN-6.7.4)
            if let Some(tag) = decode_pdf_string(lang_str.as_bytes()) {
                if !is_valid_lang_tag(&tag) {
                    error(
                        report,
                        rule,
                        format!("Catalog /Lang value '{tag}' is not a valid Language-Tag"),
                    );
                }
            }
        }
        // Check structure tree /Lang entries
        if let Some(struct_root) = cat.get::<Dict<'_>>(keys::STRUCT_TREE_ROOT) {
            check_struct_elem_lang(&struct_root, rule, report);
        }
    }
    // Check /Lang in BDC inline property dicts in page content streams.
    // §6.7.4 / §6.8.4: all /Lang occurrences must be valid BCP-47. (#FN-6.8.4 / #FN-6.7.4)
    for page in pdf.pages().iter() {
        if let Some(content) = page.page_stream() {
            if content.len() <= MAX_CONTENT_STREAM_SCAN_SIZE {
                for tag in scan_bdc_lang_values(content) {
                    if !is_valid_lang_tag(&tag) {
                        error(
                            report,
                            rule,
                            format!("BDC /Lang value '{tag}' is not a valid Language-Tag"),
                        );
                    }
                }
            }
        }
    }
}

fn check_struct_elem_lang(elem: &Dict<'_>, rule: &str, report: &mut ComplianceReport) {
    check_struct_elem_lang_bounded(elem, rule, report, 0, &mut 0);
}

fn check_struct_elem_lang_bounded(
    elem: &Dict<'_>,
    rule: &str,
    report: &mut ComplianceReport,
    depth: usize,
    visited: &mut usize,
) {
    const MAX_DEPTH: usize = 100;
    const MAX_NODES: usize = 10_000;

    if depth > MAX_DEPTH || *visited >= MAX_NODES {
        return;
    }
    *visited += 1;

    if let Some(lang_str) = elem.get::<pdf_syntax::object::String>(keys::LANG) {
        if let Some(tag) = decode_pdf_string(lang_str.as_bytes()) {
            if !is_valid_lang_tag(&tag) {
                error(
                    report,
                    rule,
                    format!("Structure element /Lang value '{tag}' is not a valid Language-Tag"),
                );
            }
        }
    }
    if let Some(kids) = elem.get::<Array<'_>>(keys::K) {
        for kid in kids.iter::<Dict<'_>>() {
            check_struct_elem_lang_bounded(&kid, rule, report, depth + 1, visited);
            if *visited >= MAX_NODES {
                return;
            }
        }
    }
}

/// Check ViewerPreferences/DisplayDocTitle.
pub fn display_doc_title(pdf: &Pdf) -> bool {
    let Some(cat) = catalog(pdf) else {
        return false;
    };
    let Some(vp) = cat.get::<Dict<'_>>(keys::VIEWER_PREFERENCES) else {
        return false;
    };
    matches!(
        vp.get::<Object<'_>>(keys::DISPLAY_DOC_TITLE),
        Some(Object::Boolean(true))
    )
}

/// Check if page has /Tabs = /S.
pub fn page_has_tab_order_s(page_dict: &Dict<'_>) -> bool {
    if let Some(tabs) = page_dict.get::<Name>(b"Tabs" as &[u8]) {
        tabs.as_ref() == keys::S
    } else {
        false
    }
}

/// Check XMP properties use only predefined or declared extension schemas (§6.6.2.3.1 / §6.7.9).
///
/// All XMP properties must come from known schemas (xmp, dc, xmpMM, pdf, pdfaid, etc.)
/// or be declared via pdfaExtension:schemas.
#[allow(dead_code)]
pub fn check_xmp_schemas(xmp: &[u8], rule: &str, report: &mut ComplianceReport) {
    let Ok(text) = std::str::from_utf8(xmp) else {
        return;
    };

    // Known predefined XMP namespaces (PDF/A-1 §6.7.9, PDF/A-2 §6.6.2.3.1).
    // xmlns: is included so that namespace binding declarations (xmlns:foo="...") on
    // rdf:Description elements are never flagged as undeclared property usages. Fixes #453.
    let predefined_prefixes = [
        "dc:",
        "xmp:",
        "xmpMM:",
        "xmpRights:",
        "xmpTPg:",
        "xmpDM:",
        "pdf:",
        "pdfaid:",
        "pdfuaid:",
        "pdfx:",
        "pdfxid:",
        "pdfa:",
        "pdfaExtension:",
        "pdfaSchema:",
        "pdfaProperty:",
        "pdfaType:",
        "pdfaField:",
        "photoshop:",
        "tiff:",
        "exif:",
        "stRef:",
        "stEvt:",
        "stFnt:",
        "stDim:",
        "xmpG:",
        "xmpBJ:",
        "rdf:",
        "xml:",
        "x:",
        // Namespace binding declarations — not property usages
        "xmlns:",
        // Common tool-specific namespaces added by real-world applications
        "Iptc4xmpCore:",
        "Iptc4xmpExt:",
        "illustrator:",
        "crs:",
        "lr:",
        "xmpNote:",
        "MicrosoftPhoto:",
        "mediapro:",
        "mp:",
        "creatorAtom:",
        "GPano:",
        "aux:",
        "stArea:",
        "stCamera:",
        "stJob:",
        "xmpGImg:",
        "xmp_iptc_ext:",
        "plus:",
        "acdsee:",
        "digiKam:",
        "kipi:",
    ];

    // Check if extension schemas are declared
    let has_extension_schemas = text.contains("pdfaExtension:schemas");

    // Find all namespace-prefixed properties in XMP
    // Look for patterns like <prefix:property> or prefix:property="value"
    let mut pos = 0;
    let bytes = text.as_bytes();
    while pos < bytes.len() {
        // Look for '<' or space followed by a namespace prefix
        if bytes[pos] == b'<' || bytes[pos] == b' ' {
            let start = pos + 1;
            if start < bytes.len() && bytes[start].is_ascii_alphabetic() {
                // Find the colon
                if let Some(colon_offset) = text[start..].find(':') {
                    let prefix_end = start + colon_offset + 1;
                    let prefix = &text[start..prefix_end];

                    // Skip closing tags
                    if prefix.starts_with('/') {
                        pos = prefix_end;
                        continue;
                    }

                    // Check if it's a known prefix
                    if !predefined_prefixes.contains(&prefix)
                        && !has_extension_schemas
                        && prefix
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == ':')
                        && prefix.len() < 30
                    {
                        let prop_end = text[prefix_end..]
                            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                            .map(|i| prefix_end + i)
                            .unwrap_or(prefix_end);
                        let full_prop = &text[start..prop_end];
                        if !full_prop.is_empty() && full_prop.contains(':') {
                            error(
                                report,
                                rule,
                                format!(
                                    "XMP property '{full_prop}' uses undeclared schema prefix without extension schema"
                                ),
                            );
                            return; // Report once per document
                        }
                    }
                }
            }
        }
        pos += 1;
    }
}

/// Check for forbidden actions with configurable rule clause.
///
/// PDF/A-1: Launch, Sound, Movie, ResetForm, ImportData, JavaScript + deprecated.
/// PDF/A-2/3: additionally Hide, Rendition, Trans, GoTo3DView, SetOCGState.
pub fn check_forbidden_actions_rule(
    pdf: &Pdf,
    part: u8,
    rule: &str,
    report: &mut ComplianceReport,
) {
    let mut forbidden: Vec<&[u8]> = vec![
        b"Launch",
        b"Sound",
        b"Movie",
        b"ResetForm",
        b"ImportData",
        keys::JAVA_SCRIPT,
        b"SetState",
        b"NoOp",
        b"NOP",
    ];

    if part >= 2 {
        forbidden.extend_from_slice(&[
            b"Hide",
            b"SetOCGState",
            b"Rendition",
            b"Trans",
            b"GoTo3DView",
        ]);
    }

    // Check catalog OpenAction
    if let Some(cat) = catalog(pdf) {
        if let Some(action) = cat.get::<Dict<'_>>(keys::OPEN_ACTION) {
            check_action_forbidden(&action, &forbidden, rule, "catalog", report);
        }
        // Check catalog AA
        if let Some(aa) = cat.get::<Dict<'_>>(keys::AA) {
            for (trigger, _) in aa.entries() {
                if let Some(action) = aa.get::<Dict<'_>>(trigger.as_ref()) {
                    check_action_forbidden(&action, &forbidden, rule, "catalog AA", report);
                }
            }
        }
    }

    // Check page annotations
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();

        // Check page-level AA
        if let Some(aa) = page_dict.get::<Dict<'_>>(keys::AA) {
            for (trigger, _) in aa.entries() {
                if let Some(action) = aa.get::<Dict<'_>>(trigger.as_ref()) {
                    let loc = format!("page {}", page_idx + 1);
                    check_action_forbidden(&action, &forbidden, rule, &loc, report);
                }
            }
        }

        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            if let Some(action) = annot.get::<Dict<'_>>(keys::A) {
                let loc = format!("page {}", page_idx + 1);
                check_action_forbidden(&action, &forbidden, rule, &loc, report);
            }
        }
    }
}

fn check_action_forbidden(
    action: &Dict<'_>,
    forbidden: &[&[u8]],
    rule: &str,
    location: &str,
    report: &mut ComplianceReport,
) {
    if let Some(s) = action.get::<Name>(keys::S) {
        if forbidden.iter().any(|f| s.as_ref() == *f) {
            let action_name = std::str::from_utf8(s.as_ref()).unwrap_or("?");
            error_at(
                report,
                rule,
                format!("Forbidden action type: {action_name}"),
                location.to_string(),
            );
        }
    }
}

/// Check device-dependent color spaces have Default alternatives (§6.2.4.3).
///
/// DeviceRGB/CMYK/Gray may only be used if DefaultRGB/DefaultCMYK/DefaultGray
/// is set in the ColorSpace resources (unless an OutputIntent is present).
pub fn check_device_colorspaces(pdf: &Pdf, report: &mut ComplianceReport) {
    // OutputIntent profile covers matching device colors but NOT all colors.
    // E.g., an RGB OutputIntent covers DeviceRGB but NOT DeviceCMYK.
    let profile = output_intent_profile_components(pdf);
    let has_intent = profile.is_some();

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let res = page.resources();

        // Check if Default color spaces are defined in resolved resources
        let cs_dict_ref = &res.color_spaces;
        let has_default_rgb = cs_dict_ref.get::<Object<'_>>(keys::DEFAULT_RGB).is_some();
        let has_default_cmyk = cs_dict_ref.get::<Object<'_>>(keys::DEFAULT_CMYK).is_some();
        let has_default_gray = cs_dict_ref.get::<Object<'_>>(keys::DEFAULT_GRAY).is_some();

        // A device color is "covered" if there's a Default* CS or matching profile.
        // DeviceGray is covered by any OutputIntent (gray maps to any profile).
        let rgb_ok = has_default_rgb || profile == Some(3);
        let cmyk_ok = has_default_cmyk || profile == Some(4);
        let gray_ok = has_default_gray || has_intent;

        let loc = format!("page {}", page_idx + 1);

        // Scan page content stream
        if let Some(content) = page.page_stream() {
            report_device_color_ops(content, rgb_ok, cmyk_ok, gray_ok, &loc, report);
        }

        // Scan Form XObject content streams (using resolved resources)
        scan_xobjects_device_colors(&res.x_objects, rgb_ok, cmyk_ok, gray_ok, &loc, report);

        // Scan annotation appearance streams
        if let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) {
            for annot in annots.iter::<Dict<'_>>() {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    scan_appearance_dict_colors(&ap, rgb_ok, cmyk_ok, gray_ok, &loc, report);
                }
            }
        }

        // Scan Shading resources for device color spaces (using resolved resources)
        scan_shading_dict_device_colors(&res.shadings, rgb_ok, cmyk_ok, gray_ok, &loc, report);
        scan_pattern_dict_device_colors(&res.patterns, rgb_ok, cmyk_ok, gray_ok, &loc, report);

        // Scan Type 3 font CharProcs for device color operators
        scan_fonts_type3_charprocs(&res.fonts, rgb_ok, cmyk_ok, gray_ok, &loc, report);

        // Scan SMask Form XObjects in ExtGState for device color operators
        scan_extgstate_smask_colors(&res.ext_g_states, rgb_ok, cmyk_ok, gray_ok, &loc, report);

        // Check ColorSpace resources for device CS references (direct names and arrays)
        let cs_dict = &res.color_spaces;
        for (name, _) in cs_dict.entries() {
            if let Some(cs_name) = cs_dict.get::<Name>(name.as_ref()) {
                let cs = cs_name.as_ref();
                if !rgb_ok && cs == keys::DEVICE_RGB {
                    error_at(report, "6.2.4.3", "DeviceRGB in ColorSpace resources without DefaultRGB or matching OutputIntent", loc.clone());
                }
                if !cmyk_ok && cs == b"DeviceCMYK" {
                    error_at(report, "6.2.4.3", "DeviceCMYK in ColorSpace resources without DefaultCMYK or matching OutputIntent", loc.clone());
                }
                if !gray_ok && cs == b"DeviceGray" {
                    error_at(
                        report,
                        "6.2.4.3",
                        "DeviceGray in ColorSpace resources without DefaultGray or OutputIntent",
                        loc.clone(),
                    );
                }
            } else if let Some(cs_arr) = cs_dict.get::<Array<'_>>(name.as_ref()) {
                // Separation/DeviceN alternate or Indexed base may be a device CS
                let kind = extract_base_device_cs_kind(&cs_arr);
                let nstr = std::str::from_utf8(name.as_ref()).unwrap_or("?");
                if kind == 1 && !rgb_ok {
                    error_at(report, "6.2.4.3", format!("ColorSpace {nstr} has DeviceRGB alternate/base without DefaultRGB or matching OutputIntent"), loc.clone());
                }
                if kind == 2 && !cmyk_ok {
                    error_at(report, "6.2.4.3", format!("ColorSpace {nstr} has DeviceCMYK alternate/base without DefaultCMYK or matching OutputIntent"), loc.clone());
                }
                if kind == 3 && !gray_ok {
                    error_at(report, "6.2.4.3", format!("ColorSpace {nstr} has DeviceGray alternate/base without DefaultGray or OutputIntent"), loc.clone());
                }
            }
        }

        // Also scan Form XObject resources for named CS with device alternates/bases
        scan_xobject_cs_arrays(&res.x_objects, rgb_ok, cmyk_ok, gray_ok, &loc, report);
    }
}

/// Check if a resource dict has a Default color space defined.
fn has_default_cs(res: Option<&Dict<'_>>, key: &[u8]) -> bool {
    res.and_then(|r| r.get::<Dict<'_>>(keys::COLORSPACE))
        .and_then(|cs| cs.get::<Object<'_>>(key))
        .is_some()
}

/// Scan XObject dict for device color operators.
fn scan_xobjects_device_colors(
    xobj_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    for (xname, _) in xobj_dict.entries() {
        let Some(stream) = xobj_dict.get::<Stream<'_>>(xname.as_ref()) else {
            continue;
        };
        let dict = stream.dict();
        let is_form = dict
            .get::<Name>(keys::SUBTYPE)
            .is_some_and(|s| s.as_ref() == b"Form");
        if !is_form {
            continue;
        }
        // Form XObjects may have their own Default CS
        let form_res = dict.get::<Dict<'_>>(keys::RESOURCES);
        let f_rgb = rgb_ok || has_default_cs(form_res.as_ref(), keys::DEFAULT_RGB);
        let f_cmyk = cmyk_ok || has_default_cs(form_res.as_ref(), keys::DEFAULT_CMYK);
        let f_gray = gray_ok || has_default_cs(form_res.as_ref(), keys::DEFAULT_GRAY);

        if let Ok(decoded) = stream.decoded() {
            let xname_str = std::str::from_utf8(xname.as_ref()).unwrap_or("?");
            let xloc = format!("{base_loc} XObject {xname_str}");
            report_device_color_ops(&decoded, f_rgb, f_cmyk, f_gray, &xloc, report);
        }
    }
}

/// Scan Form XObject resources for named color spaces with device alternates/bases.
fn scan_xobject_cs_arrays(
    xobj_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    for (xname, _) in xobj_dict.entries() {
        let Some(stream) = xobj_dict.get::<Stream<'_>>(xname.as_ref()) else {
            continue;
        };
        let dict = stream.dict();
        let is_form = dict
            .get::<Name>(keys::SUBTYPE)
            .is_some_and(|s| s.as_ref() == b"Form");
        if !is_form {
            continue;
        }
        let Some(form_res) = dict.get::<Dict<'_>>(keys::RESOURCES) else {
            continue;
        };
        let Some(cs_dict) = form_res.get::<Dict<'_>>(keys::COLORSPACE) else {
            continue;
        };
        let xname_str = std::str::from_utf8(xname.as_ref()).unwrap_or("?");
        let xloc = format!("{base_loc} XObject {xname_str}");
        for (csname, _) in cs_dict.entries() {
            if let Some(cs_arr) = cs_dict.get::<Array<'_>>(csname.as_ref()) {
                let kind = extract_base_device_cs_kind(&cs_arr);
                let nstr = std::str::from_utf8(csname.as_ref()).unwrap_or("?");
                if kind == 1 && !rgb_ok {
                    error_at(
                        report,
                        "6.2.4.3",
                        format!("ColorSpace {nstr} has DeviceRGB alternate/base"),
                        xloc.clone(),
                    );
                }
                if kind == 2 && !cmyk_ok {
                    error_at(
                        report,
                        "6.2.4.3",
                        format!("ColorSpace {nstr} has DeviceCMYK alternate/base"),
                        xloc.clone(),
                    );
                }
                if kind == 3 && !gray_ok {
                    error_at(
                        report,
                        "6.2.4.3",
                        format!("ColorSpace {nstr} has DeviceGray alternate/base"),
                        xloc.clone(),
                    );
                }
            }
        }
    }
}

/// Scan an annotation's /AP appearance dict for device color operators.
fn scan_appearance_dict_colors(
    ap: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    // /N (normal), /R (rollover), /D (down) can each be a stream or dict of streams
    for key in [b"N" as &[u8], b"R", b"D"] {
        if let Some(stream) = ap.get::<Stream<'_>>(key) {
            if let Ok(decoded) = stream.decoded() {
                let kloc = format!("{base_loc} AP/{}", std::str::from_utf8(key).unwrap_or("?"));
                report_device_color_ops(&decoded, rgb_ok, cmyk_ok, gray_ok, &kloc, report);
            }
        }
    }
}

/// Scan Shading resources for device-dependent color spaces (§6.2.4.3).
fn scan_shading_device_colors(
    res_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    let Some(shading_dict) = res_dict.get::<Dict<'_>>(b"Shading" as &[u8]) else {
        return;
    };
    scan_shading_dict_device_colors(&shading_dict, rgb_ok, cmyk_ok, gray_ok, base_loc, report);
}

/// Scan a resolved Shading dict for device color spaces.
fn scan_shading_dict_device_colors(
    shading_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    for (name, _) in shading_dict.entries() {
        let sname = std::str::from_utf8(name.as_ref()).unwrap_or("?");
        let loc = format!("{base_loc} Shading {sname}");
        if let Some(sh) = shading_dict.get::<Dict<'_>>(name.as_ref()) {
            report_dict_cs_device(&sh, rgb_ok, cmyk_ok, gray_ok, &loc, report);
        } else if let Some(sh_stream) = shading_dict.get::<Stream<'_>>(name.as_ref()) {
            report_dict_cs_device(sh_stream.dict(), rgb_ok, cmyk_ok, gray_ok, &loc, report);
        }
    }
}

/// Scan Pattern resources for device-dependent color spaces (§6.2.4.3).
fn scan_pattern_device_colors(
    res_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    let Some(pat_dict) = res_dict.get::<Dict<'_>>(b"Pattern" as &[u8]) else {
        return;
    };
    scan_pattern_dict_device_colors(&pat_dict, rgb_ok, cmyk_ok, gray_ok, base_loc, report);
}

/// Scan a resolved Pattern dict for device color spaces.
fn scan_pattern_dict_device_colors(
    pat_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    for (name, _) in pat_dict.entries() {
        let pname = std::str::from_utf8(name.as_ref()).unwrap_or("?");
        // Type 2 patterns (shading patterns) have /Shading dict with /ColorSpace
        if let Some(pat) = pat_dict.get::<Dict<'_>>(name.as_ref()) {
            if let Some(shading) = pat.get::<Dict<'_>>(b"Shading" as &[u8]) {
                let ploc = format!("{base_loc} Pattern {pname}");
                report_dict_cs_device(&shading, rgb_ok, cmyk_ok, gray_ok, &ploc, report);
            }
        }
        // Pattern can also be a stream (tiling pattern) — scan its content
        if let Some(pat_stream) = pat_dict.get::<Stream<'_>>(name.as_ref()) {
            if let Ok(decoded) = pat_stream.decoded() {
                let ploc = format!("{base_loc} Pattern {pname}");
                report_device_color_ops(&decoded, rgb_ok, cmyk_ok, gray_ok, &ploc, report);
            }
        }
    }
}

/// Scan Type 3 font CharProcs for device color operators (§6.2.4.3).
/// Scan a resolved Font dict for Type 3 CharProc device color operators.
fn scan_fonts_type3_charprocs(
    font_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    for (fname, _) in font_dict.entries() {
        let Some(font) = font_dict.get::<Dict<'_>>(fname.as_ref()) else {
            continue;
        };
        // Only Type 3 fonts have CharProcs
        let is_type3 = font
            .get::<Name>(keys::SUBTYPE)
            .is_some_and(|s| s.as_ref() == b"Type3");
        if !is_type3 {
            continue;
        }
        let Some(charprocs) = font.get::<Dict<'_>>(b"CharProcs" as &[u8]) else {
            continue;
        };
        let fstr = std::str::from_utf8(fname.as_ref()).unwrap_or("?");
        for (cname, _) in charprocs.entries() {
            if let Some(stream) = charprocs.get::<Stream<'_>>(cname.as_ref()) {
                if let Ok(decoded) = stream.decoded() {
                    let cstr = std::str::from_utf8(cname.as_ref()).unwrap_or("?");
                    let cloc = format!("{base_loc} Type3Font {fstr} CharProc {cstr}");
                    report_device_color_ops(&decoded, rgb_ok, cmyk_ok, gray_ok, &cloc, report);
                    // Also check implicit DeviceGray in CharProcs:
                    // If no explicit color was set but painting operators are used,
                    // the default color space is DeviceGray
                    if !gray_ok {
                        let ops = detect_device_color_ops(&decoded);
                        if !ops.has_rgb
                            && !ops.has_cmyk
                            && !ops.has_gray
                            && content_has_implicit_gray(&decoded)
                        {
                            error_at(
                                report,
                                "6.2.4.3",
                                "Implicit DeviceGray (painting without setting color) in Type3 CharProc",
                                cloc,
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Scan ExtGState /SMask Form XObjects for device color operators (§6.2.4.3).
/// Scan a resolved ExtGState dict for SMask Form XObject device color operators.
fn scan_extgstate_smask_colors(
    gs_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    for (gs_name, _) in gs_dict.entries() {
        let Some(gs) = gs_dict.get::<Dict<'_>>(gs_name.as_ref()) else {
            continue;
        };
        // SMask can be a dict with /G pointing to a Form XObject stream
        let Some(smask) = gs.get::<Dict<'_>>(keys::SMASK) else {
            continue;
        };
        if let Some(g_stream) = smask.get::<Stream<'_>>(b"G" as &[u8]) {
            if let Ok(decoded) = g_stream.decoded() {
                let gs_str = std::str::from_utf8(gs_name.as_ref()).unwrap_or("?");
                let sloc = format!("{base_loc} SMask {gs_str}");
                report_device_color_ops(&decoded, rgb_ok, cmyk_ok, gray_ok, &sloc, report);
            }
            // Also check /ColorSpace on the SMask's Group dict
            let g_dict = g_stream.dict();
            if let Some(group) = g_dict.get::<Dict<'_>>(b"Group" as &[u8]) {
                if let Some(cs) = group.get::<Name>(keys::CS) {
                    report_device_cs_name(cs.as_ref(), rgb_ok, cmyk_ok, gray_ok, base_loc, report);
                }
            }
            // Check resources within the SMask form XObject
            if let Some(smask_res) = g_dict.get::<Dict<'_>>(keys::RESOURCES) {
                scan_shading_device_colors(&smask_res, rgb_ok, cmyk_ok, gray_ok, base_loc, report);
                scan_pattern_device_colors(&smask_res, rgb_ok, cmyk_ok, gray_ok, base_loc, report);
            }
        }
    }
}

/// Report a device CS name violation for 6.2.4.3.
fn report_device_cs_name(
    cs_bytes: &[u8],
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    location: &str,
    report: &mut ComplianceReport,
) {
    if !rgb_ok && cs_bytes == keys::DEVICE_RGB {
        error_at(
            report,
            "6.2.4.3",
            "DeviceRGB without DefaultRGB or matching OutputIntent",
            location.to_string(),
        );
    }
    if !cmyk_ok && cs_bytes == b"DeviceCMYK" {
        error_at(
            report,
            "6.2.4.3",
            "DeviceCMYK without DefaultCMYK or matching OutputIntent",
            location.to_string(),
        );
    }
    if !gray_ok && cs_bytes == b"DeviceGray" {
        error_at(
            report,
            "6.2.4.3",
            "DeviceGray without DefaultGray or OutputIntent",
            location.to_string(),
        );
    }
}

/// 0=none, 1=DeviceRGB, 2=DeviceCMYK, 3=DeviceGray
fn device_cs_kind(name: &[u8]) -> u8 {
    if name == keys::DEVICE_RGB {
        1
    } else if name == b"DeviceCMYK" {
        2
    } else if name == b"DeviceGray" {
        3
    } else {
        0
    }
}

fn extract_base_device_cs_kind(cs_arr: &Array<'_>) -> u8 {
    let mut items = cs_arr.iter::<Object<'_>>();
    let Some(Object::Name(cs_type)) = items.next() else {
        return 0;
    };
    let t = cs_type.as_ref();
    let skip = if t == b"Indexed" {
        0
    } else if t == keys::SEPARATION || t == keys::DEVICE_N {
        1
    } else {
        return device_cs_kind(t);
    };
    for _ in 0..skip {
        let _ = items.next();
    }
    if let Some(Object::Name(base)) = items.next() {
        return device_cs_kind(base.as_ref());
    }
    0
}

fn report_image_device_cs_kind(
    kind: u8,
    xn: &str,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    loc: &str,
    report: &mut ComplianceReport,
) {
    if kind == 1 && !rgb_ok {
        error_at(
            report,
            "6.2.4.3",
            format!("Image {xn} uses DeviceRGB without DefaultRGB or matching OutputIntent"),
            loc.to_string(),
        );
    }
    if kind == 2 && !cmyk_ok {
        error_at(
            report,
            "6.2.4.3",
            format!("Image {xn} uses DeviceCMYK without DefaultCMYK or matching OutputIntent"),
            loc.to_string(),
        );
    }
    if kind == 3 && !gray_ok {
        error_at(
            report,
            "6.2.4.3",
            format!("Image {xn} uses DeviceGray without DefaultGray or OutputIntent"),
            loc.to_string(),
        );
    }
}

fn check_image_cs_in_resources(
    res_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    location: &str,
    report: &mut ComplianceReport,
) {
    let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) else {
        return;
    };
    check_image_cs_in_xobjects(&xobj_dict, rgb_ok, cmyk_ok, gray_ok, location, report);
}

/// Check image XObject color spaces from a resolved XObject dict.
fn check_image_cs_in_xobjects(
    xobj_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    location: &str,
    report: &mut ComplianceReport,
) {
    for (name, _) in xobj_dict.entries() {
        let Some(stream) = xobj_dict.get::<Stream<'_>>(name.as_ref()) else {
            continue;
        };
        let dict = stream.dict();
        let subtype = dict.get::<Name>(keys::SUBTYPE);
        if subtype.as_ref().is_some_and(|s| s.as_ref() == keys::FORM) {
            if let Some(fr) = dict.get::<Dict<'_>>(keys::RESOURCES) {
                check_image_cs_in_resources(&fr, rgb_ok, cmyk_ok, gray_ok, location, report);
            }
            continue;
        }
        if subtype.is_none_or(|s| s.as_ref() != keys::IMAGE) {
            continue;
        }
        let xn = std::str::from_utf8(name.as_ref()).unwrap_or("?");
        if let Some(cs_name) = dict.get::<Name>(keys::COLORSPACE) {
            report_image_device_cs_kind(
                device_cs_kind(cs_name.as_ref()),
                xn,
                rgb_ok,
                cmyk_ok,
                gray_ok,
                location,
                report,
            );
        } else if let Some(cs_arr) = dict.get::<Array<'_>>(keys::COLORSPACE) {
            let kind = extract_base_device_cs_kind(&cs_arr);
            if kind > 0 {
                report_image_device_cs_kind(kind, xn, rgb_ok, cmyk_ok, gray_ok, location, report);
            }
        }
    }
}

fn report_dict_cs_device(
    dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    location: &str,
    report: &mut ComplianceReport,
) {
    if let Some(cs) = dict.get::<Name>(keys::COLORSPACE) {
        report_device_cs_name(cs.as_ref(), rgb_ok, cmyk_ok, gray_ok, location, report);
    } else if let Some(cs_arr) = dict.get::<Array<'_>>(keys::COLORSPACE) {
        let kind = extract_base_device_cs_kind(&cs_arr);
        if kind == 1 {
            report_device_cs_name(keys::DEVICE_RGB, rgb_ok, cmyk_ok, gray_ok, location, report);
        } else if kind == 2 {
            report_device_cs_name(b"DeviceCMYK", rgb_ok, cmyk_ok, gray_ok, location, report);
        } else if kind == 3 {
            report_device_cs_name(b"DeviceGray", rgb_ok, cmyk_ok, gray_ok, location, report);
        }
    }
}

/// Helper: report device color ops found in a content stream.
///
/// Parameters `rgb_ok`, `cmyk_ok`, `gray_ok` indicate whether each device
/// color is covered (either by a Default* color space or a matching OutputIntent).
fn report_device_color_ops(
    content: &[u8],
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    location: &str,
    report: &mut ComplianceReport,
) {
    let ops = detect_device_color_ops(content);
    if !rgb_ok && ops.has_rgb {
        error_at(
            report,
            "6.2.4.3",
            "DeviceRGB used without DefaultRGB or matching OutputIntent",
            location.to_string(),
        );
    }
    if !cmyk_ok && ops.has_cmyk {
        error_at(
            report,
            "6.2.4.3",
            "DeviceCMYK used without DefaultCMYK or matching OutputIntent",
            location.to_string(),
        );
    }
    if !gray_ok && ops.has_gray {
        error_at(
            report,
            "6.2.4.3",
            "DeviceGray used without DefaultGray or OutputIntent",
            location.to_string(),
        );
    }
}

/// Result of scanning a content stream for device-dependent color operators.
struct DeviceColorOps {
    has_rgb: bool,
    has_cmyk: bool,
    has_gray: bool,
}

/// Scan a PDF content stream for device-dependent color operators.
///
/// Operators: rg/RG (DeviceRGB), k/K (DeviceCMYK), g/G (DeviceGray),
/// cs/CS with DeviceRGB/DeviceCMYK/DeviceGray operand,
/// and inline images (BI ... /CS /DeviceRGB ... ID ... EI).
fn detect_device_color_ops(content: &[u8]) -> DeviceColorOps {
    let mut result = DeviceColorOps {
        has_rgb: false,
        has_cmyk: false,
        has_gray: false,
    };

    // Tokenize the content stream by splitting on whitespace/newlines
    let text = String::from_utf8_lossy(content);
    let tokens: Vec<&str> = text.split_ascii_whitespace().collect();

    let mut in_inline_image = false;

    for (i, &tok) in tokens.iter().enumerate() {
        // Track inline image state (BI ... ID ... EI)
        if tok == "BI" {
            in_inline_image = true;
            continue;
        }
        if tok == "ID" || tok == "EI" {
            in_inline_image = false;
            continue;
        }

        if in_inline_image {
            // Inside BI block: check for /CS or /ColorSpace keys
            if (tok == "/CS" || tok == "/ColorSpace" || tok == "CS" || tok == "ColorSpace")
                && i + 1 < tokens.len()
            {
                let cs_val = tokens[i + 1].strip_prefix('/').unwrap_or(tokens[i + 1]);
                match cs_val {
                    "DeviceRGB" | "RGB" => result.has_rgb = true,
                    "DeviceCMYK" | "CMYK" => result.has_cmyk = true,
                    "DeviceGray" | "G" => result.has_gray = true,
                    _ => {}
                }
            }
            continue;
        }

        match tok {
            // rg: set non-stroking DeviceRGB (3 operands + op)
            "rg" | "RG" => result.has_rgb = true,
            // k: set non-stroking DeviceCMYK (4 operands + op)
            "k" | "K" => result.has_cmyk = true,
            // g: set non-stroking DeviceGray (1 operand + op)
            "g" | "G" => result.has_gray = true,
            // cs/CS: set color space by name
            "cs" | "CS" => {
                if i > 0 {
                    let operand = tokens[i - 1];
                    // Operand may be /DeviceRGB or just DeviceRGB
                    let name = operand.strip_prefix('/').unwrap_or(operand);
                    match name {
                        "DeviceRGB" => result.has_rgb = true,
                        "DeviceCMYK" => result.has_cmyk = true,
                        "DeviceGray" => result.has_gray = true,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    result
}

/// Check if a content stream uses painting operators without setting an explicit color.
/// When this happens, the implicit color space is DeviceGray.
fn content_has_implicit_gray(content: &[u8]) -> bool {
    let text = String::from_utf8_lossy(content);
    let tokens: Vec<&str> = text.split_ascii_whitespace().collect();
    let has_painting = tokens
        .iter()
        .any(|&t| matches!(t, "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "S" | "s"));
    let has_color = tokens.iter().any(|&t| {
        matches!(
            t,
            "g" | "G" | "rg" | "RG" | "k" | "K" | "cs" | "CS" | "sc" | "SC" | "scn" | "SCN"
        )
    });
    has_painting && !has_color
}

/// Check if a content stream references named resources (fonts, XObjects, color spaces, etc.).
fn stream_references_resources(content: &[u8]) -> bool {
    let text = String::from_utf8_lossy(content);
    let tokens: Vec<&str> = text.split_ascii_whitespace().collect();
    // Operators that require named resources from the Resources dict
    let resource_ops = [
        "Tf", // font
        "Do", // XObject
        "cs", "CS", // color space
        "scn", "SCN", // pattern/separation color
        "gs",  // ExtGState
        "sh",  // shading
        "BDC", // marked content with properties
    ];
    for (i, &tok) in tokens.iter().enumerate() {
        if resource_ops.contains(&tok) && i > 0 && tokens[i - 1].starts_with('/') {
            return true;
        }
    }
    false
}

/// Check if a content stream uses any named resource NOT present in `own_names`.
///
/// Returns `true` when the stream references a `/Name op` pattern where the name
/// is absent from the caller's explicitly-defined resource names — meaning the
/// resource would have to be inherited from a parent dictionary.  Used to detect
/// §6.2.2 T2 violations in Form XObjects that have an explicit (but incomplete)
/// Resources dict.
fn stream_has_inherited_resource_refs(
    content: &[u8],
    own_names: &std::collections::HashSet<Vec<u8>>,
) -> bool {
    let text = String::from_utf8_lossy(content);
    let tokens: Vec<&str> = text.split_ascii_whitespace().collect();
    let resource_ops = ["Do", "cs", "CS", "gs", "sh"];
    for (i, &tok) in tokens.iter().enumerate() {
        if resource_ops.contains(&tok) && i > 0 {
            if let Some(name) = tokens[i - 1].strip_prefix('/') {
                if !own_names.contains(name.as_bytes()) {
                    return true;
                }
            }
        }
    }
    false
}

/// Returns true if the raw Info dict (located via the trailer) contains
/// the given key (e.g. b"/Title"), even when its value is not a string.
///
/// Needed for §6.7.3.2: when /Title exists as an indirect reference (a PDF
/// struct violation itself), pdf-syntax parses it as None, but veraPDF still
/// flags the Info/XMP inconsistency. Fixes §6.7.3 FN on 6-1-5-t01-fail-j.
fn raw_info_has_key(data: &[u8], key: &[u8]) -> bool {
    // Find last trailer dict
    let Some(trailer_pos) = data.windows(7).rposition(|w| w == b"trailer") else {
        return false;
    };
    let trailer_end = data.len().min(trailer_pos + 2000);
    let trailer_region = &data[trailer_pos..trailer_end];

    // Extract Info object number from "/Info N M R"
    let Some(info_off) = trailer_region.windows(5).position(|w| w == b"/Info") else {
        return false;
    };
    let after = &trailer_region[info_off + 5..];
    let text = std::str::from_utf8(&after[..after.len().min(30)]).unwrap_or("");
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() < 3 || parts[2] != "R" {
        return false;
    }
    let Ok(obj_num) = parts[0].parse::<u32>() else {
        return false;
    };
    let Ok(gen_num) = parts[1].parse::<u32>() else {
        return false;
    };

    // Find "N M obj" in raw bytes and scan its dict for the key
    let marker = format!("{obj_num} {gen_num} obj");
    let Some(obj_pos) = data
        .windows(marker.len())
        .position(|w| w == marker.as_bytes())
    else {
        return false;
    };
    let region_end = data.len().min(obj_pos + 2000);
    let region = &data[obj_pos..region_end];
    // Stop at "stream" or "endobj" to avoid scanning stream content
    let dict_end = region
        .windows(6)
        .position(|w| w == b"stream" || w == b"endobj")
        .unwrap_or(region.len().min(1000));
    region[..dict_end].windows(key.len()).any(|w| w == key)
}

/// Decode a PDF /Info string to a UTF-8 Rust string for comparison.
///
/// PDF /Info strings are either PDFDocEncoding (raw bytes, ASCII-compatible) or
/// UTF-16BE (starting with BOM 0xFE 0xFF). XMP values are always UTF-8.
/// Returns None for PDFDocEncoding strings containing non-ASCII bytes — value
/// comparison is skipped in that case to avoid false positives from encoding
/// differences. Fixes §6.7.3 false positives. Fixes #454.
fn decode_pdf_info_string(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        // UTF-16BE with BOM
        let utf16: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&utf16).ok()
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        Some(s.to_string())
    } else {
        // PDFDocEncoding: decode non-ASCII bytes as Latin-1 (ISO 8859-1).
        // PDFDocEncoding ≈ Latin-1 for 0x80-0xFF (minor differences in 0x80-0x9F range).
        // We decode anyway so that mismatches with XMP (which uses UTF-8) are detected.
        // Fixes §6.7.3 FN on 6-1-5-t01-fail-d (Keywords with \xe5 byte vs XMP "Test keywords").
        Some(bytes.iter().map(|&b| b as char).collect())
    }
}

/// Check Info dict / XMP metadata consistency (§6.7.3).
///
/// Properties in /Info dict must have matching values in XMP metadata.
/// Uses veraPDF subclause numbers for exact match with the oracle. (#467)
pub fn check_info_xmp_consistency(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(xmp_data) = get_xmp_metadata(pdf) else {
        return;
    };

    let metadata = pdf.metadata();
    let has_info_meta = metadata.title.is_some()
        || metadata.author.is_some()
        || metadata.creator.is_some()
        || metadata.producer.is_some()
        || metadata.subject.is_some()
        || metadata.keywords.is_some();

    let Ok(xmp_text) = std::str::from_utf8(&xmp_data) else {
        // Non-UTF-8 XMP: consistency cannot be verified. veraPDF still reports
        // §6.7.3 for any Info dict metadata that cannot be cross-checked. (#467)
        if has_info_meta {
            error(
                report,
                "6.7.3",
                "Info dict metadata cannot be verified: XMP stream is not valid UTF-8",
            );
        }
        return;
    };

    // If XMP is structurally malformed (§6.7.11: unparseable XML), veraPDF also
    // reports §6.7.3 because Info dict fields cannot be verified against broken XMP.
    // Only trigger this for genuine structural breakage (6.7.11), not for semantic
    // extension-schema violations (6.7.9.x) which don't prevent XMP parsing.
    // Fixes #467 (isartor-6-7-9-t01). Narrowed to 6.7.11-only to avoid FP on
    // PDFs that have 6.7.9 violations but parseable XMP. (#FP-6.7.3)
    let xmp_structurally_invalid = report.issues.iter().any(|i| i.rule == "6.7.11");
    if xmp_structurally_invalid && has_info_meta {
        error(
            report,
            "6.7.3",
            "Info dict metadata cannot be reliably verified against malformed XMP",
        );
        return;
    }

    // Note: if Info dict is genuinely empty (no metadata fields), that's NOT a
    // violation — XMP can have whatever it wants. The per-field checks below
    // handle specific property-level inconsistencies. veraPDF only fires §6.7.3
    // when both Info and XMP have conflicting values for the SAME property.

    // Check Creator (/Info Creator vs xmp:CreatorTool) — §6.7.3.6
    // Also accept legacy xap: alias. (#FP-6.7.3)
    if metadata.creator.is_some() {
        let xmp_creator = extract_xmp_value(xmp_text, "xmp:CreatorTool")
            .or_else(|| extract_xmp_attr(xmp_text, "xmp:CreatorTool"))
            .or_else(|| extract_xmp_value(xmp_text, "xap:CreatorTool"))
            .or_else(|| extract_xmp_attr(xmp_text, "xap:CreatorTool"));
        if xmp_creator.is_none() {
            error(
                report,
                "6.7.3.6",
                "/Info has Creator but XMP is missing xmp:CreatorTool",
            );
        }
    }

    // Check Producer (/Info Producer vs pdf:Producer) — §6.7.3.7
    if metadata.producer.is_some() {
        let xmp_producer = extract_xmp_value(xmp_text, "pdf:Producer")
            .or_else(|| extract_xmp_attr(xmp_text, "pdf:Producer"));
        if xmp_producer.is_none() {
            error(
                report,
                "6.7.3.7",
                "/Info has Producer but XMP is missing pdf:Producer",
            );
        }
    }

    // Check CreationDate (/Info CreationDate vs xmp:CreateDate) — §6.7.3.1
    // Also accept legacy xap: alias (xap: was renamed to xmp: in XMP spec 2008). (#FP-6.7.3)
    let xmp_create_date = extract_xmp_value(xmp_text, "xmp:CreateDate")
        .or_else(|| extract_xmp_attr(xmp_text, "xmp:CreateDate"))
        .or_else(|| extract_xmp_value(xmp_text, "xap:CreateDate"))
        .or_else(|| extract_xmp_attr(xmp_text, "xap:CreateDate"));
    if metadata.creation_date.is_some() && xmp_create_date.is_none() {
        error(
            report,
            "6.7.3.1",
            "/Info has CreationDate but XMP is missing xmp:CreateDate",
        );
    }

    // Check ModDate (/Info ModDate vs xmp:ModifyDate) — §6.7.3.8
    // Also accept legacy xap: alias. (#FP-6.7.3)
    let xmp_mod_date = extract_xmp_value(xmp_text, "xmp:ModifyDate")
        .or_else(|| extract_xmp_attr(xmp_text, "xmp:ModifyDate"))
        .or_else(|| extract_xmp_value(xmp_text, "xap:ModifyDate"))
        .or_else(|| extract_xmp_attr(xmp_text, "xap:ModifyDate"));
    if metadata.modification_date.is_some() && xmp_mod_date.is_none() {
        error(
            report,
            "6.7.3.8",
            "/Info has ModDate but XMP is missing xmp:ModifyDate",
        );
    } else if metadata.modification_date.is_none()
        && xmp_mod_date.is_some()
        && raw_info_has_key(pdf.data().as_ref(), b"/ModDate")
    {
        // /Info has a /ModDate key but its value is not a valid PDF D: date (e.g. ISO 8601 format).
        // veraPDF reports §6.7.3 because consistency cannot be verified.
        // Fixes §6.7.3 FN on cs/tagged-veraPDF test suite 6-1-5-t01-fail-h.pdf.
        error(
            report,
            "6.7.3",
            "/Info has ModDate but value is not in PDF D: format — cannot verify consistency with XMP",
        );
    }

    // Check Title (/Info Title vs dc:title) — §6.7.3.2
    if let Some(title) = &metadata.title {
        if !xmp_text.contains("dc:title") {
            error(
                report,
                "6.7.3.2",
                "/Info has Title but XMP is missing dc:title",
            );
        } else {
            // Extract dc:title value (usually in rdf:Alt/rdf:li)
            let xmp_title = extract_rdf_alt_value(xmp_text, "dc:title");
            if let Some(xmp_val) = &xmp_title {
                if let Some(info_decoded) = decode_pdf_info_string(title) {
                    if info_decoded.trim() != xmp_val.trim() {
                        error(
                            report,
                            "6.7.3.2",
                            format!(
                                "Title mismatch: Info='{}' vs XMP='{}'",
                                info_decoded.chars().take(50).collect::<String>(),
                                xmp_val.chars().take(50).collect::<String>()
                            ),
                        );
                    }
                }
            }
            // veraPDF requires rdf:Alt/rdf:li to have xml:lang="x-default".
            // If dc:title exists but lacks an x-default language entry,
            // veraPDF treats the title as null → §6.7.3.2 mismatch. (#467)
            let dc_title_region = xmp_text
                .find("<dc:title>")
                .or_else(|| xmp_text.find("<dc:title "));
            if let Some(pos) = dc_title_region {
                let region_end = xmp_text[pos..].find("</dc:title>").unwrap_or(0) + pos;
                let region = &xmp_text[pos..region_end];
                let has_xdefault = region.contains("xml:lang=\"x-default\"")
                    || region.contains("xml:lang='x-default'");
                if !has_xdefault && xmp_title.is_some() {
                    // Title value exists but lacks x-default lang tag
                    error(
                        report,
                        "6.7.3.2",
                        "dc:title in XMP lacks xml:lang=\"x-default\" — value not accessible as x-default",
                    );
                }
            }
        }
    } else if xmp_text.contains("dc:title") {
        // /Title key exists in Info dict but its value is not a string (e.g. an
        // indirect reference to a stream). pdf-syntax can't parse it → metadata.title
        // is None, but veraPDF still reports §6.7.3.2 because the key is present.
        // Use raw byte scan to confirm /Title key actually exists in the Info dict.
        if raw_info_has_key(pdf.data().as_ref(), b"/Title") {
            error(
                report,
                "6.7.3.2",
                "/Info /Title key exists but is not a string — cannot match XMP dc:title",
            );
        }
    }

    // Check Author (/Info Author vs dc:creator) — §6.7.3.3
    if let Some(author) = &metadata.author {
        if xmp_text.contains("dc:creator") {
            let (xmp_vals, _) = extract_rdf_seq_values(xmp_text, "dc:creator");
            // §6.7.3.3: dc:creator SHALL contain exactly one entry (ISO 19005-1, clause 6.7.3.3).
            // Multiple entries is a violation even if the first matches /Info /Author. (#454 was
            // wrong to allow multiples — reverting that allowance.)
            if xmp_vals.len() > 1 {
                error(
                    report,
                    "6.7.3.3",
                    format!(
                        "dc:creator must contain exactly one entry; found {}",
                        xmp_vals.len()
                    ),
                );
            } else if let Some(xmp_val) = xmp_vals.first() {
                if let Some(info_decoded) = decode_pdf_info_string(author) {
                    // Trim both sides: Info dict strings may have leading/trailing
                    // whitespace that gets stripped when written to XMP. (#FP-6.7.3.3)
                    if info_decoded.trim() != xmp_val.trim() {
                        error(
                            report,
                            "6.7.3.3",
                            format!(
                                "Author mismatch: Info='{}' vs XMP='{}'",
                                info_decoded.chars().take(50).collect::<String>(),
                                xmp_val.chars().take(50).collect::<String>()
                            ),
                        );
                    }
                }
            }
        }
    }

    // Check Subject (/Info Subject vs dc:description) — §6.7.3.4
    if let Some(subject) = &metadata.subject {
        if !xmp_text.contains("dc:description") {
            error(
                report,
                "6.7.3.4",
                "/Info has Subject but XMP is missing dc:description",
            );
        } else {
            let xmp_desc = extract_rdf_alt_value(xmp_text, "dc:description");
            if let Some(xmp_val) = &xmp_desc {
                if let Some(info_decoded) = decode_pdf_info_string(subject) {
                    if info_decoded.trim() != xmp_val.trim() {
                        error(
                            report,
                            "6.7.3.4",
                            format!(
                                "Subject mismatch: Info='{}' vs XMP='{}'",
                                info_decoded.chars().take(50).collect::<String>(),
                                xmp_val.chars().take(50).collect::<String>()
                            ),
                        );
                    }
                }
            }
        }
    }

    // Check Keywords (/Info Keywords vs pdf:Keywords) — §6.7.3.5
    if let Some(keywords) = &metadata.keywords {
        // Detect wrong-case variant: pdf:keywords (lowercase) is not a valid XMP property.
        // veraPDF flags this as §6.7.9.3 (predefined property type violation). (#467)
        let has_lowercase_keywords =
            xmp_text.contains("<pdf:keywords>") || xmp_text.contains("pdf:keywords=");
        if has_lowercase_keywords {
            error(
                report,
                "6.7.9.3",
                "XMP contains 'pdf:keywords' (lowercase) — correct property name is 'pdf:Keywords'",
            );
        }
        let xmp_keywords = extract_xmp_value(xmp_text, "pdf:Keywords")
            .or_else(|| extract_xmp_attr(xmp_text, "pdf:Keywords"));
        // If /Info Keywords is empty, veraPDF considers it trivially consistent with
        // an absent XMP pdf:Keywords property — no violation. (#FP-6.7.3.5)
        let info_decoded = decode_pdf_info_string(keywords);
        let info_is_empty = info_decoded.as_deref().unwrap_or("").trim().is_empty();
        if let Some(xmp_val) = &xmp_keywords {
            if let Some(ref info_str) = info_decoded {
                if !info_is_empty && info_str.as_str() != xmp_val.as_str() {
                    error(
                        report,
                        "6.7.3.5",
                        format!(
                            "Keywords mismatch: Info='{}' vs XMP='{}'",
                            info_str.chars().take(50).collect::<String>(),
                            xmp_val.chars().take(50).collect::<String>()
                        ),
                    );
                }
            }
        } else if !info_is_empty {
            // Non-empty /Info Keywords but pdf:Keywords absent from XMP.
            error(
                report,
                "6.7.3.5",
                "/Info has Keywords but XMP is missing pdf:Keywords (correct-case property)",
            );
        }
    }

    // Check date VALUE equivalence (not just presence)
    check_date_equivalence(
        &metadata.creation_date,
        "CreationDate",
        "xmp:CreateDate",
        xmp_text,
        report,
    );
    check_date_equivalence(
        &metadata.modification_date,
        "ModDate",
        "xmp:ModifyDate",
        xmp_text,
        report,
    );

    // PDF/A-4 requires pdfaid:rev to be a 4-digit year (ISO 19005-4, §6.7.3).
    // This check runs regardless of Info dict presence. Fixes #468 (fail-e FN).
    let is_pdfa4 =
        xmp_text.contains("pdfaid:part=\"4\"") || xmp_text.contains("<pdfaid:part>4</pdfaid:part>");
    if is_pdfa4 {
        let rev_val = extract_xmp_value(xmp_text, "pdfaid:rev")
            .or_else(|| extract_xmp_attr(xmp_text, "pdfaid:rev"));
        let valid = rev_val
            .as_deref()
            .and_then(|s| s.parse::<u32>().ok())
            .is_some_and(|n| (1000..=9999).contains(&n));
        if !valid {
            error(
                report,
                "6.7.3",
                "PDF/A-4 requires pdfaid:rev to be a 4-digit year, but it is missing or invalid",
            );
        }
    }
}

/// Check predefined XMP properties with Lang Alt type are properly structured (§6.7.9.3).
///
/// Properties dc:description, dc:rights, and xmpRights:UsageTerms must be Lang Alt
/// type (rdf:Alt with xml:lang-tagged rdf:li entries), not plain strings or attribute
/// values. veraPDF flags this as §6.7.9.3 (isValueTypeCorrect == true).
///
/// XMP Specification Part 1, §8.2.2: dc:description and dc:rights are "Lang Alt" type.
/// XMP Rights Management Schema: xmpRights:UsageTerms is "Lang Alt" type.
pub fn check_xmp_lang_alt_properties(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(xmp_data) = get_xmp_metadata(pdf) else {
        return;
    };
    let Ok(xmp_text) = std::str::from_utf8(&xmp_data) else {
        return;
    };

    // Properties that must be Lang Alt (rdf:Alt), not plain strings.
    let lang_alt_props = ["dc:description", "dc:rights", "xmpRights:UsageTerms"];

    for prop in lang_alt_props {
        let open_tag = format!("<{prop}>");
        let open_tag_space = format!("<{prop} ");
        let close_tag = format!("</{prop}>");

        let element_start = xmp_text
            .find(&open_tag)
            .or_else(|| xmp_text.find(&open_tag_space));

        if let Some(start) = element_start {
            let region_end = xmp_text[start..]
                .find(&close_tag)
                .map(|i| start + i)
                .unwrap_or(xmp_text.len());
            let region = &xmp_text[start..region_end];
            if !region.contains("<rdf:Alt") {
                // No rdf:Alt container — plain string, not a valid Lang Alt.
                error(
                    report,
                    "6.7.9.3",
                    format!(
                        "XMP property '{prop}' must be Lang Alt (rdf:Alt) type, not a plain string"
                    ),
                );
                return;
            }
            // Has rdf:Alt — check every rdf:li carries xml:lang (required for Lang Alt).
            let mut search = 0;
            while let Some(li_pos) = region[search..].find("<rdf:li") {
                let abs = search + li_pos;
                let tag_end = region[abs..].find('>').map(|e| abs + e).unwrap_or(abs);
                if !region[abs..=tag_end].contains("xml:lang") {
                    error(
                        report,
                        "6.7.9.3",
                        format!(
                            "XMP property '{prop}' has rdf:Alt but rdf:li is missing xml:lang attribute"
                        ),
                    );
                    return;
                }
                search = tag_end + 1;
            }
        } else if xmp_text.contains(&format!("{prop}=\""))
            || xmp_text.contains(&format!("{prop}='"))
        {
            // Attribute form is always a plain scalar, never rdf:Alt — violation.
            error(
                report,
                "6.7.9.3",
                format!(
                    "XMP property '{prop}' must be Lang Alt (rdf:Alt) type, not an attribute value"
                ),
            );
            return;
        }
    }
}

/// Check annotation dictionaries have required /F key and correct flags.
///
/// All annotations (except Popup) must have /F key. When present, Print flag
/// must be set, Hidden/Invisible/ToggleNoView/NoView flags must be clear.
///
/// Clause numbering by part:
/// - PDF/A-1: §6.5.3 (ISO 19005-1)
/// - PDF/A-2/3: §6.3.2 (ISO 19005-2/3)
/// - PDF/A-4: §6.3.2 (ISO 19005-4) → normalize_pdfa4_clause("6.3.2")="6.5.2"
///
/// Fixes #467.
pub fn check_annotation_flags(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    // PDF/A-1: §6.5.3; PDF/A-2/3/4: §6.3.2 (veraPDF uses this directly for parts 2/3;
    // normalize_pdfa4_clause("6.3.2")="6.5.2" handles PDF/A-4). Fixes #467.
    let rule = if part == 1 { "6.5.3" } else { "6.3.2" };

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            let is_popup = annot
                .get::<Name>(keys::SUBTYPE)
                .is_some_and(|s| s.as_ref() == b"Popup");

            if let Some(flags) = annot.get::<i32>(keys::F) {
                // Bit 1 (0x01) = Invisible, Bit 2 (0x02) = Hidden,
                // Bit 3 (0x04) = Print, Bit 6 (0x20) = NoView,
                // Bit 9 (0x100) = ToggleNoView
                let invisible = flags & 0x01 != 0;
                let hidden = flags & 0x02 != 0;
                let print = flags & 0x04 != 0;
                let no_view = flags & 0x20 != 0;
                let toggle_no_view = flags & 0x100 != 0;

                // Popup annotations are exempt from the Print flag requirement
                // (§6.3.2 / §6.5.3), but forbidden flags must still be clear. (#FN-6.3.2)
                let print_ok = is_popup || print;
                if !print_ok || invisible || hidden || no_view || toggle_no_view {
                    error_at(
                        report,
                        rule,
                        format!(
                            "Annotation /F flags {flags:#x}: Print must be set, Hidden/Invisible/NoView/ToggleNoView must be clear"
                        ),
                        format!("page {}", page_idx + 1),
                    );
                }
            } else if !is_popup {
                let subtype_name = annot
                    .get::<Name>(keys::SUBTYPE)
                    .map(|n| std::str::from_utf8(n.as_ref()).unwrap_or("?").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                error_at(
                    report,
                    rule,
                    format!("{subtype_name} annotation missing required /F key"),
                    format!("page {}", page_idx + 1),
                );
            }
        }
    }
}

/// Check annotation /C and /IC color arrays (§6.5.3).
///
/// Annotation /C (color) and /IC (interior color) arrays are RGB-based.
/// They must not be present unless the OutputIntent destination profile
/// is RGB-based (3 components).
pub fn check_annotation_color_arrays(pdf: &Pdf, report: &mut ComplianceReport) {
    let profile_components = output_intent_profile_components(pdf);
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            let subtype_name = || {
                annot
                    .get::<Name>(keys::SUBTYPE)
                    .map(|n| std::str::from_utf8(n.as_ref()).unwrap_or("?").to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            };
            for key in [b"C" as &[u8], b"IC" as &[u8]] {
                let Some(arr) = annot.get::<Array<'_>>(key) else {
                    continue;
                };
                let n = arr.raw_iter().count();
                if n == 0 {
                    // Empty array = transparent; always allowed. Fixes #455.
                    continue;
                }
                // Color components must be compatible with the OutputIntent.
                // 1-component (gray) is compatible with any intent.
                // 3-component (RGB) requires an RGB OutputIntent (3 components).
                // 4-component (CMYK) requires a CMYK OutputIntent (4 components).
                let compatible = match profile_components {
                    Some(1) => n == 1,
                    Some(3) => n == 1 || n == 3,
                    Some(4) => n == 1 || n == 4,
                    _ => false,
                };
                if !compatible {
                    let key_name = if key == b"C" { "/C" } else { "/IC" };
                    error_at(
                        report,
                        "6.5.3",
                        format!(
                            "{} annotation {} has {n}-component color array incompatible with OutputIntent ({} components)",
                            subtype_name(),
                            key_name,
                            profile_components.map_or("none".to_string(), |c| c.to_string()),
                        ),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }
}

/// Check Form XObjects don't contain forbidden keys (§6.2.9).
///
/// Form XObjects must not contain OPI key, PS key, or Subtype2=PS.
/// Reference XObjects (Ref key) are also forbidden.
pub fn check_form_xobjects(pdf: &Pdf, _part: u8, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let res_dict = page_dict.get::<Dict<'_>>(keys::RESOURCES);

        let xobj_dict = if let Some(ref rd) = res_dict {
            rd.get::<Dict<'_>>(keys::XOBJECT)
        } else {
            None
        };

        let Some(xobj_dict) = xobj_dict else {
            continue;
        };

        for (name, _) in xobj_dict.entries() {
            // XObjects are almost always indirect references — add xref fallback so we
            // can inspect form XObjects defined outside the resources dict. (#FN-6.2.4)
            let stream_opt = xobj_dict.get::<Stream<'_>>(name.as_ref()).or_else(|| {
                xobj_dict
                    .get_ref(name.as_ref())
                    .and_then(|r| xref.get::<Stream<'_>>(r.into()))
            });
            let Some(stream) = stream_opt else {
                continue;
            };
            let dict = stream.dict();

            // Check it's a Form XObject
            if let Some(subtype) = dict.get::<Name>(keys::SUBTYPE) {
                if subtype.as_ref() != b"Form" {
                    continue;
                }
            } else {
                continue;
            }

            let xobj_name = std::str::from_utf8(name.as_ref()).unwrap_or("?");
            let loc = format!("page {}", page_idx + 1);
            // "6.2.9-form-opi" remaps per part: PDF/A-1 → §6.2.4, PDF/A-4 → §6.2.8.1,
            // PDF/A-2/3 → §6.2.9. Use this internal rule for ALL parts so the remap
            // table can handle each case. (#FN-6.2.4 isartor-6-2-5-t01-fail-a)
            let opi_rule = "6.2.9-form-opi";
            // PS/Subtype2=PS violations: veraPDF fires "6.2.5" for PDF/A-1 (not "6.2.6").
            // "6.2.9-form-ps" remaps to "6.2.5" for part=1, "6.2.8.1" for part=4. (#FN-6.2.5)
            let ps_rule = "6.2.9-form-ps";
            // /Ref key: veraPDF fires "6.2.8.2" for PDF/A-4 (not "6.2.8.1"). (#FN-6.2.8.2)
            let ref_rule = "6.2.9-form-ref";

            if dict.contains_key(keys::OPI) {
                error_at(
                    report,
                    opi_rule,
                    format!("Form XObject {xobj_name} contains forbidden /OPI key"),
                    loc.clone(),
                );
            }
            if dict.contains_key(keys::PS) {
                error_at(
                    report,
                    ps_rule,
                    format!("Form XObject {xobj_name} contains forbidden /PS key"),
                    loc.clone(),
                );
            }
            if let Some(sub2) = dict.get::<Name>(b"Subtype2" as &[u8]) {
                if sub2.as_ref() == keys::PS {
                    error_at(
                        report,
                        ps_rule,
                        format!("Form XObject {xobj_name} has Subtype2=PS"),
                        loc.clone(),
                    );
                }
            }
            if dict.contains_key(b"Ref" as &[u8]) {
                error_at(
                    report,
                    ref_rule,
                    format!("Form XObject {xobj_name} is a reference XObject (contains /Ref)"),
                    loc,
                );
            }
        }
    }
}

/// Check page boundary sizes are within spec limits (§6.1.13).
///
/// Page boundaries must be ≥ 3 units and ≤ 14400 units in each direction.
pub fn check_page_boundary_sizes(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let rect = page.media_box();
        let width = (rect.x1 - rect.x0).abs();
        let height = (rect.y1 - rect.y0).abs();

        if width < 3.0 || height < 3.0 {
            error_at(
                report,
                "6.1.13",
                format!(
                    "Page boundary {:.1}x{:.1} is less than minimum 3 units",
                    width, height
                ),
                format!("page {}", page_idx + 1),
            );
        }
        if width > 14400.0 || height > 14400.0 {
            error_at(
                report,
                "6.1.13",
                format!(
                    "Page boundary {:.0}x{:.0} exceeds maximum 14400 units",
                    width, height
                ),
                format!("page {}", page_idx + 1),
            );
        }
    }
}

// ─── §6.2.3.3 — ICC profile version must match PDF/A part ───────────────────

/// Check ICC profile version in OutputIntent (§6.2.3.3).
///
/// PDF/A-1 requires ICC v2 (major=2), PDF/A-2/3 allows up to v4 (major=4).
pub fn check_icc_profile_version(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(intents) = cat.get::<Array<'_>>(keys::OUTPUT_INTENTS) else {
        return;
    };
    for intent in intents.iter::<Dict<'_>>() {
        let Some(profile_stream) = intent.get::<Stream<'_>>(keys::DEST_OUTPUT_PROFILE) else {
            continue;
        };
        let Ok(profile_data) = profile_stream.decoded() else {
            continue;
        };
        if profile_data.len() < 12 {
            // Internal rule id "6.2.3.3-iccver" is remapped to the correct
            // per-part clause by remap_clause_numbers: "6.6.2.3.3" for PDF/A-1,
            // "6.2.3.3" for PDF/A-2/3/4. Distinct from the device-color rule
            // which also uses "6.2.3.3". (#467)
            error(
                report,
                "6.2.3.3-iccver",
                "ICC profile too short to parse header",
            );
            continue;
        }
        let major = profile_data[8];
        let max_version = if part == 1 { 2 } else { 4 };
        if major > max_version {
            error(
                report,
                "6.2.3.3-iccver",
                format!(
                    "ICC profile version {major}.x exceeds maximum v{max_version} for PDF/A-{part}"
                ),
            );
        }
    }
}

// ─── §6.2.4.2 — ICCBased Alternate CS consistency ──────────────────────────

/// Check ICCBased color spaces have consistent Alternate CS (§6.2.4.2).
///
/// Also checks that the required /N key is present in each ICCBased stream
/// dict (§6.2.3.2 for all PDF/A parts). Fixes #467.
pub fn check_iccbased_alternate(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        // Resources may be a direct dict or an indirect ref. (#FN-6.2.3.2)
        let res_dict_opt: Option<Dict<'_>> =
            page_dict.get::<Dict<'_>>(keys::RESOURCES).or_else(|| {
                page_dict
                    .get_ref(keys::RESOURCES)
                    .and_then(|r| xref.get::<Dict<'_>>(r.into()))
            });
        let Some(res_dict) = res_dict_opt else {
            continue;
        };
        let Some(cs_dict) = res_dict.get::<Dict<'_>>(keys::COLORSPACE) else {
            continue;
        };
        for (name, _) in cs_dict.entries() {
            // The colorspace value may be a direct array or an indirect array ref.
            let cs_arr_opt: Option<Array<'_>> =
                cs_dict.get::<Array<'_>>(name.as_ref()).or_else(|| {
                    cs_dict
                        .get_ref(name.as_ref())
                        .and_then(|r| xref.get::<Array<'_>>(r.into()))
                });
            let Some(cs_arr) = cs_arr_opt else {
                continue;
            };
            let mut items = cs_arr.iter::<Object<'_>>();
            let Some(Object::Name(cs_type)) = items.next() else {
                continue;
            };
            if cs_type.as_ref() != keys::ICC_BASED {
                continue;
            }
            // Second element is the ICCBased stream. Array::iter::<Object> resolves
            // indirect refs automatically, so Object::Stream matches regardless of
            // whether the stream is direct or indirect. (#FN-6.2.3.2)
            // Extract owned values immediately to avoid lifetime conflicts.
            #[allow(clippy::type_complexity)]
            let icc_props: Option<(
                bool,
                Option<i32>,
                Option<Vec<u8>>,
                Option<Vec<u8>>,
            )> = items.next().and_then(|o| match o {
                Object::Stream(s) => {
                    let d = s.dict();
                    Some((
                        d.contains_key(keys::N),
                        d.get::<i32>(keys::N),
                        d.get::<Name>(keys::ALTERNATE).map(|n| n.as_ref().to_vec()),
                        s.decoded().ok(),
                    ))
                }
                _ => None,
            });
            let Some((icc_has_n, n_components, alt_name_bytes, icc_data)) = icc_props else {
                continue;
            };
            let cs_name = std::str::from_utf8(name.as_ref()).unwrap_or("?");

            // §6.2.3.2: /N is required in ICCBased streams.
            // veraPDF emits "6.2.3.2" for ALL PDF/A parts — no remap needed. (#467)
            if !icc_has_n {
                error_at(
                    report,
                    "6.2.3.2",
                    format!("ICCBased CS '{cs_name}' missing required /N key"),
                    format!("page {}", page_idx + 1),
                );
            }

            // §6.2.3.2: N must match the actual ICC profile color space signature.
            // ICC header bytes 16-19 = color space signature (GRAY/RGB /Lab /CMYK).
            // veraPDF fires "6.2.3.2" when N does not match. (#FN-6.2.3.2)
            if let (Some(n), Some(data)) = (n_components, icc_data.as_deref()) {
                if data.len() >= 20 {
                    let cs_sig = &data[16..20];
                    let expected_n: Option<i32> = match cs_sig {
                        b"GRAY" => Some(1),
                        b"RGB " | b"Lab " => Some(3),
                        b"CMYK" => Some(4),
                        _ => None,
                    };
                    if let Some(expected) = expected_n {
                        if n != expected {
                            let sig = std::str::from_utf8(cs_sig).unwrap_or("?");
                            error_at(
                                report,
                                "6.2.3.2",
                                format!(
                                    "ICCBased CS '{cs_name}' has /N {n} but ICC profile \
                                     color space is '{sig}' (expects N={expected})"
                                ),
                                format!("page {}", page_idx + 1),
                            );
                        }
                    }
                }
            }

            if let Some(alt) = alt_name_bytes.as_deref() {
                if let Some(n) = n_components {
                    let expected = if alt == keys::DEVICE_RGB {
                        3
                    } else if alt == keys::DEVICE_CMYK {
                        4
                    } else if alt == keys::DEVICE_GRAY {
                        1
                    } else {
                        continue;
                    };
                    if n != expected {
                        let alt_str = std::str::from_utf8(alt).unwrap_or("?");
                        error_at(
                            report,
                            "6.2.4.2",
                            format!(
                                "ICCBased CS '{cs_name}' has N={n} but Alternate={alt_str} expects {expected} components"
                            ),
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
            }
        }
    }
}

// ─── §6.2.4.2 — ICCBased CMYK identical to OutputIntent/transparency CS ─────

/// Extract ICCBased profile ref from a colorspace array.
///
/// Handles direct `[/ICCBased <stream>]` and nested alternates in
/// `[/DeviceN [...] [/ICCBased <stream>] ...]` or `[/Separation name [/ICCBased <stream>] ...]`.
fn extract_iccbased_cmyk_ref(cs_arr: &Array<'_>) -> Option<ObjRef> {
    // Try direct ICCBased first
    if let Some(r) = icc_based_profile_ref(cs_arr) {
        return Some(r);
    }
    // Check if DeviceN/Separation alternate is ICCBased
    let mut items = cs_arr.iter::<Object<'_>>();
    let Object::Name(cs_type) = items.next()? else {
        return None;
    };
    if cs_type.as_ref() == keys::DEVICE_N || cs_type.as_ref() == keys::SEPARATION {
        items.next()?; // skip names array or colorant name
        if let Some(Object::Array(alt_arr)) = items.next() {
            return icc_based_profile_ref(&alt_arr);
        }
    }
    None
}

/// Extract the ICC stream object reference from an ICCBased colorspace array.
///
/// The array has the form `[/ICCBased <stream-ref-or-inline-stream>]`.
/// Returns `Some(ObjRef)` when the ICC profile is stored as an indirect object.
fn icc_based_profile_ref(cs_arr: &Array<'_>) -> Option<ObjRef> {
    // Use raw_iter so indirect references are not resolved — we need the ObjRef
    let mut raw = cs_arr.raw_iter();
    // First element must be /ICCBased name
    raw.next()?; // skip /ICCBased name
    raw.next()?.as_obj_ref()
}

/// Compare two ICC profile byte slices for identity, ignoring the Profile ID
/// field (16 bytes at offset 84–99), which may legitimately differ between
/// two copies of the same profile generated by different software.
fn icc_profiles_identical(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    if a.len() <= 84 {
        return a == b;
    }
    let skip_end = a.len().min(100);
    a[..84] == b[..84] && a[skip_end..] == b[skip_end..]
}

/// Check that no ICCBased CMYK colorspace uses the same ICC profile as the
/// OutputIntent's DestOutputProfile or any transparency group's CS
/// (§6.2.4.2 — identical profile check).
///
/// veraPDF checks identity in two ways:
///   (a) same object reference — always checked here.
///   (b) same decoded bytes (content identity) — checked only for CMYK profiles
///       (N≥4). sRGB/RGB profiles (N=3) are exempt because PDF/A converters
///       deliberately create two separate sRGB ICC objects for OutputIntent and
///       DefaultRGB, and veraPDF does not flag that as a violation. (#FP-6.2.4.2)
pub fn check_iccbased_cmyk_not_identical_to_outputintent(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();

    // ── 1. Collect forbidden ICC profile refs and, for CMYK profiles, their
    //       decoded bytes for content-identity comparison. ────────────────────
    let mut forbidden_refs: std::collections::HashSet<ObjRef> = std::collections::HashSet::new();
    // Decoded bytes of CMYK (N≥4) OutputIntent profiles for content comparison.
    let mut forbidden_cmyk_profiles: Vec<Vec<u8>> = Vec::new();

    let mut collect_output_intent_profile = |r: ObjRef| {
        forbidden_refs.insert(r);
        // For CMYK profiles (N≥4), also store decoded bytes so we can catch
        // the case where two different ICC stream objects carry the same profile.
        if let Some(stream) = xref.get::<Stream<'_>>(r.into()) {
            let n: i64 = stream.dict().get::<i64>(b"N" as &[u8]).unwrap_or(0);
            if n >= 4 {
                if let Ok(decoded) = stream.decoded() {
                    forbidden_cmyk_profiles.push(decoded);
                }
            }
        }
    };

    // (a) OutputIntent DestOutputProfile — Catalog level and Page level.
    // PDF/A-4 allows OutputIntents at the page level (§6.2.4). Collect from both.
    let collect_from_dict = |dict: &Dict<'_>, refs: &mut Vec<ObjRef>| {
        if let Some(intents) = dict.get::<Array<'_>>(keys::OUTPUT_INTENTS) {
            for intent in intents.iter::<Dict<'_>>() {
                if let Some(r) = intent.get_ref(keys::DEST_OUTPUT_PROFILE) {
                    refs.push(r);
                }
            }
        }
    };
    let mut oi_refs: Vec<ObjRef> = Vec::new();
    if let Some(cat) = catalog(pdf) {
        collect_from_dict(&cat, &mut oi_refs);
    }
    for page in pdf.pages().iter() {
        collect_from_dict(page.raw(), &mut oi_refs);
    }
    for r in oi_refs {
        collect_output_intent_profile(r);
    }

    // (b) Transparency Group CS on pages and Form XObjects
    for obj in pdf.objects() {
        let dict = match &obj {
            Object::Dict(d) => Some(d.clone()),
            Object::Stream(s) => Some(s.dict().clone()),
            _ => None,
        };
        let Some(d) = dict else { continue };
        let group = d.get::<Dict<'_>>(keys::GROUP).or_else(|| {
            d.get_ref(keys::GROUP)
                .and_then(|r| xref.get::<Dict<'_>>(r.into()))
        });
        let Some(group_dict) = group else { continue };
        if let Some(cs_arr) = group_dict.get::<Array<'_>>(keys::CS) {
            if let Some(r) = icc_based_profile_ref(&cs_arr) {
                collect_output_intent_profile(r);
            }
        }
    }

    if forbidden_refs.is_empty() && forbidden_cmyk_profiles.is_empty() {
        return; // nothing to compare against
    }

    // ── 2. Scan all objects for ICCBased colorspaces ─────────────────────────
    'outer: for (obj_idx, obj) in pdf.objects().into_iter().enumerate() {
        let cs_dict = match &obj {
            Object::Dict(d) => {
                if d.get::<Name>(keys::TYPE)
                    .is_some_and(|t| t.as_ref() == b"Page")
                {
                    d.get::<Dict<'_>>(keys::RESOURCES)
                        .and_then(|r| r.get::<Dict<'_>>(keys::COLORSPACE))
                } else {
                    d.get::<Dict<'_>>(keys::COLORSPACE)
                }
            }
            Object::Stream(s) => {
                let d = s.dict();
                let is_xobj = d
                    .get::<Name>(keys::TYPE)
                    .is_some_and(|t| t.as_ref() == b"XObject");
                if is_xobj {
                    d.get::<Dict<'_>>(keys::RESOURCES)
                        .and_then(|r| r.get::<Dict<'_>>(keys::COLORSPACE))
                } else {
                    None
                }
            }
            _ => None,
        };
        let Some(cs_dict) = cs_dict else { continue };

        for (cs_name, _) in cs_dict.entries() {
            let Some(cs_arr) = cs_dict.get::<Array<'_>>(cs_name.as_ref()) else {
                continue;
            };
            // Extract ICCBased profile ref — either top-level or nested
            // in DeviceN/Separation alternate colorspace.
            let Some(prof_ref) = extract_iccbased_cmyk_ref(&cs_arr) else {
                continue;
            };

            let name_str = std::str::from_utf8(cs_name.as_ref()).unwrap_or("?");

            // (a) Object-reference identity — catches same-object reuse directly.
            if forbidden_refs.contains(&prof_ref) {
                error(
                    report,
                    "6.2.4.2",
                    format!(
                        "ICCBased colorspace '{name_str}' (obj {obj_idx}) reuses the same \
                         ICC profile object as the OutputIntent DestOutputProfile or \
                         transparency group color space"
                    ),
                );
                break 'outer; // one error per document is sufficient
            }

            // (b) Content identity for CMYK profiles (N≥4): two different stream
            // objects may carry an identical profile. sRGB (N=3) is excluded
            // because PDF/A converters use separate sRGB objects for OutputIntent
            // and DefaultRGB, which veraPDF accepts. (#FN-6.2.4.2)
            if !forbidden_cmyk_profiles.is_empty() {
                if let Some(stream) = xref.get::<Stream<'_>>(prof_ref.into()) {
                    let n: i64 = stream.dict().get::<i64>(b"N" as &[u8]).unwrap_or(0);
                    if n >= 4 {
                        if let Ok(decoded) = stream.decoded() {
                            for fp in &forbidden_cmyk_profiles {
                                if icc_profiles_identical(&decoded, fp) {
                                    error(
                                        report,
                                        "6.2.4.2",
                                        format!(
                                            "ICCBased colorspace '{name_str}' (obj {obj_idx}) \
                                             uses a CMYK ICC profile with identical content to \
                                             the OutputIntent DestOutputProfile (different \
                                             stream objects but same profile bytes)"
                                        ),
                                    );
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── §6.2.4.4 — DeviceN/Separation consistency across document ──────────────

/// Check that all Separation/DeviceN arrays with the same colorant name have
/// consistent `alternateSpace` and `tintTransform` (§6.2.4.4).
///
/// Collects Separation/NChannel/DeviceN colorspaces from every object in the
/// document, groups them by colorant name, and reports an error if any two
/// entries differ in alternateSpace.
pub fn check_separation_consistency(pdf: &Pdf, report: &mut ComplianceReport) {
    // Map: colorant name → (alternateSpace raw bytes, first object index)
    let mut seen: std::collections::HashMap<Vec<u8>, (Vec<u8>, usize)> =
        std::collections::HashMap::new();

    for (obj_idx, obj) in pdf.objects().into_iter().enumerate() {
        // Collect all Separation/DeviceN colorspace arrays from Resources/ColorSpace dicts.
        // Page dicts are Object::Dict with /Resources/ColorSpace nested — check both paths.
        // Fixes §6.2.4.4 FNs where Separation arrays in page Resources were not found. (#496)
        let cs_dict: Option<Dict<'_>> = match &obj {
            Object::Dict(d) => d.get::<Dict<'_>>(keys::COLORSPACE).or_else(|| {
                d.get::<Dict<'_>>(keys::RESOURCES)
                    .and_then(|r| r.get::<Dict<'_>>(keys::COLORSPACE))
            }),
            Object::Stream(s) => {
                let d = s.dict();
                d.get::<Dict<'_>>(keys::RESOURCES)
                    .and_then(|r| r.get::<Dict<'_>>(keys::COLORSPACE))
            }
            _ => None,
        };

        // Also look for Colorants dicts that may be inside attributes objects
        let colorants_dict: Option<Dict<'_>> = match &obj {
            Object::Dict(d) => d.get::<Dict<'_>>(b"Colorants" as &[u8]),
            _ => None,
        };

        for maybe_cs_dict in [cs_dict, colorants_dict].into_iter().flatten() {
            for (cs_name, _) in maybe_cs_dict.entries() {
                let Some(cs_arr) = maybe_cs_dict.get::<Array<'_>>(cs_name.as_ref()) else {
                    continue;
                };
                check_separation_array(&cs_arr, obj_idx, &mut seen, report);
            }
        }
    }
}

/// Inspect a colorspace array: if it's `[/Separation /Name altCS tintFn]`,
/// record the colorant name → alternateSpace binding and report inconsistencies.
fn check_separation_array(
    cs_arr: &Array<'_>,
    obj_idx: usize,
    seen: &mut std::collections::HashMap<Vec<u8>, (Vec<u8>, usize)>,
    report: &mut ComplianceReport,
) {
    let mut items = cs_arr.iter::<Object<'_>>();
    let Some(Object::Name(cs_type)) = items.next() else {
        return;
    };
    if cs_type.as_ref() != keys::SEPARATION {
        // DeviceN/NChannel: check each component's Colorants sub-dict too
        // but the top-level altCS check works the same way
        return;
    }
    // [/Separation /ColorantName altCS tintFn]
    let Some(Object::Name(colorant_name)) = items.next() else {
        return;
    };
    let colorant_bytes: Vec<u8> = colorant_name.as_ref().to_vec();

    // §6.2.6: Separation colorant name shall not be /None.
    // veraPDF fires "6.2.6" when a Separation colorspace uses the reserved name /None. (#483)
    if colorant_bytes == b"None" {
        error(
            report,
            "6.2.6",
            "Separation colorspace uses forbidden colorant name /None",
        );
        return;
    }

    // Compare full Separation array (alternateSpace + tintTransform).
    // veraPDF §6.2.4.4: same colorant name must have same alternateSpace AND tintTransform.
    let full_sig = cs_arr.data().to_vec();

    let colorant_str = std::str::from_utf8(&colorant_bytes)
        .unwrap_or("?")
        .to_string();
    match seen.entry(colorant_bytes) {
        std::collections::hash_map::Entry::Occupied(e) => {
            let (existing_sig, first_idx) = e.get();
            if *existing_sig != full_sig {
                error(
                    report,
                    "6.2.4.4",
                    format!(
                        "Separation colorant '{colorant_str}' has inconsistent definition \
                         (first at obj {first_idx}, differs at obj {obj_idx})"
                    ),
                );
            }
        }
        std::collections::hash_map::Entry::Vacant(v) => {
            v.insert((full_sig, obj_idx));
        }
    }
}

// ─── §6.2.4.4 — DeviceN/Separation alternate CS ────────────────────────────

/// Check DeviceN/Separation alternate CS is not device-dependent (§6.2.4.4).
pub fn check_devicen_separation_alternate(pdf: &Pdf, report: &mut ComplianceReport) {
    if has_output_intent(pdf) {
        return;
    }
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) else {
            continue;
        };
        let Some(cs_dict) = res_dict.get::<Dict<'_>>(keys::COLORSPACE) else {
            continue;
        };
        for (name, _) in cs_dict.entries() {
            let Some(cs_arr) = cs_dict.get::<Array<'_>>(name.as_ref()) else {
                continue;
            };
            let mut items = cs_arr.iter::<Object<'_>>();
            let Some(Object::Name(cs_type)) = items.next() else {
                continue;
            };
            let cs_type_bytes = cs_type.as_ref();
            if cs_type_bytes != keys::SEPARATION && cs_type_bytes != keys::DEVICE_N {
                continue;
            }
            // Skip index 1 (colorant name/names), alternate CS at index 2
            let _ = items.next(); // skip colorant name(s)
            if let Some(Object::Name(alt_name)) = items.next() {
                let alt = alt_name.as_ref();
                if alt == keys::DEVICE_RGB || alt == keys::DEVICE_CMYK || alt == keys::DEVICE_GRAY {
                    let cs_name = std::str::from_utf8(name.as_ref()).unwrap_or("?");
                    let type_str = std::str::from_utf8(cs_type_bytes).unwrap_or("?");
                    let alt_str = std::str::from_utf8(alt).unwrap_or("?");
                    error_at(
                        report,
                        "6.2.4.4",
                        format!(
                            "{type_str} CS '{cs_name}' uses device-dependent alternate {alt_str} without OutputIntent"
                        ),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }
}

/// Check DeviceN/NChannel colorants are defined in the Colorants dictionary (§6.2.4.4).
pub fn check_devicen_colorants(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) else {
            continue;
        };
        let Some(cs_dict) = res_dict.get::<Dict<'_>>(keys::COLORSPACE) else {
            continue;
        };
        for (name, _) in cs_dict.entries() {
            let Some(cs_arr) = cs_dict.get::<Array<'_>>(name.as_ref()) else {
                continue;
            };
            let items: Vec<Object<'_>> = cs_arr.iter::<Object<'_>>().collect();
            let Some(Object::Name(cs_type)) = items.first() else {
                continue;
            };
            if cs_type.as_ref() != keys::DEVICE_N {
                continue;
            }
            // DeviceN array: [/DeviceN names alternateCS tintTransform attributes?]
            // Colorant names are at index 1 (an Array of Names)
            let Some(Object::Array(colorant_names)) = items.get(1) else {
                continue;
            };
            // Attributes dict (if present) at index 4
            let attrs = items.get(4).and_then(|o| {
                if let Object::Dict(d) = o {
                    Some(d)
                } else {
                    None
                }
            });
            // Get Colorants dictionary from attributes
            let colorants_dict = attrs.and_then(|a| a.get::<Dict<'_>>(b"Colorants" as &[u8]));
            // Each colorant name (except None and All) must be in Colorants dict
            // Standard process colorants are implicitly defined by the PDF spec;
            // they do not need to appear in the Colorants dictionary. Only SPOT
            // (non-process) colorants require Colorants entries. veraPDF does
            // not flag missing Colorants entries for process colorants. (#FP-6.2.4.4)
            const PROCESS_COLORANTS: &[&[u8]] = &[
                b"Cyan", b"Magenta", b"Yellow", b"Black", b"Red", b"Green", b"Blue", b"White",
                b"None", b"All",
            ];
            for cn in colorant_names.iter::<Name>() {
                let cn_bytes = cn.as_ref();
                if PROCESS_COLORANTS.contains(&cn_bytes) {
                    continue;
                }
                let defined = colorants_dict
                    .as_ref()
                    .map(|d| d.contains_key(cn_bytes))
                    .unwrap_or(false);
                if !defined {
                    let cn_str = std::str::from_utf8(cn_bytes).unwrap_or("?");
                    let cs_name = std::str::from_utf8(name.as_ref()).unwrap_or("?");
                    error_at(
                        report,
                        "6.2.4.4",
                        format!(
                            "DeviceN CS '{cs_name}' spot colorant '{cn_str}' not defined in Colorants dictionary"
                        ),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }
}

// ─── §6.2.5 — Rendering intent validation ───────────────────────────────────

/// Check rendering intents are valid (§6.2.5).
///
/// Scans page content, annotation appearances, and Form XObjects.
pub fn check_rendering_intents(pdf: &Pdf, report: &mut ComplianceReport) {
    let valid_intents: &[&[u8]] = &[
        b"RelativeColorimetric",
        b"AbsoluteColorimetric",
        b"Perceptual",
        b"Saturation",
    ];

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let loc = format!("page {}", page_idx + 1);

        // Scan page content stream for 'ri' operator
        if let Some(content) = page.page_stream() {
            check_ri_in_content(content, &valid_intents, &loc, report);
        }

        let page_dict = page.raw();

        // Check ExtGState /RI in page resources
        if let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) {
            check_ri_in_extgstate(&res_dict, &valid_intents, &loc, report);

            // Check Form XObjects
            if let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) {
                for (xname, _) in xobj_dict.entries() {
                    if let Some(stream) = xobj_dict.get::<Stream<'_>>(xname.as_ref()) {
                        let dict = stream.dict();
                        if dict
                            .get::<Name>(keys::SUBTYPE)
                            .is_some_and(|s| s.as_ref() == b"Form")
                        {
                            if let Ok(decoded) = stream.decoded() {
                                let xloc = format!("{loc}/XObject");
                                check_ri_in_content(&decoded, &valid_intents, &xloc, report);
                            }
                            if let Some(xo_res) = dict.get::<Dict<'_>>(keys::RESOURCES) {
                                check_ri_in_extgstate(&xo_res, &valid_intents, &loc, report);
                            }
                        }
                    }
                }
            }
        }

        // Check annotation appearances
        if let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) {
            for annot in annots.iter::<Dict<'_>>() {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    for key in [b"N" as &[u8], b"R", b"D"] {
                        if let Some(stream) = ap.get::<Stream<'_>>(key) {
                            if let Ok(decoded) = stream.decoded() {
                                let ap_loc = format!("{loc}/Annot/AP");
                                check_ri_in_content(&decoded, &valid_intents, &ap_loc, report);
                            }
                            let ap_dict = stream.dict();
                            if let Some(ap_res) = ap_dict.get::<Dict<'_>>(keys::RESOURCES) {
                                check_ri_in_extgstate(&ap_res, &valid_intents, &loc, report);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Check 'ri' operators and inline image /Intent in a content stream.
fn check_ri_in_content(
    content: &[u8],
    valid_intents: &&[&[u8]],
    location: &str,
    report: &mut ComplianceReport,
) {
    let text = String::from_utf8_lossy(content);
    let tokens: Vec<&str> = text.split_ascii_whitespace().collect();
    for (i, &tok) in tokens.iter().enumerate() {
        if tok == "ri" && i > 0 {
            let operand = tokens[i - 1];
            let name = operand.strip_prefix('/').unwrap_or(operand);
            if !valid_intents.iter().any(|v| v == &name.as_bytes()) {
                error_at(
                    report,
                    "6.2.6",
                    format!("Invalid rendering intent '{name}'"),
                    location,
                );
            }
        }
    }
    // §6.2.6: inline image /Intent must also be a valid rendering intent.
    // Scan for BI ... /Intent /Name ... ID patterns.
    let mut pos = 0;
    while pos + 2 < content.len() {
        if content[pos] == b'B'
            && content[pos + 1] == b'I'
            && (pos == 0 || content[pos - 1].is_ascii_whitespace())
        {
            if let Some(id_off) = content[pos..].windows(2).position(|w| w == b"ID") {
                // id_off must be >= 2 to have any header bytes between "BI" and "ID".
                // Content like "BID" would give id_off=1 and make content[pos+2..pos+1] panic.
                if id_off < 2 {
                    pos += id_off + 2;
                    continue;
                }
                let header = &content[pos + 2..pos + id_off];
                if let Some(ip) = header.windows(7).position(|w| w == b"/Intent") {
                    let after = &header[ip + 7..];
                    let name_start = after.iter().position(|b| *b == b'/');
                    if let Some(ns) = name_start {
                        let name_end = after[ns + 1..]
                            .iter()
                            .position(|b| b.is_ascii_whitespace() || *b == b'/')
                            .unwrap_or(after.len() - ns - 1)
                            + ns
                            + 1;
                        let name = &after[ns + 1..name_end];
                        if !valid_intents.contains(&name) {
                            let ns = std::str::from_utf8(name).unwrap_or("?");
                            error_at(
                                report,
                                "6.2.6",
                                format!("Inline image has invalid /Intent /{ns}"),
                                location,
                            );
                            return;
                        }
                    }
                }
                pos += id_off + 2;
            } else {
                break;
            }
        } else {
            pos += 1;
        }
    }
}

/// Check /RI in ExtGState resources.
fn check_ri_in_extgstate(
    res_dict: &Dict<'_>,
    valid_intents: &&[&[u8]],
    location: &str,
    report: &mut ComplianceReport,
) {
    let Some(gs_dict) = res_dict.get::<Dict<'_>>(keys::EXT_G_STATE) else {
        return;
    };
    for (gs_name, _) in gs_dict.entries() {
        let Some(gs) = gs_dict.get::<Dict<'_>>(gs_name.as_ref()) else {
            continue;
        };
        if let Some(ri) = gs.get::<Name>(keys::RI) {
            if !valid_intents.iter().any(|v| *v == ri.as_ref()) {
                let ri_str = std::str::from_utf8(ri.as_ref()).unwrap_or("?");
                error_at(
                    report,
                    "6.2.6",
                    format!("Invalid rendering intent '{ri_str}' in ExtGState"),
                    location,
                );
            }
        }
    }
}

// ─── §6.2.8 — Image XObject restrictions ────────────────────────────────────

/// Check Image XObject restrictions (§6.2.8).
///
/// Scans page resources and annotation appearance resources.
pub fn check_image_xobjects(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let loc = format!("page {}", page_idx + 1);

        if let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) {
            check_image_restrictions_in_res(&res_dict, xref, &loc, report);
        }

        // Check inline images for /Interpolate true (§6.2.8.1)
        if let Some(content) = page.page_stream() {
            check_inline_image_interpolate(content, &loc, report);
        }

        // Check annotation appearances
        if let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) {
            for annot in annots.iter::<Dict<'_>>() {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    for key in [b"N" as &[u8], b"R", b"D"] {
                        if let Some(stream) = ap.get::<Stream<'_>>(key) {
                            let ap_dict = stream.dict();
                            if let Some(ap_res) = ap_dict.get::<Dict<'_>>(keys::RESOURCES) {
                                let ap_loc = format!("{loc}/Annot/AP");
                                check_image_restrictions_in_res(&ap_res, xref, &ap_loc, report);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Check inline images in a content stream for /Interpolate true (§6.2.8.1).
fn check_inline_image_interpolate(content: &[u8], location: &str, report: &mut ComplianceReport) {
    // Scan for BI ... /I true ... ID patterns (inline image with Interpolate=true)
    // /I is the abbreviation for /Interpolate in inline images.
    let mut pos = 0;
    while pos + 2 < content.len() {
        if content[pos] == b'B'
            && content[pos + 1] == b'I'
            && (pos == 0 || content[pos - 1].is_ascii_whitespace())
        {
            if let Some(id_off) = content[pos..].windows(2).position(|w| w == b"ID") {
                if id_off < 2 {
                    pos += id_off + 2;
                    continue;
                }
                let header = &content[pos + 2..pos + id_off];
                // Check for /I true or /Interpolate true
                let has_interp = header.windows(6).any(|w| w == b"/I tru")
                    || header.windows(16).any(|w| w == b"/Interpolate tru");
                if has_interp {
                    error_at(
                        report,
                        "6.2.8.1",
                        "Inline image has /Interpolate true (forbidden in PDF/A)",
                        location,
                    );
                    return;
                }
                pos += id_off + 2;
            } else {
                break;
            }
        } else {
            pos += 1;
        }
    }
}

/// Check image XObject restrictions within a resource dict.
fn check_image_restrictions_in_res(
    res_dict: &Dict<'_>,
    xref: &pdf_syntax::xref::XRef,
    location: &str,
    report: &mut ComplianceReport,
) {
    let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) else {
        return;
    };
    for (name, _) in xobj_dict.entries() {
        // XObjects are almost always indirect references — add xref fallback so we
        // can inspect images defined outside the resources dict. (#FN-6.2.7.1)
        let stream_opt = xobj_dict.get::<Stream<'_>>(name.as_ref()).or_else(|| {
            xobj_dict
                .get_ref(name.as_ref())
                .and_then(|r| xref.get::<Stream<'_>>(r.into()))
        });
        let Some(stream) = stream_opt else {
            continue;
        };
        let dict = stream.dict();

        if dict
            .get::<Name>(keys::SUBTYPE)
            .is_none_or(|s| s.as_ref() != keys::IMAGE)
        {
            continue;
        }

        let xobj_name = std::str::from_utf8(name.as_ref()).unwrap_or("?");

        if let Some(Object::Boolean(true)) = dict.get::<Object<'_>>(keys::INTERPOLATE) {
            error_at(
                report,
                "6.2.8.1",
                format!("Image XObject {xobj_name} has /Interpolate true"),
                location,
            );
        }

        if dict.contains_key(b"Alternates" as &[u8]) {
            // §6.2.7.1 — Image XObjects must not contain /Alternates key. (#FN-6.2.7.1)
            error_at(
                report,
                "6.2.7.1",
                format!("Image XObject {xobj_name} contains forbidden /Alternates key"),
                location,
            );
        }

        if dict.contains_key(keys::OPI) {
            error_at(
                report,
                "6.2.8.3",
                format!("Image XObject {xobj_name} contains forbidden /OPI key"),
                location,
            );
        }

        // OPI 1.3 keys embedded directly in Image XObject dictionaries (§6.2.9 / §6.2.6).
        // /XDPI and /YDPI are OPI 1.3 metadata keys. Internal rule "6.2.6" remaps to
        // "6.2.9" for PDF/A-1, stays "6.2.6" otherwise. (#FN-6.2.9 6-2-4-t04-fail-a)
        if dict.contains_key(b"XDPI" as &[u8]) || dict.contains_key(b"YDPI" as &[u8]) {
            error_at(
                report,
                "6.2.6",
                format!("Image XObject {xobj_name} contains forbidden OPI 1.3 keys (XDPI/YDPI)"),
                location,
            );
        }

        // §6.2.8.3 — JPEG2000 (JPXDecode) images must have a valid 'colr' box.
        // Valid METH values: 1 (enumerated CS), 2 (sRGB), 3 (restricted ICC).
        // METH=4 (enumerated with restricted ICC) and others are forbidden. (#467)
        let is_jpx = dict
            .get::<Name>(keys::FILTER)
            .is_some_and(|f| f.as_ref() == keys::JPX_DECODE);
        if is_jpx {
            let raw = stream.raw_data();
            check_jpeg2000_colr_box(&raw, xobj_name, location, report);
        }
    }
}

/// Check JPEG2000 'colr' box METH value for §6.2.8.3.
///
/// PDF/A-2 §6.2.8.3: JPEG2000 images must have a ColorSpace entry, and if the
/// colour space information is defined through a 'colr' box:
/// - METH must be 0x01 (Enumerated CS), 0x02 (Restricted ICC), or 0x03 (Any ICC).
///   METH=0x04 and above are not permitted.
/// - If METH=0x01, the Enumerated CS value must be 16 (sRGB), 17 (greyscale),
///   or 18 (sYCC). Other values (e.g. 19=CIEJab) are forbidden. (#467)
fn check_jpeg2000_colr_box(
    jp2_data: &[u8],
    xobj_name: &str,
    location: &str,
    report: &mut ComplianceReport,
) {
    // Scan for the 'colr' box tag anywhere in the data.
    let colr_tag = b"colr";
    let mut search_pos = 0;
    while search_pos + 8 < jp2_data.len() {
        // 'colr' box type is at bytes [pos+4..pos+8]
        if &jp2_data[search_pos + 4..search_pos + 8] == colr_tag {
            // Box header: 4 bytes length + 4 bytes type
            let box_len = u32::from_be_bytes([
                jp2_data[search_pos],
                jp2_data[search_pos + 1],
                jp2_data[search_pos + 2],
                jp2_data[search_pos + 3],
            ]) as usize;
            if box_len >= 9 && search_pos + box_len <= jp2_data.len() {
                let meth = jp2_data[search_pos + 8];
                // Valid METH for PDF/A: 1, 2, or 3
                if meth == 0 || meth > 3 {
                    error_at(
                        report,
                        "6.2.8.3",
                        format!(
                            "JPEG2000 image {xobj_name} has invalid 'colr' box METH value \
                             {meth:#04x} (must be 0x01, 0x02, or 0x03)"
                        ),
                        location,
                    );
                    return;
                }
                // METH=1: Enumerated CS — check that CS value is sRGB/Grey/sYCC
                if meth == 1 && box_len >= 12 {
                    let cs_bytes = &jp2_data[search_pos + 9..search_pos + 12];
                    // EnumCS is a 4-byte big-endian value at offset 9 (after METH+PREC+APPROX)
                    let enum_cs = if box_len >= 13 {
                        u32::from_be_bytes([
                            jp2_data[search_pos + 9],
                            jp2_data[search_pos + 10],
                            jp2_data[search_pos + 11],
                            jp2_data[search_pos + 12],
                        ])
                    } else {
                        // Fallback: try 3-byte read
                        u32::from_be_bytes([0, cs_bytes[0], cs_bytes[1], cs_bytes[2]])
                    };
                    // Allowed: 16=sRGB, 17=greyscale, 18=sYCC
                    if enum_cs != 16 && enum_cs != 17 && enum_cs != 18 {
                        error_at(
                            report,
                            "6.2.8.3",
                            format!(
                                "JPEG2000 image {xobj_name} uses enumerated colour space {enum_cs} \
                                 which is not permitted (only sRGB=16, greyscale=17, sYCC=18)"
                            ),
                            location,
                        );
                        return;
                    }
                }
            }
            break; // Found the colr box, done
        }
        search_pos += 1;
    }
}

// ─── §6.2.10 — Halftone and transfer function restrictions ──────────────────

/// Check halftone and transfer function restrictions in ExtGState (§6.2.10).
///
/// Scans page resources, annotation appearances, and Form XObjects.
pub fn check_halftone_and_transfer(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let loc = format!("page {}", page_idx + 1);

        // Page-level resources
        if let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) {
            check_halftone_in_extgstate(&res_dict, &loc, xref, report);

            // Form XObjects in page resources. Form XObjects are streams (possibly
            // indirect refs), so use get::<Stream<'_>> to resolve them correctly.
            // Using get::<Dict<'_>> silently failed for indirect stream refs. (#FN-6.2.10)
            if let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) {
                for (xo_name, _) in xobj_dict.entries() {
                    let Some(xo_stream) = xobj_dict.get::<Stream<'_>>(xo_name.as_ref()) else {
                        continue;
                    };
                    let xo_dict = xo_stream.dict();
                    if xo_dict
                        .get::<Name>(keys::SUBTYPE)
                        .is_some_and(|s| s.as_ref() == b"Form")
                    {
                        if let Some(xo_res) = xo_dict.get::<Dict<'_>>(keys::RESOURCES) {
                            let xo_loc = format!("{loc}/XObject");
                            check_halftone_in_extgstate(&xo_res, &xo_loc, xref, report);
                        }
                    }
                }
            }
        }

        // Annotation appearances. AP entries (/N, /R, /D) are streams — use
        // get::<Stream<'_>> so indirect stream refs are resolved correctly. (#FN-6.2.10)
        if let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) {
            for annot in annots.iter::<Dict<'_>>() {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    for (ap_key, _) in ap.entries() {
                        let Some(ap_stream) = ap.get::<Stream<'_>>(ap_key.as_ref()) else {
                            continue;
                        };
                        if let Some(ap_res) = ap_stream.dict().get::<Dict<'_>>(keys::RESOURCES) {
                            let ap_loc = format!("{loc}/Annot/AP");
                            check_halftone_in_extgstate(&ap_res, &ap_loc, xref, report);
                        }
                    }
                }
            }
        }
    }
}

/// Check halftone and transfer function restrictions in a resource dict's ExtGState.
fn check_halftone_in_extgstate(
    res_dict: &Dict<'_>,
    location: &str,
    xref: &pdf_syntax::xref::XRef,
    report: &mut ComplianceReport,
) {
    let Some(gs_dict) = res_dict.get::<Dict<'_>>(keys::EXT_G_STATE) else {
        return;
    };
    for (gs_name, _) in gs_dict.entries() {
        // Resolve indirect ExtGState entry references via xref
        let gs_opt: Option<Dict<'_>> = gs_dict.get::<Dict<'_>>(gs_name.as_ref()).or_else(|| {
            gs_dict
                .get_ref(gs_name.as_ref())
                .and_then(|r| xref.get::<Dict<'_>>(r.into()))
        });
        let Some(gs) = gs_opt else {
            continue;
        };
        let gs_str = std::str::from_utf8(gs_name.as_ref()).unwrap_or("?");

        // §6.2.10: halftone type. Resolve indirect /HT references via xref.
        let ht_dict_opt: Option<Dict<'_>> = gs.get::<Dict<'_>>(b"HT" as &[u8]).or_else(|| {
            gs.get_ref(b"HT" as &[u8])
                .and_then(|r| xref.get::<Dict<'_>>(r.into()))
        });
        if let Some(ht_dict) = ht_dict_opt {
            if let Some(ht_type) = ht_dict.get::<i32>(b"HalftoneType" as &[u8]) {
                if ht_type != 1 && ht_type != 5 {
                    error_at(
                        report,
                        "6.2.10",
                        format!(
                            "ExtGState {gs_str} uses HalftoneType {ht_type} (only 1 and 5 allowed)"
                        ),
                        location,
                    );
                }
            }
            // §6.2.10.4.1: No HalftoneName
            if ht_dict.contains_key(b"HalftoneName" as &[u8]) {
                error_at(
                    report,
                    "6.2.10.4.1",
                    format!("ExtGState {gs_str} halftone contains forbidden /HalftoneName"),
                    location,
                );
            }
            // §6.2.10.5 / §6.2.5: TransferFunction is forbidden in standalone halftones
            // (colorantName absent) and in Type 5 sub-halftones for primary CMYK colorants.
            // Any value — including /Identity — is prohibited. (ISO 19005-2 §6.2.5, -4 §6.2.10.5)
            let ht_type_val = ht_dict.get::<i32>(b"HalftoneType" as &[u8]).unwrap_or(0);
            if ht_type_val != 5 {
                // Standalone (not Type 5): TransferFunction must be absent entirely.
                if ht_dict
                    .get::<Object<'_>>(b"TransferFunction" as &[u8])
                    .is_some()
                {
                    error_at(
                        report,
                        "6.2.10.5",
                        format!("ExtGState {gs_str} halftone has forbidden /TransferFunction"),
                        location,
                    );
                }
            } else {
                // Type 5: check sub-halftone entries.
                // Primary CMYK colorants must NOT have TransferFunction.
                // Spot colorants (any name except Default) must HAVE TransferFunction.
                // Default colorant: no restriction.
                const PRIMARY: &[&[u8]] = &[b"Cyan", b"Magenta", b"Yellow", b"Black"];
                for (key, _) in ht_dict.entries() {
                    let key_bytes = key.as_ref();
                    // Skip well-known Type 5 dictionary keys (not sub-halftone entries).
                    if matches!(
                        key_bytes,
                        b"HalftoneType" | b"Type" | b"HalftoneName" | b"Default"
                    ) {
                        continue;
                    }
                    // Resolve sub-halftone: direct dict or indirect reference.
                    let sub_ht_opt: Option<Dict<'_>> =
                        ht_dict.get::<Dict<'_>>(key_bytes).or_else(|| {
                            ht_dict
                                .get_ref(key_bytes)
                                .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                        });
                    let Some(sub_ht) = sub_ht_opt else { continue };
                    let kstr = std::str::from_utf8(key_bytes).unwrap_or("?");
                    let has_tf = sub_ht
                        .get::<Object<'_>>(b"TransferFunction" as &[u8])
                        .is_some();
                    if PRIMARY.contains(&key_bytes) {
                        // Primary colorant: TransferFunction must be absent.
                        if has_tf {
                            error_at(
                                report,
                                "6.2.10.5",
                                format!(
                                    "ExtGState {gs_str} Type5/{kstr} has forbidden /TransferFunction"
                                ),
                                location,
                            );
                        }
                    } else {
                        // Spot colorant: TransferFunction must be present.
                        if !has_tf {
                            error_at(
                                report,
                                "6.2.10.5",
                                format!(
                                    "ExtGState {gs_str} Type5/{kstr} missing required /TransferFunction"
                                ),
                                location,
                            );
                        }
                    }
                }
            }
        }

        // §6.2.10.5: TR forbidden
        if gs.contains_key(keys::TR) {
            error_at(
                report,
                "6.2.10.5",
                format!("ExtGState {gs_str} contains forbidden /TR (transfer function)"),
                location,
            );
        }

        // TR2 allowed only if /Default
        if let Some(tr2) = gs.get::<Object<'_>>(keys::TR2) {
            match tr2 {
                Object::Name(n) if n.as_ref() == keys::DEFAULT => {}
                _ => {
                    error_at(
                        report,
                        "6.2.10.5",
                        format!("ExtGState {gs_str} has /TR2 that is not /Default"),
                        location,
                    );
                }
            }
        }

        // §6.2.5: HTO key forbidden (PDF/A-4).
        // Uses internal rule "6.2.5-hto" so remap_clause_numbers does not fold it into
        // "6.2.6" (which is the remap for Image XObject /Intent errors). (#FP-6.2.5)
        if gs.contains_key(b"HTO" as &[u8]) {
            error_at(
                report,
                "6.2.5-hto",
                format!("ExtGState {gs_str} contains forbidden /HTO key"),
                location,
            );
        }
    }
}

// ─── §6.2.10.6-9 — ExtGState blend mode and soft mask ───────────────────────

/// Check ExtGState blend mode and soft mask restrictions (§6.2.10.6-9).
pub fn check_extgstate_restrictions(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    // Check if there's an ICCBased CMYK OutputIntent profile
    let has_cmyk_intent = output_intent_profile_components(pdf) == Some(4);

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let res = page.resources();

        // Check if page uses ICCBased CMYK color spaces
        let has_icc_cmyk = cs_dict_has_iccbased_cmyk(&res.color_spaces);

        let gs_dict = &res.ext_g_states;
        if gs_dict.entries().next().is_none() {
            continue;
        }
        for (gs_name, _) in gs_dict.entries() {
            let Some(gs) = gs_dict.get::<Dict<'_>>(gs_name.as_ref()) else {
                continue;
            };
            let gs_str = std::str::from_utf8(gs_name.as_ref()).unwrap_or("?");

            if part == 1 {
                if let Some(bm) = gs.get::<Name>(keys::BM) {
                    let bm_val = bm.as_ref();
                    if bm_val != b"Normal" && bm_val != keys::COMPATIBLE {
                        let bm_str = std::str::from_utf8(bm_val).unwrap_or("?");
                        error_at(
                            report,
                            "6.2.10.6",
                            format!("ExtGState {gs_str} has BM={bm_str} (only Normal/Compatible allowed in PDF/A-1)"),
                            format!("page {}", page_idx + 1),
                        );
                    }
                }

                if let Some(smask) = gs.get::<Object<'_>>(keys::SMASK) {
                    match smask {
                        Object::Name(n) if n.as_ref() == b"None" => {}
                        _ => {
                            error_at(
                                report,
                                "6.2.10.7",
                                format!("ExtGState {gs_str} has non-None /SMask (transparency forbidden in PDF/A-1)"),
                                format!("page {}", page_idx + 1),
                            );
                        }
                    }
                }

                // §6.4 — CA (stroke alpha) must be 1.0 in PDF/A-1
                if let Some(ca_val) = gs.get::<f64>(b"CA" as &[u8]) {
                    if (ca_val - 1.0).abs() > 0.001 {
                        error_at(
                            report,
                            "6.4",
                            format!("ExtGState {gs_str} has CA={ca_val} (must be 1.0 in PDF/A-1)"),
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
                // §6.4 — ca (fill alpha) must be 1.0 in PDF/A-1
                if let Some(ca_val) = gs.get::<f64>(b"ca" as &[u8]) {
                    if (ca_val - 1.0).abs() > 0.001 {
                        error_at(
                            report,
                            "6.4",
                            format!("ExtGState {gs_str} has ca={ca_val} (must be 1.0 in PDF/A-1)"),
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
            }

            // §6.2.4.2 — OPM must not be 1 when ICCBased CMYK is in use with overprinting
            if has_icc_cmyk || has_cmyk_intent {
                let stroke_overprint = matches!(
                    gs.get::<Object<'_>>(b"OP" as &[u8]),
                    Some(Object::Boolean(true))
                );
                let fill_overprint = matches!(
                    gs.get::<Object<'_>>(b"op" as &[u8]),
                    Some(Object::Boolean(true))
                );
                if stroke_overprint || fill_overprint {
                    if let Some(Object::Number(opm)) = gs.get::<Object<'_>>(b"OPM" as &[u8]) {
                        if opm.as_f64() as i64 == 1 {
                            error_at(
                                report,
                                "6.2.4.2",
                                format!("ExtGState {gs_str} has OPM=1 with ICCBased CMYK and overprinting enabled"),
                                format!("page {}", page_idx + 1),
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Check if a ColorSpace dict contains any ICCBased CMYK color spaces.
fn cs_dict_has_iccbased_cmyk(cs_dict: &Dict<'_>) -> bool {
    for (name, _) in cs_dict.entries() {
        let Some(cs_arr) = cs_dict.get::<Array<'_>>(name.as_ref()) else {
            continue;
        };
        let mut items = cs_arr.iter::<Object<'_>>();
        let Some(Object::Name(cs_type)) = items.next() else {
            continue;
        };
        if cs_type.as_ref() != keys::ICC_BASED {
            continue;
        }
        if let Some(Object::Stream(icc_stream)) = items.next() {
            if let Some(n) = icc_stream.dict().get::<i32>(keys::N) {
                if n == 4 {
                    return true;
                }
            }
        }
    }
    false
}

// ─── §6.2.11 — CIDFont embedding requirements ──────────────────────────────

/// Check CIDFont Type2 embedding and CIDToGIDMap (§6.2.11).
pub fn check_cidfont_embedding(pdf: &Pdf, report: &mut ComplianceReport) {
    for_each_font(pdf, |name, font_dict, page_idx| {
        let Some(descendants) = font_dict.get::<Array<'_>>(keys::DESCENDANT_FONTS) else {
            return;
        };
        for desc_font in descendants.iter::<Dict<'_>>() {
            let Some(subtype) = desc_font.get::<Name>(keys::SUBTYPE) else {
                continue;
            };
            if subtype.as_ref() != keys::CID_FONT_TYPE2 {
                continue;
            }

            if desc_font.get::<Object<'_>>(keys::CID_TO_GID_MAP).is_none() {
                // PDF/A-4 §6.2.10.3.2 requires CIDToGIDMap to be explicitly /Identity
                // or a stream — absent counts as a violation. Internal rule "6.3.7-absent"
                // remaps to "6.2.10.3.2" for PDF/A-4, suppressed for other parts. (#FN-6.2.10.3.2)
                error_at(
                    report,
                    "6.3.7-absent",
                    format!("CIDFont Type2 '{name}' missing /CIDToGIDMap"),
                    format!("page {}", page_idx + 1),
                );
            }

            if let Some(desc) = desc_font.get::<Dict<'_>>(keys::FONT_DESC) {
                if desc.get::<Stream<'_>>(keys::FONT_FILE2).is_none() {
                    error_at(
                        report,
                        "6.2.11",
                        format!("CIDFont Type2 '{name}' missing /FontFile2 embedding"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    });
}

/// Check CIDFont /W or /DW widths presence (§6.2.11.6).
///
/// Every CIDFont (Type0 descendant) must have either /W (widths array) or /DW
/// (default width scalar). Missing widths cause text-extraction and rendering
/// failures and are caught by veraPDF as rule 6.2.11.6.
pub fn check_cidfont_w_arrays(pdf: &Pdf, report: &mut ComplianceReport) {
    for_each_font(pdf, |name, font_dict, page_idx| {
        let Some(descendants) = font_dict.get::<Array<'_>>(keys::DESCENDANT_FONTS) else {
            return;
        };
        for desc_font in descendants.iter::<Dict<'_>>() {
            // /W or /DW must be present on the CIDFont dictionary
            let has_w = desc_font.contains_key(keys::W);
            let has_dw = desc_font.contains_key(keys::DW);
            if !has_w && !has_dw {
                error_at(
                    report,
                    "6.2.11.6",
                    format!("CIDFont '{name}' has neither /W nor /DW widths array"),
                    format!("page {}", page_idx + 1),
                );
            }
        }
    });
}

// ─── §6.2.11.6 — Font encoding BaseEncoding constraint ─────────────────────

/// Check that non-symbolic TrueType fonts have valid Encoding and all fonts have
/// valid BaseEncoding values (§6.2.11.6 / §6.2.10.6 for PDF/A-4).
///
/// Non-symbolic TrueType fonts must have Encoding = /MacRomanEncoding or
/// /WinAnsiEncoding (as a Name) or an Encoding dict with one of those as BaseEncoding.
/// A missing Encoding entry is also a violation. Fixes FN t02.
///
/// For any simple font with an Encoding dict, the /BaseEncoding entry (if present)
/// must be /WinAnsiEncoding or /MacRomanEncoding.
///
/// The Encoding may be an indirect reference — resolved via xref. (#467)
pub fn check_font_base_encoding(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for_each_font(pdf, |name, font_dict, page_idx| {
        // Only applies to simple fonts (not Type0 CIDFont wrappers or Type3 fonts).
        // Type3 fonts define glyphs via /CharProcs — names in /Differences refer to
        // procedure names, not AGL glyph names. (#FP-6.2.11.6)
        let subtype = font_dict.get::<Name>(keys::SUBTYPE);
        let is_truetype = subtype.as_ref().is_some_and(|s| s.as_ref() == b"TrueType");
        let is_type0 = subtype.as_ref().is_some_and(|s| s.as_ref() == b"Type0");
        let is_type3 = subtype.as_ref().is_some_and(|s| s.as_ref() == b"Type3");
        if is_type0 || is_type3 {
            return;
        }

        // For non-symbolic TrueType fonts: Encoding must be a valid standard
        // encoding (as Name) or have a valid BaseEncoding in an Encoding dict.
        // A missing Encoding is also a violation. (#FN-6.2.11.6-t02)
        if is_truetype {
            let desc = font_dict.get::<Dict<'_>>(keys::FONT_DESC);
            let flags = desc.as_ref().and_then(|d| d.get::<i32>(keys::FLAGS));
            let symbolic = flags.is_some_and(|f| f & 0x04 != 0);
            if !symbolic {
                // Check if Encoding is a valid standard Name
                if let Some(enc_name) = font_dict.get::<Name>(keys::ENCODING) {
                    let allowed =
                        matches!(enc_name.as_ref(), b"WinAnsiEncoding" | b"MacRomanEncoding");
                    if !allowed {
                        let enc_str = std::str::from_utf8(enc_name.as_ref()).unwrap_or("?");
                        error_at(
                            report,
                            "6.2.11.6",
                            format!(
                                "Non-symbolic TrueType font '{name}' has invalid Encoding \
                                 '/{enc_str}'; only /WinAnsiEncoding or /MacRomanEncoding allowed"
                            ),
                            format!("page {}", page_idx + 1),
                        );
                    }
                    return; // Encoding is a Name; BaseEncoding in dict doesn't apply
                }
                // Encoding is absent, a dict, or an indirect ref — check for dict
                let enc_dict_opt: Option<Dict<'_>> =
                    font_dict.get::<Dict<'_>>(keys::ENCODING).or_else(|| {
                        font_dict
                            .get_ref(keys::ENCODING)
                            .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                    });
                match enc_dict_opt {
                    None => {
                        // Missing Encoding for non-symbolic TrueType is a violation.
                        error_at(
                            report,
                            "6.2.11.6",
                            format!(
                                "Non-symbolic TrueType font '{name}' missing required /Encoding \
                                 (/WinAnsiEncoding or /MacRomanEncoding)"
                            ),
                            format!("page {}", page_idx + 1),
                        );
                    }
                    Some(enc_dict) => {
                        // BaseEncoding in dict must be WinAnsiEncoding or MacRomanEncoding
                        let base_enc = enc_dict.get::<Name>(keys::BASE_ENCODING);
                        let allowed = base_enc.as_ref().is_some_and(|b| {
                            matches!(b.as_ref(), b"WinAnsiEncoding" | b"MacRomanEncoding")
                        });
                        if !allowed {
                            let base_str = base_enc
                                .as_ref()
                                .and_then(|b| std::str::from_utf8(b.as_ref()).ok())
                                .unwrap_or("(none)");
                            error_at(
                                report,
                                "6.2.11.6",
                                format!(
                                    "Font '{name}' Encoding has invalid BaseEncoding '/{base_str}'; \
                                     only /WinAnsiEncoding or /MacRomanEncoding are allowed"
                                ),
                                format!("page {}", page_idx + 1),
                            );
                        }
                        // §6.2.11.6: glyph names in /Differences must be in the AGL.
                        check_encoding_differences_agl(&enc_dict, name, page_idx, report);
                    }
                }
                return;
            }
        }

        // For non-TrueType simple fonts: only check if an Encoding dict has invalid BaseEncoding.
        let enc_dict_opt: Option<Dict<'_>> =
            font_dict.get::<Dict<'_>>(keys::ENCODING).or_else(|| {
                font_dict
                    .get_ref(keys::ENCODING)
                    .and_then(|r| xref.get::<Dict<'_>>(r.into()))
            });

        let Some(enc_dict) = enc_dict_opt else {
            return;
        };

        // If BaseEncoding is present, it must be WinAnsiEncoding or MacRomanEncoding
        if let Some(base_enc) = enc_dict.get::<Name>(keys::BASE_ENCODING) {
            let allowed = matches!(base_enc.as_ref(), b"WinAnsiEncoding" | b"MacRomanEncoding");
            if !allowed {
                let base_str = std::str::from_utf8(base_enc.as_ref()).unwrap_or("?");
                error_at(
                    report,
                    "6.2.11.6",
                    format!(
                        "Font '{name}' Encoding has invalid BaseEncoding '/{base_str}'; \
                         only /WinAnsiEncoding or /MacRomanEncoding are allowed"
                    ),
                    format!("page {}", page_idx + 1),
                );
            }
        }
        // §6.2.11.6: all names in /Differences must be valid AGL glyph names.
        // Exception: fonts with a /ToUnicode CMap are exempt because the Unicode
        // mapping is provided by the CMap, not by glyph name → AGL lookup.
        // veraPDF does not fire for fonts that have /ToUnicode. (#FP-6.2.11.6)
        let has_tounicode = font_has_tounicode(font_dict);
        if !has_tounicode {
            check_encoding_differences_agl(&enc_dict, name, page_idx, report);
        }
    });
}

/// Check that all glyph names in an Encoding /Differences array are valid AGL
/// glyph names (§6.2.11.6).  A valid name is one defined in the Adobe Glyph List
/// (AGLFN or common extensions), one of the unicode naming patterns (`uni<HEX>+`
/// or `u<HEX>{4,6}`), or `.notdef`/`.null`.
fn check_encoding_differences_agl(
    enc_dict: &Dict<'_>,
    font_name: &str,
    page_idx: usize,
    report: &mut ComplianceReport,
) {
    let Some(diffs) = enc_dict.get::<Array<'_>>(b"Differences" as &[u8]) else {
        return;
    };
    // Differences is [code name name name code name ...] — integers reset the
    // current code, Names are glyph names that must be in the AGL.
    for item in diffs.iter::<Object<'_>>() {
        let Object::Name(n) = item else { continue };
        let glyph = n.as_ref();
        if !is_valid_agl_glyph_name(glyph) {
            let gstr = std::str::from_utf8(glyph).unwrap_or("?");
            error_at(
                report,
                "6.2.11.6",
                format!(
                    "Font '{font_name}' Encoding /Differences contains glyph name \
                     '/{gstr}' not listed in the Adobe Glyph List"
                ),
                format!("page {}", page_idx + 1),
            );
        }
    }
}

/// Return `true` when `name` is a valid AGL glyph name.
///
/// Accepts:
/// - Special names: `.notdef`, `.null`, `.nonmarkingreturn`
/// - Unicode-derived names: `uni[0-9A-Fa-f]{4}+` and `u[0-9A-Fa-f]{4,6}`
/// - All entries from the Adobe Glyph List for New Fonts (AGLFN v1.7, ~302 entries)
///   plus commonly used names from the full AGL that are absent from the AGLFN
///   (ligatures `ff`/`ffi`/`ffl`, fractions, old-style Greek names, etc.)
fn is_valid_agl_glyph_name(name: &[u8]) -> bool {
    // Syntactic validity: only [A-Za-z0-9._], no leading digit, not empty.
    if name.is_empty() {
        return false;
    }
    if !name[0].is_ascii_alphabetic() && name[0] != b'.' {
        return false;
    }
    if !name
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
    {
        return false;
    }

    // Special names
    if matches!(name, b".notdef" | b".null" | b".nonmarkingreturn") {
        return true;
    }

    // Zapf Dingbats names: a1–a202 (AGL 2.0 Appendix D)
    if let Some(rest) = name.strip_prefix(b"a") {
        if !rest.is_empty() && rest.iter().all(|b| b.is_ascii_digit()) && rest.len() <= 3 {
            let n: u32 = std::str::from_utf8(rest)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if (1..=202).contains(&n) {
                return true;
            }
        }
    }

    // Unicode naming convention: uni[0-9A-Fa-f]{4}+
    if let Some(rest) = name.strip_prefix(b"uni") {
        if rest.len() >= 4 && rest.len() % 4 == 0 && rest.iter().all(|b| b.is_ascii_hexdigit()) {
            return true;
        }
    }
    // Unicode naming convention: u[0-9A-Fa-f]{4,6}
    if let Some(rest) = name.strip_prefix(b"u") {
        if (4..=6).contains(&rest.len()) && rest.iter().all(|b| b.is_ascii_hexdigit()) {
            return true;
        }
    }

    // AGLFN v1.7 + common full-AGL extras — sorted for binary search.
    const AGL_NAMES: &[&[u8]] = &[
        b"A",
        b"AE",
        b"AEacute",
        b"AEsmall",
        b"Aacute",
        b"Abreve",
        b"Acircumflex",
        b"Adieresis",
        b"Agrave",
        b"Amacron",
        b"Aogonek",
        b"Aring",
        b"Aringacute",
        b"Atilde",
        b"B",
        b"C",
        b"Cacute",
        b"Ccaron",
        b"Ccedilla",
        b"D",
        b"Dcaron",
        b"Dcroat",
        b"E",
        b"Eacute",
        b"Ebreve",
        b"Ecaron",
        b"Ecircumflex",
        b"Edieresis",
        b"Edotaccent",
        b"Egrave",
        b"Emacron",
        b"Eogonek",
        b"Eth",
        b"Euro",
        b"F",
        b"G",
        b"Gbreve",
        b"Gcommaaccent",
        b"H",
        b"I",
        b"IJ",
        b"Iacute",
        b"Ibreve",
        b"Icircumflex",
        b"Idieresis",
        b"Idotaccent",
        b"Igrave",
        b"Imacron",
        b"Iogonek",
        b"J",
        b"K",
        b"Kcommaaccent",
        b"L",
        b"Lacute",
        b"Lcaron",
        b"Lcommaaccent",
        b"Ldot",
        b"Lslash",
        b"M",
        b"N",
        b"Nacute",
        b"Ncaron",
        b"Ncommaaccent",
        b"Ntilde",
        b"O",
        b"OE",
        b"OEsmall",
        b"Oacute",
        b"Obreve",
        b"Ocircumflex",
        b"Odieresis",
        b"Ograve",
        b"Ohungarumlaut",
        b"Omacron",
        b"Oslash",
        b"Oslashacute",
        b"Otilde",
        b"P",
        b"Q",
        b"R",
        b"Racute",
        b"Rcaron",
        b"Rcommaaccent",
        b"S",
        b"Sacute",
        b"Scaron",
        b"Scedilla",
        b"Scommaaccent",
        b"T",
        b"Tbar",
        b"Tcaron",
        b"Tcommaaccent",
        b"Thorn",
        b"U",
        b"Uacute",
        b"Ubreve",
        b"Ucircumflex",
        b"Udieresis",
        b"Ugrave",
        b"Uhungarumlaut",
        b"Umacron",
        b"Uogonek",
        b"Uring",
        b"V",
        b"W",
        b"Wacute",
        b"Wcircumflex",
        b"Wdieresis",
        b"Wgrave",
        b"X",
        b"Y",
        b"Yacute",
        b"Ycircumflex",
        b"Ydieresis",
        b"Z",
        b"Zacute",
        b"Zcaron",
        b"Zdotaccent",
        b"a",
        b"aacute",
        b"abreve",
        b"acircumflex",
        b"acute",
        b"adieresis",
        b"ae",
        b"aeacute",
        b"agrave",
        b"amacron",
        b"ampersand",
        b"aogonek",
        b"aring",
        b"aringacute",
        b"asciicircum",
        b"asciitilde",
        b"asterisk",
        b"at",
        b"atilde",
        b"b",
        b"backslash",
        b"bar",
        b"braceleft",
        b"braceright",
        b"bracketleft",
        b"bracketright",
        b"breve",
        b"brokenbar",
        b"bullet",
        b"c",
        b"cacute",
        b"caron",
        b"ccaron",
        b"ccedilla",
        b"cedilla",
        b"cent",
        b"circumflex",
        b"colon",
        b"comma",
        b"copyright",
        b"currency",
        b"d",
        b"dagger",
        b"daggerdbl",
        b"dcaron",
        b"dcroat",
        b"degree",
        b"dieresis",
        b"divide",
        b"dollar",
        b"dotaccent",
        b"dotlessi",
        b"e",
        b"eacute",
        b"ebreve",
        b"ecaron",
        b"ecircumflex",
        b"edieresis",
        b"edotaccent",
        b"egrave",
        b"eight",
        b"ellipsis",
        b"emacron",
        b"emdash",
        b"endash",
        b"eogonek",
        b"equal",
        b"eth",
        b"exclam",
        b"exclamdown",
        b"f",
        b"ff",
        b"ffi",
        b"ffl",
        b"fi",
        b"five",
        b"fl",
        b"florin",
        b"four",
        b"fraction",
        b"g",
        b"gbreve",
        b"gcommaaccent",
        b"germandbls",
        b"grave",
        b"greater",
        b"guillemotleft",
        b"guillemotright",
        b"guilsinglleft",
        b"guilsinglright",
        b"h",
        b"hungarumlaut",
        b"hyphen",
        b"i",
        b"iacute",
        b"ibreve",
        b"icircumflex",
        b"idieresis",
        b"igrave",
        b"ij",
        b"imacron",
        b"iogonek",
        b"j",
        b"k",
        b"kcommaaccent",
        b"l",
        b"lacute",
        b"lcaron",
        b"lcommaaccent",
        b"ldot",
        b"less",
        b"logicalnot",
        b"lozenge",
        b"lslash",
        b"m",
        b"macron",
        b"minus",
        b"mu",
        b"multiply",
        b"n",
        b"nacute",
        b"nbspace",
        b"ncaron",
        b"ncommaaccent",
        b"nine",
        b"notequal",
        b"ntilde",
        b"numbersign",
        b"o",
        b"oacute",
        b"obreve",
        b"ocircumflex",
        b"odieresis",
        b"oe",
        b"ograve",
        b"ohungarumlaut",
        b"omacron",
        b"one",
        b"onehalf",
        b"onequarter",
        b"onesuperior",
        b"ordfeminine",
        b"ordmasculine",
        b"oslash",
        b"oslashacute",
        b"otilde",
        b"p",
        b"paragraph",
        b"parenleft",
        b"parenright",
        b"partialdiff",
        b"percent",
        b"period",
        b"periodcentered",
        b"perthousand",
        b"plus",
        b"plusminus",
        b"q",
        b"question",
        b"questiondown",
        b"quotedbl",
        b"quotedblbase",
        b"quotedblleft",
        b"quotedblright",
        b"quoteleft",
        b"quoteright",
        b"quotesinglbase",
        b"quotesingle",
        b"r",
        b"racute",
        b"radical",
        b"rcaron",
        b"rcommaaccent",
        b"registered",
        b"ring",
        b"s",
        b"sacute",
        b"scaron",
        b"scedilla",
        b"scommaaccent",
        b"section",
        b"semicolon",
        b"seven",
        b"sfthyphen",
        b"six",
        b"slash",
        b"space",
        b"square",
        b"sterling",
        b"summation",
        b"t",
        b"tbar",
        b"tcaron",
        b"tcommaaccent",
        b"thorn",
        b"three",
        b"threequarters",
        b"threesuperior",
        b"tilde",
        b"trademark",
        b"triangle",
        b"two",
        b"twosuperior",
        b"u",
        b"uacute",
        b"ubreve",
        b"ucircumflex",
        b"udieresis",
        b"ugrave",
        b"uhungarumlaut",
        b"umacron",
        b"underscore",
        b"uogonek",
        b"uring",
        b"v",
        b"w",
        b"wacute",
        b"wcircumflex",
        b"wdieresis",
        b"wgrave",
        b"x",
        b"y",
        b"yacute",
        b"ycircumflex",
        b"ydieresis",
        b"yen",
        b"z",
        b"zacute",
        b"zcaron",
        b"zdotaccent",
        b"zero",
    ];
    if AGL_NAMES.binary_search(&name).is_ok() {
        return true;
    }

    // Greek alphabet glyph names from AGL 2.0 (capital and lowercase).
    // These are in the full AGL but absent from the AGLFN subset above.
    // (#FP-6.2.11.6 — fonts using Gamma, Upsilon, etc. from standard Greek set)
    matches!(
        name,
        b"Alpha"
            | b"Alphatonos"
            | b"Beta"
            | b"Chi"
            | b"Delta"
            | b"Epsilon"
            | b"Epsilontonos"
            | b"Eta"
            | b"Etatonos"
            | b"Gamma"
            | b"Iota"
            | b"Iotadieresis"
            | b"Iotatonos"
            | b"Kappa"
            | b"Lambda"
            | b"Mu"
            | b"Nu"
            | b"Omega"
            | b"Omegatonos"
            | b"Omicron"
            | b"Omicrontonos"
            | b"Phi"
            | b"Pi"
            | b"Psi"
            | b"Rho"
            | b"Sigma"
            | b"Tau"
            | b"Theta"
            | b"Upsilon"
            | b"Upsilondieresis"
            | b"Upsilontonos"
            | b"Xi"
            | b"Zeta"
            | b"alpha"
            | b"alphatonos"
            | b"beta"
            | b"chi"
            | b"delta"
            | b"epsilon"
            | b"epsilontonos"
            | b"eta"
            | b"etatonos"
            | b"gamma"
            | b"iota"
            | b"iotadieresis"
            | b"iotadieresistonos"
            | b"iotatonos"
            | b"kappa"
            | b"lambda"
            | b"mu"
            | b"nu"
            | b"omega"
            | b"omegaadscript"
            | b"omegatonos"
            | b"omicron"
            | b"omicrontonos"
            | b"phi"
            | b"pi"
            | b"psi"
            | b"rho"
            | b"sigma"
            | b"sigmafinal"
            | b"tau"
            | b"theta"
            | b"theta1"
            | b"upsilon"
            | b"upsilonadscript"
            | b"upsilondieresis"
            | b"upsilondieresistonos"
            | b"upsilontonos"
            | b"xi"
            | b"zeta"
            // Mathematical operators and symbols (AGL 2.0, common subset)
            | b"circlemultiply"
            | b"circleplus"
            | b"circledot"
            | b"circleminus"
            | b"equivalence"
            | b"greaterequal"
            | b"lessequal"
            | b"notequal"
            | b"approxequal"
            | b"union"
            | b"intersection"
            | b"propersuperset"
            | b"propersubset"
            | b"reflexsuperset"
            | b"reflexsubset"
            | b"logicaland"
            | b"logicalor"
            | b"logicalnot"
            | b"universal"
            | b"existential"
            | b"plusminus"
            | b"divide"
            | b"multiply"
            | b"aleph"
            | b"infinity"
            | b"integral"
            | b"integraltp"
            | b"integralbt"
            | b"gradient"
            | b"partialdiff"
            | b"increment"
            | b"anglebracketleft"
            | b"anglebracketright"
            // Arrows (AGL 2.0)
            | b"arrowboth"
            | b"arrowdblboth"
            | b"arrowdblleft"
            | b"arrowdblright"
            | b"arrowdblup"
            | b"arrowdbldown"
            | b"arrowdown"
            | b"arrowleft"
            | b"arrowright"
            | b"arrowup"
            | b"arrowupdn"
            | b"arrowupdnbse"
            // Bracket/paren extensions (AGL 2.0)
            | b"bracketleftbt"
            | b"bracketleftex"
            | b"bracketlefttp"
            | b"bracketrightbt"
            | b"bracketrightex"
            | b"bracketrighttp"
            | b"parenleftbt"
            | b"parenleftex"
            | b"parenlefttp"
            | b"parenrightbt"
            | b"parenrightex"
            | b"parenrighttp"
            // Miscellaneous common symbols (AGL 2.0)
            | b"endash"
            | b"emdash"
            | b"figuredash"
            | b"softhyphen"
            | b"perthousand"
            | b"lozenge"
            | b"dagger"
            | b"daggerdbl"
            | b"filledbox"
            | b"filledrect"
            | b"openbullet"
            | b"musicalnote"
            | b"musicalnotedbl"
            | b"dotlessi"
            | b"dotlessj"
            // More mathematical relation/set symbols (AGL 2.0)
            | b"similar"
            | b"proportional"
            | b"perpendicular"
            | b"angle"
            | b"congruent"
            | b"notgreater"
            | b"notless"
            | b"notsubset"
            | b"suchthat"
            | b"therefore"
            | b"notelement"
            | b"element"
            | b"emptyset"
            | b"negationslash"
            | b"angbracketleft"
            | b"angbracketright"
            | b"lessmuch"
            | b"greatermuch"
            | b"lessequivlnt"
            | b"greaterequivlnt"
            | b"equalorfollows"
            | b"equalorprecedes"
            | b"follows"
            | b"precedes"
            | b"turnstileleft"
            | b"turnstileright"
            | b"forcesbar"
            | b"forces"
            | b"rho1"
            | b"vector"
            // CMEx / TeX math extension bracket names (veraPDF accepts these)
            | b"parenleftbig"
            | b"parenrightbig"
            | b"parenleftBig"
            | b"parenrightBig"
            | b"parenleftbigg"
            | b"parenrightbigg"
            | b"parenleftBigg"
            | b"parenrightBigg"
            | b"bracketleftbig"
            | b"bracketrightbig"
            | b"bracketleftBig"
            | b"bracketrightBig"
            | b"bracketleftbigg"
            | b"bracketrightbigg"
            | b"bracketleftBigg"
            | b"bracketrightBigg"
            | b"braceleftbig"
            | b"bracerightbig"
            | b"braceleftBig"
            | b"bracerightBig"
            | b"braceleftbigg"
            | b"bracerightbigg"
            | b"braceleftBigg"
            | b"bracerightBigg"
            | b"arrowvertex"
            | b"arrowvertexdbl"
            | b"braceex"
            | b"bracerightmid"
            | b"braceleftmid"
            | b"bracelefttp"
            | b"bracerightbt"
            | b"braceleftbt"
            | b"bracerightex"
            | b"braceleftex"
            | b"bracerighttp"
            | b"ceilingleft"
            | b"ceilingright"
            | b"floorleft"
            | b"floorright"
            | b"hatwide"
            | b"hatwider"
            | b"hatwideest"
            | b"tildewide"
            | b"tildewider"
            | b"tildewideest"
            | b"widehat"
            | b"widetilde"
            | b"radical"
            | b"radicalBig"
            | b"radicalBigg"
            | b"radicalbt"
            | b"radicalex"
            | b"radicaltp"
            | b"radicalbig"
            | b"radicalbigg"
            | b"slashbig"
            | b"slashBig"
            | b"slashbigg"
            | b"slashBigg"
            | b"backslashbig"
            | b"backslashBig"
            | b"backslashbigg"
            | b"backslashBigg"
            | b"summationdisplay"
            | b"summationtext"
            | b"productdisplay"
            | b"producttext"
            | b"coproductdisplay"
            | b"coproducttext"
            | b"integraldisplay"
            | b"integraltext"
            | b"uniondisplay"
            | b"uniontext"
            | b"intersectiondisplay"
            | b"intersectiontext"
            | b"unionmultidisplay"
            | b"unionmultitext"
            | b"logicalordisplay"
            | b"logicalortext"
            | b"logicalanddisplay"
            | b"logicalandtext"
            | b"integralmultidisplay"
            | b"integralmultitext"
            | b"circledotdisplay"
            | b"circledottext"
            | b"circleplusdisplay"
            | b"circleplustext"
            | b"circlemultiplydisplay"
            | b"circlemultiplytext"
            | b"contintegraldisplay"
            | b"contintegraltext"
    )
}

// ─── §6.2.10.3 — CIDSystemInfo Registry/Ordering consistency ───────────────

/// Check that CIDFont and its CMap have matching CIDSystemInfo Registry and
/// Ordering values (§6.2.10.3.1).
///
/// For each Type0 font, the /Encoding CMap stream's /CIDSystemInfo must have
/// the same /Registry and /Ordering as the /CIDSystemInfo of the CIDFont
/// in /DescendantFonts (case-sensitive comparison per PDF spec).
pub fn check_cidsystem_info_consistency(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for_each_font(pdf, |name, font_dict, page_idx| {
        // Only Type0 fonts have DescendantFonts + Encoding CMap
        let Some(subtype) = font_dict.get::<Name>(keys::SUBTYPE) else {
            return;
        };
        if subtype.as_ref() != b"Type0" {
            return;
        }

        let loc = format!("page {}", page_idx + 1);

        // Get the CMap stream's CIDSystemInfo.
        // /Encoding may be an indirect reference to an embedded CMap stream — resolve it.
        // (#FN-6.2.10.3.1)
        let cmap_stream = font_dict.get::<Stream<'_>>(keys::ENCODING).or_else(|| {
            font_dict
                .get_ref(keys::ENCODING)
                .and_then(|r| xref.get::<Stream<'_>>(r.into()))
        });
        let cmap_csi = cmap_stream
            .as_ref()
            .and_then(|s| s.dict().get::<Dict<'_>>(keys::CIDSYSTEMINFO));

        let Some(cmap_csi) = cmap_csi else {
            // Encoding is a predefined CMap name (e.g. Identity-H) — no embedded
            // CIDSystemInfo to compare directly; the standard CMap is exempt.
            return;
        };

        let cmap_registry = cmap_csi.get::<pdf_syntax::object::String>(keys::REGISTRY);
        let cmap_ordering = cmap_csi.get::<pdf_syntax::object::String>(keys::ORDERING);

        // Get the CIDFont's CIDSystemInfo
        let Some(descendants) = font_dict.get::<Array<'_>>(keys::DESCENDANT_FONTS) else {
            return;
        };
        for cid_font in descendants.iter::<Dict<'_>>() {
            // CIDFont's CIDSystemInfo may be inline or indirect
            let cid_csi_opt: Option<Dict<'_>> =
                cid_font.get::<Dict<'_>>(keys::CIDSYSTEMINFO).or_else(|| {
                    cid_font
                        .get_ref(keys::CIDSYSTEMINFO)
                        .and_then(|r| pdf.xref().get::<Dict<'_>>(r.into()))
                });
            let Some(cid_csi) = cid_csi_opt else {
                continue;
            };

            let cid_registry = cid_csi.get::<pdf_syntax::object::String>(keys::REGISTRY);
            let cid_ordering = cid_csi.get::<pdf_syntax::object::String>(keys::ORDERING);

            if let (Some(cr), Some(mr)) = (&cid_registry, &cmap_registry) {
                if cr.as_bytes() != mr.as_bytes() {
                    let cr_s = std::str::from_utf8(cr.as_bytes()).unwrap_or("?");
                    let mr_s = std::str::from_utf8(mr.as_bytes()).unwrap_or("?");
                    error_at(
                        report,
                        "6.2.10.3.1",
                        format!("Font {name}: CIDFont Registry ({cr_s}) != CMap Registry ({mr_s})"),
                        loc.clone(),
                    );
                }
            }

            if let (Some(co), Some(mo)) = (&cid_ordering, &cmap_ordering) {
                if co.as_bytes() != mo.as_bytes() {
                    let co_s = std::str::from_utf8(co.as_bytes()).unwrap_or("?");
                    let mo_s = std::str::from_utf8(mo.as_bytes()).unwrap_or("?");
                    error_at(
                        report,
                        "6.2.10.3.1",
                        format!("Font {name}: CIDFont Ordering ({co_s}) != CMap Ordering ({mo_s})"),
                        loc.clone(),
                    );
                }
            }
        }
    });
}

// ─── §6.2.3.2 — OutputIntent ICC profile embedding ─────────────────────────

/// Check OutputIntent has embedded ICC profile (§6.2.3.2).
pub fn check_output_intent_profile(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(intents) = cat.get::<Array<'_>>(keys::OUTPUT_INTENTS) else {
        return;
    };
    for intent in intents.iter::<Dict<'_>>() {
        if let Some(s) = intent.get::<Name>(keys::S) {
            if s.as_ref() == b"GTS_PDFA1"
                && intent
                    .get::<Stream<'_>>(keys::DEST_OUTPUT_PROFILE)
                    .is_none()
            {
                error(
                    report,
                    "6.2.3.2",
                    "OutputIntent GTS_PDFA1 missing DestOutputProfile (ICC profile)",
                );
            }
        }
    }
}

/// Check catalog Version key for PDF/A-4 (§6.1.12).
///
/// PDF/A-4 (ISO 19005-4) requires the Version key in the catalog dictionary
/// to match the pattern "2.n" where n is a single digit (0-9). Exactly 3 chars.
pub fn check_catalog_version_pdfa4(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else { return };
    if let Some(version) = cat.get::<Name>(b"Version" as &[u8]) {
        let v = version.as_ref();
        let valid = v.len() == 3 && v[0] == b'2' && v[1] == b'.' && v[2].is_ascii_digit();
        if !valid {
            let vs = std::str::from_utf8(v).unwrap_or("?");
            error(
                report,
                "6.1.12",
                format!("Catalog Version key '{vs}' does not match required pattern '2.n'"),
            );
        }
    }
    // Note: absence of Version key is acceptable (PDF header version is used)
}

/// Check implementation limits (§6.1.12 for PDF/A-1, §6.1.13 for PDF/A-2/3/4).
///
/// Real values ≤ 32767, name ≤ 127 bytes, string ≤ 65535/32767 bytes,
/// graphics state nesting ≤ 28 levels.
pub fn check_page_dimensions(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    check_page_dimensions_with_cache(pdf, &ObjectCache::new(pdf), part, report);
}

/// Cached version that accepts pre-collected objects.
pub fn check_page_dimensions_with_cache(
    pdf: &Pdf,
    cache: &ObjectCache<'_>,
    part: u8,
    report: &mut ComplianceReport,
) {
    // PDF/A-1 uses clause 6.1.12, PDF/A-2/3/4 uses 6.1.13
    let rule = if part == 1 { "6.1.12" } else { "6.1.13" };

    const MAX_REAL: f64 = 32767.0;

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let rect = page.media_box();
        // Check all four MediaBox values individually
        for val in [rect.x0, rect.y0, rect.x1, rect.y1] {
            if val.abs() > MAX_REAL {
                error_at(
                    report,
                    rule,
                    format!(
                        "Absolute real value {:.1} exceeds maximum 32767.0",
                        val.abs()
                    ),
                    format!("page {}", page_idx + 1),
                );
                break; // One error per page is enough
            }
        }

        let width = (rect.x1 - rect.x0).abs();
        let height = (rect.y1 - rect.y0).abs();
        if width > MAX_REAL || height > MAX_REAL {
            error_at(
                report,
                rule,
                format!(
                    "Page dimensions {:.0}x{:.0} exceed maximum 32767.0",
                    width, height
                ),
                format!("page {}", page_idx + 1),
            );
        }

        // Scan content stream numeric operands (both overflow and subnormal).
        if let Some(content) = page.page_stream() {
            if scan_content_stream_reals(content, MAX_REAL) {
                error_at(
                    report,
                    rule,
                    "Content stream contains real value exceeding 32767 or subnormal float",
                    format!("page {}", page_idx + 1),
                );
            }
        }

        // Also scan Form XObject content streams — they may contain subnormal
        // floats in color operators etc. Fixes TWG A018 §6.1.13 FN. (#467)
        let page_dict = page.raw();
        if let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) {
            if let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) {
                for (xname, _) in xobj_dict.entries() {
                    let Some(stream) = xobj_dict.get::<Stream<'_>>(xname.as_ref()) else {
                        continue;
                    };
                    let is_form = stream
                        .dict()
                        .get::<Name>(keys::SUBTYPE)
                        .is_some_and(|s| s.as_ref() == b"Form");
                    if !is_form {
                        continue;
                    }
                    if let Ok(decoded) = stream.decoded() {
                        if scan_content_stream_reals(&decoded, MAX_REAL) {
                            let xname_str = std::str::from_utf8(xname.as_ref()).unwrap_or("?");
                            error_at(
                                report,
                                rule,
                                format!(
                                    "Form XObject {xname_str} contains real value \
                                     exceeding 32767 or subnormal float"
                                ),
                                format!("page {}", page_idx + 1),
                            );
                        }
                    }
                }
            }
        }
    }

    // Name objects must not exceed 127 bytes.
    // Use both approaches: cache (for top-level objects) and raw scan
    // (for inline dicts/nested name tokens not in the xref). (#467)
    check_name_lengths_cached(cache, rule, report);
    check_name_lengths_raw(pdf, rule, report);

    // String objects must not exceed 65535 bytes
    check_string_lengths_cached(cache, rule, report);

    // Array objects must not exceed 8191 elements.
    // check_array_sizes_cached covers most objects but skips large PDFs (bounded cache).
    // check_pages_tree_kids_sizes directly checks /Kids in the pages tree — the
    // most common location for an oversized array (e.g. flat 10000-page tree). (#FN-6.1.12)
    check_array_sizes_cached(cache, rule, report);
    check_pages_tree_kids_sizes(pdf, rule, report);

    // Dictionary objects must not exceed 4095 entries
    check_dict_sizes_cached(cache, rule, report);

    // Graphics state nesting depth (q/Q) must not exceed 28
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        if let Some(content) = page.page_stream() {
            check_gs_nesting_depth(content, page_idx, rule, report);
        }
    }

    // DeviceN color spaces must not have more than 8 components (PDF/A-1)
    // or 32 components (PDF/A-2/3/4)
    let max_components: usize = if part == 1 { 8 } else { 32 };
    check_devicen_components(pdf, max_components, rule, report);
}

/// All Name objects must not exceed 127 bytes.
#[allow(dead_code)]
fn check_name_lengths(pdf: &Pdf, rule: &str, report: &mut ComplianceReport) {
    check_name_lengths_cached(&ObjectCache::new(pdf), rule, report);
}

fn check_name_lengths_cached(cache: &ObjectCache<'_>, rule: &str, report: &mut ComplianceReport) {
    for obj in cache.iter() {
        if let Object::Dict(d) = obj {
            for (key, _) in d.entries() {
                if key.as_ref().len() > 127 {
                    error(
                        report,
                        rule,
                        format!("Name key exceeds 127 bytes ({})", key.as_ref().len()),
                    );
                    return;
                }
            }
        }
        if let Object::Name(n) = obj {
            if n.as_ref().len() > 127 {
                error(
                    report,
                    rule,
                    format!("Name object exceeds 127 bytes ({})", n.as_ref().len()),
                );
                return;
            }
        }
    }
}

/// Scan raw PDF bytes for name tokens longer than 127 bytes.
///
/// The ObjectCache only covers top-level xref objects; inline dicts (e.g. page
/// Resources/ColorSpace) are not iterated. A raw byte scan catches all names
/// regardless of nesting depth. (#467)
pub fn check_name_lengths_raw(pdf: &Pdf, rule: &str, report: &mut ComplianceReport) {
    let data = pdf.data().as_ref();
    let len = data.len();
    let mut pos = 0;
    while pos < len {
        if data[pos] == b'/' {
            // Scan the name token: valid name chars are anything except delimiters
            // and whitespace. Stop at whitespace, '/', '(', ')', '[', ']',
            // '{', '}', '<', '>', '%', null.
            let name_start = pos + 1;
            let mut name_end = name_start;
            while name_end < len {
                let b = data[name_end];
                if b <= 0x20
                    || b == b'/'
                    || b == b'('
                    || b == b')'
                    || b == b'['
                    || b == b']'
                    || b == b'{'
                    || b == b'}'
                    || b == b'<'
                    || b == b'>'
                    || b == b'%'
                {
                    break;
                }
                name_end += 1;
            }
            let name_len = name_end - name_start;
            if name_len > 127 {
                error(
                    report,
                    rule,
                    format!("PDF name token exceeds 127 bytes ({name_len} bytes)"),
                );
                return; // Report once
            }
            pos = name_end;
        } else {
            pos += 1;
        }
    }
}

fn check_string_lengths_cached(cache: &ObjectCache<'_>, rule: &str, report: &mut ComplianceReport) {
    for obj in cache.iter() {
        if let Object::String(ref s) = obj {
            if s.as_bytes().len() > 65535 {
                error(
                    report,
                    rule,
                    format!("String object exceeds 65535 bytes ({})", s.as_bytes().len()),
                );
                return;
            }
        }
    }
}

fn check_array_sizes_cached(cache: &ObjectCache<'_>, rule: &str, report: &mut ComplianceReport) {
    use pdf_syntax::object::MaybeRef;
    for obj in cache.iter() {
        match obj {
            Object::Array(ref a) => {
                let count = a.raw_iter().count();
                if count > 8191 {
                    error(
                        report,
                        rule,
                        format!("Array object exceeds 8191 elements ({count})"),
                    );
                    return;
                }
            }
            // Also check arrays that are values inside a dict (e.g. /Kids in a Pages dict).
            // Top-level objects are dicts or arrays; arrays nested as dict values are not
            // returned directly by cache.iter(). (#FN-6.1.12)
            Object::Dict(ref d) => {
                for (_, val) in d.entries() {
                    if let MaybeRef::NotRef(Object::Array(ref inner)) = val {
                        let count = inner.raw_iter().count();
                        if count > 8191 {
                            error(
                                report,
                                rule,
                                format!("Array object exceeds 8191 elements ({count})"),
                            );
                            return;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Check /Kids arrays in the pages tree for the 8191-element limit.
///
/// The object cache is skipped for large PDFs (>20K objects), so a flat
/// /Kids array with e.g. 10000 entries would be invisible to
/// `check_array_sizes_cached`. This function resolves the pages tree
/// via xref and checks the /Kids count directly. (#FN-6.1.12)
fn check_pages_tree_kids_sizes(pdf: &Pdf, rule: &str, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(pages_ref) = cat.get_ref(b"Pages" as &[u8]) else {
        return;
    };
    let Some(pages) = xref.get::<Dict<'_>>(pages_ref.into()) else {
        return;
    };
    // Check the root /Kids array — a flat tree with >8191 pages violates §6.1.12.
    // Nested trees typically have small /Kids arrays (<1000 each); the root is
    // the only realistic place for an oversized array. (#FN-6.1.12)
    if let Some(kids) = pages.get::<Array<'_>>(keys::KIDS) {
        let count = kids.raw_iter().count();
        if count > 8191 {
            error(
                report,
                rule,
                format!("Pages /Kids array exceeds 8191 elements ({count})"),
            );
        }
    }
}

fn check_dict_sizes_cached(cache: &ObjectCache<'_>, rule: &str, report: &mut ComplianceReport) {
    for obj in cache.iter() {
        if let Object::Dict(ref d) = obj {
            if d.len() > 4095 {
                error(
                    report,
                    rule,
                    format!("Dictionary object exceeds 4095 entries ({})", d.len()),
                );
                return;
            }
        }
    }
}

/// DeviceN color spaces must not exceed max components.
fn check_devicen_components(pdf: &Pdf, max: usize, rule: &str, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) else {
            continue;
        };
        let Some(cs_dict) = res_dict.get::<Dict<'_>>(keys::COLORSPACE) else {
            continue;
        };
        for (name, _) in cs_dict.entries() {
            let Some(cs_arr) = cs_dict.get::<Array<'_>>(name.as_ref()) else {
                continue;
            };
            let mut items = cs_arr.iter::<Object<'_>>();
            let Some(Object::Name(cs_type)) = items.next() else {
                continue;
            };
            if cs_type.as_ref() != keys::DEVICE_N {
                continue;
            }
            // Second element is the names array
            if let Some(Object::Array(names_arr)) = items.next() {
                let count = names_arr.raw_iter().count();
                if count > max {
                    error_at(
                        report,
                        rule,
                        format!("DeviceN has {count} components (max {max})"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }
}

// ─── Batch 3: §6.1.x and §6.6.1 — File structure, actions, streams ─────────

/// Check all page boundary boxes including BleedBox, TrimBox, ArtBox (§6.1.13).
pub fn check_all_page_boundaries(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        // Check MediaBox via page.media_box() which handles inheritance
        let mb = page.media_box();
        let mw = (mb.x1 - mb.x0).abs();
        let mh = (mb.y1 - mb.y0).abs();
        if mw < 3.0 || mh < 3.0 {
            error_at(
                report,
                "6.1.13",
                format!("MediaBox {mw:.1}x{mh:.1} less than 3 units"),
                format!("page {}", page_idx + 1),
            );
        }
        if mw > 14400.0 || mh > 14400.0 {
            error_at(
                report,
                "6.1.13",
                format!("MediaBox {mw:.0}x{mh:.0} exceeds 14400 units"),
                format!("page {}", page_idx + 1),
            );
        }
        let boxes: &[(&[u8], &str)] = &[
            (b"CropBox" as &[u8], "CropBox"),
            (keys::BLEED_BOX, "BleedBox"),
            (keys::TRIM_BOX, "TrimBox"),
            (keys::ART_BOX, "ArtBox"),
        ];
        for &(key, name) in boxes {
            if let Some(arr) = page_dict.get::<Array<'_>>(key) {
                let vals: Vec<f64> = arr.iter::<f64>().collect();
                if vals.len() == 4 {
                    let w = (vals[2] - vals[0]).abs();
                    let h = (vals[3] - vals[1]).abs();
                    if w < 3.0 || h < 3.0 {
                        error_at(
                            report,
                            "6.1.13",
                            format!("{name} {w:.1}x{h:.1} less than 3 units"),
                            format!("page {}", page_idx + 1),
                        );
                    }
                    if w > 14400.0 || h > 14400.0 {
                        error_at(
                            report,
                            "6.1.13",
                            format!("{name} {w:.0}x{h:.0} exceeds 14400 units"),
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
            }
        }
    }
}

/// Check stream filters for PDF/A compliance (§6.1.8, §6.1.9).
pub fn check_stream_filters(pdf: &Pdf, pdfa_part: u8, report: &mut ComplianceReport) {
    check_stream_filters_cached(&ObjectCache::new(pdf), pdfa_part, report);
}

pub fn check_stream_filters_cached(
    cache: &ObjectCache<'_>,
    pdfa_part: u8,
    report: &mut ComplianceReport,
) {
    for obj in cache.iter() {
        if let Object::Stream(s) = obj {
            let dict = s.dict();
            let Some(filter) = dict.get::<Object<'_>>(keys::FILTER) else {
                continue;
            };
            match &filter {
                Object::Name(name) => {
                    check_single_filter(name.as_ref(), pdfa_part, dict, report);
                }
                Object::Array(arr) => {
                    for fname in arr.iter::<Name>() {
                        check_single_filter(fname.as_ref(), pdfa_part, dict, report);
                    }
                }
                _ => {}
            }
        }
    }
}

fn check_single_filter(
    filter_name: &[u8],
    pdfa_part: u8,
    dict: &Dict<'_>,
    report: &mut ComplianceReport,
) {
    if filter_name == keys::LZW_DECODE || filter_name == keys::LZW_DECODE_ABBREVIATION {
        // PDF/A-1: §6.1.10, PDF/A-2/3: §6.1.8
        let rule = if pdfa_part == 1 { "6.1.10" } else { "6.1.8" };
        error(report, rule, "LZWDecode filter is forbidden in PDF/A");
    }
    if filter_name == keys::JBIG2_DECODE {
        // §6.1.6.2 / §6.1.8 / §6.1.10 — JBIG2Decode with global segments (/JBIG2Globals)
        // is forbidden. /JBIG2Globals is almost always an indirect stream ref, so we must
        // check get_ref() in addition to direct stream (the direct case is extremely rare).
        // When multiple filters are used, /DecodeParms is an array instead of a dict; scan
        // all elements of the array to find JBIG2Globals. (#FN-6.1.6.2)
        let has_globals = if let Some(params) = dict.get::<Dict<'_>>(keys::DECODE_PARMS) {
            params.get::<Stream<'_>>(keys::JBIG2_GLOBALS).is_some()
                || params.get_ref(keys::JBIG2_GLOBALS).is_some()
        } else if let Some(params_arr) = dict.get::<Array<'_>>(keys::DECODE_PARMS) {
            // Multi-filter stream: DecodeParms is an array of dicts (or nulls).
            params_arr.iter::<Dict<'_>>().any(|p| {
                p.get::<Stream<'_>>(keys::JBIG2_GLOBALS).is_some()
                    || p.get_ref(keys::JBIG2_GLOBALS).is_some()
            })
        } else {
            false
        };
        if has_globals {
            // PDF/A-1: §6.1.10 (forbidden filters), PDF/A-2/3/4: §6.1.8
            // (remapped to §6.1.6.2 for parts 2-4 in pdfa.rs)
            let rule = if pdfa_part == 1 { "6.1.10" } else { "6.1.8" };
            error(report, rule, "JBIG2Decode with global segments");
        }
    }
    if filter_name == keys::JPX_DECODE && pdfa_part == 1 {
        error(report, "6.1.9", "JPXDecode (JPEG2000) forbidden in PDF/A-1");
    }
    // §6.1.6.2 (PDF/A-4) / §6.1.8 (PDF/A-2/3) — non-standard stream filter names.
    // PDF filter names are case-sensitive; /Flatedecode ≠ /FlateDecode.
    // Any name not in the standard set is a violation. (#FN-6.1.6.2)
    const STANDARD_FILTERS: &[&[u8]] = &[
        b"ASCIIHexDecode",
        b"ASCII85Decode",
        b"LZWDecode",
        b"FlateDecode",
        b"RunLengthDecode",
        b"CCITTFaxDecode",
        b"JBIG2Decode",
        b"DCTDecode",
        b"JPXDecode",
        b"Crypt",
        // Inline image abbreviations also appear in stream filters in some PDFs:
        b"AHx",
        b"A85",
        b"LZW",
        b"Fl",
        b"RL",
        b"CCF",
        b"DCT",
    ];
    if !STANDARD_FILTERS.contains(&filter_name) {
        let name_str = std::str::from_utf8(filter_name).unwrap_or("?");
        let rule = if pdfa_part == 1 { "6.1.10" } else { "6.1.8" };
        error(
            report,
            rule,
            format!("Non-standard stream filter /{name_str} (§6.1.6.2)"),
        );
    }
}

/// Check inline image filters in content streams (§6.1.9).
///
/// LZW and Crypt filters are forbidden in inline images too.
pub fn check_inline_image_filters(pdf: &Pdf, pdfa_part: u8, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let Some(content) = page.page_stream() else {
            continue;
        };
        if content.len() > MAX_CONTENT_STREAM_SCAN_SIZE {
            continue;
        }
        let text = String::from_utf8_lossy(content);
        let loc = format!("page {}", page_idx + 1);
        // Find BI ... ID sequences and check /F or /Filter keys within them
        let mut pos = 0;
        let mut inline_count = 0usize;
        while let Some(bi_pos) = text[pos..].find("BI") {
            let abs_bi = pos + bi_pos;
            // Make sure BI is at a word boundary
            let before_ok = abs_bi == 0 || text.as_bytes()[abs_bi - 1].is_ascii_whitespace();
            let after_ok = abs_bi + 2 >= text.len()
                || text.as_bytes()[abs_bi + 2].is_ascii_whitespace()
                || text.as_bytes()[abs_bi + 2] == b'/';
            if !before_ok || !after_ok {
                pos = abs_bi + 2;
                continue;
            }
            inline_count += 1;
            if inline_count > 200 {
                break; // Avoid pathological scans
            }
            // Find ID marker (preceded by any whitespace: space, newline, etc.)
            let search_region = &text[abs_bi..];
            let id_pos = search_region
                .find(" ID")
                .or_else(|| search_region.find("\nID"))
                .or_else(|| search_region.find("\rID"))
                .or_else(|| search_region.find("\tID"));
            let Some(id_pos) = id_pos else {
                pos = abs_bi + 2;
                continue;
            };
            let header = &text[abs_bi..abs_bi + id_pos];
            // Check for /F or /Filter with LZW or Crypt value (case-insensitive)
            // Handles: /F /LZW, /F/LZW, /Filter /LZWDecode, /F[/LZW], etc.
            let rule = if pdfa_part == 1 { "6.1.10" } else { "6.1.9" };
            let header_lower = header.to_ascii_lowercase();
            let has_lzw = header_lower.contains("/f /lzw")
                || header_lower.contains("/f/lzw")
                || header_lower.contains("/filter /lzw")
                || header_lower.contains("/filter/lzw")
                || header_lower.contains("/f[/lzw")
                || header_lower.contains("/filter[/lzw")
                || header_lower.contains("/f [/lzw")
                || header_lower.contains("/filter [/lzw");
            let has_crypt = header_lower.contains("/f /cr")
                || header_lower.contains("/f/cr")
                || header_lower.contains("/filter /cr")
                || header_lower.contains("/filter/cr")
                || header_lower.contains("/f[/cr")
                || header_lower.contains("/filter[/cr");
            if has_lzw {
                error_at(
                    report,
                    rule,
                    "Inline image uses forbidden LZWDecode filter",
                    loc.clone(),
                );
            }
            if has_crypt {
                error_at(
                    report,
                    rule,
                    "Inline image uses forbidden Crypt filter",
                    loc.clone(),
                );
            }
            pos = abs_bi + id_pos;
        }
    }
}

/// Check no data after last %%EOF marker (§6.1.3 test 3).
pub fn check_no_data_after_eof(pdf: &Pdf, report: &mut ComplianceReport) {
    let data = pdf.data().as_ref();
    // Find last %%EOF
    if let Some(eof_pos) = data.windows(5).rposition(|w| w == b"%%EOF") {
        let after = &data[eof_pos + 5..];
        // Allow trailing whitespace/EOL markers but nothing else
        let has_trailing_data = after.iter().any(|&b| !b.is_ascii_whitespace());
        if has_trailing_data {
            error(report, "6.1.3", "Data found after last %%EOF marker");
        }
    }
}

/// Check Widget annotations don't have /A or /AA keys.
///
/// PDF/A-1 §6.6.2 test 1: Widget annotation must not have /AA.
/// PDF/A-2/3/4 §6.4.1 test 1: Widget annotation must not have /A or /AA.
pub fn check_widget_no_action(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    // For PDF/A-1, veraPDF reports §6.6.2 for the /AA presence check.
    // For PDF/A-2/3/4, veraPDF uses §6.4.1.
    let aa_rule = if part == 1 { "6.6.2" } else { "6.4.1" };

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            let subtype = annot.get::<Name>(keys::SUBTYPE);
            let is_widget = subtype.as_ref().is_some_and(|s| s.as_ref() == b"Widget");
            if !is_widget {
                continue;
            }
            if annot.contains_key(b"A" as &[u8]) {
                error_at(
                    report,
                    "6.4.1",
                    "Widget annotation contains /A key (forbidden action)",
                    format!("page {}", page_idx + 1),
                );
            }
            if annot.contains_key(b"AA" as &[u8]) {
                error_at(
                    report,
                    aa_rule,
                    "Widget annotation contains /AA key (forbidden additional actions)",
                    format!("page {}", page_idx + 1),
                );
            }
        }
    }
}

/// Check AcroForm field dictionaries don't have /AA entry (PDF/A-1 §6.6.2 test 2).
///
/// veraPDF reports §6.6.2 for any field dictionary containing an /AA entry.
/// This is independent of what actions are inside the /AA.
pub fn check_field_aa_pdfa1(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(acroform) = cat.get::<Dict<'_>>(keys::ACRO_FORM) else {
        return;
    };
    let Some(fields) = acroform.get::<Array<'_>>(keys::FIELDS) else {
        return;
    };
    check_field_aa_recursive(&fields, report);
}

fn check_field_aa_recursive(fields: &Array<'_>, report: &mut ComplianceReport) {
    for (idx, field) in fields.iter::<Dict<'_>>().enumerate() {
        if field.contains_key(b"AA" as &[u8]) {
            error_at(
                report,
                "6.6.2",
                "Field dictionary contains /AA entry (forbidden additional-actions)",
                format!("field {}", idx + 1),
            );
        }
        // Recurse into Kids
        if let Some(kids) = field.get::<Array<'_>>(keys::KIDS) {
            check_field_aa_recursive(&kids, report);
        }
    }
}

/// Check document Catalog does not contain /NeedsRendering (PDF/A-2+ §6.4.2 test 2).
///
/// ISO 19005-2 §6.4.2 test 2 (and ISO 19005-4 §6.4.2 test 2): Catalog shall not
/// contain the NeedsRendering key. veraPDF reports clause "6.4.2" for PDF/A-2/3/4.
pub fn check_catalog_needs_rendering(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    if cat.contains_key(b"NeedsRendering" as &[u8]) {
        error(
            report,
            "6.4.2",
            "Document Catalog contains forbidden /NeedsRendering key (§6.4.2 test 2)",
        );
    }
}

/// Check OutputIntent profile class (§6.2.3 test 1).
///
/// DestOutputProfile ICC profile must be output ("prtr") or monitor ("mntr") class.
pub fn check_output_intent_profile_class(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(intents) = cat.get::<Array<'_>>(keys::OUTPUT_INTENTS) else {
        return;
    };
    for dict in intents.iter::<Dict<'_>>() {
        if let Some(stream) = dict.get::<Stream<'_>>(keys::DEST_OUTPUT_PROFILE) {
            if let Ok(data) = stream.decoded() {
                if data.len() >= 20 {
                    // ICC profile device class is at bytes 12-15
                    let class = &data[12..16];
                    if class != b"prtr" && class != b"mntr" {
                        let class_str = std::str::from_utf8(class).unwrap_or("?");
                        error(
                            report,
                            "6.2.3",
                            format!(
                                "OutputIntent profile device class is '{class_str}', expected 'prtr' or 'mntr'"
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// Check OutputIntent ICC profile color space signature is valid.
///
/// The ICC profile header bytes 16–19 encode the data color space of the profile.
/// For a DestOutputProfile the color space must be one of the known ICC color
/// space signatures.  An unknown signature indicates a malformed or non-ICC
/// stream being used as a color profile.
///
/// veraPDF reports all ICC profile validity issues under §6.2.3.2 for all
/// PDF/A parts (remap_clause_numbers leaves §6.2.3.2 unchanged). Fixes #467.
pub fn check_output_intent_icc_signature(pdf: &Pdf, report: &mut ComplianceReport) {
    // Known valid ICC color space signatures (ICC.1:2004, Table 18)
    const VALID_SIGNATURES: &[&[u8]] = &[
        b"RGB ", b"CMYK", b"GRAY", b"Lab ", b"XYZ ", b"Luv ", b"YCbr", b"Yxy ", b"HSV ", b"HLS ",
        b"CMY ", b"2CLR", b"3CLR", b"4CLR", b"5CLR", b"6CLR", b"7CLR", b"8CLR", b"9CLR", b"ACLR",
        b"BCLR", b"CCLR", b"DCLR", b"ECLR", b"FCLR", b"ncl ", // n-channel, colour not known
    ];

    let Some(cat) = catalog(pdf) else { return };
    let Some(intents) = cat.get::<Array<'_>>(keys::OUTPUT_INTENTS) else {
        return;
    };
    for dict in intents.iter::<Dict<'_>>() {
        let Some(stream) = dict.get::<Stream<'_>>(keys::DEST_OUTPUT_PROFILE) else {
            continue;
        };
        let Ok(data) = stream.decoded() else {
            continue;
        };
        if data.len() < 20 {
            error(
                report,
                "6.2.3.2",
                "OutputIntent ICC profile too short to contain a valid header (< 20 bytes)",
            );
            continue;
        }
        // Bytes 0–3: declared profile size (big-endian u32). A mismatch means the
        // ICC profile data is corrupt; veraPDF reports this under §6.2.3.2. (#467)
        // Per ICC.1:2004 §6.1, bytes 0–3 are the profile size; bytes 4–7 are CMM type.
        let declared_size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if declared_size != data.len() {
            error(
                report,
                "6.2.3.2",
                format!(
                    "OutputIntent ICC profile declared size {} does not match actual size {}",
                    declared_size,
                    data.len()
                ),
            );
        }
        // Bytes 16–19: color space signature.
        let cs_sig = &data[16..20];
        if !VALID_SIGNATURES.contains(&cs_sig) {
            let sig_str = std::str::from_utf8(cs_sig).unwrap_or("????");
            error(
                report,
                "6.2.3.2",
                format!(
                    "OutputIntent ICC profile has unknown color space signature '{}' at bytes 16–19",
                    sig_str
                ),
            );
        }

        // §6.2.3.2: /N in the ICC-based stream dict shall equal the actual number
        // of components in the profile. A mismatch means the OutputIntent is malformed.
        // Fixes #467 (MOZILLA-869065-0 has /N 4 but ICC is RGB = 3 components).
        let icc_components: Option<u32> = match cs_sig {
            b"GRAY" => Some(1),
            b"RGB " | b"Lab " | b"XYZ " | b"Luv " | b"Yxy " | b"YCbr" | b"HSV " | b"HLS "
            | b"3CLR" => Some(3),
            b"CMYK" | b"4CLR" => Some(4),
            b"2CLR" => Some(2),
            b"5CLR" => Some(5),
            b"6CLR" | b"CMY " => Some(6),
            b"7CLR" => Some(7),
            b"8CLR" => Some(8),
            b"9CLR" => Some(9),
            b"ACLR" => Some(10),
            _ => None,
        };
        if let (Some(icc_n), Some(dict_n)) = (icc_components, stream.dict().get::<i32>(keys::N)) {
            if dict_n as u32 != icc_n {
                error(
                    report,
                    "6.2.3.2",
                    format!(
                        "OutputIntent DestOutputProfile /N {dict_n} does not match \
                         ICC profile color space component count {icc_n}"
                    ),
                );
            }
        }
    }
}

/// Check transparency blending color space is consistent with OutputIntent (§6.6.4).
///
/// When an OutputIntent with a DestOutputProfile exists, any transparency
/// group on a page must use a blending color space that is consistent with
/// the OutputIntent's color space (same number of components).
/// Applies only when an OutputIntent is present; the no-OutputIntent case
/// is already handled by `check_transparency_vs_output_intent`.
pub fn check_transparency_blending_vs_output_intent(
    pdf: &Pdf,
    part: u8,
    report: &mut ComplianceReport,
) {
    let Some(profile_components) = output_intent_profile_components(pdf) else {
        return; // No OutputIntent profile — other checks handle this
    };
    if profile_components == 0 {
        return; // Unknown color space in profile — caught by icc_signature check
    }

    // §6.6.4 is PDF/A-1 clause; PDF/A-2/3 uses §6.2.10 for blending CS
    let rule = if part == 1 { "6.6.4" } else { "6.2.10" };

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(group) = page_dict.get::<Dict<'_>>(b"Group" as &[u8]) else {
            continue;
        };
        let is_transparency = group
            .get::<Name>(keys::S)
            .is_some_and(|s| s.as_ref() == b"Transparency");
        if !is_transparency {
            continue;
        }
        // Check /CS entry for explicit device color space
        if let Some(cs) = group.get::<Name>(keys::CS) {
            let cs_bytes = cs.as_ref();
            let group_components: Option<u32> = match cs_bytes {
                b"DeviceRGB" => Some(3),
                b"DeviceCMYK" => Some(4),
                b"DeviceGray" => Some(1),
                _ => None,
            };
            if let Some(n) = group_components {
                if n != profile_components {
                    error_at(
                        report,
                        rule,
                        format!(
                            "Transparency blending CS '{}' ({} components) is inconsistent \
                             with OutputIntent profile ({} components)",
                            std::str::from_utf8(cs_bytes).unwrap_or("?"),
                            n,
                            profile_components
                        ),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }
}

/// Check multiple OutputIntents have identical profiles (§6.6.1 for PDF/A-1, §6.2.2 for PDF/A-2/3).
///
/// When multiple OutputIntents each carry a DestOutputProfile the profiles
/// must be identical (same ICC data).  Uses a byte-level prefix comparison
/// of the first 64 bytes to avoid decompressing full profiles twice.
pub fn check_output_intent_consistency_pdfa(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else { return };
    let Some(intents) = cat.get::<Array<'_>>(keys::OUTPUT_INTENTS) else {
        return;
    };

    // ISO 19005 requires all OutputIntents to use the SAME indirect object for
    // DestOutputProfile. veraPDF checks `sameOutputProfileIndirect == true`, meaning
    // the object numbers must match — content comparison is insufficient because
    // `get::<Stream>()` returns None for indirect refs. Use get_ref() instead.
    // (#FN-6.2.3)
    let mut refs: Vec<Option<ObjRef>> = Vec::new();
    for intent in intents.iter::<Dict<'_>>() {
        if intent.contains_key(keys::DEST_OUTPUT_PROFILE) {
            // get_ref() returns the indirect object reference without resolving it
            refs.push(intent.get_ref(keys::DEST_OUTPUT_PROFILE));
        }
    }

    if refs.len() > 1 {
        let first = refs[0];
        if refs.iter().any(|r| *r != first) {
            // §6.6.1 in PDF/A-1, §6.2.2 in PDF/A-2/3, §6.2.3 in PDF/A-4
            let rule = match part {
                1 => "6.6.1",
                4 => "6.2.3",
                _ => "6.2.2",
            };
            error(
                report,
                rule,
                "Multiple OutputIntents have different DestOutputProfile indirect objects",
            );
        }
    }
}

/// Check embedded file streams have /Type /EmbeddedFile (§6.1.7, §6.1.7.1, §6.9).
///
/// For PDF/A-4 the violation falls under §6.9 (embedded file requirements).
/// For earlier parts it is §6.1.7.1. Fixes §6.9 FNs on PDF/A-4 files. (#FN-6.9)
pub fn check_embedded_file_streams(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(names) = cat.get::<Dict<'_>>(keys::NAMES) else {
        return;
    };
    // Rule for missing /Type /EmbeddedFile: §6.9 for PDF/A-4, §6.1.7.1 for earlier.
    let rule = if part == 4 { "6.9" } else { "6.1.7.1" };
    if let Some(ef_tree) = names.get::<Dict<'_>>(keys::EMBEDDED_FILES) {
        walk_name_tree_filespec(&ef_tree, rule, report);
    }
}

fn walk_name_tree_filespec(node: &Dict<'_>, rule: &str, report: &mut ComplianceReport) {
    if let Some(names_arr) = node.get::<Array<'_>>(keys::NAMES) {
        let items: Vec<Object<'_>> = names_arr.iter::<Object<'_>>().collect();
        for chunk in items.chunks(2) {
            if chunk.len() == 2 {
                if let Object::Dict(ref fs) = chunk[1] {
                    if let Some(ef) = fs.get::<Dict<'_>>(keys::EF) {
                        if let Some(stream) = ef.get::<Stream<'_>>(keys::F) {
                            let sd = stream.dict();
                            let ok = sd
                                .get::<Name>(keys::TYPE)
                                .is_some_and(|t| t.as_ref() == b"EmbeddedFile");
                            if !ok {
                                error(
                                    report,
                                    rule,
                                    "EmbeddedFile stream missing /Type /EmbeddedFile",
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(kids) = node.get::<Array<'_>>(keys::KIDS) {
        for kid in kids.iter::<Dict<'_>>() {
            walk_name_tree_filespec(&kid, rule, report);
        }
    }
}

/// Check stream dicts for external file reference keys (§6.1.7.1 test 3).
///
/// Stream dictionaries must not contain /F, /FFilter, or /FDecodeParms keys
/// (these reference external files, forbidden in PDF/A).
pub fn check_stream_external_refs(pdf: &Pdf, report: &mut ComplianceReport) {
    let cache = ObjectCache::new(pdf);
    check_stream_external_refs_cached(&cache, report);
}

/// Cached version using pre-collected objects.
pub fn check_stream_external_refs_cached(cache: &ObjectCache<'_>, report: &mut ComplianceReport) {
    for obj in cache.iter() {
        if let Object::Stream(s) = obj {
            let dict = s.dict();
            // /FFilter and /FDecodeParms are unambiguously external-file keys
            if dict.contains_key(b"FFilter" as &[u8]) {
                error(
                    report,
                    "6.1.7.1",
                    "Stream dictionary contains /FFilter key (external file reference)",
                );
            }
            if dict.contains_key(b"FDecodeParms" as &[u8]) {
                error(
                    report,
                    "6.1.7.1",
                    "Stream dictionary contains /FDecodeParms key (external file reference)",
                );
            }
            // /F as a file specification (string value) in a stream = external reference
            // Skip if /Type is EmbeddedFile (that's legitimate)
            let is_embedded = dict
                .get::<Name>(keys::TYPE)
                .is_some_and(|t| t.as_ref() == b"EmbeddedFile");
            // /F in a stream dict = external file specification, regardless of
            // value type (string path, file spec dict, or indirect reference).
            // Fixes FN on veraPDF 6-1-7-1-t04-fail-a where /F is an indirect
            // reference to a file spec dict. (#FN-6.1.7.1)
            if !is_embedded && dict.contains_key(keys::F) {
                error(
                    report,
                    "6.1.7.1",
                    "Stream dictionary contains /F file specification (external file reference)",
                );
            }
        }
    }
}

/// Check PDF header binary comment and version format (§6.1.2).
pub fn check_file_header(pdf: &Pdf, pdfa_part: u8, report: &mut ComplianceReport) {
    let data = pdf.data().as_ref();
    // Check header starts at offset 0
    if !data.starts_with(b"%PDF-") {
        error(report, "6.1.2", "File does not start with %PDF- header");
        return;
    }
    // Validate version format: %PDF-M.N where M and N are single digits
    if data.len() >= 9 {
        let ver = &data[5..9]; // should be "M.N\n" or similar
        let major_ok = ver[0].is_ascii_digit();
        let dot_ok = ver[1] == b'.';
        let minor_ok = ver[2].is_ascii_digit();
        let end_ok = !ver[3].is_ascii_digit(); // no extra digits
        if !(major_ok && dot_ok && minor_ok && end_ok) {
            let ver_str = std::str::from_utf8(&data[5..data.len().min(12)])
                .unwrap_or("?")
                .trim();
            error(
                report,
                "6.1.2",
                format!("File header version '{ver_str}' does not match %PDF-M.N pattern"),
            );
        }
        // For PDF/A-4: must be PDF 2.0
        if pdfa_part == 4 && !(ver[0] == b'2' && ver[2] == b'0') {
            // Only valid: %PDF-2.0
            let ver_str = std::str::from_utf8(&data[5..data.len().min(12)])
                .unwrap_or("?")
                .trim();
            error(
                report,
                "6.1.2",
                format!("PDF/A-4 requires %PDF-2.0 header, found '{ver_str}'"),
            );
        }
    }
    // Scan the first 512 bytes for a binary comment (% followed by 4+ non-ASCII bytes).
    // PDF/A-1 §6.1.2 requires such a comment near the start of the file to indicate
    // binary content. Some generators add an extra text comment between the %PDF- header
    // and the binary comment — we must not flag those as violations. Fixes #455.
    let scan_region = &data[..data.len().min(512)];
    let has_binary_comment = scan_region
        .windows(6)
        .any(|w| w[0] == b'%' && w[1..5].iter().filter(|&&b| b >= 128).count() >= 4);
    if !has_binary_comment {
        error(
            report,
            "6.1.2",
            "Missing binary comment (% followed by 4+ bytes >= 128) in first 512 bytes",
        );
    }
}

/// Check cross-reference table entry format (§6.1.3).
pub fn check_xref_format(pdf: &Pdf, report: &mut ComplianceReport) {
    let data = pdf.data().as_ref();
    let mut pos = 0;
    while pos < data.len().saturating_sub(4) {
        if &data[pos..pos + 4] == b"xref" {
            let after = pos + 4;
            if after < data.len() && (data[after] == b'\n' || data[after] == b'\r') {
                // §6.1.4 t1: xref subsection header "N M" must use single space.
                check_xref_header_spacing(&data[after..], report);
                if let Some(issue) = validate_xref_section(&data[after..]) {
                    error(report, "6.1.3", issue);
                    return;
                }
            }
        }
        pos += 1;
    }
}

/// Check xref subsection headers for single-space separator (§6.1.4 t1).
fn check_xref_header_spacing(data: &[u8], report: &mut ComplianceReport) {
    let mut pos = 0;
    // Skip initial EOL
    while pos < data.len() && (data[pos] == b'\n' || data[pos] == b'\r') {
        pos += 1;
    }
    while pos < data.len() {
        if data[pos..].starts_with(b"trailer") {
            break;
        }
        // Check if this line is a subsection header (short number + space + number)
        let line_start = pos;
        while pos < data.len() && data[pos] != b'\n' && data[pos] != b'\r' {
            pos += 1;
        }
        let line = &data[line_start..pos];
        // Subsection headers have format "N M" where N and M are numbers.
        // They're shorter than xref entries (which are exactly 18+ bytes).
        if line.len() < 18 && !line.is_empty() {
            // Check for multiple spaces between the two numbers
            if let Some(sp) = line.iter().position(|&b| b == b' ') {
                if sp + 1 < line.len() && line[sp + 1] == b' ' {
                    error(
                        report,
                        "6.1.4",
                        "Cross-reference subsection header has multiple spaces between object number and count",
                    );
                    return;
                }
            }
        }
        // Skip EOL
        while pos < data.len() && (data[pos] == b'\n' || data[pos] == b'\r') {
            pos += 1;
        }
    }
}

fn validate_xref_section(data: &[u8]) -> Option<std::string::String> {
    let mut pos = 0;
    while pos < data.len() && (data[pos] == b'\n' || data[pos] == b'\r') {
        pos += 1;
    }
    while pos < data.len() {
        if data[pos..].starts_with(b"trailer") {
            break;
        }
        while pos < data.len() && data[pos] != b'\n' && data[pos] != b'\r' {
            pos += 1;
        }
        while pos < data.len() && (data[pos] == b'\n' || data[pos] == b'\r') {
            pos += 1;
        }
        // An xref entry starts with exactly 10 digits followed by a space.
        // A subsection header (e.g. "4 2\n") starts with fewer digits before
        // a space, so pos+10 would not be a space — exit inner loop.
        while pos + 18 <= data.len()
            && data[pos..pos + 10].iter().all(|b| b.is_ascii_digit())
            && data[pos + 10] == b' '
        {
            if pos + 17 >= data.len() {
                return Some("Cross-reference entry truncated".into());
            }
            let entry = &data[pos..pos + 18];
            if !entry[..10].iter().all(|b| b.is_ascii_digit()) {
                return Some("Xref offset must be 10 digits".into());
            }
            if entry[10] != b' ' || entry[16] != b' ' {
                return Some("Xref entry spacing invalid".into());
            }
            if !entry[11..16].iter().all(|b| b.is_ascii_digit()) {
                return Some("Xref generation must be 5 digits".into());
            }
            if entry[17] != b'f' && entry[17] != b'n' {
                return Some("Xref entry type must be 'f' or 'n'".into());
            }
            pos += 18;
            while pos < data.len()
                && (data[pos] == b' ' || data[pos] == b'\n' || data[pos] == b'\r')
            {
                pos += 1;
            }
        }
    }
    None
}

/// Deep recursive action scanner (§6.6.1, §6.1.6, §6.1.6.1, §6.1.6.2).
///
/// Follows /Next chains, checks Named/GoToR, scans form field /AA.
pub fn check_actions_deep(pdf: &Pdf, part: u8, rule: &str, report: &mut ComplianceReport) {
    // GoToR (remote GoTo) is NOT forbidden by any PDF/A version. Fixes #455.
    // Named actions are handled separately in check_action_recursive below.
    let mut forbidden: Vec<&[u8]> = vec![
        b"Launch",
        b"Sound",
        b"Movie",
        b"ResetForm",
        b"ImportData",
        keys::JAVA_SCRIPT,
        b"SetState",
        b"NoOp",
        b"NOP",
    ];
    if part >= 2 {
        forbidden.extend_from_slice(&[
            b"Hide",
            b"SetOCGState",
            b"Rendition",
            b"Trans",
            b"GoTo3DView",
        ]);
    }

    if let Some(cat) = catalog(pdf) {
        if let Some(action) = cat.get::<Dict<'_>>(keys::OPEN_ACTION) {
            check_action_recursive(&action, &forbidden, rule, "catalog OpenAction", report);
        }
        if let Some(aa) = cat.get::<Dict<'_>>(keys::AA) {
            check_aa_triggers(&aa, &forbidden, rule, "catalog", report);
        }
        if let Some(acroform) = cat.get::<Dict<'_>>(keys::ACRO_FORM) {
            if let Some(fields) = acroform.get::<Array<'_>>(keys::FIELDS) {
                check_form_fields_actions(&fields, &forbidden, rule, report);
            }
        }
        // Check outline (bookmark) actions
        if let Some(outlines) = cat.get::<Dict<'_>>(keys::OUTLINES) {
            check_outline_actions(&outlines, &forbidden, rule, report, 0);
        }
        // §6.6.2.3.1 / §6.5.1 — check catalog /Names/JavaScript name tree.
        // A document may define named JavaScript scripts in the catalog's /Names
        // dict even without referencing them from actions. veraPDF flags ANY
        // JavaScript presence under §6.6.2.3.1 regardless of where it appears.
        // ("fail-c" variant of the veraPDF test suite tests this location.)
        // PDF/A-2/3: veraPDF emits "6.6.2.3.1" specifically for /Names/JavaScript, not
        // the generic action rule "6.5.1" — use the correct sub-clause. (#FN-6.6.2.3.1)
        let js_names_rule = if matches!(part, 2 | 3) {
            "6.6.2.3.1"
        } else {
            rule
        };
        if let Some(names) = cat.get::<Dict<'_>>(keys::NAMES) {
            if names.get::<Object<'_>>(keys::JAVA_SCRIPT).is_some() {
                error_at(
                    report,
                    js_names_rule,
                    "Catalog /Names/JavaScript present (named JavaScript objects forbidden)",
                    "catalog Names".to_string(),
                );
            }
        }
    }

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let loc = format!("page {}", page_idx + 1);
        if let Some(aa) = page_dict.get::<Dict<'_>>(keys::AA) {
            error_at(report, "6.1.6.1", "Page-level /AA present", loc.clone());
            check_aa_triggers(&aa, &forbidden, rule, &loc, report);
        }
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for (i, annot) in annots.iter::<Dict<'_>>().enumerate() {
            let aloc = format!("page {} annot {}", page_idx + 1, i + 1);
            if let Some(action) = annot.get::<Dict<'_>>(keys::A) {
                check_action_recursive(&action, &forbidden, rule, &aloc, report);
            }
            if let Some(aa) = annot.get::<Dict<'_>>(keys::AA) {
                // Use the main forbidden-action rule for action TYPE violations within /AA.
                // "6.1.6.1"/"6.1.6.2" are /AA *presence* rules (handled by the
                // supplementary scan in pdfa.rs). Using them here caused FNs where veraPDF
                // reports the action-type clause (§6.5.1/§6.6.1) but we emitted the
                // /AA-presence clause. (#FN-6.5.1)
                check_aa_triggers(&aa, &forbidden, rule, &aloc, report);
            }
        }
    }
}

fn check_aa_triggers(
    aa: &Dict<'_>,
    forbidden: &[&[u8]],
    rule: &str,
    location: &str,
    report: &mut ComplianceReport,
) {
    for (trigger, _) in aa.entries() {
        if let Some(action) = aa.get::<Dict<'_>>(trigger.as_ref()) {
            let tname = std::str::from_utf8(trigger.as_ref()).unwrap_or("?");
            let loc = format!("{location} AA/{tname}");
            check_action_recursive(&action, forbidden, rule, &loc, report);
        }
    }
}

fn check_form_fields_actions(
    fields: &Array<'_>,
    forbidden: &[&[u8]],
    rule: &str,
    report: &mut ComplianceReport,
) {
    for (idx, field) in fields.iter::<Dict<'_>>().enumerate() {
        let loc = format!("form field {}", idx + 1);
        if let Some(action) = field.get::<Dict<'_>>(keys::A) {
            // Use the main forbidden-action rule (§6.5.1 for PDF/A-2/3, §6.6.1 for PDF/A-1/4).
            // Previously hardcoded "6.1.6.2" (the /AA presence rule) which caused FNs.
            check_action_recursive(&action, forbidden, rule, &loc, report);
        }
        if let Some(aa) = field.get::<Dict<'_>>(keys::AA) {
            check_aa_triggers(&aa, forbidden, rule, &loc, report);
        }
        if let Some(kids) = field.get::<Array<'_>>(keys::KIDS) {
            check_form_fields_actions(&kids, forbidden, rule, report);
        }
    }
}

fn check_action_recursive(
    action: &Dict<'_>,
    forbidden: &[&[u8]],
    rule: &str,
    location: &str,
    report: &mut ComplianceReport,
) {
    if let Some(s) = action.get::<Name>(keys::S) {
        let bytes = s.as_ref();
        if bytes == b"Named" {
            // Named actions are allowed only for the four navigation destinations.
            // PDF/A-1b §6.6.1: only NextPage, PrevPage, FirstPage, LastPage.
            // Flagging ALL Named actions was causing false positives. Fixes #455.
            const ALLOWED_NAMED: &[&[u8]] = &[b"NextPage", b"PrevPage", b"FirstPage", b"LastPage"];
            if let Some(n) = action.get::<Name>(keys::N) {
                if !ALLOWED_NAMED.contains(&n.as_ref()) {
                    let n_str = std::str::from_utf8(n.as_ref()).unwrap_or("?");
                    error_at(
                        report,
                        rule,
                        format!("Forbidden Named action: {n_str}"),
                        location.to_string(),
                    );
                }
            }
        } else if forbidden.contains(&bytes) {
            let name = std::str::from_utf8(bytes).unwrap_or("?");
            error_at(
                report,
                rule,
                format!("Forbidden action type: {name}"),
                location.to_string(),
            );
        }
    } else if action.contains_key(keys::S) {
        // /S key exists but is not a Name (e.g. a string literal or null value).
        // veraPDF treats this as "Unknown or not permitted Action type null".
        // ISO 19005-2 §6.5.1 / PDF/A-1/4 §6.6.1 require /S to be one of the
        // allowed action name values. A non-Name /S is never in the allowed set.
        error_at(
            report,
            rule,
            "Unknown action type (non-Name /S value)",
            location.to_string(),
        );
    }
    if let Some(next) = action.get::<Dict<'_>>(b"Next" as &[u8]) {
        check_action_recursive(&next, forbidden, rule, location, report);
    }
    if let Some(next_arr) = action.get::<Array<'_>>(b"Next" as &[u8]) {
        for next_a in next_arr.iter::<Dict<'_>>() {
            check_action_recursive(&next_a, forbidden, rule, location, report);
        }
    }
}

/// Walk outline tree checking for forbidden actions.
fn check_outline_actions(
    item: &Dict<'_>,
    forbidden: &[&[u8]],
    rule: &str,
    report: &mut ComplianceReport,
    depth: usize,
) {
    if depth > 100 {
        return; // Prevent infinite loops in circular outline trees
    }
    if let Some(action) = item.get::<Dict<'_>>(keys::A) {
        check_action_recursive(&action, forbidden, rule, "outline item", report);
    }
    // Walk children: First → Next chain
    if let Some(first) = item.get::<Dict<'_>>(keys::FIRST) {
        check_outline_actions(&first, forbidden, rule, report, depth + 1);
    }
    if let Some(next) = item.get::<Dict<'_>>(keys::NEXT) {
        check_outline_actions(&next, forbidden, rule, report, depth + 1);
    }
}

/// Check Form XObjects have required /BBox (§6.1.10).
pub fn check_form_xobject_geometry(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) else {
            continue;
        };
        let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) else {
            continue;
        };
        for (name, _) in xobj_dict.entries() {
            let Some(stream) = xobj_dict.get::<Stream<'_>>(name.as_ref()) else {
                continue;
            };
            let dict = stream.dict();
            let is_form = dict
                .get::<Name>(keys::SUBTYPE)
                .is_some_and(|s| s.as_ref() == b"Form");
            if !is_form {
                continue;
            }
            if dict.get::<Array<'_>>(keys::BBOX).is_none() {
                let xn = std::str::from_utf8(name.as_ref()).unwrap_or("?");
                error_at(
                    report,
                    "6.1.10",
                    format!("Form XObject {xn} missing required /BBox"),
                    format!("page {}", page_idx + 1),
                );
            }
        }
    }
}

/// Check optional content restrictions (§6.1.11).
pub fn check_optional_content(pdf: &Pdf, pdfa_part: u8, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(ocprops) = cat.get::<Dict<'_>>(keys::OCPROPERTIES) else {
        return;
    };
    if pdfa_part == 1 {
        // §6.1.13 (ISO 19005-1) forbids OCProperties in the catalog.
        // Internal rule "6.1.13-ocprops" is remapped to "6.1.13" for PDF/A-1 so it
        // doesn't get caught by the general (1,"6.1.13")=>"6.1.12" remap. (#FN-6.1.13)
        error(
            report,
            "6.1.13-ocprops",
            "Optional content (OCProperties) forbidden in PDF/A-1",
        );
        return;
    }
    // §6.6.4.3: /AS entries must not contain Export or Print events.
    let oc_rule = if pdfa_part == 4 { "6.10" } else { "6.6.4" };
    if let Some(as_arr) = ocprops.get::<Array<'_>>(keys::AS) {
        for as_dict in as_arr.iter::<Dict<'_>>() {
            if let Some(event) = as_dict.get::<Name>(b"Event" as &[u8]) {
                let evt = event.as_ref();
                if evt == b"Export" || evt == b"Print" {
                    let e = std::str::from_utf8(evt).unwrap_or("?");
                    error(
                        report,
                        oc_rule,
                        format!("OCProperties /AS entry uses forbidden event '{e}' (only 'View' allowed)"),
                    );
                }
            }
        }
    }
    // §6.6.4 (PDF/A-2/3) / §6.10 (PDF/A-4): OCG/config dicts must have /Name entry.
    // Also check individual OCG dictionaries in OCProperties/OCGs array — each
    // OCG must have /Name per §6.6.4.1 (Fixes #439 false-negative cluster).
    if pdfa_part >= 2 {
        let rule = if pdfa_part == 4 { "6.10" } else { "6.6.4" };

        // Check each individual OCG dictionary for required /Name.
        if let Some(ocgs) = ocprops.get::<Array<'_>>(b"OCGs" as &[u8]) {
            for (idx, ocg) in ocgs.iter::<Dict<'_>>().enumerate() {
                if ocg.get::<Object<'_>>(keys::NAME).is_none() {
                    error(
                        report,
                        rule,
                        format!("OCG dictionary {idx} missing required /Name entry"),
                    );
                }
            }
        }

        // Check default OCG configuration dictionary.
        if let Some(d_dict) = ocprops.get::<Dict<'_>>(b"D" as &[u8]) {
            if d_dict.get::<Object<'_>>(keys::NAME).is_none() {
                error(
                    report,
                    rule,
                    "Default OCG configuration dictionary missing /Name entry",
                );
            }
            // §6.6.4.2: /BaseState in default config must be /ON or /OFF (not /Unchanged).
            if let Some(bs) = d_dict.get::<Name>(b"BaseState" as &[u8]) {
                if bs.as_ref() == b"Unchanged" {
                    error(
                        report,
                        rule,
                        "Default OCG configuration /BaseState must not be /Unchanged",
                    );
                }
            }
        }
        if let Some(configs) = ocprops.get::<Array<'_>>(b"Configs" as &[u8]) {
            // Check each Configs entry has a /Name and that names are unique.
            // /Name in OCG config dicts is a text string (not a PDF name token).
            // Duplicate /Name values in Configs violate §6.9 T2 (veraPDF testNum=2). (#FN-6.9)
            let mut seen_names: std::collections::HashSet<Vec<u8>> =
                std::collections::HashSet::new();
            for (idx, cfg) in configs.iter::<Dict<'_>>().enumerate() {
                // Use Object to capture both Name and String /Name values.
                if let Some(name_obj) = cfg.get::<Object<'_>>(keys::NAME) {
                    // Extract the raw bytes for comparison.
                    let name_bytes: Vec<u8> = match name_obj {
                        Object::Name(n) => n.as_ref().to_vec(),
                        Object::String(s) => s.as_ref().to_vec(),
                        _ => format!("{:?}", name_obj).into_bytes(),
                    };
                    if !seen_names.insert(name_bytes.clone()) {
                        let dup = String::from_utf8_lossy(&name_bytes);
                        error(
                            report,
                            rule,
                            format!("OCG Configs[{idx}] /Name '{dup}' is a duplicate (names must be unique)"),
                        );
                    }
                } else {
                    error(
                        report,
                        rule,
                        format!("OCG configuration {idx} missing /Name entry"),
                    );
                }
            }
        }
    }
}

/// Check linearization dictionary (§6.1.5).
pub fn check_linearization(pdf: &Pdf, report: &mut ComplianceReport) {
    if let Some(Object::Dict(dict)) = pdf.objects().into_iter().next().as_ref() {
        if dict.get::<Object<'_>>(keys::LINEARIZED).is_some()
            && dict.get::<Object<'_>>(keys::LENGTH).is_none()
        {
            warning(report, "6.1.5", "Linearization dict missing /L");
        }
    }
}

/// String objects must not exceed 65535 bytes.
#[allow(dead_code)]
fn check_string_lengths(pdf: &Pdf, rule: &str, report: &mut ComplianceReport) {
    check_string_lengths_cached(&ObjectCache::new(pdf), rule, report);
}

/// Array objects must not exceed 8191 elements.
#[allow(dead_code)]
fn check_array_sizes(pdf: &Pdf, rule: &str, report: &mut ComplianceReport) {
    check_array_sizes_cached(&ObjectCache::new(pdf), rule, report);
}

/// Dictionary objects must not exceed 4095 entries.
#[allow(dead_code)]
fn check_dict_sizes(pdf: &Pdf, rule: &str, report: &mut ComplianceReport) {
    check_dict_sizes_cached(&ObjectCache::new(pdf), rule, report);
}

/// Graphics state nesting (q/Q) must not exceed 28 levels.
fn check_gs_nesting_depth(
    content: &[u8],
    page_idx: usize,
    rule: &str,
    report: &mut ComplianceReport,
) {
    let text = std::string::String::from_utf8_lossy(content);
    let mut depth: i32 = 0;
    let mut max_depth: i32 = 0;

    for token in text.split_ascii_whitespace() {
        if token == "q" {
            depth += 1;
            if depth > max_depth {
                max_depth = depth;
            }
        } else if token == "Q" {
            depth -= 1;
        }
    }

    if max_depth > 28 {
        error_at(
            report,
            rule,
            format!(
                "Graphics state nesting depth {} exceeds maximum 28",
                max_depth
            ),
            format!("page {}", page_idx + 1),
        );
    }
}

/// Minimum positive normalized float (IEEE 754 single-precision).
///
/// PDF/A §6.1.12/§6.1.13 prohibits non-zero values with absolute value below
/// this threshold (subnormal / denormal floats). Value ≈ 1.175494e-38.
const MIN_POSITIVE_REAL: f64 = 1.175_494e-38;

/// Scan content stream for numeric tokens exceeding max or below min positive (§6.1.12).
///
/// Detects both overflow (> 32767) and subnormal floats (0 < |v| < 1.175e-38).
fn scan_content_stream_reals(content: &[u8], max: f64) -> bool {
    let text = std::string::String::from_utf8_lossy(content);
    for token in text.split_ascii_whitespace() {
        if token.starts_with(|c: char| c.is_ascii_alphabetic() || c == '\'' || c == '"')
            || token.starts_with('/')
        {
            continue;
        }
        if let Ok(val) = token.parse::<f64>() {
            if val.abs() > max {
                return true;
            }
            // Subnormal: non-zero value below the minimum normalized positive float
            if val != 0.0 && val.abs() < MIN_POSITIVE_REAL {
                return true;
            }
        }
    }
    false
}

// ─── Iteration 11: Deeper 6.2.x fixes ───────────────────────────────────────

/// Check Image XObject color spaces (§6.2.4.3).
///
/// Image XObjects with direct device color spaces (DeviceRGB, DeviceCMYK,
/// DeviceGray) as their /ColorSpace key violate 6.2.4.3 unless a Default
/// color space or OutputIntent is present.
pub fn check_image_xobject_colorspaces(pdf: &Pdf, report: &mut ComplianceReport) {
    let profile = output_intent_profile_components(pdf);
    let has_intent = profile.is_some();

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let res = page.resources();
        let cs_dict = &res.color_spaces;

        let rgb_ok = cs_dict.get::<Object<'_>>(keys::DEFAULT_RGB).is_some() || profile == Some(3);
        let cmyk_ok = cs_dict.get::<Object<'_>>(keys::DEFAULT_CMYK).is_some() || profile == Some(4);
        let gray_ok = cs_dict.get::<Object<'_>>(keys::DEFAULT_GRAY).is_some() || has_intent;

        let xobj_dict = &res.x_objects;
        let loc = format!("page {}", page_idx + 1);
        // Check image XObjects via resolved resources
        check_image_cs_in_xobjects(xobj_dict, rgb_ok, cmyk_ok, gray_ok, &loc, report);

        // Also scan annotation appearance streams for image XObjects
        if let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) {
            for annot in annots.iter::<Dict<'_>>() {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    for key in [b"N" as &[u8], b"R", b"D"] {
                        if let Some(ap_stream) = ap.get::<Stream<'_>>(key) {
                            let ap_dict = ap_stream.dict();
                            if let Some(ap_res) = ap_dict.get::<Dict<'_>>(keys::RESOURCES) {
                                check_image_cs_in_resources(
                                    &ap_res, rgb_ok, cmyk_ok, gray_ok, &loc, report,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Check page-level Group color spaces for device CS violations (§6.2.4.3).
///
/// Pages with a /Group dict (transparency group) may have a /CS entry
/// with a device color space (DeviceRGB, DeviceCMYK, DeviceGray).
pub fn check_page_group_colorspaces(pdf: &Pdf, report: &mut ComplianceReport) {
    let profile = output_intent_profile_components(pdf);
    let has_intent = profile.is_some();
    let xref = pdf.xref();

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(group) = resolve_page_group_dict(page_dict, xref) else {
            continue;
        };
        let cs_res = &page.resources().color_spaces;
        let rgb_ok = cs_res.get::<Object<'_>>(keys::DEFAULT_RGB).is_some() || profile == Some(3);
        let cmyk_ok = cs_res.get::<Object<'_>>(keys::DEFAULT_CMYK).is_some() || profile == Some(4);
        let gray_ok = cs_res.get::<Object<'_>>(keys::DEFAULT_GRAY).is_some() || has_intent;
        let loc = format!("page {} Group", page_idx + 1);

        if let Some(cs) = group.get::<Name>(keys::CS) {
            let cs_bytes = cs.as_ref();
            report_device_cs_name(cs_bytes, rgb_ok, cmyk_ok, gray_ok, &loc, report);
        } else if let Some(cs_arr) = group.get::<Array<'_>>(keys::CS) {
            let kind = extract_base_device_cs_kind(&cs_arr);
            if kind == 1 {
                report_device_cs_name(keys::DEVICE_RGB, rgb_ok, cmyk_ok, gray_ok, &loc, report);
            } else if kind == 2 {
                report_device_cs_name(b"DeviceCMYK", rgb_ok, cmyk_ok, gray_ok, &loc, report);
            } else if kind == 3 {
                report_device_cs_name(b"DeviceGray", rgb_ok, cmyk_ok, gray_ok, &loc, report);
            }
        }
    }
}

/// Check multiple OutputIntents have identical profiles (§6.2.2).
///
/// If multiple OutputIntents with DestOutputProfile exist, they must
/// reference the same ICC profile (identical data).
pub fn check_output_intent_consistency(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else { return };
    let Some(intents) = cat.get::<Array<'_>>(keys::OUTPUT_INTENTS) else {
        return;
    };

    let mut profile_hashes: Vec<u64> = Vec::new();
    for intent in intents.iter::<Dict<'_>>() {
        if let Some(profile_stream) = intent.get::<Stream<'_>>(keys::DEST_OUTPUT_PROFILE) {
            if let Ok(data) = profile_stream.decoded() {
                // Simple hash: use length + first/last bytes
                let hash = data.len() as u64
                    ^ (data.first().copied().unwrap_or(0) as u64) << 32
                    ^ (data.last().copied().unwrap_or(0) as u64) << 40
                    ^ (data.get(data.len() / 2).copied().unwrap_or(0) as u64) << 48;
                profile_hashes.push(hash);
            }
        }
    }

    if profile_hashes.len() > 1 {
        let first = profile_hashes[0];
        if profile_hashes.iter().any(|h| *h != first) {
            error(
                report,
                "6.2.2",
                "Multiple OutputIntents have different DestOutputProfile ICC profiles",
            );
        }
    }
}

/// §6.2.2 T2 — Type3 CharProc streams must not inherit resources from the page.
///
/// PDF/A-2/3 §6.2.2 requires that every content stream (including Type3 CharProc
/// streams) declares all named resources it uses in its own (or the owning font's)
/// Resources dictionary. CharProcs that use `/Name cs`, `/Name Tf`, `/Name Do`,
/// `/Name gs`, or `/Name sh` without those names being defined in the Type3 font
/// dict's /Resources entry are relying on page-level resource inheritance — which
/// ISO 19005-2/3 §6.2.2 T2 explicitly forbids. Fixes §6.2.2 FNs (#FN-6.2.2).
pub fn check_type3_charproc_resources(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let loc = format!("page {}", page_idx + 1);
        let fonts = &page.resources().fonts;
        for (fname, _) in fonts.entries() {
            let font_dict_opt: Option<Dict<'_>> =
                fonts.get::<Dict<'_>>(fname.as_ref()).or_else(|| {
                    fonts
                        .get_ref(fname.as_ref())
                        .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                });
            let Some(font_dict) = font_dict_opt else {
                continue;
            };
            if font_dict
                .get::<Name>(keys::SUBTYPE)
                .is_none_or(|s| s.as_ref() != b"Type3")
            {
                continue;
            }

            // Collect resource names declared in the Type3 font's /Resources dict.
            let font_res_names: std::collections::HashSet<Vec<u8>> = {
                let mut names = std::collections::HashSet::new();
                // Resolve /Resources — may be direct or indirect.
                let res_dict_opt: Option<Dict<'_>> =
                    font_dict.get::<Dict<'_>>(keys::RESOURCES).or_else(|| {
                        font_dict
                            .get_ref(keys::RESOURCES)
                            .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                    });
                if let Some(res) = res_dict_opt {
                    // Collect names from all sub-dicts (ColorSpace, Font, XObject, …).
                    for sub_key in [
                        keys::COLORSPACE,
                        keys::FONT,
                        keys::XOBJECT,
                        keys::EXT_G_STATE,
                        keys::SHADING,
                        keys::PATTERN,
                    ] {
                        if let Some(sub) = res.get::<Dict<'_>>(sub_key) {
                            for (n, _) in sub.entries() {
                                names.insert(n.as_ref().to_vec());
                            }
                        }
                    }
                }
                names
            };

            // /CharProcs may be a direct dict or an indirect reference.
            let charprocs_opt: Option<Dict<'_>> = font_dict
                .get::<Dict<'_>>(b"CharProcs" as &[u8])
                .or_else(|| {
                    font_dict
                        .get_ref(b"CharProcs" as &[u8])
                        .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                });
            let Some(charprocs) = charprocs_opt else {
                continue;
            };
            let fstr = std::str::from_utf8(fname.as_ref()).unwrap_or("?");

            for (cname, _) in charprocs.entries() {
                let cp_stream_opt: Option<Stream<'_>> =
                    charprocs.get::<Stream<'_>>(cname.as_ref()).or_else(|| {
                        charprocs
                            .get_ref(cname.as_ref())
                            .and_then(|r| xref.get::<Stream<'_>>(r.into()))
                    });
                let Some(cp_stream) = cp_stream_opt else {
                    continue;
                };
                let Ok(content) = cp_stream.decoded() else {
                    continue;
                };

                // Scan for /Name <resource-op> pairs where the name is not declared
                // in the font's /Resources dict. These are inherited from the page
                // and violate §6.2.2 T2.
                let resource_ops: &[&str] = &["cs", "CS", "Tf", "Do", "gs", "sh"];
                let text = String::from_utf8_lossy(&content);
                let tokens: Vec<&str> = text.split_ascii_whitespace().collect();
                let cstr = std::str::from_utf8(cname.as_ref()).unwrap_or("?");
                for i in 1..tokens.len() {
                    if !resource_ops.contains(&tokens[i]) {
                        continue;
                    }
                    let Some(raw_name) = tokens[i - 1].strip_prefix('/') else {
                        continue;
                    };
                    if !font_res_names.contains(raw_name.as_bytes()) {
                        error_at(
                            report,
                            "6.2.2",
                            format!(
                                "Type3 font {fstr} CharProc {cstr}: \
                                 resource /{raw_name} used via '{op}' not declared in \
                                 font /Resources (inherited from page — §6.2.2 T2)",
                                op = tokens[i]
                            ),
                            loc.clone(),
                        );
                        // Report each CharProc at most once.
                        break;
                    }
                }
            }
        }
    }
}

/// Check content stream operators are valid PDF operators (§6.2.10).
///
/// Operators not defined in PDF Reference are forbidden even if
/// bracketed by BX/EX compatibility markers.
pub fn check_undefined_operators(pdf: &Pdf, report: &mut ComplianceReport) {
    // All valid PDF content stream operators — built as HashSet once before the
    // page loop so scan_for_undefined_ops can do O(1) lookups. (#perf)
    let valid_ops: std::collections::HashSet<&'static str> = [
        // General graphics state
        "w", "J", "j", "M", "d", "ri", "i", "gs", // Special graphics state
        "q", "Q", "cm", // Path construction
        "m", "l", "c", "v", "y", "h", "re", // Path painting
        "S", "s", "f", "F", "f*", "B", "B*", "b", "b*", "n", // Clipping paths
        "W", "W*", // Text objects
        "BT", "ET", // Text state
        "Tc", "Tw", "Tz", "TL", "Tf", "Tr", "Ts", // Text positioning
        "Td", "TD", "Tm", "T*", // Text showing
        "Tj", "TJ", "'", "\"", // Type 3 fonts
        "d0", "d1", // Color
        "CS", "cs", "SC", "SCN", "sc", "scn", "G", "g", "RG", "rg", "K", "k",  // Shading
        "sh", // Inline images
        "BI", "ID", "EI", // XObjects
        "Do", // Marked content
        "MP", "DP", "BMC", "BDC", "EMC", // Compatibility
        "BX", "EX",
    ]
    .iter()
    .copied()
    .collect();

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let loc = format!("page {}", page_idx + 1);
        if let Some(content) = page.page_stream() {
            if content.len() > MAX_CONTENT_STREAM_SCAN_SIZE {
                continue;
            }
            if scan_for_undefined_ops(content, &valid_ops) {
                error_at(
                    report,
                    "6.2.2",
                    "Content stream contains undefined operator",
                    loc.clone(),
                );
            }
        }
        // Also scan annotation appearance streams
        let page_dict = page.raw();
        if let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) {
            for annot in annots.iter::<Dict<'_>>() {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    if let Some(n_stream) = ap.get::<Stream<'_>>(keys::N) {
                        if let Ok(decoded) = n_stream.decoded() {
                            if scan_for_undefined_ops(&decoded, &valid_ops) {
                                error_at(
                                    report,
                                    "6.2.2",
                                    "Annotation appearance stream contains undefined operator",
                                    loc.clone(),
                                );
                            }
                        }
                    }
                }
            }
        }
        // Scan Form XObjects on the page
        let xobjects = &page.resources().x_objects;
        for (name, _) in xobjects.entries() {
            let Some(stream) = xobjects.get::<Stream<'_>>(name.as_ref()) else {
                continue;
            };
            let is_form = stream
                .dict()
                .get::<Name>(keys::SUBTYPE)
                .is_some_and(|s| s.as_ref() == b"Form");
            if !is_form {
                continue;
            }
            if let Ok(decoded) = stream.decoded() {
                if scan_for_undefined_ops(&decoded, &valid_ops) {
                    let xn = std::str::from_utf8(name.as_ref()).unwrap_or("?");
                    error_at(
                        report,
                        "6.2.2",
                        format!("Form XObject {xn} contains undefined operator"),
                        loc.clone(),
                    );
                }
            }
        }
    }
}

/// Scan a content stream for undefined operators.
///
/// `valid_ops` is a `HashSet` for O(1) lookup per token — using a slice here
/// was O(V × T) per content stream where V≈60 and T can be thousands of
/// tokens on a dense page.  Callers build the HashSet once before the page
/// loop and pass a reference in, so the set is constructed at most once per
/// compliance check. (#perf)
fn scan_for_undefined_ops(
    content: &[u8],
    valid_ops: &std::collections::HashSet<&'static str>,
) -> bool {
    // Pre-process: replace the contents of PDF string literals `(...)` with
    // spaces so that embedded text (e.g. `(text with TrueType font)`) is not
    // tokenised into words that look like undefined operators. (#FP-6.2.10)
    let stripped = strip_string_literal_content(content);
    let text = String::from_utf8_lossy(&stripped);
    let mut in_inline_image = false;
    for token in text.split_ascii_whitespace() {
        // Skip inline image data
        if token == "ID" {
            in_inline_image = true;
            continue;
        }
        if token == "EI" {
            in_inline_image = false;
            continue;
        }
        if in_inline_image {
            continue;
        }

        // Skip operands (numbers, names, strings, arrays, dicts)
        if token.starts_with('/')
            || token.starts_with('(')
            || token.starts_with(')')
            || token.starts_with('<')
            || token.starts_with('[')
            || token == "true"
            || token == "false"
            || token == "null"
        {
            continue;
        }
        // Skip numeric operands
        if token
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'+')
            && !token.is_empty()
        {
            continue;
        }
        // Check if it's a valid operator — O(1) HashSet lookup
        if !token.is_empty()
            && token
                .bytes()
                .all(|b| b.is_ascii_alphabetic() || b == b'*' || b == b'\'' || b == b'"')
            && !valid_ops.contains(token)
        {
            return true;
        }
    }
    false
}

/// Replace the contents of PDF string literals `(...)` with spaces so that
/// embedded text cannot be confused with operator tokens during scanning.
/// Handles escape sequences (`\n`, `\\`, `\(`, `\)`) and nested parentheses.
fn strip_string_literal_content(content: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len());
    let mut i = 0;
    let mut depth = 0u32;
    while i < content.len() {
        match content[i] {
            b'\\' if depth > 0 => {
                // Escape inside a string: skip both bytes, emit spaces
                out.push(b' ');
                if i + 1 < content.len() {
                    out.push(b' ');
                    i += 2;
                } else {
                    i += 1;
                }
            }
            b'(' if depth == 0 => {
                depth = 1;
                out.push(b'(');
                i += 1;
            }
            b'(' => {
                depth += 1;
                out.push(b' ');
                i += 1;
            }
            b')' if depth == 1 => {
                depth = 0;
                out.push(b')');
                i += 1;
            }
            b')' if depth > 1 => {
                depth -= 1;
                out.push(b' ');
                i += 1;
            }
            _ if depth > 0 => {
                // Inside a string literal: replace with space
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

/// Check transparency groups on pages without OutputIntent (§6.2.9).
///
/// Two checks:
/// 1. If no OutputIntent, pages with transparency groups must not use
///    device color spaces in the group.
/// 2. Pages that use transparency features (via ExtGState) must have a
///    /Group entry when no OutputIntent is present.
pub fn check_transparency_vs_output_intent(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    let has_oi = has_output_intent(pdf);
    // PDF/A-4 merges transparency checks into 6.2.9; parts 1-3 use internal
    // "6.2.10-tgroup" (remapped to §6.4 for PDF/A-1, §6.2.10 for PDF/A-2/3).
    // Using a distinct tag avoids colliding with halftone "6.2.10" violations
    // from check_halftone_in_extgstate, which veraPDF reports as §6.2.10 even
    // in PDF/A-1. Fixes §6.2.10 FNs where halftone violations were incorrectly
    // remapped to §6.4. (#FN-6.2.10)
    let page_group_rule = if part == 4 { "6.2.9" } else { "6.2.10-tgroup" };

    let xref = pdf.xref();
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();

        // Resolve /Group dict, handling both inline dicts and indirect references.
        let group_dict: Option<Dict<'_>> = resolve_page_group_dict(page_dict, xref);

        let has_page_group = group_dict
            .as_ref()
            .and_then(|g| g.get::<Name>(keys::S))
            .is_some_and(|s| s.as_ref() == b"Transparency");

        if !has_oi {
            // Check 1: existing transparency group must not use device CS
            if let Some(group) = group_dict.as_ref() {
                if let Some(s) = group.get::<Name>(keys::S) {
                    if s.as_ref() == b"Transparency" {
                        if let Some(cs) = group.get::<Name>(keys::CS) {
                            // CS present as a Name (device CS) — check it's not a device CS
                            let cs_bytes = cs.as_ref();
                            if cs_bytes == keys::DEVICE_RGB
                                || cs_bytes == b"DeviceCMYK"
                                || cs_bytes == b"DeviceGray"
                            {
                                error_at(
                                    report,
                                    page_group_rule,
                                    format!(
                                        "Transparency group uses device CS {} without OutputIntent",
                                        std::str::from_utf8(cs_bytes).unwrap_or("?")
                                    ),
                                    format!("page {}", page_idx + 1),
                                );
                            }
                        } else if group.get::<Array<'_>>(keys::CS).is_none() {
                            // /CS is truly absent (not a Name and not an Array like [/ICCBased …])
                            error_at(
                                report,
                                page_group_rule,
                                "Transparency group without /CS and no OutputIntent",
                                format!("page {}", page_idx + 1),
                            );
                        }
                    }
                }
            }
        }

        // Check 2: pages using transparency features need /Group with CS
        let uses_transparency = page_uses_transparency(page.resources())
            || page_annots_use_transparency(page_dict)
            || page_fonts_use_transparency(page.resources());
        if !has_page_group && uses_transparency {
            error_at(
                report,
                page_group_rule,
                "Page uses transparency but has no /Group entry",
                format!("page {}", page_idx + 1),
            );
        }

        // Check 3: Group exists with S=Transparency but no CS entry
        if has_page_group && !has_oi {
            if let Some(group) = group_dict.as_ref() {
                if group.get::<Name>(keys::CS).is_none()
                    && group.get::<Array<'_>>(keys::CS).is_none()
                {
                    error_at(
                        report,
                        page_group_rule,
                        "Transparency group missing /CS entry and no OutputIntent",
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }
}

/// Check if a page uses transparency features.
///
/// Uses resolved Resources (handles inherited resources correctly).
/// Checks ExtGState (SMask, CA, ca, BM), Image XObjects with /SMask,
/// and recursively checks Form XObject resources.
fn page_uses_transparency(res: &Resources<'_>) -> bool {
    if extgstate_uses_transparency(&res.ext_g_states) {
        return true;
    }
    if xobjects_use_transparency(&res.x_objects) {
        return true;
    }
    if patterns_use_transparency(&res.patterns) {
        return true;
    }
    false
}

/// Check if any ExtGState entry uses transparency features.
fn extgstate_uses_transparency(gs_dict: &Dict<'_>) -> bool {
    for (name, _) in gs_dict.entries() {
        let Some(gs) = gs_dict.get::<Dict<'_>>(name.as_ref()) else {
            continue;
        };
        if let Some(smask) = gs.get::<Object<'_>>(keys::SMASK) {
            match smask {
                Object::Name(n) if n.as_ref() == b"None" => {}
                Object::Name(_) => return true,
                Object::Dict(_) => return true,
                _ => {}
            }
        }
        if let Some(bm) = gs.get::<Name>(keys::BM) {
            let bm_bytes = bm.as_ref();
            if bm_bytes != b"Normal" && bm_bytes != b"Compatible" {
                return true;
            }
        }
        if let Some(Object::Number(ca)) = gs.get::<Object<'_>>(b"CA" as &[u8]) {
            if ca.as_f64() < 1.0 {
                return true;
            }
        }
        if let Some(Object::Number(ca)) = gs.get::<Object<'_>>(b"ca" as &[u8]) {
            if ca.as_f64() < 1.0 {
                return true;
            }
        }
    }
    false
}

/// Check XObjects for transparency: images with /SMask or Form XObjects
/// containing transparency features in their own resources.
fn xobjects_use_transparency(xobj_dict: &Dict<'_>) -> bool {
    for (name, _) in xobj_dict.entries() {
        let Some(stream) = xobj_dict.get::<Stream<'_>>(name.as_ref()) else {
            continue;
        };
        let dict = stream.dict();
        let subtype = dict.get::<Name>(keys::SUBTYPE);
        let st = subtype.as_ref().map(|s| s.as_ref());

        // Image with soft mask = transparency
        if st == Some(keys::IMAGE) && dict.contains_key(keys::SMASK) {
            return true;
        }

        // Form XObject: check its internal resources for transparency
        if st == Some(keys::FORM) {
            // Form XObject with its own transparency Group
            if let Some(group) = dict.get::<Dict<'_>>(b"Group" as &[u8]) {
                if group
                    .get::<Name>(keys::S)
                    .is_some_and(|s| s.as_ref() == b"Transparency")
                {
                    return true;
                }
            }
            // Check Form XObject's own ExtGState resources
            if let Some(form_res) = dict.get::<Dict<'_>>(keys::RESOURCES) {
                if let Some(gs) = form_res.get::<Dict<'_>>(keys::EXT_G_STATE) {
                    if extgstate_uses_transparency(&gs) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if any Pattern resource uses transparency features.
///
/// Tiling patterns can have their own Resources with ExtGState
/// entries that use transparency (SMask, CA, ca, BM).
fn patterns_use_transparency(pat_dict: &Dict<'_>) -> bool {
    for (name, _) in pat_dict.entries() {
        // Tiling patterns are streams, shading patterns are dicts
        if let Some(stream) = pat_dict.get::<Stream<'_>>(name.as_ref()) {
            let dict = stream.dict();
            // Check the pattern's own Resources for transparency
            if let Some(res) = dict.get::<Dict<'_>>(keys::RESOURCES) {
                if let Some(gs) = res.get::<Dict<'_>>(keys::EXT_G_STATE) {
                    if extgstate_uses_transparency(&gs) {
                        return true;
                    }
                }
                if let Some(xobj) = res.get::<Dict<'_>>(keys::XOBJECT) {
                    if xobjects_use_transparency(&xobj) {
                        return true;
                    }
                }
            }
            // Check if the pattern itself has a transparency Group
            if let Some(group) = dict.get::<Dict<'_>>(b"Group" as &[u8]) {
                if group
                    .get::<Name>(keys::S)
                    .is_some_and(|s| s.as_ref() == b"Transparency")
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if page annotations use transparency features.
///
/// Annotations can use transparency via:
/// - /BM (blend mode) key directly in the annotation dict
/// - Appearance streams (/AP /N) that use ExtGState with transparency
fn page_annots_use_transparency(page_dict: &Dict<'_>) -> bool {
    let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
        return false;
    };
    for annot in annots.iter::<Dict<'_>>() {
        // Check /BM directly on the annotation
        if let Some(bm) = annot.get::<Name>(keys::BM) {
            let bm_bytes = bm.as_ref();
            if bm_bytes != b"Normal" && bm_bytes != b"Compatible" {
                return true;
            }
        }
        // Check /CA and /ca on annotation dict
        if let Some(Object::Number(ca)) = annot.get::<Object<'_>>(b"CA" as &[u8]) {
            if ca.as_f64() < 1.0 {
                return true;
            }
        }
        if let Some(Object::Number(ca)) = annot.get::<Object<'_>>(b"ca" as &[u8]) {
            if ca.as_f64() < 1.0 {
                return true;
            }
        }
        // Check appearance stream resources for transparency.
        // /AP /N may be a direct stream OR a dict of appearance states
        // (e.g. /On and /Off for checkboxes). Check both cases.
        if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
            // Collect all AP /N streams to check: direct stream or each state in dict.
            let mut ap_streams: Vec<Stream<'_>> = Vec::new();
            if let Some(n_stream) = ap.get::<Stream<'_>>(keys::N) {
                ap_streams.push(n_stream);
            } else if let Some(n_dict) = ap.get::<Dict<'_>>(keys::N) {
                // Appearance state dict: /On, /Off, /Yes, /No, etc.
                for (state, _) in n_dict.entries() {
                    if let Some(s) = n_dict.get::<Stream<'_>>(state.as_ref()) {
                        ap_streams.push(s);
                    }
                }
            }
            for n_stream in ap_streams {
                let ap_dict = n_stream.dict();
                if let Some(res) = ap_dict.get::<Dict<'_>>(keys::RESOURCES) {
                    if let Some(gs) = res.get::<Dict<'_>>(keys::EXT_G_STATE) {
                        if extgstate_uses_transparency(&gs) {
                            return true;
                        }
                    }
                    // Check XObjects in appearance stream resources
                    if let Some(xobj) = res.get::<Dict<'_>>(keys::XOBJECT) {
                        if xobjects_use_transparency(&xobj) {
                            return true;
                        }
                    }
                }
                // Check if the appearance Form XObject itself has a Transparency group
                if let Some(group) = ap_dict.get::<Dict<'_>>(b"Group" as &[u8]) {
                    if group
                        .get::<Name>(keys::S)
                        .is_some_and(|s| s.as_ref() == b"Transparency")
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if any Type3 font on the page uses transparency in its char proc resources.
fn page_fonts_use_transparency(res: &Resources<'_>) -> bool {
    let fonts = &res.fonts;
    for (name, _) in fonts.entries() {
        let Some(font_dict) = fonts.get::<Dict<'_>>(name.as_ref()) else {
            continue;
        };
        // Only check Type3 fonts
        let is_type3 = font_dict
            .get::<Name>(keys::SUBTYPE)
            .is_some_and(|s| s.as_ref() == b"Type3");
        if !is_type3 {
            continue;
        }
        // Check the font's own Resources for transparency
        if let Some(font_res) = font_dict.get::<Dict<'_>>(keys::RESOURCES) {
            if let Some(gs) = font_res.get::<Dict<'_>>(keys::EXT_G_STATE) {
                if extgstate_uses_transparency(&gs) {
                    return true;
                }
            }
            // Check XObjects in Type3 font resources: Form XObjects with /Group /S
            // /Transparency introduce a transparency group. (#FN-6.2.9 t04-fail-e)
            if let Some(xobj_dict) = font_res.get::<Dict<'_>>(keys::XOBJECT) {
                for (xo_name, _) in xobj_dict.entries() {
                    let Some(xo_stream) = xobj_dict.get::<Stream<'_>>(xo_name.as_ref()) else {
                        continue;
                    };
                    let xo_dict = xo_stream.dict();
                    if xo_dict
                        .get::<Name>(keys::SUBTYPE)
                        .is_some_and(|s| s.as_ref() == b"Form")
                    {
                        if let Some(group) = xo_dict.get::<Dict<'_>>(keys::GROUP) {
                            if group
                                .get::<Name>(keys::S)
                                .is_some_and(|s| s.as_ref() == b"Transparency")
                            {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        // Check CharProcs for embedded resources
        if let Some(char_procs) = font_dict.get::<Dict<'_>>(b"CharProcs" as &[u8]) {
            for (cp_name, _) in char_procs.entries() {
                if let Some(stream) = char_procs.get::<Stream<'_>>(cp_name.as_ref()) {
                    let cp_dict = stream.dict();
                    if let Some(cp_res) = cp_dict.get::<Dict<'_>>(keys::RESOURCES) {
                        if let Some(gs) = cp_res.get::<Dict<'_>>(keys::EXT_G_STATE) {
                            if extgstate_uses_transparency(&gs) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// Check for PostScript XObjects (forbidden in PDF/A, §6.2.9.3).
///
/// veraPDF uses "6.2.9.3" for PDF/A-2/3 (PostScript XObjects specifically),
/// "6.2.7" for PDF/A-1, and "6.2.9" for PDF/A-4.
pub fn check_postscript_xobjects(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    // PDF/A-1: §6.2.7 (PS XObject prohibition; veraPDF uses §6.2.7).
    // PDF/A-2/3: §6.2.9.3 (PostScript XObjects specifically forbidden). (#FN-6.2.9.3)
    // PDF/A-4: §6.2.9 (different structure; exact veraPDF rule TBD).
    let rule = match part {
        1 => "6.2.7",
        2 | 3 => "6.2.9.3",
        _ => "6.2.9",
    };
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let xobjects = &page.resources().x_objects;
        for (name, _) in xobjects.entries() {
            let Some(stream) = xobjects.get::<Stream<'_>>(name.as_ref()) else {
                continue;
            };
            let subtype = stream.dict().get::<Name>(keys::SUBTYPE);
            if subtype.is_some_and(|s| s.as_ref() == b"PS") {
                let xn = std::str::from_utf8(name.as_ref()).unwrap_or("?");
                error_at(
                    report,
                    rule,
                    format!("PostScript XObject {xn} is not allowed in PDF/A"),
                    format!("page {}", page_idx + 1),
                );
            }
        }
    }
}

/// §6.2.11.8: detect .notdef glyph references in content stream text operators.
///
/// Scans page content streams for Tj/TJ operators using hex strings that
/// encode character code 0 (simple fonts) or CID 0 (CID fonts).
/// Code/CID 0 always maps to the .notdef glyph.
///
/// For pages that contain Type0 (CID) fonts, 2-byte hex strings like <0041>
/// represent a single CID (65 = 'A'), not two 1-byte codes. In that mode
/// only <0000> counts as notdef. Pages with only simple fonts use 1-byte logic
/// where any 0x00 byte is code 0 = notdef. (#FP-6.2.11.8)
pub fn check_notdef_glyph_reference(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        // Determine if any font on this page is a Type0 (CID) font. Type0 fonts use
        // 2-byte character codes; a 0x00 high byte does NOT mean notdef unless the
        // full 2-byte CID is 0x0000. Collect names first to avoid borrow conflicts.
        // (#FP-6.2.11.8)
        let font_names: Vec<Vec<u8>> = page
            .resources()
            .fonts
            .entries()
            .map(|(n, _)| n.as_ref().to_vec())
            .collect();
        let has_type0_font = font_names.iter().any(|name| {
            let name_slice: &[u8] = name.as_slice();
            let font_dict = page
                .resources()
                .fonts
                .get::<Dict<'_>>(name_slice)
                .or_else(|| {
                    page.resources()
                        .fonts
                        .get_ref(name_slice)
                        .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                });
            font_dict.is_some_and(|fd| {
                fd.get::<Name>(keys::SUBTYPE)
                    .is_some_and(|s| s.as_ref() == b"Type0")
            })
        });

        if let Some(content) = page.page_stream() {
            if scan_for_notdef_in_content(content, has_type0_font) {
                error_at(
                    report,
                    "6.2.11.8",
                    "Content stream contains reference to .notdef glyph (code 0 / CID 0)",
                    format!("page {}", page_idx + 1),
                );
                return;
            }
        }
    }
}

/// Scan a content stream for Tj/TJ operators with hex-encoded .notdef references.
///
/// Detects byte value 0x00 in hex strings used by text operators. For simple fonts
/// (1-byte encoding), ANY 0x00 byte is character code 0 = .notdef. For CID fonts
/// (2-byte Identity-H), 0x0000 = CID 0 = .notdef.
///
/// `cid_mode`: if true (page has Type0 fonts), only flag hex strings that decode
/// to exactly 0x0000 (2-byte null CID). If false, any 0x00 byte = notdef.
fn scan_for_notdef_in_content(content: &[u8], cid_mode: bool) -> bool {
    let mut i = 0;
    while i + 3 < content.len() {
        if content[i] == b'<' && content.get(i + 1).is_some_and(|b| b.is_ascii_hexdigit()) {
            let start = i + 1;
            let mut end = start;
            while end < content.len() && content[end] != b'>' {
                end += 1;
            }
            if end < content.len() {
                let hex = &content[start..end];
                // In CID mode (page has Type0 fonts), a hex string like <0041>
                // means CID 65, not two 1-byte codes. Only flag if ALL decoded bytes
                // are 0x00 (i.e. CID 0 = notdef). In simple-font mode, any 0x00 byte
                // is character code 0 = notdef. (#FP-6.2.11.8)
                let has_null = if cid_mode {
                    hex_is_all_null(hex) // only <0000> or <000000> etc.
                } else {
                    hex_contains_null_byte(hex) // any 0x00 byte
                };
                if has_null {
                    // Check context: must be near a Tj or inside TJ array
                    let after = &content[end + 1..content.len().min(end + 10)];
                    let after_trimmed: Vec<u8> = after
                        .iter()
                        .copied()
                        .skip_while(|b| b.is_ascii_whitespace())
                        .take(3)
                        .collect();
                    if after_trimmed.starts_with(b"Tj") || after_trimmed.starts_with(b"TJ") {
                        return true;
                    }
                    // Also detect inside [...] TJ: look backward for '['
                    let before_start = i.saturating_sub(200);
                    let before = &content[before_start..i];
                    if before.iter().rev().any(|&b| b == b'[') {
                        // Inside an array — check if the array is followed by TJ
                        if let Some(close) = content[end..].iter().position(|&b| b == b']') {
                            let after_arr =
                                &content[end + close + 1..content.len().min(end + close + 10)];
                            let trimmed: Vec<u8> = after_arr
                                .iter()
                                .copied()
                                .skip_while(|b| b.is_ascii_whitespace())
                                .take(3)
                                .collect();
                            if trimmed.starts_with(b"TJ") {
                                return true;
                            }
                        }
                    }
                }
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    false
}

/// Check if a hex string (ASCII hex digits without '<''>') decodes to bytes containing 0x00.
fn hex_contains_null_byte(hex: &[u8]) -> bool {
    // Collect hex digit pairs, skipping whitespace
    let digits: Vec<u8> = hex
        .iter()
        .copied()
        .filter(|b| b.is_ascii_hexdigit())
        .collect();
    // Process pairs
    let mut j = 0;
    while j + 1 < digits.len() {
        let hi = hex_val(digits[j]);
        let lo = hex_val(digits[j + 1]);
        if hi == 0 && lo == 0 {
            return true;
        }
        j += 2;
    }
    false
}

/// Returns true only if ALL decoded bytes in the hex string are 0x00.
/// Used in CID mode where a single hex pair like 0x00 0x41 = CID 65, NOT notdef.
fn hex_is_all_null(hex: &[u8]) -> bool {
    let digits: Vec<u8> = hex
        .iter()
        .copied()
        .filter(|b| b.is_ascii_hexdigit())
        .collect();
    if digits.is_empty() {
        return false;
    }
    let mut j = 0;
    while j + 1 < digits.len() {
        let hi = hex_val(digits[j]);
        let lo = hex_val(digits[j + 1]);
        if hi != 0 || lo != 0 {
            return false;
        }
        j += 2;
    }
    true
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

/// §6.2.11.4.1 — Content stream renders a character whose glyph is not defined
/// in the embedded Type1/CFF subset font program.
///
/// The font's /CharSet entry declares which glyphs are embedded. If the content
/// stream renders a character code whose glyph name (via /Encoding) is NOT listed
/// in /CharSet, the font program cannot supply the glyph → §6.2.11.4.1.
///
/// Strategy: for each page, build a map of font-resource-name → CharSet, then
/// tokenize the page content stream, track the active simple font via /Tf, and
/// for each text operator check that every byte's glyph name is in CharSet.
pub fn check_type1_charset_coverage(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        // Build resource-name → (charset_names, winansi) for Type1 subset fonts.
        let mut type1_charsets: std::collections::HashMap<
            Vec<u8>,
            (std::collections::HashSet<String>, bool),
        > = std::collections::HashMap::new();
        let fonts = &page.resources().fonts;
        for (rname, _) in fonts.entries() {
            let font_dict_opt: Option<Dict<'_>> =
                fonts.get::<Dict<'_>>(rname.as_ref()).or_else(|| {
                    fonts
                        .get_ref(rname.as_ref())
                        .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                });
            let Some(fd) = font_dict_opt else { continue };
            let subtype = fd.get::<Name>(keys::SUBTYPE);
            if subtype.as_ref().is_none_or(|s| s.as_ref() != b"Type1") {
                continue;
            }
            let base = fd
                .get::<Name>(keys::BASE_FONT)
                .map(|n| std::str::from_utf8(n.as_ref()).unwrap_or("").to_string())
                .unwrap_or_default();
            if !is_subset_font(&base) {
                continue;
            }
            let Some(desc) = fd.get::<Dict<'_>>(keys::FONT_DESC) else {
                continue;
            };
            let cs_opt = desc
                .get::<pdf_syntax::object::String>(keys::CHAR_SET)
                .map(|s| s.as_bytes().to_vec());
            let Some(cs_bytes) = cs_opt else { continue };
            if cs_bytes.is_empty() {
                continue;
            }
            let ct = std::str::from_utf8(&cs_bytes).unwrap_or("");
            let names: std::collections::HashSet<String> = ct
                .split('/')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            // WinAnsiEncoding or Encoding dict with WinAnsi base
            let winansi = fd
                .get::<Name>(keys::ENCODING)
                .is_some_and(|e| e.as_ref() == b"WinAnsiEncoding");
            type1_charsets.insert(rname.as_ref().to_vec(), (names, winansi));
        }
        if type1_charsets.is_empty() {
            continue;
        }

        let Some(content) = page.page_stream() else {
            continue;
        };
        if content.len() > MAX_CONTENT_STREAM_SCAN_SIZE {
            continue;
        }

        let tokens = tokenize_pdf_content(content);
        let n = tokens.len();
        let loc = format!("page {}", page_idx + 1);
        let mut active: Option<&(std::collections::HashSet<String>, bool)> = None;

        'tokens: for i in 0..n {
            let tok = tokens[i].as_slice();
            // Track font switch: /FontName size Tf
            if tok == b"Tf" && i >= 2 {
                let rname = tokens[i - 2].as_slice();
                if let Some(name_bytes) = rname.strip_prefix(b"/") {
                    active = type1_charsets.get(name_bytes);
                } else {
                    active = None;
                }
            }
            let Some((charset, winansi)) = active else {
                continue;
            };
            // Text show operators: preceding token is the string
            let str_tok = if matches!(tok, b"Tj" | b"'" | b"\"") && i >= 1 {
                Some(tokens[i - 1].as_slice())
            } else if tok == b"TJ" {
                None // handled below
            } else {
                continue;
            };

            // Check a single string token
            let check_str = |s: &[u8]| -> Option<String> {
                let codes = extract_simple_codes(s);
                for code in codes {
                    if code == 0 {
                        continue;
                    } // .notdef handled elsewhere
                    if let Some(gname) = if *winansi {
                        t1_winansi_glyph_name(code)
                    } else {
                        None
                    } {
                        if !charset.contains(gname) {
                            return Some(gname.to_string());
                        }
                    }
                }
                None
            };

            if let Some(s) = str_tok {
                if let Some(gname) = check_str(s) {
                    error_at(
                        report,
                        "6.2.11.4.1",
                        format!(
                            "Content renders '/{gname}' which is not defined in the \
                             embedded Type1 subset font (not in /CharSet)"
                        ),
                        loc.clone(),
                    );
                    break 'tokens;
                }
            } else if tok == b"TJ" {
                // Scan backward through array tokens until '['
                let mut j = i as isize - 1;
                while j >= 0 {
                    let t = tokens[j as usize].as_slice();
                    if t == b"[" {
                        break;
                    }
                    if let Some(gname) = check_str(t) {
                        error_at(
                            report,
                            "6.2.11.4.1",
                            format!(
                                "Content renders '/{gname}' which is not defined in the \
                                 embedded Type1 subset font (not in /CharSet)"
                            ),
                            loc.clone(),
                        );
                        break 'tokens;
                    }
                    j -= 1;
                }
            }
        }
    }
}

/// Extract 1-byte character codes from a PDF string token (hex or literal).
fn extract_simple_codes(tok: &[u8]) -> Vec<u8> {
    if let Some(inner) = tok.strip_prefix(b"<").and_then(|t| t.strip_suffix(b">")) {
        // Hex string: pairs of hex digits
        let digits: Vec<u8> = inner
            .iter()
            .copied()
            .filter(|b| b.is_ascii_hexdigit())
            .collect();
        digits
            .chunks(2)
            .map(|c| {
                let hi = hex_val(c[0]);
                let lo = if c.len() > 1 { hex_val(c[1]) } else { 0 };
                (hi << 4) | lo
            })
            .collect()
    } else if let Some(inner) = tok.strip_prefix(b"(").and_then(|t| t.strip_suffix(b")")) {
        // Literal string: bytes with backslash escapes
        let mut codes = Vec::new();
        let mut i = 0;
        while i < inner.len() {
            if inner[i] == b'\\' {
                i += 1;
                if i >= inner.len() {
                    break;
                }
                match inner[i] {
                    b'n' => {
                        codes.push(b'\n');
                        i += 1;
                    }
                    b'r' => {
                        codes.push(b'\r');
                        i += 1;
                    }
                    b't' => {
                        codes.push(b'\t');
                        i += 1;
                    }
                    b'(' | b')' | b'\\' => {
                        codes.push(inner[i]);
                        i += 1;
                    }
                    b'0'..=b'7' => {
                        // Octal: up to 3 digits
                        let start = i;
                        let end = (start + 3).min(inner.len());
                        let mut val = 0u32;
                        let mut k = start;
                        while k < end && inner[k] >= b'0' && inner[k] <= b'7' {
                            val = val * 8 + (inner[k] - b'0') as u32;
                            k += 1;
                        }
                        codes.push(val as u8);
                        i = k;
                    }
                    _ => {
                        codes.push(inner[i]);
                        i += 1;
                    }
                }
            } else {
                codes.push(inner[i]);
                i += 1;
            }
        }
        codes
    } else {
        vec![]
    }
}

// ─── Batch 4: Font & Annotation Deep Validation (§6.3.x, §6.5.x) ───────────

/// Check every font has a /Type key set to /Font (§6.3.1).
pub fn check_font_type_key(pdf: &Pdf, report: &mut ComplianceReport) {
    for_each_font(pdf, |name, font_dict, page_idx| {
        match font_dict.get::<Name>(keys::TYPE) {
            Some(t) if t.as_ref() == keys::FONT => {}
            Some(t) => {
                let val = std::str::from_utf8(t.as_ref()).unwrap_or("?");
                error_at(
                    report,
                    "6.3.1",
                    format!("Font {name} /Type is {val}, expected Font"),
                    format!("page {}", page_idx + 1),
                );
            }
            None => {
                error_at(
                    report,
                    "6.3.1",
                    format!("Font {name} missing /Type key"),
                    format!("page {}", page_idx + 1),
                );
            }
        }
    });
}

/// Deep font embedding validation (§6.3.3).
///
/// Beyond simple embedding presence, validates:
/// - CIDFont descriptors have matching FontFile subtypes
/// - Subset fonts (ABCDEF+Name) have CIDSet or CharSet
/// - FontFile3 subtype matches font type
pub fn check_font_embedding_deep(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for_each_font(pdf, |name, font_dict, page_idx| {
        let subtype = font_dict.get::<Name>(keys::SUBTYPE);
        let subtype_bytes = subtype.as_ref().map(|s| s.as_ref());

        // Type3 fonts don't need embedding (they define glyphs inline)
        if subtype_bytes == Some(b"Type3") {
            return;
        }

        // Get the actual font name (BaseFont) for subset detection
        let base_font_name = font_dict
            .get::<Name>(keys::BASE_FONT)
            .map(|n| std::str::from_utf8(n.as_ref()).unwrap_or(name).to_string());
        let font_name = base_font_name.as_deref().unwrap_or(name);

        // Check font descriptor — may be a direct dict or an indirect ref.
        // FontDescriptor is almost always indirect; resolve via xref. (#FN-6.3.4)
        let desc_opt: Option<Dict<'_>> = font_dict.get::<Dict<'_>>(keys::FONT_DESC).or_else(|| {
            font_dict
                .get_ref(keys::FONT_DESC)
                .and_then(|r| xref.get::<Dict<'_>>(r.into()))
        });
        if let Some(desc) = desc_opt {
            // Check font program is actually embedded (§6.3.4 test 1)
            // PDF/A requires ALL fonts to be embedded — no standard-14 exemption
            if !font_has_embedding(&desc) {
                error_at(
                    report,
                    "6.3.4",
                    format!(
                        "Font {font_name} is not embedded (missing FontFile/FontFile2/FontFile3)"
                    ),
                    format!("page {}", page_idx + 1),
                );
            } else {
                // Font file key exists — check the content is not corrupt/empty.
                // An invalid font program stream (all-zero or missing magic bytes) means
                // glyphs are effectively not present. (#467)
                // veraPDF emits §6.3.2 (PDF/A-1) or §6.3.4 (PDF/A-2/3/4) for this.
                // Track which font file key is used — CFF (FontFile3) needs a different
                // corrupt check than Type1 (FontFile) or TrueType (FontFile2).
                let is_truetype = desc.get::<Stream<'_>>(keys::FONT_FILE2).is_some();
                let is_cff = !is_truetype && desc.get::<Stream<'_>>(keys::FONT_FILE3).is_some();
                let ff_stream: Option<Stream<'_>> = desc
                    .get::<Stream<'_>>(keys::FONT_FILE)
                    .or_else(|| desc.get::<Stream<'_>>(keys::FONT_FILE2))
                    .or_else(|| desc.get::<Stream<'_>>(keys::FONT_FILE3));
                if let Some(ff) = ff_stream {
                    if let Ok(data) = ff.decoded() {
                        let is_corrupt = is_font_program_corrupt(&data, is_truetype, is_cff);
                        if is_corrupt {
                            // veraPDF fires §6.3.4 (not §6.3.2) for corrupt font programs in
                            // all PDF/A parts. "6.3.3" remaps to "6.3.4" for PDF/A-1;
                            // "6.3.4" remaps to "6.2.11.4.1" for PDF/A-2/3. (#FP-6.3.2)
                            let rule = if part == 1 { "6.3.3" } else { "6.3.4" };
                            error_at(
                                report,
                                rule,
                                format!(
                                    "Font {font_name} has corrupt/null font program (invalid or empty stream)"
                                ),
                                format!("page {}", page_idx + 1),
                            );
                        }
                    }
                }
            }
            check_fontfile_subtype_match(&desc, font_name, page_idx, report);
            if is_subset_font(font_name) {
                // §6.3.5 t2 + §6.2.11.4.2 t1: Type1/CFF CharSet checks
                let is_type1 = subtype_bytes == Some(b"Type1");
                if is_type1 {
                    let cs = desc
                        .get::<pdf_syntax::object::String>(keys::CHAR_SET)
                        .map(|s| s.as_bytes().to_vec());
                    if cs.as_ref().is_none_or(|v| v.is_empty()) {
                        error_at(
                            report,
                            "6.3.5",
                            format!("Type1 font subset {font_name} missing or empty /CharSet"),
                            format!("page {}", page_idx + 1),
                        );
                    } else if let Some(cb) = &cs {
                        // §6.2.11.4.2: CharSet must list ALL glyphs with non-zero width
                        let ct = std::str::from_utf8(cb).unwrap_or("");
                        let names: std::collections::HashSet<&str> =
                            ct.split('/').filter(|s| !s.is_empty()).collect();
                        let fc = font_dict.get::<i32>(keys::FIRST_CHAR).unwrap_or(0);
                        if let Some(wa) = font_dict.get::<Array<'_>>(keys::WIDTHS) {
                            let enc = font_dict
                                .get::<Name>(keys::ENCODING)
                                .map(|n| n.as_ref().to_vec());
                            let winansi = enc.as_deref() == Some(b"WinAnsiEncoding");
                            for (i, w) in wa.iter::<pdf_syntax::object::Number>().enumerate() {
                                if w.as_f64() > 0.0 {
                                    let code = fc as usize + i;
                                    if let Some(gn) = if winansi {
                                        t1_winansi_glyph_name(code as u8)
                                    } else {
                                        None
                                    } {
                                        if !names.contains(gn) {
                                            error_at(report, "6.2.11.4.2",
                                                format!("Type1 font {font_name}: /CharSet missing '/{gn}' (code {code})"),
                                                format!("page {}", page_idx + 1));
                                            break;
                                        }
                                    }
                                }
                            }
                        }

                        // §6.3.5 T2: For CFF (FontFile3/Type1C) subset fonts, /CharSet must
                        // list ALL glyph names present in the font program. Enumerate via
                        // cff-parser and flag any unlisted glyph. Fixes FN for fail-c.
                        if let Some(ff3) = desc.get::<Stream<'_>>(keys::FONT_FILE3) {
                            if let Ok(cff_data) = ff3.decoded() {
                                if let Some(cff) = cff_parser::Table::parse(&cff_data) {
                                    for gid in 1..cff.number_of_glyphs() {
                                        // GID 0 is always .notdef — skip
                                        if let Some(gname) =
                                            cff.glyph_name(cff_parser::GlyphId(gid))
                                        {
                                            if gname != ".notdef" && !names.contains(gname) {
                                                error_at(
                                                    report,
                                                    "6.3.5",
                                                    format!("CFF font {font_name}: /CharSet missing '/{gname}' (glyph present in font program)"),
                                                    format!("page {}", page_idx + 1),
                                                );
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check CIDFont descendants
        if let Some(descendants) = font_dict.get::<Array<'_>>(keys::DESCENDANT_FONTS) {
            for desc_font in descendants.iter::<Dict<'_>>() {
                // Also check CIDFont embedding; FontDescriptor is typically indirect.
                let cid_desc_opt: Option<Dict<'_>> =
                    desc_font.get::<Dict<'_>>(keys::FONT_DESC).or_else(|| {
                        desc_font
                            .get_ref(keys::FONT_DESC)
                            .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                    });
                if let Some(cid_desc) = cid_desc_opt {
                    if !font_has_embedding(&cid_desc) {
                        error_at(
                            report,
                            "6.3.4",
                            format!("CIDFont {name} is not embedded"),
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
                check_cidfont_descriptor_deep(&desc_font, name, page_idx, part, xref, report);
            }
        }
    });
}

fn check_cidfont_descriptor_deep(
    cid_font: &Dict<'_>,
    name: &str,
    page_idx: usize,
    part: u8,
    xref: &pdf_syntax::xref::XRef,
    report: &mut ComplianceReport,
) {
    // FontDescriptor is almost always an indirect ref. Use get_ref fallback like
    // check_font_embedding_deep does for the same reason. (#FN-6.3.5)
    let desc_opt: Option<Dict<'_>> = cid_font.get::<Dict<'_>>(keys::FONT_DESC).or_else(|| {
        cid_font
            .get_ref(keys::FONT_DESC)
            .and_then(|r| xref.get::<Dict<'_>>(r.into()))
    });
    let Some(desc) = desc_opt else {
        return;
    };
    // Use BaseFont name for subset detection
    let cid_base = cid_font
        .get::<Name>(keys::BASE_FONT)
        .map(|n| std::str::from_utf8(n.as_ref()).unwrap_or(name).to_string());
    let cid_name = cid_base.as_deref().unwrap_or(name);

    // Check for corrupt/empty font program in CIDFont descriptor. (#467)
    // The main font_has_embedding check only verifies the key exists, not content validity.
    // CIDFontType2 uses FontFile2 (TrueType); check that the stream is a valid program.
    let is_truetype = desc.get::<Stream<'_>>(keys::FONT_FILE2).is_some();
    let is_cff = !is_truetype && desc.get::<Stream<'_>>(keys::FONT_FILE3).is_some();
    let ff_stream: Option<Stream<'_>> = desc
        .get::<Stream<'_>>(keys::FONT_FILE)
        .or_else(|| desc.get::<Stream<'_>>(keys::FONT_FILE2))
        .or_else(|| desc.get::<Stream<'_>>(keys::FONT_FILE3));
    if let Some(ff) = ff_stream {
        if let Ok(data) = ff.decoded() {
            if is_font_program_corrupt(&data, is_truetype, is_cff) {
                // veraPDF fires §6.3.4 (not §6.3.2) for corrupt font programs in all parts.
                // "6.3.3" remaps to "6.3.4" for PDF/A-1; "6.3.4" → "6.2.11.4.1" for PDF/A-2/3.
                // (#FP-6.3.2)
                let rule = if part == 1 { "6.3.3" } else { "6.3.4" };
                error_at(
                    report,
                    rule,
                    format!(
                        "CIDFont {cid_name} has corrupt/null font program (invalid or empty stream)"
                    ),
                    format!("page {}", page_idx + 1),
                );
                // A corrupt font program means glyph metrics cannot be verified — §6.3.5. (#467)
                error_at(
                    report,
                    "6.3.5",
                    format!(
                        "CIDFont {cid_name}: glyph metrics unverifiable (corrupt font program)"
                    ),
                    format!("page {}", page_idx + 1),
                );
            }
        }
    }

    // NOTE: An all-zero CIDToGIDMap stream (all GIDs → .notdef) is NOT flagged here.
    // veraPDF does not report §6.3.4/§6.3.5 for an all-zero CIDToGIDMap — the font
    // IS embedded, just with a degenerate mapping. The former check caused FPs for
    // pdfbox-3017.pdf (AAAJYI+Code2000). Removed to fix #FP-6.3.4 / #FP-6.3.5.

    check_fontfile_subtype_match(&desc, cid_name, page_idx, report);

    if is_subset_font(cid_name) {
        // CIDSet is almost always an indirect stream ref. Check both direct and indirect.
        // Missing the ref causes FP §6.3.5 (we fire "CIDSet missing" when it's present
        // as an indirect ref that get::<Stream> doesn't resolve). (#FP-6.3.5)
        let cidset_opt: Option<Stream<'_>> = desc.get::<Stream<'_>>(keys::CID_SET).or_else(|| {
            desc.get_ref(keys::CID_SET)
                .and_then(|r| xref.get::<Stream<'_>>(r.into()))
        });
        match cidset_opt {
            None => {
                error_at(
                    report,
                    "6.3.5",
                    format!("Subset CIDFont {cid_name} missing required /CIDSet"),
                    format!("page {}", page_idx + 1),
                );
            }
            Some(cidset_stream) => {
                // §6.3.5: CIDSet must not be an empty stream. (#467)
                // An empty CIDSet stream is equivalent to no CIDSet.
                let raw = cidset_stream.raw_data();
                if raw.is_empty() || raw.iter().all(|&b| b == 0) {
                    error_at(
                        report,
                        "6.3.5",
                        format!("Subset CIDFont {cid_name} has empty /CIDSet stream"),
                        format!("page {}", page_idx + 1),
                    );
                }
                // §6.2.11.4.2: if a CIDSet is present, it must identify ALL CIDs present
                // in the font program. Check only individually-declared CIDs (array form:
                // `c [w1 w2 ...]`) against the CIDSet. Range entries (`c1 c2 w`) are bulk
                // width defaults that often cover CIDs not in the font subset — using them
                // causes FP §6.3.5 (isartor-6-3-3-2-t01). (#FP-6.3.5)
                let cidset_bits = cidset_stream.decoded().unwrap_or_else(|_| raw.to_vec());
                if let Some(w_arr) = cid_font.get::<Array<'_>>(keys::W) {
                    use pdf_syntax::object::MaybeRef;
                    let raw_w: Vec<_> = w_arr.raw_iter().collect();
                    let mut wi = 0usize;
                    'w_check: while wi < raw_w.len() {
                        let c1 = match &raw_w[wi] {
                            MaybeRef::NotRef(Object::Number(n)) => n.as_f64() as u32,
                            _ => {
                                wi += 1;
                                continue;
                            }
                        };
                        wi += 1;
                        if wi >= raw_w.len() {
                            break;
                        }
                        match &raw_w[wi] {
                            // Array form: c1 [w1 w2 ...] — individually declared CIDs
                            MaybeRef::NotRef(Object::Array(inner)) => {
                                for (j, _) in inner.iter::<i32>().enumerate() {
                                    let cid = c1 + j as u32;
                                    let byte_idx = (cid / 8) as usize;
                                    let bit_pos = 7 - (cid % 8);
                                    let is_set = cidset_bits
                                        .get(byte_idx)
                                        .map(|&b| (b >> bit_pos) & 1 == 1)
                                        .unwrap_or(false);
                                    if !is_set {
                                        error_at(
                                            report,
                                            "6.2.11.4.2",
                                            format!(
                                                "CIDFont {cid_name}: CIDSet missing CID {cid} \
                                                 (individually declared in /W array)"
                                            ),
                                            format!("page {}", page_idx + 1),
                                        );
                                        break 'w_check;
                                    }
                                }
                                wi += 1;
                            }
                            // Range form: c1 c2 w — skip; range covers nominal CIDs,
                            // many of which may not be in the font subset. (#FP-6.3.5)
                            MaybeRef::NotRef(Object::Number(_)) => {
                                wi += 2;
                            }
                            _ => {
                                wi += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn check_fontfile_subtype_match(
    desc: &Dict<'_>,
    name: &str,
    page_idx: usize,
    report: &mut ComplianceReport,
) {
    if let Some(ff3) = desc.get::<Stream<'_>>(keys::FONT_FILE3) {
        let ff3_dict = ff3.dict();
        if ff3_dict.get::<Name>(keys::SUBTYPE).is_none() {
            error_at(
                report,
                "6.3.3",
                format!("Font {name} /FontFile3 missing required /Subtype"),
                format!("page {}", page_idx + 1),
            );
        }
    }
}

fn is_subset_font(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() > 7 && bytes[6] == b'+' && bytes[..6].iter().all(|&b| b.is_ascii_uppercase())
}

/// Returns true if a font program stream is corrupt or empty.
///
/// For FontFile2 (TrueType): checks that the stream starts with a valid sfVersion
/// magic (0x00010000 or 'true'). A stream of all zeros or one that starts with
/// null bytes instead of the magic is considered corrupt — veraPDF flags §6.3.2
/// (PDF/A-1) or §6.3.4 (PDF/A-2/3/4) for this. (#467)
///
/// For FontFile3 (CFF/OpenType): checks for CFF major version 1 header byte or
/// OpenType 'OTTO' magic. Does NOT apply Type1 PostScript magic check.
///
/// For FontFile (Type1 PostScript): must start with '%!' or PFB binary marker.
fn is_font_program_corrupt(data: &[u8], is_truetype: bool, is_cff: bool) -> bool {
    if data.is_empty() || data.iter().all(|&b| b == 0) {
        return true;
    }
    if is_truetype && data.len() >= 4 {
        // Valid TrueType/OpenType sfVersion magic values:
        // 0x00010000 — standard TrueType/OpenType with TT outlines
        // 0x74727565 — 'true' (Apple TrueType)
        // 0x4F54544F — 'OTTO' (OpenType with CFF outlines — unusual for FontFile2 but allowed)
        let magic = &data[..4];
        let valid = magic == b"\x00\x01\x00\x00"
            || magic == b"true"
            || magic == b"OTTO"
            || magic == b"typ1"; // legacy Mac Type 1 in sfnt wrapper
        if !valid {
            return true;
        }
    } else if is_cff {
        // CFF (FontFile3/Type1C): header byte 0 = major version (must be 1).
        // OpenType CFF uses 'OTTO' magic (4 bytes). Both are valid.
        // Do NOT apply Type1 PostScript magic here — CFF starts with \x01\x00 not '%!'.
        if data[0] != 1 && data.len() >= 4 && &data[..4] != b"OTTO" {
            return true;
        }
    } else if data.len() >= 2 {
        // Type1 (FontFile) must start with '%!' (ASCII) or 0x80 0x01 (PFB binary marker).
        // Any other start bytes mean the stream is not a valid PostScript/PFB font program.
        // isartor-6-3-2-t01-fail-b uses a /FontFile stream filled with garbage bytes
        // that starts with spaces. Fixes §6.3.4 FN.
        let magic2 = &data[..2];
        let valid_type1 = magic2 == b"%!" || magic2 == b"\x80\x01";
        if !valid_type1 {
            return true;
        }
    }
    false
}

/// Check ToUnicode CMap presence for non-symbolic fonts.
///
/// PDF/A-1: §6.3.8 (all renderable fonts require ToUnicode).
/// PDF/A-2/3: §6.2.11.7.2 (Type1 and all non-symbolic non-Type0 fonts). (#483)
/// PDF/A-4: §6.2.10.7 (ToUnicode required for fonts that can encode characters).
pub fn check_tounicode_cmap(
    pdf: &Pdf,
    part: u8,
    requires_unicode: bool,
    report: &mut ComplianceReport,
) {
    for_each_font(pdf, |name, font_dict, _page_idx| {
        let subtype = font_dict.get::<Name>(keys::SUBTYPE);

        // Type3 fonts are exempt from the ToUnicode requirement EXCEPT for PDF/A-2u and
        // PDF/A-3u (part 2 or 3 with Unicode conformance). veraPDF fires §6.2.11.7.2 for
        // Type3 fonts without ToUnicode in PDF/A-2u/3u, but does NOT fire §6.2.10.7 for
        // Type3 fonts in PDF/A-4 (veraPDF exempts them there). (#FN-6.2.11.7.2)
        let type3_exempt = subtype.as_ref().is_some_and(|s| s.as_ref() == b"Type3")
            && !(requires_unicode && part < 4);
        if type3_exempt {
            return;
        }

        // PDF/A-2/3/4: Identity-H/V and Type0 fonts are exempt from §6.2.11.7.2/§6.2.10.7,
        // EXCEPT for conformance level 'U' which requires Unicode mapping for ALL fonts.
        // PDF/A-1 §6.3.8 applies to ALL renderable fonts — no Type0 or encoding exemption.
        if part >= 2 && !requires_unicode {
            if let Some(enc) = font_dict.get::<Name>(keys::ENCODING) {
                if enc.as_ref() == keys::IDENTITY_H || enc.as_ref() == keys::IDENTITY_V {
                    return;
                }
            }
            if subtype.as_ref().is_some_and(|s| s.as_ref() == b"Type0") {
                return;
            }
        }

        let is_type1 = subtype
            .as_ref()
            .is_some_and(|s| s.as_ref() == b"Type1" || s.as_ref() == b"MMType1");

        // Symbolic font exemption: only for non-Type1 fonts and only for non-Unicode levels.
        // PDF/A-2u/3u and PDF/A-4 require ToUnicode for ALL fonts including symbolic.
        // §6.2.11.7.2: conformance level 'U' means full Unicode mapping required.
        if !is_type1 && part >= 2 && !requires_unicode {
            if let Some(desc) = font_dict.get::<Dict<'_>>(keys::FONT_DESC) {
                if let Some(flags) = desc.get::<i32>(keys::FLAGS) {
                    if flags & 0x04 != 0 {
                        return;
                    }
                }
            }
        }

        if !font_has_tounicode(font_dict) {
            // ISO 19005-2 §6.2.11.7.2 / ISO 19005-1 §6.3.8: fonts with a predefined
            // encoding from ISO 32000-1 Tables D.2/D.3/D.4 (WinAnsiEncoding,
            // MacRomanEncoding, StandardEncoding, MacExpertEncoding) are exempt because
            // the Unicode mapping is derivable from the encoding alone. Applies to all
            // PDF/A parts.
            //
            // Additionally, fonts with NO /Encoding key use their built-in encoding
            // (typically StandardEncoding for Type1), and veraPDF does not fire for them.
            //
            // An encoding DICT with /BaseEncoding set to a predefined name is also exempt:
            // the small /Differences array only overrides a few slots of the predefined
            // base, so Unicode mapping is still derivable. (#FP-6.2.11.7.2)
            let is_predefined_enc_name = |name: &[u8]| {
                matches!(
                    name,
                    b"WinAnsiEncoding"
                        | b"MacRomanEncoding"
                        | b"StandardEncoding"
                        | b"MacExpertEncoding"
                )
            };
            let xref = pdf.xref();
            // Resolve /Encoding: may be a direct Name, direct Dict, or indirect ref.
            let encoding_name = font_dict.get::<Name>(keys::ENCODING).or_else(|| {
                font_dict
                    .get_ref(keys::ENCODING)
                    .and_then(|r| xref.get::<Name>(r.into()))
            });
            let encoding_dict: Option<Dict<'_>> =
                font_dict.get::<Dict<'_>>(keys::ENCODING).or_else(|| {
                    font_dict
                        .get_ref(keys::ENCODING)
                        .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                });
            // Check for predefined name encoding or predefined /BaseEncoding in dict.
            let uses_predefined_encoding = encoding_name
                .as_ref()
                .is_some_and(|n| is_predefined_enc_name(n.as_ref()))
                || encoding_dict
                    .as_ref()
                    .and_then(|d| d.get::<Name>(b"BaseEncoding" as &[u8]))
                    .is_some_and(|n| is_predefined_enc_name(n.as_ref()));
            // Symbolic font check: symbolic fonts have non-AGL built-in encodings
            // (e.g. CMSY8/TeX math). Even Type1 symbolic fonts with no /Encoding
            // cannot have Unicode derived from their built-in encoding — veraPDF fires
            // §6.3.8/§6.2.11.7.2 for them. Resolve FontDescriptor indirect ref. (#FN-6.3.8)
            let is_symbolic = {
                let desc: Option<Dict<'_>> =
                    font_dict.get::<Dict<'_>>(keys::FONT_DESC).or_else(|| {
                        font_dict
                            .get_ref(keys::FONT_DESC)
                            .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                    });
                desc.and_then(|d| d.get::<i32>(keys::FLAGS))
                    .is_some_and(|f| f & 0x04 != 0)
            };
            // Exempt if: predefined encoding OR (non-Unicode level AND no encoding AND
            // not symbolic). "No encoding" means built-in encoding — for non-symbolic Type1
            // fonts this is StandardEncoding, whose Unicode mapping is derivable. For
            // symbolic fonts there is no derivable mapping, so they are not exempt.
            // (#FN-6.3.8, #FN-6.2.11.7.2)
            let no_enc_exempt = !requires_unicode
                && encoding_name.is_none()
                && encoding_dict.is_none()
                && !is_symbolic
                && (part != 1 || is_type1);
            if uses_predefined_encoding || no_enc_exempt {
                // Exempt: predefined or built-in encoding — Unicode mapping known.
            } else if part == 4 {
                // §6.2.10.7: ToUnicode required for all fonts in PDF/A-4. (#483)
                error(
                    report,
                    "6.2.10.7",
                    format!("Font {name} missing /ToUnicode CMap (§6.2.10.7)"),
                );
            } else if part >= 2 {
                // §6.2.11.7.2: applies to Type1 and all non-symbolic non-Type0 fonts
                // in PDF/A-2/3. (#483)
                error(
                    report,
                    "6.2.11.7.2",
                    format!("Font {name} missing /ToUnicode CMap (§6.2.11.7.2)"),
                );
            } else {
                // PDF/A-1 §6.3.8
                error(
                    report,
                    "6.3.8",
                    format!("Font {name} missing /ToUnicode CMap (§6.3.8)"),
                );
            }
        }
    });
}

/// Check ToUnicode CMap values for forbidden Unicode code points.
///
/// §6.2.11.7.2: U+0000, U+FEFF (BOM), and U+FFFE are forbidden.
/// §6.2.11.7.3: U+FFFF is forbidden (replacement character sentinel).
///
/// Only destination values in beginbfchar/beginbfrange sections are checked.
/// Codespace range bounds (e.g. `<0000> <FFFF>`) are NOT destinations and
/// must not be flagged as violations.
pub fn check_tounicode_values(pdf: &Pdf, report: &mut ComplianceReport) {
    // First pass: scan fonts and their direct ToUnicode streams.
    for_each_font(pdf, |name, font_dict, page_idx| {
        let Some(cmap_stream) = font_dict.get::<Stream<'_>>(keys::TO_UNICODE) else {
            return;
        };
        let Ok(data) = cmap_stream.decoded() else {
            return;
        };
        let text = String::from_utf8_lossy(&data);

        // Parse line by line, tracking which section we are in.
        // beginbfchar: each line is  <srccode> <dstcode>  — check dstcode (2nd token)
        // beginbfrange: each line is <srclo> <srchi> <dststart> — check dststart (3rd token)
        // codespacerange lines look like beginbfchar but must NOT be checked.
        let mut in_bfchar = false;
        let mut in_bfrange = false;

        for line in text.lines() {
            let t = line.trim();
            if t.ends_with("beginbfchar") {
                in_bfchar = true;
                continue;
            }
            if t == "endbfchar" {
                in_bfchar = false;
                continue;
            }
            if t.ends_with("beginbfrange") {
                in_bfrange = true;
                continue;
            }
            if t == "endbfrange" {
                in_bfrange = false;
                continue;
            }

            if !in_bfchar && !in_bfrange {
                continue;
            }

            // Collect all <XXXX> hex tokens on this line as u32 to support
            // 4-byte extended form (e.g. <0000E000> for BMP PUA). (#FN-6.2.11.7.3)
            let tokens: Vec<u32> = t
                .split('<')
                .skip(1) // first chunk is before the first '<'
                .filter_map(|chunk| {
                    let end = chunk.find('>')?;
                    let hex = &chunk[..end];
                    u32::from_str_radix(hex, 16).ok()
                })
                .collect();

            // Helper: check one destination value for forbidden codepoints.
            // Returns true if an error was emitted (caller should return).
            //
            // §6.2.11.7.3 forbids: surrogates (U+D800-U+DFFF), BMP PUA
            // (U+E000-U+F8FF), U+FFFE, U+FFFF.  4-byte hex destinations that
            // contain surrogate code units (e.g. <DBC0DD6D>) are also forbidden
            // even when they form a valid UTF-16 surrogate pair. (#FN-6.2.11.7.3)
            let mut check_dst = |val: u32| -> bool {
                // 4-byte form: check if the high 2 bytes are in the surrogate
                // range (0xD800-0xDFFF) — indicates a UTF-16 surrogate pair
                // encoding a supplementary character, which §6.2.11.7.3 forbids.
                if val > 0xFFFF {
                    let high = (val >> 16) as u16;
                    if (0xD800..=0xDFFF).contains(&high) {
                        error_at(
                            report,
                            "6.2.11.7.3",
                            format!(
                                "Font {name} ToUnicode CMap contains surrogate pair \
                                 encoding (U+{val:08X}) which is forbidden"
                            ),
                            format!("page {}", page_idx + 1),
                        );
                        return true;
                    }
                    return false; // other 4-byte supplementary chars are fine
                }
                // §6.2.11.7.3: surrogates U+D800-U+DFFF forbidden.
                if (0xD800u32..=0xDFFF).contains(&val) {
                    error_at(
                        report,
                        "6.2.11.7.3",
                        format!("Font {name} ToUnicode CMap contains surrogate U+{val:04X}"),
                        format!("page {}", page_idx + 1),
                    );
                    return true;
                }
                // §6.2.11.7.3: BMP Private Use Area U+E000-U+F8FF forbidden.
                // (#FN-6.2.11.7.3)
                if (0xE000u32..=0xF8FF).contains(&val) {
                    error_at(
                        report,
                        "6.2.11.7.3",
                        format!("Font {name} ToUnicode CMap maps to PUA U+{val:04X}"),
                        format!("page {}", page_idx + 1),
                    );
                    return true;
                }
                // §6.2.11.7.3: non-character positions U+FFFE and U+FFFF.
                if val == 0xFFFF || val == 0xFFFE {
                    error_at(
                        report,
                        "6.2.11.7.3",
                        format!("Font {name} ToUnicode CMap contains forbidden U+{val:04X}"),
                        format!("page {}", page_idx + 1),
                    );
                    return true;
                }
                // §6.2.11.7.2: U+0000, U+FEFF (BOM), U+FFFE forbidden
                if val == 0x0000 || val == 0xFEFF || val == 0xFFFE {
                    error_at(
                        report,
                        "6.2.11.7.2",
                        format!("Font {name} ToUnicode CMap contains forbidden U+{val:04X}"),
                        format!("page {}", page_idx + 1),
                    );
                    return true;
                }
                false
            };

            if in_bfchar {
                // A bfchar section can have multiple <src> <dst> pairs on one line.
                // Destinations are at odd indices: 1, 3, 5, ...
                // Previously only index 1 was checked, missing later pairs. (#FN-6.2.11.7.3)
                let mut idx = 1usize;
                while let Some(&val) = tokens.get(idx) {
                    if check_dst(val) {
                        return; // one error per font is enough
                    }
                    idx += 2;
                }
            } else {
                // bfrange: <srclo> <srchi> <dststart> — one entry per line.
                // Check both ends of the destination range. (#FN-6.2.11.7.3)
                if let Some(&dstlo) = tokens.get(2) {
                    let dsthi = tokens
                        .get(1)
                        .and_then(|&srchi| {
                            tokens
                                .first()
                                .map(|&srclo| dstlo.saturating_add(srchi.saturating_sub(srclo)))
                        })
                        .unwrap_or(dstlo);
                    for val in [dstlo, dsthi] {
                        if check_dst(val) {
                            return; // one error per font is enough
                        }
                    }
                }
            }
        }
    });

    // Second pass: scan all CMap streams in the PDF for §6.2.11.7.3 violations.
    // ToUnicode CMaps may chain via /UseCMap to resource streams that are not
    // directly linked as font ToUnicode — those must also be checked. (#467)
    check_cmap_streams_for_ffff(pdf, report);
}

/// Scan all stream objects that look like CMap programs for U+FFFF destination values.
fn check_cmap_streams_for_ffff(pdf: &Pdf, report: &mut ComplianceReport) {
    for obj in pdf.objects() {
        let stream = match &obj {
            Object::Stream(s) => s,
            _ => continue,
        };
        let Ok(data) = stream.decoded() else {
            continue;
        };
        let text = String::from_utf8_lossy(&data);
        if !text.contains("beginbfchar") && !text.contains("beginbfrange") {
            continue;
        }
        let mut in_bfchar = false;
        let mut in_bfrange = false;
        for line in text.lines() {
            let t = line.trim();
            if t.ends_with("beginbfchar") {
                in_bfchar = true;
                continue;
            }
            if t == "endbfchar" {
                in_bfchar = false;
                continue;
            }
            if t.ends_with("beginbfrange") {
                in_bfrange = true;
                continue;
            }
            if t == "endbfrange" {
                in_bfrange = false;
                continue;
            }
            if !in_bfchar && !in_bfrange {
                continue;
            }
            // Parse as u32 to handle 4-byte extended hex destinations. (#FN-6.2.11.7.3)
            let tokens: Vec<u32> = t
                .split('<')
                .skip(1)
                .filter_map(|chunk| {
                    let end = chunk.find('>')?;
                    u32::from_str_radix(&chunk[..end], 16).ok()
                })
                .collect();
            let dst_idx = if in_bfchar { 1 } else { 2 };
            if let Some(&dstlo) = tokens.get(dst_idx) {
                // Check both start and end of bfrange destination. (#FN-6.2.11.7.3)
                let dsthi = if in_bfrange {
                    tokens
                        .get(1)
                        .and_then(|&srchi| {
                            tokens
                                .first()
                                .map(|&srclo| dstlo.saturating_add(srchi.saturating_sub(srclo)))
                        })
                        .unwrap_or(dstlo)
                } else {
                    dstlo
                };
                for val in [dstlo, dsthi] {
                    // §6.2.11.7.3: surrogates (D800-DFFF), BMP PUA (E000-F8FF),
                    // U+FFFE, U+FFFF; plus 4-byte surrogate pair encodings.
                    let is_violation = if val > 0xFFFF {
                        let high = (val >> 16) as u16;
                        (0xD800u16..=0xDFFF).contains(&high)
                    } else {
                        (0xD800u32..=0xDFFF).contains(&val)
                            || (0xE000u32..=0xF8FF).contains(&val)
                            || val == 0xFFFF
                            || val == 0xFFFE
                    };
                    if is_violation {
                        error(
                            report,
                            "6.2.11.7.3",
                            format!("ToUnicode CMap (via UseCMap chain) contains forbidden mapping to U+{val:04X}"),
                        );
                        return; // one error per document is enough
                    }
                }
            }
        }
    }
}

/// Check that all glyphs in a TrueType simple font have ToUnicode mappings (§6.2.10.7/9).
///
/// PDF/A-4 §6.2.10.7 requires that character codes with valid glyphs have Unicode
/// mappings via ToUnicode. §6.2.10.9 requires the character set to be fully covered.
/// A subset TrueType with a ToUnicode CMap that omits some glyphs violates both. (#467)
pub fn check_tounicode_glyph_coverage(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    if part != 4 {
        return;
    }

    for_each_font(pdf, |name, font_dict, page_idx| {
        if !matches!(
            font_dict
                .get::<Name>(keys::SUBTYPE)
                .as_ref()
                .map(|s| s.as_ref()),
            Some(b"TrueType")
        ) {
            return;
        }

        let Some(first_char) = font_dict.get::<i32>(keys::FIRST_CHAR) else {
            return;
        };
        let Some(last_char) = font_dict.get::<i32>(keys::LAST_CHAR) else {
            return;
        };

        // ToUnicode CMap must be present for this check to apply.
        let Some(cmap_stream) = font_dict.get::<Stream<'_>>(keys::TO_UNICODE) else {
            return;
        };
        let Ok(cmap_data) = cmap_stream.decoded() else {
            return;
        };
        let Ok(cmap_text) = std::str::from_utf8(&cmap_data) else {
            return;
        };

        let Some(desc) = font_dict.get::<Dict<'_>>(keys::FONT_DESC) else {
            return;
        };
        let Some(ff2) = desc.get::<Stream<'_>>(keys::FONT_FILE2) else {
            return;
        };
        let Ok(font_data) = ff2.decoded() else {
            return;
        };
        let Ok(face) = ttf_parser::Face::parse(&font_data, 0) else {
            return;
        };
        let upem = face.units_per_em() as f64;
        if upem <= 0.0 {
            return;
        }

        // Only handle named standard encodings.
        let enc_bytes: Vec<u8> = font_dict
            .get::<Name>(keys::ENCODING)
            .map(|n| n.as_ref().to_vec())
            .unwrap_or_default();
        let use_winansi = enc_bytes == b"WinAnsiEncoding";
        let use_macroman = enc_bytes == b"MacRomanEncoding";
        if !use_winansi && !use_macroman {
            return;
        }

        let mapped = parse_tounicode_source_codes(cmap_text);
        let first = first_char as usize;
        let last = last_char as usize;
        let loc = format!("page {}", page_idx + 1);

        for code in first..=last {
            let ch = if use_winansi {
                winansi_code_to_char(code as u8)
            } else {
                macroman_code_to_char(code as u8)
            };
            let Some(ch) = ch else {
                continue;
            };
            let Some(gid) = face.glyph_index(ch) else {
                continue;
            };
            if gid.0 == 0 {
                continue; // .notdef
            }
            let Some(advance) = face.glyph_hor_advance(gid) else {
                continue;
            };
            if advance == 0 {
                continue;
            }

            if !mapped.contains(&(code as u32)) {
                // §6.2.10.7: code with valid glyph has no ToUnicode mapping (#467)
                error_at(
                    report,
                    "6.2.10.7",
                    format!(
                        "Font {name} code {code} (U+{:04X}) has valid glyph but \
                         no ToUnicode CMap mapping",
                        ch as u32
                    ),
                    loc.clone(),
                );
                // §6.2.10.9: character not covered by font's character repertoire (#467)
                error_at(
                    report,
                    "6.2.10.9",
                    format!(
                        "Font {name} code {code} (U+{:04X}) not covered by ToUnicode CMap",
                        ch as u32
                    ),
                    loc.clone(),
                );
                return; // one error per font
            }
        }
    });
}

/// Check that no text operator references the .notdef glyph (§6.2.10.9).
///
/// For CID (Type0) fonts, CID 0 maps to .notdef. Content streams that emit
/// `<0000>Tj` or `[...<0000>...] TJ` while a Type0 font is active violate
/// the prohibition on rendering .notdef glyphs. Fixes #496.
pub fn check_notdef_glyph_usage(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        // Collect Type0 (CID) font resource names for this page.
        let mut type0_fonts: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();
        let fonts = &page.resources().fonts;
        for (name, _) in fonts.entries() {
            let font_dict_opt: Option<Dict<'_>> =
                fonts.get::<Dict<'_>>(name.as_ref()).or_else(|| {
                    fonts
                        .get_ref(name.as_ref())
                        .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                });
            if let Some(font_dict) = font_dict_opt {
                if font_dict
                    .get::<Name>(keys::SUBTYPE)
                    .is_some_and(|s| s.as_ref() == b"Type0")
                {
                    type0_fonts.insert(name.as_ref().to_vec());
                }
            }
        }
        if type0_fonts.is_empty() {
            continue;
        }

        let Some(content) = page.page_stream() else {
            continue;
        };
        if content.len() > MAX_CONTENT_STREAM_SCAN_SIZE {
            continue;
        }

        let tokens = tokenize_pdf_content(content);
        let loc = format!("page {}", page_idx + 1);
        let mut current_font_is_type0 = false;
        let n = tokens.len();

        'page: for i in 0..n {
            let tok = tokens[i].as_slice();

            // Track active font: /FontName size Tf
            if tok == b"Tf" && i >= 2 {
                let font_name = tokens[i - 2].as_slice();
                if let Some(name_bytes) = font_name.strip_prefix(b"/") {
                    current_font_is_type0 = type0_fonts.contains(name_bytes);
                }
            }

            if !current_font_is_type0 {
                continue;
            }

            // Tj / ' / ": string argument is the immediately preceding token.
            if matches!(tok, b"Tj" | b"'" | b"\"")
                && i >= 1
                && cid_hex_has_notdef(tokens[i - 1].as_slice())
            {
                error_at(
                    report,
                    "6.2.10.9",
                    "Text operator references .notdef glyph (CID 0x0000)",
                    loc.clone(),
                );
                break 'page;
            }

            // TJ: scan backward through array tokens until '['.
            if tok == b"TJ" && i >= 1 {
                let mut j = i as isize - 1;
                while j >= 0 {
                    let t = tokens[j as usize].as_slice();
                    if t == b"[" {
                        break;
                    }
                    if cid_hex_has_notdef(t) {
                        error_at(
                            report,
                            "6.2.10.9",
                            "Text operator references .notdef glyph (CID 0x0000)",
                            loc.clone(),
                        );
                        break 'page;
                    }
                    j -= 1;
                }
            }
        }
    }
}

/// §6.3.5 (PDF/A-1) / §6.2.11.4.1 (PDF/A-2+): CID referenced in content stream
/// not present in the embedded CIDFont program (as indicated by the CIDSet).
///
/// For each Type0 (composite CIDFont) subset resource that has a /CIDSet, extracts
/// all 2-byte CID codes from Tj/TJ text operators in the page content stream and
/// checks whether each CID is set in the CIDSet bit array.  A CID that is used for
/// rendering but not in the CIDSet means the font program cannot supply the glyph
/// → §6.3.5 (PDF/A-1) or §6.2.11.4.1 (PDF/A-2+).
///
/// Only §6.2.11.4.1 / §6.3.5 is emitted here; §6.2.11.8 ("renders as .notdef")
/// is NOT emitted because CIDSet absence alone doesn't guarantee the glyph is
/// absent from the font program — the CIDSet may be incomplete. §6.2.11.8 is
/// handled by check_notdef_glyph_reference. (#FP-6.2.11.8)
///
/// This check only fires for subset fonts (ABCDEF+ prefix) that already have a
/// CIDSet; missing/empty CIDSet is handled by check_font_embedding_deep.
pub fn check_cidset_content_coverage(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    // PDF/A-1 §6.3.5 = "font programs shall define all glyphs referenced for rendering".
    // PDF/A-2+ §6.2.11.4.1 = same concept, different clause numbering.
    let glyph_rule = if part == 1 { "6.3.5" } else { "6.2.11.4.1" };
    let xref = pdf.xref();

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        // Build map: resource name → CIDSet bit array (only for subset Type0 fonts).
        let mut font_cidsets: std::collections::HashMap<Vec<u8>, Vec<u8>> =
            std::collections::HashMap::new();
        let fonts = &page.resources().fonts;

        for (name, _) in fonts.entries() {
            let font_dict_opt: Option<Dict<'_>> =
                fonts.get::<Dict<'_>>(name.as_ref()).or_else(|| {
                    fonts
                        .get_ref(name.as_ref())
                        .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                });
            let Some(font_dict) = font_dict_opt else {
                continue;
            };
            if font_dict
                .get::<Name>(keys::SUBTYPE)
                .is_none_or(|s| s.as_ref() != b"Type0")
            {
                continue;
            }
            if let Some(cidset_bits) = get_type0_cidset(&font_dict) {
                font_cidsets.insert(name.as_ref().to_vec(), cidset_bits);
            }
        }

        if font_cidsets.is_empty() {
            continue;
        }

        let Some(content) = page.page_stream() else {
            continue;
        };
        if content.len() > MAX_CONTENT_STREAM_SCAN_SIZE {
            continue;
        }

        let tokens = tokenize_pdf_content(content);
        let loc = format!("page {}", page_idx + 1);
        let n = tokens.len();
        let mut active_cidset: Option<&Vec<u8>> = None;
        // Report at most one glyph-rule violation per page to match veraPDF's
        // single-check-per-rule behaviour.
        // §6.2.11.8 ("renders as .notdef") is NOT emitted here: whether a CID
        // absent from the CIDSet actually renders as .notdef depends on the font
        // program, not just the CIDSet metadata. Emitting §6.2.11.8 based on
        // CIDSet absence alone produces FPs when the CIDSet is wrong/incomplete but
        // the glyph IS in the font. §6.2.11.8 is detected by
        // check_notdef_glyph_reference (literal <0000> codes). (#FP-6.2.11.8)
        'page: for i in 0..n {
            let tok = tokens[i].as_slice();

            // /FontName size Tf — track which Type0 font is active.
            if tok == b"Tf" && i >= 2 {
                let font_name = tokens[i - 2].as_slice();
                active_cidset = font_name
                    .strip_prefix(b"/")
                    .and_then(|n| font_cidsets.get(n));
            }

            let Some(cidset) = active_cidset else {
                continue;
            };

            // Tj / ' / " — the preceding token is the string argument.
            // Handle both hex strings (<...>) and literal strings ((...)) because
            // some test PDFs use literal strings for single-byte CID codes. For
            // Identity-H an odd-length literal string is padded with 0xFF, giving
            // e.g. byte 0x23 → CID 0x23FF. Fixes §6.2.11.4.1 FN for literal Tj.
            if matches!(tok, b"Tj" | b"'" | b"\"") && i >= 1 {
                for cid in extract_cids_from_token(tokens[i - 1].as_slice()) {
                    // CID 0 = .notdef by definition; handled separately.
                    if cid == 0 {
                        continue;
                    }
                    if !cid_in_cidset(cid, cidset) {
                        error_at(
                            report,
                            glyph_rule,
                            format!(
                                "CID 0x{cid:04X} used in content stream but absent from CIDSet"
                            ),
                            loc.clone(),
                        );
                        break 'page;
                    }
                }
            }

            // TJ — scan the preceding array for hex string tokens.
            if tok == b"TJ" {
                let mut j = i as isize - 1;
                while j >= 0 {
                    let t = tokens[j as usize].as_slice();
                    if t == b"[" {
                        break;
                    }
                    if t.starts_with(b"<") || t.starts_with(b"(") {
                        for cid in extract_cids_from_token(t) {
                            if cid == 0 {
                                continue;
                            }
                            if !cid_in_cidset(cid, cidset) {
                                error_at(
                                    report,
                                    glyph_rule,
                                    format!(
                                        "CID 0x{cid:04X} used in content stream but absent from CIDSet"
                                    ),
                                    loc.clone(),
                                );
                                break 'page;
                            }
                        }
                    }
                    j -= 1;
                }
            }
        }
    }
}

/// Resolve the CIDSet bit array for the first CIDFont descendant of a Type0 font
/// dict that is a subset font (ABCDEF+ prefix) and has a non-empty /CIDSet.
fn get_type0_cidset(font_dict: &Dict<'_>) -> Option<Vec<u8>> {
    let descendants = font_dict.get::<Array<'_>>(keys::DESCENDANT_FONTS)?;
    for cid_font in descendants.iter::<Dict<'_>>() {
        let base = cid_font.get::<Name>(keys::BASE_FONT)?;
        let base_str = std::str::from_utf8(base.as_ref()).unwrap_or("");
        if !is_subset_font(base_str) {
            continue;
        }
        let desc = cid_font.get::<Dict<'_>>(keys::FONT_DESC)?;
        let cidset = desc.get::<Stream<'_>>(keys::CID_SET)?;
        let bits = cidset.decoded().ok()?;
        if bits.is_empty() || bits.iter().all(|&b| b == 0) {
            continue;
        }
        return Some(bits);
    }
    None
}

/// Returns `true` if CID `cid` has its bit set in the CIDSet bit array.
/// CIDSet is a bit string where bit 7 of byte 0 corresponds to CID 0 (MSB first).
#[inline]
fn cid_in_cidset(cid: u32, cidset: &[u8]) -> bool {
    let byte_idx = (cid / 8) as usize;
    let bit_pos = 7 - (cid % 8);
    cidset
        .get(byte_idx)
        .map(|&b| (b >> bit_pos) & 1 == 1)
        .unwrap_or(false)
}

/// Extract 2-byte big-endian CID values from either a hex-string (`<XXYY...>`) or
/// a literal-string (`(...)`) token for Type0/Identity-encoded CIDFonts.
///
/// For hex strings each group of 4 hex digits encodes one CID.
/// For literal strings the raw bytes are paired as big-endian 2-byte CIDs; an
/// odd trailing byte is padded with 0xFF (matches how veraPDF processes
/// single-byte literal strings in Identity-H fonts).
fn extract_cids_from_token(tok: &[u8]) -> Vec<u32> {
    if tok.starts_with(b"<") {
        return extract_cids_from_hex(tok);
    }
    if tok.starts_with(b"(") && tok.ends_with(b")") && tok.len() >= 2 {
        // Decode PDF literal string escape sequences into raw bytes.
        let inner = &tok[1..tok.len() - 1];
        let bytes = decode_literal_string_bytes(inner);
        // Pair bytes into 2-byte big-endian CIDs; pad odd trailing byte with 0xFF.
        // This matches how PDF processors handle 1-byte strings in Identity-H fonts.
        let mut cids = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let hi = bytes[i];
            let lo = if i + 1 < bytes.len() {
                bytes[i + 1]
            } else {
                0xFF
            };
            cids.push((hi as u32) << 8 | lo as u32);
            i += 2;
        }
        return cids;
    }
    vec![]
}

/// Decode PDF literal string escape sequences into a raw byte vector.
fn decode_literal_string_bytes(inner: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(inner.len());
    let mut i = 0;
    while i < inner.len() {
        if inner[i] == b'\\' {
            i += 1;
            if i >= inner.len() {
                break;
            }
            match inner[i] {
                b'n' => {
                    out.push(b'\n');
                    i += 1;
                }
                b'r' => {
                    out.push(b'\r');
                    i += 1;
                }
                b't' => {
                    out.push(b'\t');
                    i += 1;
                }
                b'(' | b')' | b'\\' => {
                    out.push(inner[i]);
                    i += 1;
                }
                b'0'..=b'7' => {
                    // Octal escape: up to 3 digits.
                    let start = i;
                    let end = (start + 3).min(inner.len());
                    let mut k = start;
                    while k < end && inner[k].is_ascii() && inner[k] >= b'0' && inner[k] <= b'7' {
                        k += 1;
                    }
                    let octal = std::str::from_utf8(&inner[start..k]).unwrap_or("0");
                    let val = u16::from_str_radix(octal, 8).unwrap_or(0);
                    out.push(val as u8);
                    i = k;
                }
                _ => {
                    out.push(inner[i]);
                    i += 1;
                }
            }
        } else {
            out.push(inner[i]);
            i += 1;
        }
    }
    out
}

/// Extract 2-byte big-endian CID values from a PDF hex-string token (`<XXYY...>`).
/// Each group of 4 hex digits encodes one CID (2 bytes, big-endian).
fn extract_cids_from_hex(tok: &[u8]) -> Vec<u32> {
    if !tok.starts_with(b"<") || !tok.ends_with(b">") {
        return vec![];
    }
    let hex = &tok[1..tok.len() - 1];
    let digits: Vec<u8> = hex
        .iter()
        .copied()
        .filter(|b| b.is_ascii_hexdigit())
        .collect();
    let mut cids = Vec::new();
    let mut i = 0;
    while i + 4 <= digits.len() {
        let hi = hex_nibble(digits[i]) << 4 | hex_nibble(digits[i + 1]);
        let lo = hex_nibble(digits[i + 2]) << 4 | hex_nibble(digits[i + 3]);
        cids.push((hi as u32) << 8 | lo as u32);
        i += 4;
    }
    cids
}

#[inline]
fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

/// §6.2.10.9 — ToUnicode C0 forbidden codepoints (PDF/A-4).
///
/// Scans font ToUnicode CMaps for destination codepoints in the C0 control
/// range (U+0001–U+0008, U+000B–U+000C, U+000E–U+001F) which are forbidden.
/// TAB (U+0009), LF (U+000A) and CR (U+000D) are permitted.
pub fn check_tounicode_c0_forbidden(pdf: &Pdf, report: &mut ComplianceReport) {
    for_each_font(pdf, |name, font_dict, page_idx| {
        let Some(cmap_stream) = font_dict.get::<Stream<'_>>(keys::TO_UNICODE) else {
            return;
        };
        let Ok(data) = cmap_stream.decoded() else {
            return;
        };
        let text = String::from_utf8_lossy(&data);
        let mut in_bfchar = false;
        let mut in_bfrange = false;
        for line in text.lines() {
            let t = line.trim();
            if t.ends_with("beginbfchar") {
                in_bfchar = true;
                continue;
            }
            if t == "endbfchar" {
                in_bfchar = false;
                continue;
            }
            if t.ends_with("beginbfrange") {
                in_bfrange = true;
                continue;
            }
            if t == "endbfrange" {
                in_bfrange = false;
                continue;
            }
            if !in_bfchar && !in_bfrange {
                continue;
            }
            let tokens: Vec<u32> = t
                .split('<')
                .skip(1)
                .filter_map(|chunk| {
                    let end = chunk.find('>')?;
                    u32::from_str_radix(&chunk[..end], 16).ok()
                })
                .collect();
            let dst_idx = if in_bfchar { 1 } else { 2 };
            if let Some(&val) = tokens.get(dst_idx) {
                // C0 forbidden: U+0001–U+0008, U+000B–U+000C, U+000E–U+001F
                // (TAB=0x09, LF=0x0A, CR=0x0D are allowed)
                if matches!(val, 0x0001..=0x0008 | 0x000B..=0x000C | 0x000E..=0x001F) {
                    error_at(
                        report,
                        "6.2.10.9",
                        format!("Font {name} ToUnicode maps to forbidden C0 codepoint U+{val:04X}"),
                        format!("page {}", page_idx + 1),
                    );
                    return;
                }
            }
        }
    });
}

/// §6.2.10.9 — ToUnicode coverage for Type0 (CID) font rendered glyphs (PDF/A-4).
///
/// Scans page content streams for 2-byte CIDs rendered with Type0 fonts.
/// Each CID must have an entry in the font's ToUnicode CMap.
pub fn check_type0_cid_tounicode_coverage(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        // Build font name → covered CID set for all Type0 fonts that have ToUnicode.
        let mut font_covered: std::collections::HashMap<Vec<u8>, std::collections::HashSet<u32>> =
            std::collections::HashMap::new();

        let fonts = &page.resources().fonts;
        for (name, _) in fonts.entries() {
            let font_dict_opt: Option<Dict<'_>> =
                fonts.get::<Dict<'_>>(name.as_ref()).or_else(|| {
                    fonts
                        .get_ref(name.as_ref())
                        .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                });
            let Some(fd) = font_dict_opt else { continue };
            if fd
                .get::<Name>(keys::SUBTYPE)
                .is_none_or(|s| s.as_ref() != b"Type0")
            {
                continue;
            }
            // Only check fonts that HAVE a ToUnicode; missing ToUnicode = §6.2.10.7.
            let Some(cmap_stream) = fd.get::<Stream<'_>>(keys::TO_UNICODE) else {
                continue;
            };
            let Ok(data) = cmap_stream.decoded() else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(&data) else {
                continue;
            };
            let covered = parse_tounicode_source_codes(text);
            font_covered.insert(name.as_ref().to_vec(), covered);
        }

        if font_covered.is_empty() {
            continue;
        }

        let Some(content) = page.page_stream() else {
            continue;
        };
        if content.len() > MAX_CONTENT_STREAM_SCAN_SIZE {
            continue;
        }

        let tokens = tokenize_pdf_content(content);
        let n = tokens.len();
        let loc = format!("page {}", page_idx + 1);
        let mut active_font: Option<Vec<u8>> = None;

        'page: for i in 0..n {
            let tok = tokens[i].as_slice();

            if tok == b"Tf" && i >= 2 {
                let font_name = tokens[i - 2].as_slice();
                active_font = font_name.strip_prefix(b"/").and_then(|n| {
                    if font_covered.contains_key(n) {
                        Some(n.to_vec())
                    } else {
                        None
                    }
                });
            }

            let Some(ref font_key) = active_font else {
                continue;
            };
            let Some(covered) = font_covered.get(font_key.as_slice()) else {
                continue;
            };

            if matches!(tok, b"Tj" | b"'" | b"\"") && i >= 1 {
                if let Some(cid) = first_cid_not_in_tounicode(&tokens[i - 1], covered) {
                    error_at(
                        report,
                        "6.2.10.9",
                        format!("Type0 font CID 0x{cid:04X} not covered by ToUnicode"),
                        loc.clone(),
                    );
                    break 'page;
                }
            }

            if tok == b"TJ" {
                let mut j = i as isize - 1;
                while j >= 0 {
                    let t = &tokens[j as usize];
                    if t.as_slice() == b"[" {
                        break;
                    }
                    if let Some(cid) = first_cid_not_in_tounicode(t, covered) {
                        error_at(
                            report,
                            "6.2.10.9",
                            format!("Type0 font CID 0x{cid:04X} not covered by ToUnicode"),
                            loc.clone(),
                        );
                        break 'page;
                    }
                    j -= 1;
                }
            }
        }
    }
}

/// Return the first 2-byte CID from a hex or literal string token that is NOT in `covered`.
///
/// - Hex token `<XXYYZZ…>`: each 4 nibbles encode one 2-byte CID.
/// - Literal token `(...)`: bytes are paired as 2-byte CIDs (Identity-H style).
///   An odd final byte is treated as CID `0x00XX`.
fn first_cid_not_in_tounicode(tok: &[u8], covered: &std::collections::HashSet<u32>) -> Option<u32> {
    if let Some(inner) = tok.strip_prefix(b"<").and_then(|t| t.strip_suffix(b">")) {
        if inner.is_empty() {
            return None;
        }
        let nibbles: Vec<u8> = inner
            .iter()
            .filter(|&&b| b.is_ascii_hexdigit())
            .map(|&b| (b as char).to_digit(16).unwrap_or(0) as u8)
            .collect();
        let mut k = 0;
        while k + 3 < nibbles.len() {
            let cid = ((nibbles[k] as u32) << 12)
                | ((nibbles[k + 1] as u32) << 8)
                | ((nibbles[k + 2] as u32) << 4)
                | (nibbles[k + 3] as u32);
            if !covered.contains(&cid) {
                return Some(cid);
            }
            k += 4;
        }
        None
    } else if let Some(inner) = tok.strip_prefix(b"(").and_then(|t| t.strip_suffix(b")")) {
        // Literal string: pair bytes as 2-byte CIDs (for Type0/Identity-H fonts).
        // Odd trailing byte is treated as CID 0x00XX (high-byte zero padding).
        let mut k = 0;
        while k < inner.len() {
            let cid = if k + 1 < inner.len() {
                ((inner[k] as u32) << 8) | inner[k + 1] as u32
            } else {
                inner[k] as u32
            };
            if !covered.contains(&cid) {
                return Some(cid);
            }
            k += 2;
        }
        None
    } else {
        None
    }
}

/// Tokenize a PDF content stream respecting PDF delimiter characters.
///
/// Unlike `split_ascii_whitespace`, this splits at PDF delimiter boundaries
/// (`< > ( ) [ ] / %`) so `<0000>Tj` yields two tokens: `<0000>` and `Tj`.
fn tokenize_pdf_content(content: &[u8]) -> Vec<Vec<u8>> {
    let mut tokens: Vec<Vec<u8>> = Vec::new();
    let mut i = 0;
    let len = content.len();

    while i < len {
        let b = content[i];

        // Whitespace — skip.
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Comment — skip to end of line.
        if b == b'%' {
            while i < len && content[i] != b'\n' && content[i] != b'\r' {
                i += 1;
            }
            continue;
        }

        // `<<` dict-open or `<hex>` hex string.
        if b == b'<' {
            if i + 1 < len && content[i + 1] == b'<' {
                tokens.push(b"<<".to_vec());
                i += 2;
                continue;
            }
            let start = i;
            i += 1;
            while i < len && content[i] != b'>' {
                i += 1;
            }
            if i < len {
                i += 1; // consume '>'
            }
            tokens.push(content[start..i].to_vec());
            continue;
        }

        // `>>` dict-close.
        if b == b'>' {
            if i + 1 < len && content[i + 1] == b'>' {
                tokens.push(b">>".to_vec());
                i += 2;
            } else {
                tokens.push(vec![b]);
                i += 1;
            }
            continue;
        }

        // Literal string `(...)` with balanced parentheses.
        if b == b'(' {
            let start = i;
            i += 1;
            let mut depth = 1i32;
            while i < len && depth > 0 {
                match content[i] {
                    b'\\' => {
                        i += 1;
                        if i < len {
                            i += 1;
                        }
                    }
                    b'(' => {
                        depth += 1;
                        i += 1;
                    }
                    b')' => {
                        depth -= 1;
                        i += 1;
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            tokens.push(content[start..i].to_vec());
            continue;
        }

        // Single-char array delimiters.
        if b == b'[' || b == b']' {
            tokens.push(vec![b]);
            i += 1;
            continue;
        }

        // Name: `/name`.
        if b == b'/' {
            let start = i;
            i += 1;
            while i < len && !content[i].is_ascii_whitespace() && !is_pdf_delim(content[i]) {
                i += 1;
            }
            tokens.push(content[start..i].to_vec());
            continue;
        }

        // Regular token: operator or number.
        let start = i;
        while i < len && !content[i].is_ascii_whitespace() && !is_pdf_delim(content[i]) {
            i += 1;
        }
        if i > start {
            tokens.push(content[start..i].to_vec());
        }
    }
    tokens
}

/// Return `true` if `tok` is a `<hex>` token containing CID 0x0000 (.notdef).
///
/// CID text strings encode each character as a 2-byte big-endian code point.
/// CID 0 = .notdef in all CID-keyed fonts: `<0000>` = one .notdef character.
/// Whitespace embedded in the hex string (allowed by PDF spec) is ignored.
fn cid_hex_has_notdef(tok: &[u8]) -> bool {
    if !tok.starts_with(b"<") || !tok.ends_with(b">") {
        return false;
    }
    let hex = &tok[1..tok.len() - 1];
    // Collect only hex-digit bytes, ignoring embedded whitespace.
    let digits: Vec<u8> = hex
        .iter()
        .copied()
        .filter(|b| b.is_ascii_hexdigit())
        .collect();
    // Each 2-byte CID is 4 hex digits.  Scan groups of 4 for "0000".
    let mut ci = 0;
    while ci + 4 <= digits.len() {
        if digits[ci] == b'0'
            && digits[ci + 1] == b'0'
            && digits[ci + 2] == b'0'
            && digits[ci + 3] == b'0'
        {
            return true;
        }
        ci += 4;
    }
    false
}

/// True if `b` is a PDF delimiter character that terminates a regular token.
#[inline]
fn is_pdf_delim(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// Extract source codes from a ToUnicode CMap (beginbfchar and beginbfrange sections).
fn parse_tounicode_source_codes(cmap: &str) -> std::collections::HashSet<u32> {
    let mut codes = std::collections::HashSet::new();
    let mut mode: u8 = 0; // 0=none, 1=bfchar, 2=bfrange

    for line in cmap.lines() {
        let t = line.trim();
        if t.ends_with("beginbfchar") {
            mode = 1;
            continue;
        }
        if t.ends_with("beginbfrange") {
            mode = 2;
            continue;
        }
        if t == "endbfchar" || t == "endbfrange" {
            mode = 0;
            continue;
        }
        match mode {
            1 => {
                // <srccode> <dstcode>
                if let Some(code) = extract_cmap_hex(t, 0) {
                    codes.insert(code);
                }
            }
            2 => {
                // <srclo> <srchi> <dst>
                if let (Some(lo), Some(hi)) = (extract_cmap_hex(t, 0), extract_cmap_hex(t, 1)) {
                    for c in lo..=hi {
                        codes.insert(c);
                    }
                }
            }
            _ => {}
        }
    }
    codes
}

/// Extract the N-th hex value (0-based) from a CMap line like `<0020> <0048>`.
fn extract_cmap_hex(s: &str, nth: usize) -> Option<u32> {
    let mut found = 0;
    let mut pos = 0;
    loop {
        let start = s[pos..].find('<')? + pos + 1;
        let end = s[start..].find('>')? + start;
        if found == nth {
            return u32::from_str_radix(s[start..end].trim(), 16).ok();
        }
        found += 1;
        pos = end + 1;
    }
}

const STANDARD_14: &[&str] = &[
    "Courier",
    "Courier-Bold",
    "Courier-BoldOblique",
    "Courier-Oblique",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-BoldOblique",
    "Helvetica-Oblique",
    "Times-Roman",
    "Times-Bold",
    "Times-BoldItalic",
    "Times-Italic",
    "Symbol",
    "ZapfDingbats",
];

fn is_standard_14(name: &str) -> bool {
    let base = if is_subset_font(name) {
        &name[7..]
    } else {
        name
    };
    STANDARD_14.contains(&base)
}

/// Validate font /Widths array presence (§6.3.5).
pub fn check_font_widths(pdf: &Pdf, report: &mut ComplianceReport) {
    for_each_font(pdf, |name, font_dict, page_idx| {
        if let Some(subtype) = font_dict.get::<Name>(keys::SUBTYPE) {
            if subtype.as_ref() == b"Type0" {
                return;
            }
        }

        if is_standard_14(name) {
            return;
        }

        if font_dict.get::<Array<'_>>(keys::WIDTHS).is_none()
            && font_dict.get::<Dict<'_>>(keys::FONT_DESC).is_some()
        {
            error_at(
                report,
                "6.3.5",
                format!("Font {name} missing /Widths array"),
                format!("page {}", page_idx + 1),
            );
        }
    });
}

/// Check that per-glyph advance widths in an embedded CFF/Type1C font program
/// are consistent with the /Widths array declared in the font dict (§6.2.11.5 /
/// §6.2.10.5 in PDF/A-4).
///
/// veraPDF emits this clause whenever the rounded width from the charstring
/// differs by more than 1 unit from the corresponding /Widths entry.
/// We emit rule "6.3.5-fw" which is remapped to §6.2.11.5 (parts 2/3) or
/// §6.2.10.5 (part 4) in pdfa.rs. (#467)
pub fn check_font_program_widths(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for_each_font(pdf, |name, font_dict, page_idx| {
        let subtype = font_dict.get::<Name>(keys::SUBTYPE);
        let subtype_bytes = subtype.as_ref().map(|s| s.as_ref());

        // Type0 fonts: check CIDFontType2 (TrueType) descendant widths. (#467)
        if subtype_bytes == Some(b"Type0") {
            check_cidfont_type2_widths(font_dict, xref, name, page_idx, report);
            return;
        }

        // Type3 fonts: compare d0/d1 advance widths in CharProcs against /Widths.
        // Type3 fonts have no FontDescriptor (they're defined inline), so the
        // standard font-file width check below does not apply. (§6.2.10.5, ISO 19005-4)
        if subtype_bytes == Some(b"Type3") {
            check_type3_charproc_widths(font_dict, xref, name, page_idx, report);
            return;
        }

        let Some(first_char) = font_dict.get::<i32>(keys::FIRST_CHAR) else {
            return;
        };
        let Some(last_char) = font_dict.get::<i32>(keys::LAST_CHAR) else {
            return;
        };
        let Some(widths_arr) = font_dict.get::<Array<'_>>(keys::WIDTHS) else {
            return;
        };
        // FontDescriptor is almost always an indirect reference. Add xref fallback
        // to ensure resolution succeeds even when get::<Dict>() can't follow refs.
        // (#FN-6.2.10.5)
        let Some(desc) = font_dict.get::<Dict<'_>>(keys::FONT_DESC).or_else(|| {
            font_dict
                .get_ref(keys::FONT_DESC)
                .and_then(|r| xref.get::<Dict<'_>>(r.into()))
        }) else {
            return;
        };

        // Handle TrueType simple fonts (FontFile2). FontFile2 is also typically
        // an indirect ref — apply the same xref fallback. (#FN-6.2.10.5)
        let ff2_opt = desc.get::<Stream<'_>>(keys::FONT_FILE2).or_else(|| {
            desc.get_ref(keys::FONT_FILE2)
                .and_then(|r| xref.get::<Stream<'_>>(r.into()))
        });
        if let Some(ff2) = ff2_opt {
            if let Ok(font_data) = ff2.decoded() {
                let pdf_widths: Vec<i32> = widths_arr.iter::<i32>().collect();
                check_truetype_simple_widths(
                    &font_data,
                    font_dict,
                    &desc,
                    xref,
                    name,
                    first_char,
                    last_char,
                    &pdf_widths,
                    page_idx,
                    report,
                );
            }
            return;
        }

        // Check raw Type1 (FontFile) embeddings (#467, §6.3.6 for PDF/A-1).
        // FontFile may be an indirect ref — apply xref fallback. (#FN-6.3.6)
        let ff_opt = desc.get::<Stream<'_>>(keys::FONT_FILE).or_else(|| {
            desc.get_ref(keys::FONT_FILE)
                .and_then(|r| xref.get::<Stream<'_>>(r.into()))
        });
        if let Some(ff) = ff_opt {
            if let Ok(font_data) = ff.decoded() {
                let pdf_widths: Vec<i32> = widths_arr.iter::<i32>().collect();
                let missing_width = desc.get::<i32>(keys::MISSING_WIDTH);
                check_type1_simple_widths(
                    &font_data,
                    font_dict,
                    xref,
                    name,
                    first_char,
                    last_char,
                    &pdf_widths,
                    missing_width,
                    page_idx,
                    report,
                );
            }
            return;
        }

        // Only check FontFile3 (CFF) embeddings — Type1 charstring parsing is
        // done separately and is unreliable for non-subset fonts (see memory).
        // FontFile3 may be an indirect ref — apply xref fallback. (#FN-6.3.6-cff)
        let Some(ff3) = desc.get::<Stream<'_>>(keys::FONT_FILE3).or_else(|| {
            desc.get_ref(keys::FONT_FILE3)
                .and_then(|r| xref.get::<Stream<'_>>(r.into()))
        }) else {
            return;
        };
        let Ok(cff_data) = ff3.decoded() else {
            return;
        };
        // Parse the CFF table.
        let Some(table) = cff_parser::Table::parse(&cff_data) else {
            return;
        };

        // Collect Widths entries.
        let pdf_widths: Vec<i32> = widths_arr.iter::<i32>().collect();

        let first = first_char as usize;
        let last = last_char as usize;
        if last < first || pdf_widths.len() < last - first + 1 {
            return;
        }

        // §6.2.10.5: skip codes whose /Widths entry equals /MissingWidth — those are
        // "unused" sentinel entries, not declared zero-widths. Same logic as the TrueType
        // check. (#FN-6.2.10.5-cff)
        let missing_width: Option<i32> = desc.get::<i32>(keys::MISSING_WIDTH);

        let loc = format!("page {}", page_idx + 1);

        // For each character code in [FirstChar..LastChar], compare the CFF
        // charstring advance width with the PDF /Widths entry.
        for code in first..=last {
            let idx = code - first;
            let pdf_w = pdf_widths[idx];
            // §6.2.10.5: pdf_w=0 is a declared width — skip only if it equals
            // MissingWidth (unused-code sentinel). DO NOT skip 0-width entries here.
            // (#FN-6.2.10.5-cff)
            if missing_width.is_some_and(|mw| mw == pdf_w) {
                continue;
            }

            // Map code to GID via the CFF encoding.
            let gid = match table.glyph_index(code as u8) {
                Some(g) => g,
                None => continue,
            };
            // GID 0 is always .notdef in CFF. When glyph_index(code) returns GID 0
            // it means the code is not explicitly encoded — the CFF fell back to
            // .notdef. Comparing the .notdef advance width against the PDF /Widths
            // entry is meaningless and produces FPs (veraPDF also skips GID 0).
            // (#FP-6.2.11.5-gid0)
            if gid.0 == 0 {
                continue;
            }
            let Some(cff_w) = table.glyph_width(gid) else {
                continue;
            };
            let cff_w_i32 = cff_w as i32;

            // Tolerance: 1 unit (veraPDF uses strict equality but we allow ±1
            // to avoid fp rounding FPs).
            if (cff_w_i32 - pdf_w).abs() > 1 {
                error_at(
                    report,
                    "6.3.5-fw",
                    format!(
                        "Font {name} glyph at code {code}: \
                         CFF width {cff_w} != PDF /Widths[{idx}] {pdf_w}"
                    ),
                    loc.clone(),
                );
                // Report only the first mismatch per font to avoid flooding.
                return;
            }
        }
    });
}

/// §6.3.5-fw — Check Type3 font CharProc d0/d1 advance widths against /Widths array.
///
/// For each CharProc glyph, parses the content stream to extract the horizontal
/// advance (`ux`) from the `d0` or `d1` operator and compares it with the
/// corresponding /Widths entry. Any mismatch fires rule "6.3.5-fw". (ISO 19005-4 §6.2.10.5)
fn check_type3_charproc_widths(
    font_dict: &Dict<'_>,
    xref: &pdf_syntax::xref::XRef,
    name: &str,
    page_idx: usize,
    report: &mut ComplianceReport,
) {
    let Some(charprocs) = font_dict.get::<Dict<'_>>(b"CharProcs" as &[u8]) else {
        return;
    };
    let Some(first_char) = font_dict.get::<i32>(keys::FIRST_CHAR) else {
        return;
    };
    let Some(widths_arr) = font_dict.get::<Array<'_>>(keys::WIDTHS) else {
        return;
    };

    // Build code → glyph-name mapping from the font's Encoding /Differences array.
    let mut code_to_name: std::collections::HashMap<i32, Vec<u8>> =
        std::collections::HashMap::new();
    let enc_opt: Option<Dict<'_>> = font_dict.get::<Dict<'_>>(keys::ENCODING).or_else(|| {
        font_dict
            .get_ref(keys::ENCODING)
            .and_then(|r| xref.get::<Dict<'_>>(r.into()))
    });
    if let Some(enc) = enc_opt {
        if let Some(diffs) = enc.get::<Array<'_>>(b"Differences" as &[u8]) {
            let mut current_code = 0i32;
            for item in diffs.iter::<Object<'_>>() {
                match item {
                    Object::Number(n) => current_code = n.as_i64() as i32,
                    Object::Name(n) => {
                        code_to_name.insert(current_code, n.as_ref().to_vec());
                        current_code += 1;
                    }
                    _ => {}
                }
            }
        }
    }

    let pdf_widths: Vec<i32> = widths_arr.iter::<i32>().collect();
    let loc = format!("page {}", page_idx + 1);

    for (glyph_name_key, _) in charprocs.entries() {
        let glyph_bytes = glyph_name_key.as_ref();

        // Find the character code for this glyph name.
        let code = code_to_name
            .iter()
            .find(|(_, n)| n.as_slice() == glyph_bytes)
            .map(|(c, _)| *c);
        let Some(code) = code else { continue };

        let idx = (code - first_char) as usize;
        if idx >= pdf_widths.len() {
            continue;
        }
        let pdf_w = pdf_widths[idx];

        // Resolve the CharProc content stream (may be an indirect reference).
        let stream_opt: Option<Stream<'_>> =
            charprocs.get::<Stream<'_>>(glyph_bytes).or_else(|| {
                charprocs
                    .get_ref(glyph_bytes)
                    .and_then(|r| xref.get::<Stream<'_>>(r.into()))
            });
        let Some(stream) = stream_opt else { continue };
        let Ok(stream_data) = stream.decoded() else {
            continue;
        };

        // Extract the horizontal advance width from the d0 or d1 operator.
        let Some(ux) = parse_type3_charproc_width(&stream_data) else {
            continue;
        };

        // Type3 widths must match exactly (no 1-unit tolerance; d0/d1 ux is an integer).
        if ux != pdf_w {
            let glyph_str = std::str::from_utf8(glyph_bytes).unwrap_or("?");
            error_at(
                report,
                "6.3.5-fw",
                format!(
                    "Type3 font {name} glyph '{glyph_str}' d0/d1 width {ux} != /Widths[{idx}] {pdf_w}"
                ),
                loc.clone(),
            );
            return; // Report first mismatch per font to avoid flooding.
        }
    }
}

/// Parse the first `d0` or `d1` operator in a Type3 CharProc content stream and
/// return the horizontal advance width (`ux`, the first numeric operand).
///
/// `d0 ux uy` → ux is the token two positions before `d0`.
/// `d1 ux uy llx lly urx ury` → ux is the token six positions before `d1`.
fn parse_type3_charproc_width(data: &[u8]) -> Option<i32> {
    let text = std::str::from_utf8(data).ok()?;
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for (i, token) in tokens.iter().enumerate() {
        if *token == "d0" && i >= 2 {
            return tokens[i - 2].parse().ok();
        }
        if *token == "d1" && i >= 6 {
            return tokens[i - 6].parse().ok();
        }
    }
    None
}

/// §6.3.5-fw — Check CIDFontType2 (TrueType) and CIDFontType0 (CFF) /W widths
/// against the embedded font program.
///
/// For Type0 fonts, inspect each CIDFont descendant:
///   - CIDFontType2: parse FontFile2 (TrueType) with ttf_parser
///   - CIDFontType0: parse FontFile3 (CFF) with cff_parser (#FN-6.2.11.5)
///
/// Compare declared CID widths in /W against actual glyph advance widths.
fn check_cidfont_type2_widths(
    type0_dict: &Dict<'_>,
    xref: &pdf_syntax::xref::XRef,
    name: &str,
    page_idx: usize,
    report: &mut ComplianceReport,
) {
    let Some(descendants) = type0_dict.get::<Array<'_>>(keys::DESCENDANT_FONTS) else {
        return;
    };
    for cid_font in descendants.iter::<Dict<'_>>() {
        let subtype_bytes = cid_font
            .get::<Name>(keys::SUBTYPE)
            .map(|s| s.as_ref().to_vec());
        let is_type2 = subtype_bytes.as_deref() == Some(b"CIDFontType2");
        let is_type0 = subtype_bytes.as_deref() == Some(b"CIDFontType0");
        if !is_type2 && !is_type0 {
            continue;
        }
        let cid_name: String = cid_font
            .get::<Name>(keys::BASE_FONT)
            .map(|n| std::str::from_utf8(n.as_ref()).unwrap_or(name).to_string())
            .unwrap_or_else(|| name.to_string());

        // FontDescriptor is almost always an indirect ref in CIDFont dicts. (#FN-6.2.11.5)
        let Some(desc) = cid_font.get::<Dict<'_>>(keys::FONT_DESC).or_else(|| {
            cid_font
                .get_ref(keys::FONT_DESC)
                .and_then(|r| xref.get::<Dict<'_>>(r.into()))
        }) else {
            continue;
        };

        // CIDFontType0 (CFF): use FontFile3 + cff_parser. (#FN-6.2.11.5)
        if is_type0 {
            // FontFile3 may also be an indirect ref.
            let Some(ff3) = desc.get::<Stream<'_>>(keys::FONT_FILE3).or_else(|| {
                desc.get_ref(keys::FONT_FILE3)
                    .and_then(|r| xref.get::<Stream<'_>>(r.into()))
            }) else {
                continue;
            };
            let Ok(cff_data) = ff3.decoded() else {
                continue;
            };
            let Some(table) = cff_parser::Table::parse(&cff_data) else {
                continue;
            };
            let loc = format!("page {}", page_idx + 1);
            let w_map: std::collections::HashMap<u32, i32> =
                if let Some(w_arr) = cid_font.get::<Array<'_>>(keys::W) {
                    parse_cidfont_w_array(&w_arr)
                } else {
                    std::collections::HashMap::new()
                };
            for (cid, pdf_w) in &w_map {
                if *pdf_w == 0 {
                    continue;
                }
                // For CID-keyed CFF fonts, GID == CID (direct mapping).
                let Some(cff_w) = table.glyph_width(cff_parser::GlyphId(*cid as u16)) else {
                    continue;
                };
                let cff_w_i32 = cff_w as i32;
                // Allow ±1 for CFF rounding.
                if (cff_w_i32 - pdf_w).abs() > 1 {
                    error_at(
                        report,
                        "6.3.5-fw",
                        format!(
                            "Font {cid_name} CID {cid}: CFF width {cff_w_i32} \
                             != PDF /W entry {pdf_w}"
                        ),
                        loc.clone(),
                    );
                    return; // First mismatch per font only
                }
            }
            continue; // Done with CIDFontType0
        }

        // CIDFontType2 (TrueType): use FontFile2 + ttf_parser.
        // FontFile2 may be an indirect ref. (#FN-6.2.11.5)
        let Some(ff2) = desc.get::<Stream<'_>>(keys::FONT_FILE2).or_else(|| {
            desc.get_ref(keys::FONT_FILE2)
                .and_then(|r| xref.get::<Stream<'_>>(r.into()))
        }) else {
            continue;
        };
        let Ok(font_data) = ff2.decoded() else {
            continue;
        };
        let Ok(face) = ttf_parser::Face::parse(&font_data, 0) else {
            continue;
        };
        let upem = face.units_per_em() as f64;
        if upem <= 0.0 {
            continue;
        }

        // Per PDF spec, absent CIDToGIDMap defaults to /Identity (CID == GID).
        // Stream CIDToGIDMap entries are custom non-Identity mappings — skip those.
        // Check both direct and indirect stream refs. (#FN-6.3.6)
        let has_cidtogid_stream = cid_font.get::<Stream<'_>>(keys::CID_TO_GID_MAP).is_some()
            || cid_font
                .get_ref(keys::CID_TO_GID_MAP)
                .and_then(|r| xref.get::<Stream<'_>>(r.into()))
                .is_some();
        if has_cidtogid_stream {
            continue; // Custom CID→GID stream — CID ≠ GID in general; skip
        }
        let cidtogid_is_identity = cid_font
            .get::<Name>(keys::CID_TO_GID_MAP)
            .map(|n| n.as_ref() == keys::IDENTITY)
            .unwrap_or(true); // absent = default /Identity per PDF spec
        if !cidtogid_is_identity {
            continue;
        }

        let loc = format!("page {}", page_idx + 1);

        // Parse /W array: [c1 [w1 w2 ...] c2 c3 w ...] (optional).
        let w_map: std::collections::HashMap<u32, i32> =
            if let Some(w_arr) = cid_font.get::<Array<'_>>(keys::W) {
                parse_cidfont_w_array(&w_arr)
            } else {
                std::collections::HashMap::new()
            };

        // Check each explicit /W entry against the font program.
        for (cid, pdf_w) in &w_map {
            if *pdf_w == 0 {
                // Skip entries that are explicitly 0 — this means "glyph absent/unused".
                continue;
            }
            let gid = ttf_parser::GlyphId(*cid as u16);
            let Some(advance) = face.glyph_hor_advance(gid) else {
                continue;
            };
            if advance == 0 {
                continue; // Skip .notdef or genuinely 0-width glyphs
            }
            let font_w = (advance as f64 * 1000.0 / upem).round() as i32;

            // Allow ±2 units for TrueType rounding (font-unit fractions).
            if (font_w - pdf_w).abs() > 2 {
                error_at(
                    report,
                    "6.3.5-fw",
                    format!(
                        "Font {cid_name} CID {cid}: TrueType width {font_w} \
                         != PDF /W entry {pdf_w}"
                    ),
                    loc.clone(),
                );
                return; // First mismatch per font only
            }
        }

        // Check /DW (DefaultWidth) against the actual font advance for all GIDs
        // not explicitly covered by /W. §6.3.5/§6.3.6 requires the default width
        // declared in the PDF to be consistent with the font program.
        // Fixes FN for cs-isartor-6-3-5-t01-fail-b where DW=1000 but GID 1674
        // has advance=750 and is not listed in /W.
        if let Some(dw) = cid_font.get::<i32>(keys::DW) {
            let num_glyphs = face.number_of_glyphs() as u32;
            for gid_u32 in 0..num_glyphs {
                if gid_u32 == 0 {
                    continue; // GID 0 is always .notdef — never a document character; skip
                }
                if w_map.contains_key(&gid_u32) {
                    continue; // Covered by /W — already checked above
                }
                let gid = ttf_parser::GlyphId(gid_u32 as u16);
                let Some(advance) = face.glyph_hor_advance(gid) else {
                    continue;
                };
                if advance == 0 {
                    continue; // Skip genuinely 0-width glyphs (e.g. space variants)
                }
                let font_w = (advance as f64 * 1000.0 / upem).round() as i32;
                if (font_w - dw).abs() > 2 {
                    error_at(
                        report,
                        "6.3.5-fw",
                        format!(
                            "Font {cid_name} GID {gid_u32}: TrueType advance {font_w} \
                             != /DW {dw} (not in /W)"
                        ),
                        loc.clone(),
                    );
                    return; // First mismatch per font only
                }
            }
        }
    }
}

/// Parse a CIDFont /W array into a HashMap of (CID → width).
///
/// /W format: [c1 [w1 w2 ...] c2 c3 w ...]
/// First form: c1 followed by an array gives individual widths starting at c1.
/// Second form: c2 c3 w gives the same width w for CIDs c2..=c3.
fn parse_cidfont_w_array(w_arr: &Array<'_>) -> std::collections::HashMap<u32, i32> {
    use pdf_syntax::object::MaybeRef;

    let mut map = std::collections::HashMap::new();
    let raw: Vec<_> = w_arr.raw_iter().collect();
    let mut i = 0;
    while i < raw.len() {
        // First element is always a CID integer
        let c1 = match &raw[i] {
            MaybeRef::NotRef(Object::Number(n)) => n.as_f64() as u32,
            _ => {
                i += 1;
                continue;
            }
        };
        i += 1;
        if i >= raw.len() {
            break;
        }
        match &raw[i] {
            // Array form: c1 [w1 w2 ...]
            MaybeRef::NotRef(Object::Array(inner)) => {
                for (j, w_obj) in inner.iter::<i32>().enumerate() {
                    map.insert(c1 + j as u32, w_obj);
                }
                i += 1;
            }
            // Range form: c2 c3 w
            MaybeRef::NotRef(Object::Number(c3_n)) => {
                let c3 = c3_n.as_f64() as u32;
                i += 1;
                if i >= raw.len() {
                    break;
                }
                let w = match &raw[i] {
                    MaybeRef::NotRef(Object::Number(wn)) => wn.as_f64() as i32,
                    _ => {
                        i += 1;
                        continue;
                    }
                };
                i += 1;
                for cid in c1..=c3 {
                    map.insert(cid, w);
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    map
}

/// §6.3.5-fw — Check simple TrueType font /Widths against font program.
///
/// Uses ttf-parser to look up advance widths by Unicode codepoint.
/// Supports WinAnsiEncoding and MacRomanEncoding. (#467)
///
/// Also resolves /Encoding given as an indirect reference to an Encoding dict,
/// using the /BaseEncoding key when the direct name is unavailable.
/// This fixes FNs where veraPDF fires §6.3.6 but we skipped the check because
/// we couldn't read the encoding name from an indirect reference (cs-isartor-fail-d).
#[allow(clippy::too_many_arguments)]
fn check_truetype_simple_widths(
    font_data: &[u8],
    font_dict: &Dict<'_>,
    desc: &Dict<'_>,
    xref: &pdf_syntax::xref::XRef,
    name: &str,
    first_char: i32,
    last_char: i32,
    pdf_widths: &[i32],
    page_idx: usize,
    report: &mut ComplianceReport,
) {
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return;
    };
    let upem = face.units_per_em() as f64;
    if upem <= 0.0 {
        return;
    }

    let first = first_char as usize;
    let last = last_char as usize;
    if last < first || pdf_widths.len() < last - first + 1 {
        return;
    }

    // §6.2.11.5 applies only to "characters used in the document". PDF creators
    // set the /Widths entry for unused codes to /MissingWidth (the fallback width
    // for codes not represented in the subset). If pdf_w == missing_width, treat
    // the code as "not used" and skip the width comparison. (#FP-6.2.11.5)
    let missing_width: Option<i32> = desc.get::<i32>(keys::MISSING_WIDTH);

    // Determine the encoding name. Try direct /Encoding name first; if /Encoding is
    // an indirect reference to a dict, read /BaseEncoding from that dict.
    let enc_bytes: Vec<u8> = font_dict
        .get::<Name>(keys::ENCODING)
        .map(|n| n.as_ref().to_vec())
        .or_else(|| {
            // /Encoding is an indirect ref or a dict: resolve and read /BaseEncoding.
            font_dict
                .get::<Dict<'_>>(keys::ENCODING)
                .or_else(|| {
                    font_dict
                        .get_ref(keys::ENCODING)
                        .and_then(|r| xref.get::<Dict<'_>>(r.into()))
                })
                .and_then(|d| d.get::<Name>(keys::BASE_ENCODING))
                .map(|n| n.as_ref().to_vec())
        })
        .unwrap_or_default();

    // Only handle well-known named encodings (no /Differences dict support for TrueType).
    // For non-symbolic TrueType fonts with NO explicit /Encoding key, the PDF spec
    // defaults to WinAnsiEncoding — treat absent encoding as WinAnsi. (#FN-6.3.5)
    let is_symbolic = {
        let flags: Option<i32> = desc.get(keys::FLAGS);
        flags.is_some_and(|f| f & 0x04 != 0)
    };
    let use_winansi = enc_bytes == b"WinAnsiEncoding" || (enc_bytes.is_empty() && !is_symbolic);
    let use_macroman = enc_bytes == b"MacRomanEncoding";
    if !use_winansi && !use_macroman {
        return;
    }

    let loc = format!("page {}", page_idx + 1);

    for code in first..=last {
        let idx = code - first;
        let pdf_w = pdf_widths[idx];
        // §6.2.10.5: pdf_w=0 is a declared width (not "absent") — if the font program
        // has a non-zero advance for that glyph it is a genuine mismatch. The MissingWidth
        // check below handles legitimate "unused code" entries whose value equals the
        // declared fallback. DO NOT skip 0-width entries here. (#FN-6.2.10.5)

        // §6.2.11.5 applies only to characters "used in the document". PDF creators
        // set entries for unused codes to /MissingWidth (the generic fallback width).
        // Skip codes whose PDF width equals the MissingWidth sentinel. (#FP-6.2.11.5)
        if missing_width.is_some_and(|mw| mw == pdf_w) {
            continue;
        }

        let ch = if use_winansi {
            winansi_code_to_char(code as u8)
        } else {
            macroman_code_to_char(code as u8)
        };
        let Some(ch) = ch else {
            continue; // Code not defined in encoding
        };

        // When the glyph is absent from the subset font, the renderer falls back to
        // the notdef glyph (GID 0). veraPDF uses the notdef advance as
        // "widthFromFontProgram" and compares it with the PDF /Widths entry.
        // (#FN-6.3.5/6.3.6 isartor-6-3-5-t01-fail-d)
        let explicit_gid = face.glyph_index(ch);
        // Use notdef (GID 0) when glyph is absent; skip only when glyph is explicitly
        // mapped to notdef inside the font (the font program maps it to notdef deliberately).
        let gid = explicit_gid.unwrap_or(ttf_parser::GlyphId(0));
        if gid.0 == 0 && explicit_gid.is_some() {
            continue;
        }
        let Some(advance) = face.glyph_hor_advance(gid) else {
            continue;
        };
        let font_w = (advance as f64 * 1000.0 / upem).round() as i32;

        // Allow ±2 units for TrueType fractional-unit rounding.
        if (font_w - pdf_w).abs() > 2 {
            error_at(
                report,
                "6.3.5-fw",
                format!(
                    "Font {name} code {code} (U+{:04X}): TrueType width {font_w} \
                     != PDF /Widths[{idx}] {pdf_w}",
                    ch as u32
                ),
                loc.clone(),
            );
            return; // First mismatch only
        }
    }
}

/// Map a byte code using WinAnsiEncoding (Windows-1252) to Unicode.
fn winansi_code_to_char(code: u8) -> Option<char> {
    let u: u32 = match code {
        // Control characters — no glyph
        0x00..=0x1F | 0x7F => return None,
        // ASCII printable
        0x20..=0x7E => code as u32,
        // Windows-1252 extensions (0x80–0x9F)
        0x80 => 0x20AC,
        0x81 => return None,
        0x82 => 0x201A,
        0x83 => 0x0192,
        0x84 => 0x201E,
        0x85 => 0x2026,
        0x86 => 0x2020,
        0x87 => 0x2021,
        0x88 => 0x02C6,
        0x89 => 0x2030,
        0x8A => 0x0160,
        0x8B => 0x2039,
        0x8C => 0x0152,
        0x8D => return None,
        0x8E => 0x017D,
        0x8F => return None,
        0x90 => return None,
        0x91 => 0x2018,
        0x92 => 0x2019,
        0x93 => 0x201C,
        0x94 => 0x201D,
        0x95 => 0x2022,
        0x96 => 0x2013,
        0x97 => 0x2014,
        0x98 => 0x02DC,
        0x99 => 0x2122,
        0x9A => 0x0161,
        0x9B => 0x203A,
        0x9C => 0x0153,
        0x9D => return None,
        0x9E => 0x017E,
        0x9F => 0x0178,
        // Latin-1 supplement (same as Unicode)
        0xA0..=0xFF => code as u32,
    };
    char::from_u32(u)
}

/// Map a byte code using MacRomanEncoding (Mac OS Roman) to Unicode.
fn macroman_code_to_char(code: u8) -> Option<char> {
    let u: u32 = match code {
        0x00..=0x7F => code as u32,
        // Mac OS Roman 0x80-0xFF — standard table
        0x80 => 0x00C4,
        0x81 => 0x00C5,
        0x82 => 0x00C7,
        0x83 => 0x00C9,
        0x84 => 0x00D1,
        0x85 => 0x00D6,
        0x86 => 0x00DC,
        0x87 => 0x00E1,
        0x88 => 0x00E0,
        0x89 => 0x00E2,
        0x8A => 0x00E4,
        0x8B => 0x00E5,
        0x8C => 0x00E7,
        0x8D => 0x00E9,
        0x8E => 0x00E8,
        0x8F => 0x00EA,
        0x90 => 0x00EB,
        0x91 => 0x00ED,
        0x92 => 0x00EC,
        0x93 => 0x00EE,
        0x94 => 0x00EF,
        0x95 => 0x00F1,
        0x96 => 0x00F3,
        0x97 => 0x00F2,
        0x98 => 0x00F4,
        0x99 => 0x00F6,
        0x9A => 0x00FA,
        0x9B => 0x00F9,
        0x9C => 0x00FB,
        0x9D => 0x00FC,
        0x9E => 0x2020,
        0x9F => 0x00B0,
        0xA0 => 0x00A2,
        0xA1 => 0x00A3,
        0xA2 => 0x00A7,
        0xA3 => 0x2022,
        0xA4 => 0x00B6,
        0xA5 => 0x00DF,
        0xA6 => 0x00AE,
        0xA7 => 0x00A9,
        0xA8 => 0x2122,
        0xA9 => 0x00B4,
        0xAA => 0x00A8,
        0xAB => 0x2260,
        0xAC => 0x00C6,
        0xAD => 0x00D8,
        0xAE => 0x221E,
        0xAF => 0x00B1,
        0xB0 => 0x2264,
        0xB1 => 0x2265,
        0xB2 => 0x00A5,
        0xB3 => 0x00B5,
        0xB4 => 0x2202,
        0xB5 => 0x2211,
        0xB6 => 0x220F,
        0xB7 => 0x03C0,
        0xB8 => 0x222B,
        0xB9 => 0x00AA,
        0xBA => 0x00BA,
        0xBB => 0x03A9,
        0xBC => 0x00E6,
        0xBD => 0x00F8,
        0xBE => 0x00BF,
        0xBF => 0x00A1,
        0xC0 => 0x00AC,
        0xC1 => 0x221A,
        0xC2 => 0x0192,
        0xC3 => 0x2248,
        0xC4 => 0x2206,
        0xC5 => 0x00AB,
        0xC6 => 0x00BB,
        0xC7 => 0x2026,
        0xC8 => 0x00A0,
        0xC9 => 0x00C0,
        0xCA => 0x00C3,
        0xCB => 0x00D5,
        0xCC => 0x0152,
        0xCD => 0x0153,
        0xCE => 0x2013,
        0xCF => 0x2014,
        0xD0 => 0x201C,
        0xD1 => 0x201D,
        0xD2 => 0x2018,
        0xD3 => 0x2019,
        0xD4 => 0x00F7,
        0xD5 => 0x25CA,
        0xD6 => 0x00FF,
        0xD7 => 0x0178,
        0xD8 => 0x2044,
        0xD9 => 0x20AC,
        0xDA => 0x2039,
        0xDB => 0x203A,
        0xDC => 0xFB01,
        0xDD => 0xFB02,
        0xDE => 0x2021,
        0xDF => 0x00B7,
        0xE0 => 0x201A,
        0xE1 => 0x201E,
        0xE2 => 0x2030,
        0xE3 => 0x00C2,
        0xE4 => 0x00CA,
        0xE5 => 0x00C1,
        0xE6 => 0x00CB,
        0xE7 => 0x00C8,
        0xE8 => 0x00CD,
        0xE9 => 0x00CE,
        0xEA => 0x00CF,
        0xEB => 0x00CC,
        0xEC => 0x00D3,
        0xED => 0x00D4,
        0xEE => 0xF8FF, // Apple logo (PUA)
        0xEF => 0x00D2,
        0xF0 => 0x00DA,
        0xF1 => 0x00DB,
        0xF2 => 0x00D9,
        0xF3 => 0x0131,
        0xF4 => 0x02C6,
        0xF5 => 0x02DC,
        0xF6 => 0x00AF,
        0xF7 => 0x02D8,
        0xF8 => 0x02D9,
        0xF9 => 0x02DA,
        0xFA => 0x00B8,
        0xFB => 0x02DD,
        0xFC => 0x02DB,
        0xFD => 0x02C7,
        0xFE => return None,
        0xFF => return None,
    };
    char::from_u32(u)
}

/// Validate symbolic TrueType font encoding (§6.3.7).
///
/// Symbolic fonts (bit 2 of Flags set) shall not specify a character encoding.
/// Note: this was previously labelled §6.3.6 but veraPDF (and ISO 19005-1 §6.3.7)
/// reports this as §6.3.7. §6.3.6 is the Differences array restriction. (#467)
pub fn check_symbolic_truetype_encoding(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for_each_font(pdf, |name, font_dict, page_idx| {
        let Some(subtype) = font_dict.get::<Name>(keys::SUBTYPE) else {
            return;
        };
        if subtype.as_ref() != b"TrueType" {
            return;
        }

        let Some(desc) = font_dict.get::<Dict<'_>>(keys::FONT_DESC) else {
            return;
        };
        let Some(flags) = desc.get::<i32>(keys::FLAGS) else {
            return;
        };
        let symbolic = flags & 0x04 != 0;

        if symbolic {
            // Symbolic TrueType must not have ANY /Encoding entry.
            // Use internal rule "6.3.7-se" so it can be remapped per PDF/A part:
            // PDF/A-1 §6.3.7; PDF/A-2/3 §6.2.11.6 (TrueType encoding). (#483)
            if font_dict.get::<Object<'_>>(keys::ENCODING).is_some() {
                error_at(
                    report,
                    "6.3.7-se",
                    format!("Symbolic TrueType font {name} shall not specify /Encoding"),
                    format!("page {}", page_idx + 1),
                );
            }

            // §6.3.7 t03 (PDF/A-1 only): symbolic TrueType must have exactly one
            // cmap subtable. Fixes FN where font (e.g. Wingdings) has 2 subtables
            // (Mac + Win). Use internal rule "6.3.7-cmap" so it is only active in
            // PDF/A-1 (mapped in pdfa.rs); PDF/A-2/3/4 do not have this requirement
            // and veraPDF does not fire for it there. (#467, #FP-6.2.11.6)
            let ff2_data = desc
                .get::<Stream<'_>>(keys::FONT_FILE2)
                .and_then(|s| s.decoded().ok())
                .or_else(|| {
                    desc.get_ref(keys::FONT_FILE2)
                        .and_then(|r| xref.get::<Stream<'_>>(r.into()))
                        .and_then(|s| s.decoded().ok())
                });
            if let Some(data) = ff2_data {
                if let Some(n) = count_truetype_cmap_subtables(&data) {
                    if n != 1 {
                        error_at(
                            report,
                            "6.3.7-cmap",
                            format!(
                                "Symbolic TrueType font {name} has {n} cmap subtables; exactly 1 required"
                            ),
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
            }
        }
    });
}

/// Count the number of cmap subtables in a TrueType/OpenType font.
///
/// The cmap table header contains: version (u16) + numTables (u16).
/// Returns None if the font data is malformed or has no cmap table.
fn count_truetype_cmap_subtables(font_data: &[u8]) -> Option<u16> {
    if font_data.len() < 12 {
        return None;
    }
    let num_tables = u16::from_be_bytes([font_data[4], font_data[5]]) as usize;
    for i in 0..num_tables {
        let entry_off = 12 + i * 16;
        if entry_off + 16 > font_data.len() {
            break;
        }
        if &font_data[entry_off..entry_off + 4] == b"cmap" {
            let tbl_off = u32::from_be_bytes([
                font_data[entry_off + 8],
                font_data[entry_off + 9],
                font_data[entry_off + 10],
                font_data[entry_off + 11],
            ]) as usize;
            if tbl_off + 4 > font_data.len() {
                return None;
            }
            // cmap table: version(2) + numTables(2)
            return Some(u16::from_be_bytes([
                font_data[tbl_off + 2],
                font_data[tbl_off + 3],
            ]));
        }
    }
    None
}

/// Validate CIDToGIDMap is /Identity for CIDFont Type2 (§6.3.7).
pub fn check_cidtogidmap_identity(pdf: &Pdf, report: &mut ComplianceReport) {
    for_each_font(pdf, |name, font_dict, page_idx| {
        let Some(descendants) = font_dict.get::<Array<'_>>(keys::DESCENDANT_FONTS) else {
            return;
        };
        for desc_font in descendants.iter::<Dict<'_>>() {
            let Some(subtype) = desc_font.get::<Name>(keys::SUBTYPE) else {
                continue;
            };
            if subtype.as_ref() != keys::CID_FONT_TYPE2 {
                continue;
            }

            if let Some(map) = desc_font.get::<Name>(keys::CID_TO_GID_MAP) {
                if map.as_ref() != keys::IDENTITY {
                    let val = std::str::from_utf8(map.as_ref()).unwrap_or("?");
                    error_at(
                        report,
                        "6.3.7",
                        format!("CIDFont Type2 {name} has CIDToGIDMap={val}, expected Identity"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    });
}

/// Validate CMap embedding for Type0 fonts (§6.3.3.3 / §6.2.11.3.3).
///
/// PDF/A-1: §6.3.3.3; PDF/A-2/3: §6.2.11.3.3; PDF/A-4: §6.2.10.3.3.
/// Internal rule "6.3.3.3" is remapped per-part in remap_clause_numbers. (#483)
pub fn check_cmap_embedding(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for_each_font(pdf, |name, font_dict, _page_idx| {
        let Some(subtype) = font_dict.get::<Name>(keys::SUBTYPE) else {
            return;
        };
        if subtype.as_ref() != b"Type0" {
            return;
        }

        if let Some(enc_name) = font_dict.get::<Name>(keys::ENCODING) {
            let enc = enc_name.as_ref();
            if is_standard_cmap(enc) {
                return;
            }

            let enc_str = std::str::from_utf8(enc).unwrap_or("?");
            // Non-standard CMap must be embedded as a stream object.
            // §6.3.3.3 (PDF/A-1) / §6.2.11.3.3 (PDF/A-2/3) / §6.2.10.3.3 (PDF/A-4).
            // Internal rule "6.3.3.3" is remapped per-part in remap_clause_numbers. (#483)
            error(
                report,
                "6.3.3.3",
                format!("Type0 font {name} uses non-standard CMap {enc_str} that must be embedded"),
            );
        }

        // §6.2.11.3.3 / §6.2.10.3.3: check /UseCMap within embedded CMap streams.
        // A CMap shall not reference any other CMap except standard predefined ones.
        // /Encoding may be a direct stream or an indirect reference (resolve both).
        let enc_stream_opt: Option<Stream<'_>> =
            font_dict.get::<Stream<'_>>(keys::ENCODING).or_else(|| {
                font_dict
                    .get_ref(keys::ENCODING)
                    .and_then(|r| xref.get::<Stream<'_>>(r.into()))
            });
        if let Some(enc_stream) = enc_stream_opt {
            let enc_dict = enc_stream.dict();

            // §6.3.3.3: /WMode in the CMap dict must equal the WMode in stream content.
            // Fixes §6.3.3.3 t02 FN (dict says WMode 1, stream body says WMode 0).
            if let Some(dict_wmode) = enc_dict.get::<i64>(b"WMode" as &[u8]) {
                if let Ok(decoded) = enc_stream.decoded() {
                    let content = std::str::from_utf8(&decoded).unwrap_or("");
                    // CMap stream uses PostScript syntax: "/WMode 0 def"
                    let stream_wmode: Option<i64> = content
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .windows(3)
                        .find(|w| (w[0] == "/WMode" || w[0] == "WMode") && w[2] == "def")
                        .and_then(|w| w[1].parse::<i64>().ok());
                    if let Some(sw) = stream_wmode {
                        if sw != dict_wmode {
                            error(
                                report,
                                "6.3.3.3",
                                format!(
                                    "Font {name} embedded CMap /WMode mismatch: dict={dict_wmode} stream={sw}"
                                ),
                            );
                        }
                    }
                }
            }

            // Check /UseCMap — can be a Name (predefined) or a reference to another stream
            if let Some(usecmap_name) = enc_dict.get::<Name>(b"UseCMap" as &[u8]) {
                if !is_standard_cmap(usecmap_name.as_ref()) {
                    let cm = std::str::from_utf8(usecmap_name.as_ref()).unwrap_or("?");
                    error(
                        report,
                        "6.3.3.3",
                        format!("Font {name} CMap /UseCMap references non-standard CMap /{cm}"),
                    );
                }
            }
            // /UseCMap may be an indirect reference to a stream with /CMapName
            if let Some(usecmap_ref) = enc_dict.get_ref(b"UseCMap" as &[u8]) {
                if let Some(ref_stream) = xref.get::<Stream<'_>>(usecmap_ref.into()) {
                    if let Some(cmap_name) = ref_stream.dict().get::<Name>(b"CMapName" as &[u8]) {
                        if !is_standard_cmap(cmap_name.as_ref()) {
                            let cm = std::str::from_utf8(cmap_name.as_ref()).unwrap_or("?");
                            error(
                                report,
                                "6.3.3.3",
                                format!(
                                    "Font {name} CMap /UseCMap references non-standard CMap {cm}"
                                ),
                            );
                        }
                    }
                }
            }
        }
    });
}

/// Check if a CMap name is exempt from the PDF/A §6.3.3.3 embedding requirement.
///
/// Per ISO 19005-1 §6.3.3.3, ONLY Identity-H and Identity-V are exempt.
/// All other CMaps — including predefined ones like UniJIS-UCS2-H, UniGB-UCS2-H,
/// etc. — must be embedded as stream objects. Fixes §6.3.3.3 FN.
fn is_standard_cmap(name: &[u8]) -> bool {
    name == keys::IDENTITY_H || name == keys::IDENTITY_V
}

/// Validate annotation appearance streams (§6.5.3).
pub fn check_annotation_appearance(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            if let Some(subtype) = annot.get::<Name>(keys::SUBTYPE) {
                if subtype.as_ref() == b"Popup" {
                    continue;
                }
            }

            let subtype_name = annot
                .get::<Name>(keys::SUBTYPE)
                .map(|n| std::str::from_utf8(n.as_ref()).unwrap_or("?").to_string())
                .unwrap_or_else(|| "unknown".to_string());

            match annot.get::<Dict<'_>>(keys::AP) {
                Some(ap) => {
                    if ap.get::<Object<'_>>(keys::N).is_none() {
                        error_at(
                            report,
                            "6.5.3",
                            format!("{subtype_name} annotation /AP missing /N (normal appearance)"),
                            format!("page {}", page_idx + 1),
                        );
                    }
                    // PDF/A-1 §6.5.3: AP dict shall only contain the N entry.
                    // /R (rollover) and /D (down) appearances are forbidden. Fixes #467.
                    if ap.contains_key(b"R" as &[u8]) || ap.contains_key(b"D" as &[u8]) {
                        error_at(
                            report,
                            "6.5.3",
                            format!("{subtype_name} annotation /AP has entries other than /N (rollover/down appearances not allowed)"),
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
                None => {
                    error_at(
                        report,
                        "6.5.3",
                        format!("{subtype_name} annotation missing /AP (appearance dict)"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }

            // CA key must be 1.0 if present (§6.5.3)
            if let Some(ca_val) = annot.get::<f64>(b"CA" as &[u8]) {
                if (ca_val - 1.0).abs() > f64::EPSILON {
                    error_at(
                        report,
                        "6.5.3",
                        format!("{subtype_name} annotation /CA is {ca_val} (must be 1.0)"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }

            // §6.5.3 / ISO 19005-1 Cor.2:2011: Widget+Btn → /AP/N must be a subdictionary;
            // all other annotations → /AP/N must be a stream. Uses Object enum to correctly
            // distinguish stream from dict regardless of whether N is inline or indirect. (#483)
            let is_widget = annot
                .get::<Name>(keys::SUBTYPE)
                .is_some_and(|s| s.as_ref() == b"Widget");
            // FT may be in the annotation itself or inherited from /Parent field dict.
            let is_btn = annot
                .get::<Name>(keys::FT)
                .is_some_and(|s| s.as_ref() == b"Btn")
                || annot
                    .get::<Dict<'_>>(keys::PARENT)
                    .and_then(|p| p.get::<Name>(keys::FT))
                    .is_some_and(|s| s.as_ref() == b"Btn");
            if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                if let Some(n_obj) = ap.get::<Object<'_>>(keys::N) {
                    let n_is_stream = matches!(n_obj, Object::Stream(_));
                    let n_is_dict = matches!(n_obj, Object::Dict(_));
                    if is_widget && is_btn {
                        // All Btn widgets (including pushbuttons): /AP/N must be a subdictionary.
                        // ISO 19005-1 Cor.2:2011 §6.5.3. Fixes #483.
                        if n_is_stream {
                            error_at(
                                report,
                                "6.5.3",
                                "Widget/Btn annotation /AP /N must be a subdictionary, not a stream",
                                format!("page {}", page_idx + 1),
                            );
                        }
                    } else {
                        // Non-Btn annotations: /AP/N must be a stream, not a subdictionary.
                        // ISO 19005-1 Cor.2:2011 §6.5.3. Fixes #483.
                        if n_is_dict {
                            error_at(
                                report,
                                "6.5.3",
                                format!(
                                    "{subtype_name} annotation /AP /N must be a stream, not a subdictionary"
                                ),
                                format!("page {}", page_idx + 1),
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Deep annotation subtype validation (§6.5.2 / §6.3.1 / §6.3.1).
///
/// Clause numbering differs by PDF/A part:
/// - PDF/A-1: §6.5.2 (ISO 19005-1 — annotation type restrictions)
/// - PDF/A-2/3: §6.3.1 (ISO 19005-2/3 — allowed annotation types)
/// - PDF/A-4: §6.3.1 (ISO 19005-4 — allowed annotation types)
///
/// Note: §6.3.2 is annotation *flags* (the /F key), not annotation types.
/// Fixes #467.
pub fn check_annotation_subtypes_deep(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    // Annotations forbidden in ALL PDF/A parts
    let forbidden_all: &[&[u8]] = &[b"Sound", b"Movie", b"3D"];
    // Annotations also forbidden in PDF/A-2/3/4 (added in ISO 19005-2)
    let forbidden_pdfa2plus: &[&[u8]] = &[b"Screen", b"Redact"];
    // PDF/A-4 (ISO 19005-4 §6.3.1) additionally forbids RichMedia, FileAttachment
    let forbidden_pdfa4: &[&[u8]] = &[b"RichMedia", b"FileAttachment"];

    // §6.3.1 = allowed annotation types in PDF/A-2/3/4.
    // §6.5.2 = same concept in PDF/A-1 (different numbering). Fixes #467.
    let rule = match part {
        2..=4 => "6.3.1",
        _ => "6.5.2", // PDF/A-1
    };

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            let Some(subtype) = annot.get::<Name>(keys::SUBTYPE) else {
                continue;
            };
            let st = subtype.as_ref();

            if forbidden_all.contains(&st) {
                let name = std::str::from_utf8(st).unwrap_or("?");
                error_at(
                    report,
                    rule,
                    format!("Annotation type {name} forbidden in PDF/A-{part}"),
                    format!("page {}", page_idx + 1),
                );
            }

            if part >= 2 && forbidden_pdfa2plus.contains(&st) {
                let name = std::str::from_utf8(st).unwrap_or("?");
                error_at(
                    report,
                    rule,
                    format!("Annotation type {name} forbidden in PDF/A-{part}"),
                    format!("page {}", page_idx + 1),
                );
            }

            if st == b"FileAttachment" && part <= 2 {
                error_at(
                    report,
                    rule,
                    format!("FileAttachment annotation forbidden in PDF/A-{part}"),
                    format!("page {}", page_idx + 1),
                );
            }

            if part == 4 && forbidden_pdfa4.contains(&st) {
                let name = std::str::from_utf8(st).unwrap_or("?");
                error_at(
                    report,
                    rule,
                    format!("Annotation type {name} forbidden in PDF/A-4"),
                    format!("page {}", page_idx + 1),
                );
            }

            // PDF/A-4 §6.3.1: annotation Subtype must be defined in ISO 32000-2:2020.
            // Unknown / non-standard types (e.g. lowercase "/line") are a violation. (#FN-6.5.1)
            if part == 4 {
                const ISO32000_TYPES: &[&[u8]] = &[
                    b"Text",
                    b"Link",
                    b"FreeText",
                    b"Line",
                    b"Square",
                    b"Circle",
                    b"Polygon",
                    b"PolyLine",
                    b"Highlight",
                    b"Underline",
                    b"Squiggly",
                    b"StrikeOut",
                    b"Stamp",
                    b"Caret",
                    b"Ink",
                    b"Popup",
                    b"FileAttachment",
                    b"Sound",
                    b"Movie",
                    b"Screen",
                    b"Widget",
                    b"PrinterMark",
                    b"TrapNet",
                    b"Watermark",
                    b"3D",
                    b"Redact",
                    b"Projection",
                    b"RichMedia",
                ];
                if !ISO32000_TYPES.contains(&st) {
                    let name = std::str::from_utf8(st).unwrap_or("?");
                    error_at(
                        report,
                        rule,
                        format!(
                            "Annotation type /{name} is not defined in ISO 32000-2:2020 (§6.3.1)"
                        ),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }
}

/// Deep annotation flag validation per PDF/A part (§6.3.2 / §6.5.3).
///
/// PDF/A-1: §6.5.3; PDF/A-2/3/4: §6.3.2. Fixes #467.
pub fn check_annotation_flags_deep(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    // PDF/A-1: §6.5.3; PDF/A-2/3/4: §6.3.2 (same as check_annotation_flags). Fixes #467.
    let rule = if part == 1 { "6.5.3" } else { "6.3.2" };

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            let Some(subtype) = annot.get::<Name>(keys::SUBTYPE) else {
                continue;
            };
            if subtype.as_ref() == b"Popup" {
                continue;
            }

            let Some(flags) = annot.get::<i32>(keys::F) else {
                continue; // Missing F already reported by check_annotation_flags
            };

            // PDF/A-2/3/4: Widget annotations used as form fields
            // must not have both Hidden and Print flags set simultaneously
            if part >= 2 && subtype.as_ref() == b"Widget" {
                let hidden = flags & 0x02 != 0;
                let print = flags & 0x04 != 0;
                if hidden && print {
                    error_at(
                        report,
                        rule,
                        "Widget annotation has both Hidden and Print flags set",
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }

    // PDF/A-1 §6.6.2 t3 / PDF/A-2/3 §6.5.2 t2 / PDF/A-4 §6.6.3: Catalog must not have /AA.
    // veraPDF: §6.6.2 (part 1), §6.5.2 (parts 2/3), §6.6.3 (part 4 — normalizes to §6.8.3).
    // For PDF/A-4 emit "6.1.6.1" which remap_clause_numbers maps to "6.6.3". (#FN-6.6.2)
    if let Some(cat) = catalog(pdf) {
        if cat.contains_key(b"AA" as &[u8]) {
            let cat_aa_rule = match part {
                1 => "6.6.2",
                2 | 3 => "6.5.2",
                4 => "6.1.6.1", // remaps to "6.6.3" → normalizes to "6.8.3" for PDF/A-4
                _ => "6.5.2",
            };
            error(
                report,
                cat_aa_rule,
                "Document Catalog contains forbidden /AA entry",
            );
        }
    }
}

// ─── Batch 5: Transparency, Tagged PDF, Remaining Rules ─────────────────────

// ─── §6.4 — Transparency deep checks ────────────────────────────────────────

/// Deeper transparency check: validate Group dictionaries in page and XObject (§6.4).
///
/// Beyond the simple presence check, validates that transparency groups on
/// pages and Form XObjects have valid color space references.
pub fn check_transparency_deep(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    if part == 1 {
        // PDF/A-1 forbids all transparency — already handled by has_transparency
        return;
    }

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        check_group_dict(page_dict, part, &format!("page {}", page_idx + 1), report);

        // Check Form XObjects for transparency groups
        let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) else {
            continue;
        };
        let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) else {
            continue;
        };
        for (name, _) in xobj_dict.entries() {
            let Some(stream) = xobj_dict.get::<Stream<'_>>(name.as_ref()) else {
                continue;
            };
            let dict = stream.dict();
            let is_form = dict
                .get::<Name>(keys::SUBTYPE)
                .is_some_and(|s| s.as_ref() == b"Form");
            if !is_form {
                continue;
            }
            let xn = std::str::from_utf8(name.as_ref()).unwrap_or("?");
            let loc = format!("page {} XObject {xn}", page_idx + 1);
            check_group_dict(dict, part, &loc, report);
        }
    }
}

fn check_group_dict(dict: &Dict<'_>, _part: u8, location: &str, report: &mut ComplianceReport) {
    let Some(group) = dict.get::<Dict<'_>>(keys::GROUP) else {
        return;
    };
    let Some(s) = group.get::<Name>(keys::S) else {
        return;
    };
    if s.as_ref() != keys::TRANSPARENCY {
        return;
    }

    // Transparency group CS should be present and valid
    if group.get::<Object<'_>>(keys::CS).is_none()
        && group.get::<Object<'_>>(keys::COLORSPACE).is_none()
    {
        warning(
            report,
            "6.4",
            format!("Transparency group at {location} has no color space"),
        );
    }
}

/// Check blending modes in ExtGState for PDF/A-2/3 (§6.4.1).
///
/// For PDF/A-2/3, blend modes are allowed but must be one of the standard
/// PDF blend modes defined in ISO 32000-1.
pub fn check_blending_modes(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    let valid_modes: &[&[u8]] = &[
        b"Normal",
        keys::COMPATIBLE,
        b"Multiply",
        b"Screen",
        b"Overlay",
        b"Darken",
        b"Lighten",
        b"ColorDodge",
        b"ColorBurn",
        b"HardLight",
        b"SoftLight",
        b"Difference",
        b"Exclusion",
        b"Hue",
        b"Saturation",
        b"Color",
        b"Luminosity",
    ];

    // PDF/A-4 merges blend mode checks into 6.2.9; parts 1-3 use 6.4.1
    let blend_rule = if part == 4 { "6.2.9" } else { "6.4.1" };

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) else {
            continue;
        };
        let Some(gs_dict) = res_dict.get::<Dict<'_>>(keys::EXT_G_STATE) else {
            continue;
        };
        for (gs_name, _) in gs_dict.entries() {
            let Some(gs) = gs_dict.get::<Dict<'_>>(gs_name.as_ref()) else {
                continue;
            };
            if let Some(bm) = gs.get::<Name>(keys::BM) {
                let bm_val = bm.as_ref();
                if part == 1 {
                    // PDF/A-1: only Normal/Compatible
                    if bm_val != b"Normal" && bm_val != keys::COMPATIBLE {
                        let bm_str = std::str::from_utf8(bm_val).unwrap_or("?");
                        let gs_str = std::str::from_utf8(gs_name.as_ref()).unwrap_or("?");
                        error_at(
                            report,
                            blend_rule,
                            format!("ExtGState {gs_str} BM={bm_str} (only Normal/Compatible in PDF/A-1)"),
                            format!("page {}", page_idx + 1),
                        );
                    }
                } else if !valid_modes.contains(&bm_val) {
                    let bm_str = std::str::from_utf8(bm_val).unwrap_or("?");
                    let gs_str = std::str::from_utf8(gs_name.as_ref()).unwrap_or("?");
                    error_at(
                        report,
                        blend_rule,
                        format!("ExtGState {gs_str} uses non-standard blend mode '{bm_str}'"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }
}

/// Check soft mask dictionaries have valid structure (§6.4.2).
///
/// §6.4.2 has two requirements:
/// 1. XObject dictionaries shall not contain the SMask key (ISO 19005-1 §6.4.2).
/// 2. If an ExtGState has an SMask, the SMask dictionary must have /S (Alpha or
///    Luminosity) and /G (group XObject).
///
/// Fixes #467.
pub fn check_soft_mask_structure(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) else {
            continue;
        };

        // §6.4.2: XObject dictionaries shall not contain the SMask key. (#467)
        // Applies to both Image XObjects and Form XObjects.
        if let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) {
            for (xname, _) in xobj_dict.entries() {
                let Some(stream) = xobj_dict.get::<Stream<'_>>(xname.as_ref()) else {
                    continue;
                };
                let dict = stream.dict();
                if dict.contains_key(keys::SMASK) {
                    let xname_str = std::str::from_utf8(xname.as_ref()).unwrap_or("?");
                    error_at(
                        report,
                        "6.4.2",
                        format!("XObject '{xname_str}' contains forbidden /SMask key"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }

        // §6.4.2: ExtGState SMask dict must have valid /S and /G entries.
        let Some(gs_dict) = res_dict.get::<Dict<'_>>(keys::EXT_G_STATE) else {
            continue;
        };
        for (gs_name, _) in gs_dict.entries() {
            let Some(gs) = gs_dict.get::<Dict<'_>>(gs_name.as_ref()) else {
                continue;
            };
            let Some(smask) = gs.get::<Dict<'_>>(keys::SMASK) else {
                continue;
            };

            let gs_str = std::str::from_utf8(gs_name.as_ref()).unwrap_or("?");

            // SMask dict must have /S (subtype: Alpha or Luminosity)
            if let Some(s) = smask.get::<Name>(keys::S) {
                let s_val = s.as_ref();
                if s_val != b"Alpha" && s_val != b"Luminosity" {
                    let s_str = std::str::from_utf8(s_val).unwrap_or("?");
                    error_at(
                        report,
                        "6.4.2",
                        format!(
                            "ExtGState {gs_str} SMask /S={s_str} (must be Alpha or Luminosity)"
                        ),
                        format!("page {}", page_idx + 1),
                    );
                }
            } else {
                error_at(
                    report,
                    "6.4.2",
                    format!("ExtGState {gs_str} SMask missing required /S key"),
                    format!("page {}", page_idx + 1),
                );
            }

            // SMask dict must have /G (group XObject)
            if smask.get::<Stream<'_>>(b"G" as &[u8]).is_none() {
                error_at(
                    report,
                    "6.4.2",
                    format!("ExtGState {gs_str} SMask missing required /G (group XObject)"),
                    format!("page {}", page_idx + 1),
                );
            }
        }
    }
}

// ─── §6.8 — Tagged PDF deep checks ──────────────────────────────────────────

/// Check table structure elements are correctly nested (§6.8.2.2).
///
/// Table must contain TR; TR must contain TD or TH.
/// THead, TBody, TFoot may appear between Table and TR.
pub fn check_table_structure(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(struct_tree) = cat.get::<Dict<'_>>(keys::STRUCT_TREE_ROOT) else {
        return;
    };

    if let Some(kids) = struct_tree.get::<Array<'_>>(keys::K) {
        walk_struct_elements(&kids, None, report, 0);
    } else if let Some(kid) = struct_tree.get::<Dict<'_>>(keys::K) {
        check_struct_element(&kid, None, report, 0);
    }
}

fn walk_struct_elements(
    kids: &Array<'_>,
    parent_type: Option<&[u8]>,
    report: &mut ComplianceReport,
    depth: usize,
) {
    if depth > 100 {
        return;
    }
    for kid in kids.iter::<Dict<'_>>() {
        check_struct_element(&kid, parent_type, report, depth);
    }
}

fn check_struct_element(
    elem: &Dict<'_>,
    parent_type: Option<&[u8]>,
    report: &mut ComplianceReport,
    depth: usize,
) {
    if depth > 100 {
        return;
    }

    let elem_type = elem.get::<Name>(keys::S).map(|n| n.as_ref().to_vec());
    let type_bytes = elem_type.as_deref();

    // Check table nesting rules
    if let Some(t) = type_bytes {
        match t {
            b"TR" => {
                if let Some(parent) = parent_type {
                    if parent != b"Table"
                        && parent != b"THead"
                        && parent != b"TBody"
                        && parent != b"TFoot"
                    {
                        let p = std::str::from_utf8(parent).unwrap_or("?");
                        error(
                            report,
                            "6.8.2.2",
                            format!("TR must be child of Table/THead/TBody/TFoot, found under {p}"),
                        );
                    }
                }
            }
            b"TD" | b"TH" => {
                if let Some(parent) = parent_type {
                    if parent != b"TR" {
                        let p = std::str::from_utf8(parent).unwrap_or("?");
                        let cell = std::str::from_utf8(t).unwrap_or("?");
                        error(
                            report,
                            "6.8.2.2",
                            format!("{cell} must be child of TR, found under {p}"),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    // §6.2.10.8 — ActualText must not contain PUA (Private Use Area) codepoints.
    // PUA range: U+E000–U+F8FF (BMP PUA), U+F0000–U+FFFFF, U+100000–U+10FFFF.
    // The ActualText value is a PDF string: either UTF-16BE (starts with FEFF BOM)
    // or PDFDocEncoding.  We only check UTF-16BE (most common for actual Unicode text). (#467)
    if let Some(actual_text) = elem.get::<pdf_syntax::object::String>(b"ActualText" as &[u8]) {
        let bytes = actual_text.as_bytes();
        // UTF-16BE strings start with BOM 0xFE 0xFF
        if bytes.len() >= 4 && bytes[0] == 0xFE && bytes[1] == 0xFF {
            let mut i = 2; // skip BOM
            while i + 1 < bytes.len() {
                let hi = bytes[i] as u32;
                let lo = bytes[i + 1] as u32;
                let cp = (hi << 8) | lo;
                // BMP PUA: E000–F8FF
                if (0xE000..=0xF8FF).contains(&cp) {
                    error(
                        report,
                        "6.2.10.8",
                        format!(
                            "ActualText in structure element contains PUA codepoint U+{cp:04X}"
                        ),
                    );
                    break;
                }
                // Surrogate pair: D800-DFFF encodes supplementary PUA F0000-10FFFF
                if (0xD800..=0xDBFF).contains(&cp) && i + 3 < bytes.len() {
                    let lo2 = (bytes[i + 2] as u32) << 8 | bytes[i + 3] as u32;
                    if (0xDC00..=0xDFFF).contains(&lo2) {
                        let full = 0x10000 + ((cp - 0xD800) << 10) + (lo2 - 0xDC00);
                        if full >= 0xF0000 {
                            error(
                                report,
                                "6.2.10.8",
                                format!(
                                    "ActualText contains supplementary PUA codepoint U+{full:X}"
                                ),
                            );
                            break;
                        }
                        i += 2; // consumed surrogate pair
                    }
                }
                i += 2;
            }
        }
    }

    // Recurse into children
    if let Some(kids) = elem.get::<Array<'_>>(keys::K) {
        walk_struct_elements(&kids, type_bytes, report, depth + 1);
    } else if let Some(kid) = elem.get::<Dict<'_>>(keys::K) {
        check_struct_element(&kid, type_bytes, report, depth + 1);
    }
}

/// Check Figure structure elements have /Alt text (§6.8.4).
pub fn check_figure_alt_text(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(struct_tree) = cat.get::<Dict<'_>>(keys::STRUCT_TREE_ROOT) else {
        return;
    };

    if let Some(kids) = struct_tree.get::<Array<'_>>(keys::K) {
        walk_figure_alt(&kids, report, 0);
    } else if let Some(kid) = struct_tree.get::<Dict<'_>>(keys::K) {
        check_figure_alt_elem(&kid, report, 0);
    }
}

fn walk_figure_alt(kids: &Array<'_>, report: &mut ComplianceReport, depth: usize) {
    if depth > 100 {
        return;
    }
    for kid in kids.iter::<Dict<'_>>() {
        check_figure_alt_elem(&kid, report, depth);
    }
}

fn check_figure_alt_elem(elem: &Dict<'_>, report: &mut ComplianceReport, depth: usize) {
    if depth > 100 {
        return;
    }

    if let Some(s) = elem.get::<Name>(keys::S) {
        if s.as_ref() == b"Figure" && elem.get::<Object<'_>>(keys::ALT).is_none() {
            error(
                report,
                "6.8.4",
                "Figure structure element missing required /Alt text",
            );
        }
    }

    if let Some(kids) = elem.get::<Array<'_>>(keys::K) {
        walk_figure_alt(&kids, report, depth + 1);
    } else if let Some(kid) = elem.get::<Dict<'_>>(keys::K) {
        check_figure_alt_elem(&kid, report, depth + 1);
    }
}

/// Check content streams have matching BMC/EMC pairs (§6.8.3.4).
///
/// Marked content sequences (BMC/BDC...EMC) must be properly nested and closed.
pub fn check_marked_content_sequences(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let Some(content) = page.page_stream() else {
            continue;
        };
        if content.len() > MAX_CONTENT_STREAM_SCAN_SIZE {
            continue;
        }
        let text = String::from_utf8_lossy(content);
        let tokens: Vec<&str> = text.split_ascii_whitespace().collect();

        let mut depth: i32 = 0;
        for tok in &tokens {
            match *tok {
                "BMC" | "BDC" => depth += 1,
                "EMC" => depth -= 1,
                _ => {}
            }
            if depth < 0 {
                error_at(
                    report,
                    "6.8.3.4",
                    "EMC without matching BMC/BDC",
                    format!("page {}", page_idx + 1),
                );
                break;
            }
        }
        if depth > 0 {
            error_at(
                report,
                "6.8.3.4",
                format!("{depth} unclosed marked content sequence(s) (BMC/BDC without EMC)"),
                format!("page {}", page_idx + 1),
            );
        }
    }
}

// ─── §6.9 — Interactive forms ────────────────────────────────────────────────

/// Check interactive form /NeedAppearances must be false or absent (§6.4.1).
pub fn check_need_appearances(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(acroform) = cat.get::<Dict<'_>>(keys::ACRO_FORM) else {
        return;
    };

    if let Some(Object::Boolean(true)) = acroform.get::<Object<'_>>(keys::NEED_APPEARANCES) {
        error(
            report,
            "6.4.1",
            "AcroForm /NeedAppearances is true; must be false or absent in PDF/A",
        );
    }

    // All form fields must have /AP (appearance) entry
    if let Some(fields) = acroform.get::<Array<'_>>(keys::FIELDS) {
        check_field_appearances(&fields, report, 0);
    }
}

/// Check that AcroForm does not contain /XFA key in PDF/A-4 (§6.4.2).
///
/// ISO 19005-4 §6.4.2: the interactive form dictionary shall not contain
/// the XFA key. For PDF/A-2/3, §6.4.2 is about soft-mask structure; XFA is
/// only prohibited explicitly in PDF/A-4. (#467)
pub fn check_acroform_no_xfa(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    if part != 4 {
        return;
    }
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(acroform) = cat.get::<Dict<'_>>(keys::ACRO_FORM) else {
        return;
    };
    if acroform.contains_key(keys::XFA) {
        error(
            report,
            "6.4.2",
            "AcroForm dictionary contains /XFA key, which is prohibited in PDF/A-4 (§6.4.2)",
        );
    }
}

fn check_field_appearances(fields: &Array<'_>, report: &mut ComplianceReport, depth: usize) {
    if depth > 50 {
        return;
    }
    for (idx, field) in fields.iter::<Dict<'_>>().enumerate() {
        let is_widget = field
            .get::<Name>(keys::SUBTYPE)
            .is_some_and(|s| s.as_ref() == keys::WIDGET);

        // Only terminal widget annotations need their own /AP.
        // Intermediate field nodes in the AcroForm hierarchy carry /FT for
        // inheritance but are not visual — they legitimately have no /AP.
        // Flagging them was causing false positives for PDFs with multi-level
        // form field trees. Fixes #455.
        if is_widget && field.get::<Dict<'_>>(keys::AP).is_none() {
            error_at(
                report,
                "6.9",
                format!("Widget annotation {idx} missing required /AP (appearance dictionary)"),
                "AcroForm",
            );
        }

        if let Some(kids) = field.get::<Array<'_>>(keys::KIDS) {
            check_field_appearances(&kids, report, depth + 1);
        }
    }
}

// ─── §6.10 — Digital signatures ──────────────────────────────────────────────

/// Check digital signature restrictions (§6.10).
///
/// Signature fields must have /FT /Sig and valid /ByteRange covering entire file.
/// Signature handlers must be standard (Adobe.PPKLite, etc.).
pub fn check_signature_restrictions(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(acroform) = cat.get::<Dict<'_>>(keys::ACRO_FORM) else {
        return;
    };
    let Some(fields) = acroform.get::<Array<'_>>(keys::FIELDS) else {
        return;
    };

    check_sig_fields(&fields, report, 0);
}

fn check_sig_fields(fields: &Array<'_>, report: &mut ComplianceReport, depth: usize) {
    if depth > 50 {
        return;
    }
    for field in fields.iter::<Dict<'_>>() {
        if let Some(ft) = field.get::<Name>(b"FT" as &[u8]) {
            if ft.as_ref() == b"Sig" {
                if let Some(v) = field.get::<Dict<'_>>(keys::V) {
                    // Check /Filter (handler)
                    if let Some(filter) = v.get::<Name>(keys::FILTER) {
                        let f = filter.as_ref();
                        if f != b"Adobe.PPKLite" && f != b"Adobe.PPKMS" && f != b"Entrust.PPKEF" {
                            let fs = std::str::from_utf8(f).unwrap_or("?");
                            warning(
                                report,
                                "6.10",
                                format!("Signature handler '{fs}' may not be standard"),
                            );
                        }
                    }
                    // Check /ByteRange presence
                    if v.get::<Array<'_>>(b"ByteRange" as &[u8]).is_none() {
                        error(report, "6.10", "Signature value missing /ByteRange");
                    }
                }
            }
        }
        if let Some(kids) = field.get::<Array<'_>>(keys::KIDS) {
            check_sig_fields(&kids, report, depth + 1);
        }
    }
}

/// Check Perms dictionary validity (§6.1.11 for PDF/A-4).
///
/// In PDF/A-4, the /Perms catalog entry may only contain /DocMDP. Any other
/// key (including /UR3 which is for usage rights signatures) is forbidden.
/// Reports "6.1.11" when non-DocMDP keys are found. (#FN-6.1.11)
pub fn check_perms_dict(pdf: &Pdf, pdfa_part: u8, report: &mut ComplianceReport) {
    if pdfa_part < 2 {
        return;
    }
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(perms) = cat.get::<Dict<'_>>(b"Perms" as &[u8]) else {
        return;
    };
    // /Perms may only contain /DocMDP. Any other key is a violation.
    for (key, _) in perms.entries() {
        if key.as_ref() != b"DocMDP" {
            let ks = std::str::from_utf8(key.as_ref()).unwrap_or("?");
            // PDF/A-4 §6.1.11; PDF/A-2/3 uses §6.1.12 for Perms restrictions.
            let rule = if pdfa_part == 4 { "6.1.11" } else { "6.1.12" };
            error(
                report,
                rule,
                format!("Catalog /Perms contains invalid key /{ks} (only /DocMDP allowed)"),
            );
        }
    }
}

/// Check DocMDP signature restriction (§6.1.12 for PDF/A-2/3/4).
///
/// If DocMDP permission is present, Signature Reference dicts must not
/// contain DigestLocation, DigestMethod, or DigestValue keys.
pub fn check_docmdp_signature_restriction(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    // Check if Perms dict has DocMDP
    let Some(perms) = cat.get::<Dict<'_>>(b"Perms" as &[u8]) else {
        return;
    };
    if perms.get::<Object<'_>>(b"DocMDP" as &[u8]).is_none() {
        return;
    }

    // DocMDP is present — check all signature Reference dicts
    let Some(acroform) = cat.get::<Dict<'_>>(keys::ACRO_FORM) else {
        return;
    };
    let Some(fields) = acroform.get::<Array<'_>>(keys::FIELDS) else {
        return;
    };
    check_sig_ref_digest_keys(&fields, report, 0);
}

fn check_sig_ref_digest_keys(fields: &Array<'_>, report: &mut ComplianceReport, depth: usize) {
    if depth > 50 {
        return;
    }
    for field in fields.iter::<Dict<'_>>() {
        if let Some(ft) = field.get::<Name>(b"FT" as &[u8]) {
            if ft.as_ref() == b"Sig" {
                if let Some(v) = field.get::<Dict<'_>>(keys::V) {
                    if let Some(refs) = v.get::<Array<'_>>(b"Reference" as &[u8]) {
                        for sig_ref in refs.iter::<Dict<'_>>() {
                            for key in [&b"DigestLocation"[..], b"DigestMethod", b"DigestValue"] {
                                if sig_ref.contains_key(key) {
                                    let ks = std::str::from_utf8(key).unwrap_or("?");
                                    error(
                                        report,
                                        "6.1.12",
                                        format!("Signature Reference dict contains /{ks} with DocMDP present"),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(kids) = field.get::<Array<'_>>(keys::KIDS) {
            check_sig_ref_digest_keys(&kids, report, depth + 1);
        }
    }
}

/// Check that every signature /ByteRange covers the entire file (§6.4.3).
///
/// ByteRange must be `[0, l1, b2, l2]` with `b2 + l2 == file_length`. A range
/// that stops short of the end of file means the signature does not cover the
/// file content after the signature bytes, violating PDF/A-3 §6.4.3.
pub fn check_sig_byterange_coverage(pdf: &Pdf, report: &mut ComplianceReport) {
    let file_size = pdf.data().as_ref().len() as i64;

    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(acroform) = cat.get::<Dict<'_>>(keys::ACRO_FORM) else {
        return;
    };
    let Some(fields) = acroform.get::<Array<'_>>(keys::FIELDS) else {
        return;
    };
    check_sig_byterange_fields(&fields, file_size, report, 0);
}

fn check_sig_byterange_fields(
    fields: &Array<'_>,
    file_size: i64,
    report: &mut ComplianceReport,
    depth: usize,
) {
    if depth > 50 {
        return;
    }
    for field in fields.iter::<Dict<'_>>() {
        if let Some(ft) = field.get::<Name>(b"FT" as &[u8]) {
            if ft.as_ref() == b"Sig" {
                if let Some(v) = field.get::<Dict<'_>>(keys::V) {
                    if let Some(br) = v.get::<Array<'_>>(b"ByteRange" as &[u8]) {
                        let parts: Vec<i64> = br.iter::<i64>().collect();
                        if parts.len() == 4 {
                            let (b1, _l1, b2, l2) = (parts[0], parts[1], parts[2], parts[3]);
                            // First range must start at byte 0.
                            if b1 != 0 {
                                error(
                                    report,
                                    "6.4.3",
                                    format!(
                                        "Signature ByteRange does not start at 0 (starts at {b1})"
                                    ),
                                );
                            }
                            // Second range must extend to the end of the file. (#475)
                            if b2 + l2 != file_size {
                                error(
                                    report,
                                    "6.4.3",
                                    format!(
                                        "Signature ByteRange does not cover entire file: \
                                         b2+l2={} but file_size={file_size}",
                                        b2 + l2
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }
        if let Some(kids) = field.get::<Array<'_>>(keys::KIDS) {
            check_sig_byterange_fields(&kids, file_size, report, depth + 1);
        }
    }
}

// ─── §6.11 — Document structure ─────────────────────────────────────────────

/// Check document structure requirements (§6.11).
///
/// ViewerPreferences restrictions and PageLayout checks.
pub fn check_document_structure(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };

    // §6.11: ViewerPreferences must not contain /PickTrayByPDFSize
    if let Some(vp) = cat.get::<Dict<'_>>(keys::VIEWER_PREFERENCES) {
        if vp.contains_key(b"PickTrayByPDFSize" as &[u8]) {
            warning(
                report,
                "6.11",
                "ViewerPreferences contains /PickTrayByPDFSize",
            );
        }
        // /Enforce array should not be present
        if vp.contains_key(b"Enforce" as &[u8]) {
            warning(report, "6.11", "ViewerPreferences contains /Enforce");
        }
    }
}

// ─── §6.12 — Logical structure ──────────────────────────────────────────────

/// Check role mapping in structure tree (§6.12).
///
/// All non-standard structure element types must have a role mapping
/// to a standard structure type.
pub fn check_role_mapping(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(struct_tree) = cat.get::<Dict<'_>>(keys::STRUCT_TREE_ROOT) else {
        return;
    };

    let role_map = struct_tree.get::<Dict<'_>>(keys::ROLE_MAP);

    // Standard structure types (PDF 1.7 Table 333)
    let standard_types: &[&[u8]] = &[
        b"Document",
        b"Part",
        b"Art",
        b"Sect",
        b"Div",
        b"BlockQuote",
        b"Caption",
        b"TOC",
        b"TOCI",
        b"Index",
        b"NonStruct",
        b"Private",
        b"H",
        b"H1",
        b"H2",
        b"H3",
        b"H4",
        b"H5",
        b"H6",
        b"P",
        b"L",
        b"LI",
        b"Lbl",
        b"LBody",
        b"Table",
        b"TR",
        b"TH",
        b"TD",
        b"THead",
        b"TBody",
        b"TFoot",
        b"Span",
        b"Quote",
        b"Note",
        b"Reference",
        b"BibEntry",
        b"Code",
        b"Link",
        b"Annot",
        b"Ruby",
        b"Warichu",
        b"RB",
        b"RT",
        b"RP",
        b"WT",
        b"WP",
        b"Figure",
        b"Formula",
        b"Form",
    ];

    // Walk structure tree collecting all /S values
    let mut non_standard = Vec::new();
    collect_struct_types(&struct_tree, &mut non_standard, 0);

    for t in &non_standard {
        if standard_types.contains(&t.as_slice()) {
            continue;
        }

        let Some(rm) = role_map.as_ref() else {
            let type_str = std::str::from_utf8(t).unwrap_or("?");
            error(
                report,
                "6.12",
                format!(
                    "Structure element type '{type_str}' has no role mapping to a standard type"
                ),
            );
            continue;
        };

        // Non-standard type must be in RoleMap AND the chain must eventually
        // resolve to a standard structure type. A chain that ends at a
        // non-standard type without further mapping is a §6.7.3.4 violation.
        // Cycles are already caught by check_role_map_no_cycles. (#FN-6.7.3.4)
        let Some(first_target) = rm.get::<Name>(t.as_slice()) else {
            let type_str = std::str::from_utf8(t).unwrap_or("?");
            error(
                report,
                "6.12",
                format!(
                    "Structure element type '{type_str}' has no role mapping to a standard type"
                ),
            );
            continue;
        };

        // Follow chain until standard type, cycle, or dead end.
        let mut current = first_target.as_ref().to_vec();
        let mut visited = std::collections::HashSet::new();
        visited.insert(t.clone());
        loop {
            if standard_types.contains(&current.as_slice()) {
                break; // resolved to standard — OK
            }
            if !visited.insert(current.clone()) {
                break; // cycle — handled by check_role_map_no_cycles
            }
            match rm.get::<Name>(current.as_slice()) {
                Some(next) => {
                    current = next.as_ref().to_vec();
                }
                None => {
                    // Dead end: maps to non-standard type not in RoleMap
                    let type_str = std::str::from_utf8(t).unwrap_or("?");
                    let target_str = std::str::from_utf8(&current).unwrap_or("?");
                    error(
                        report,
                        "6.12",
                        format!(
                            "Structure element type '{type_str}' maps to '{target_str}' \
                             which is not a standard structure type and has no further mapping"
                        ),
                    );
                    break;
                }
            }
        }
    }
}

fn collect_struct_types(elem: &Dict<'_>, types: &mut Vec<Vec<u8>>, depth: usize) {
    if depth > 100 {
        return;
    }
    if let Some(s) = elem.get::<Name>(keys::S) {
        let t = s.as_ref().to_vec();
        if !types.contains(&t) {
            types.push(t);
        }
    }
    if let Some(kids) = elem.get::<Array<'_>>(keys::K) {
        for kid in kids.iter::<Dict<'_>>() {
            collect_struct_types(&kid, types, depth + 1);
        }
    } else if let Some(kid) = elem.get::<Dict<'_>>(keys::K) {
        collect_struct_types(&kid, types, depth + 1);
    }
}

/// §6.7.3.4 — RoleMap must not contain circular mappings.
///
/// ISO 19005-2/3 §6.7.3.4: "A circular mapping shall not exist in the RoleMap."
/// For example: A → B → A or A → B → C → A are circular. Walk the mapping chain
/// from each key and detect if any path revisits a key. (#FN-6.7.3.4)
pub fn check_rolemap_circular(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else { return };
    let Some(struct_tree) = cat.get::<Dict<'_>>(keys::STRUCT_TREE_ROOT) else {
        return;
    };
    let Some(role_map) = struct_tree.get::<Dict<'_>>(keys::ROLE_MAP) else {
        return;
    };
    // Collect all keys as byte vecs
    let keys_list: Vec<Vec<u8>> = role_map
        .entries()
        .map(|(k, _)| k.as_ref().to_vec())
        .collect();
    'outer: for start in &keys_list {
        let mut visited: Vec<Vec<u8>> = vec![start.clone()];
        let mut current = start.clone();
        loop {
            let next = match role_map.get::<Name>(current.as_slice()) {
                Some(n) => n.as_ref().to_vec(),
                None => break,
            };
            if visited.contains(&next) {
                // Cycle detected
                error(report, "6.7.3.4", "RoleMap contains a circular mapping");
                break 'outer;
            }
            visited.push(next.clone());
            current = next;
        }
    }
}

/// §6.2.10.8 (PDF/A-4) — StructElement /ActualText must not contain PUA codepoints.
///
/// ISO 19005-4 §6.2.10.8: The value of the ActualText entry in a structure element
/// dictionary shall not contain Unicode Private Use Area codepoints (U+E000–U+F8FF,
/// U+F0000–U+FFFFF, U+100000–U+10FFFF). Walks the structure tree and checks every
/// StructElem /ActualText hex or literal string. (#FN-6.2.10.8)
pub fn check_struct_elem_actualtext_pua(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else { return };
    let Some(struct_tree) = cat.get::<Dict<'_>>(keys::STRUCT_TREE_ROOT) else {
        return;
    };
    check_struct_elem_actualtext_pua_recursive(&struct_tree, report, 0);
}

fn check_struct_elem_actualtext_pua_recursive(
    elem: &Dict<'_>,
    report: &mut ComplianceReport,
    depth: usize,
) {
    if depth > 200 {
        return;
    }
    // Check /ActualText on this element
    if let Some(Object::String(s)) = elem.get::<Object<'_>>(b"ActualText" as &[u8]) {
        // String may be a raw byte sequence; check for UTF-16BE BOM (0xFE 0xFF)
        let bytes = s.as_ref();
        if bytes.len() >= 2
            && bytes[0] == 0xFE
            && bytes[1] == 0xFF
            && utf16be_bytes_contain_pua(&bytes[2..])
        {
            error(
                report,
                "6.2.10.8",
                "StructElem /ActualText contains Private Use Area (PUA) codepoint",
            );
            return;
        }
    }
    // Recurse into kids
    if let Some(kids) = elem.get::<Array<'_>>(keys::K) {
        for kid in kids.iter::<Dict<'_>>() {
            check_struct_elem_actualtext_pua_recursive(&kid, report, depth + 1);
        }
    } else if let Some(kid) = elem.get::<Dict<'_>>(keys::K) {
        check_struct_elem_actualtext_pua_recursive(&kid, report, depth + 1);
    }
}

/// Check if a raw UTF-16BE byte sequence (BOM already stripped) contains PUA codepoints.
fn utf16be_bytes_contain_pua(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i + 1 < bytes.len() {
        let hi = bytes[i] as u32;
        let lo = bytes[i + 1] as u32;
        let cp = (hi << 8) | lo;
        if (0xE000..=0xF8FF).contains(&cp) {
            return true;
        }
        // Surrogate pair → supplementary PUA
        if (0xD800..=0xDBFF).contains(&cp) && i + 3 < bytes.len() {
            let lo2 = (bytes[i + 2] as u32) << 8 | bytes[i + 3] as u32;
            if (0xDC00..=0xDFFF).contains(&lo2) {
                let full = 0x10000 + ((cp - 0xD800) << 10) + (lo2 - 0xDC00);
                if full >= 0xF0000 {
                    return true;
                }
                i += 2;
            }
        }
        i += 2;
    }
    false
}

// ─── §6.1.13 — Name length limit ────────────────────────────────────────────

/// Check implementation limits (§6.1.13): name length ≤ 127 and string length ≤ 32767.
///
/// Non-recursive: checks top-level objects and one level of dict/array nesting
/// to avoid OOM from shared indirect reference resolution.
pub fn check_name_length_limit(pdf: &Pdf, report: &mut ComplianceReport) {
    let cache = ObjectCache::new(pdf);
    check_name_length_limit_cached(&cache, report);
}

/// Cached version that uses pre-collected objects.
pub fn check_name_length_limit_cached(cache: &ObjectCache<'_>, report: &mut ComplianceReport) {
    let mut long_name = false;
    let mut long_string = false;
    for obj in cache.iter() {
        if !long_name && check_name_length_obj(obj) {
            long_name = true;
        }
        if !long_string && check_string_length_obj(obj) {
            long_string = true;
        }
        if long_name && long_string {
            break;
        }
    }
    if long_name {
        error(report, "6.1.13", "Name length exceeded 127");
    }
    if long_string {
        error(report, "6.1.13", "String length exceeded 32767");
    }
}

fn check_name_length_obj(obj: &Object<'_>) -> bool {
    use pdf_syntax::object::MaybeRef;
    match obj {
        Object::Name(n) => n.as_ref().len() > 127,
        Object::Dict(dict) => {
            for (key, val) in dict.entries() {
                if key.as_ref().len() > 127 {
                    return true;
                }
                // Check direct name values without resolving indirect refs
                if let MaybeRef::NotRef(Object::Name(n)) = val {
                    if n.as_ref().len() > 127 {
                        return true;
                    }
                }
            }
            false
        }
        Object::Stream(s) => {
            for (key, _) in s.dict().entries() {
                if key.as_ref().len() > 127 {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

fn check_string_length_obj(obj: &Object<'_>) -> bool {
    use pdf_syntax::object::MaybeRef;
    match obj {
        Object::String(s) => s.as_ref().len() > 32767,
        Object::Dict(dict) => {
            for (_, val) in dict.entries() {
                if let MaybeRef::NotRef(Object::String(s)) = val {
                    if s.as_ref().len() > 32767 {
                        return true;
                    }
                }
            }
            false
        }
        Object::Array(arr) => {
            for item in arr.iter::<Object<'_>>() {
                if let Object::String(s) = &item {
                    if s.as_ref().len() > 32767 {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Check that all Name objects contain valid UTF-8 sequences (§6.1.7 t1).
///
/// PDF/A requires name values to be valid UTF-8 byte sequences.
/// Names with null bytes or invalid UTF-8 must be flagged.
pub fn check_name_utf8(pdf: &Pdf, report: &mut ComplianceReport) {
    let cache = ObjectCache::new(pdf);
    check_name_utf8_cached(&cache, report);
}

/// Cached version that uses pre-collected objects.
pub fn check_name_utf8_cached(cache: &ObjectCache<'_>, report: &mut ComplianceReport) {
    for obj in cache.iter() {
        if check_name_utf8_obj(obj) {
            // Use unique internal clause to avoid remap conflicts with "6.1.7.1" (stream checks).
            // Remapped to "6.1.7" for all parts in pdfa.rs.
            error(
                report,
                "6.1.7-names",
                "Name value is not a valid UTF-8 sequence",
            );
            return;
        }
    }
}

fn check_name_utf8_obj(obj: &Object<'_>) -> bool {
    use pdf_syntax::object::MaybeRef;
    fn is_bad_name(bytes: &[u8]) -> bool {
        bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
    }
    /// Recursively scan a dict's direct (non-reference) entries.
    /// Colorant names in Separation/DeviceN live in arrays that are nested
    /// inside Resources/ColorSpace dicts, so we recurse into inline dicts and
    /// scan one level into inline arrays. Depth-limited to avoid pathological
    /// inputs. (#FN-6.1.7)
    fn check_dict_entries(dict: &pdf_syntax::object::Dict<'_>, depth: u8) -> bool {
        for (key, val) in dict.entries() {
            if is_bad_name(key.as_ref()) {
                return true;
            }
            match &val {
                MaybeRef::NotRef(Object::Name(n)) => {
                    if is_bad_name(n.as_ref()) {
                        return true;
                    }
                }
                MaybeRef::NotRef(Object::Array(arr)) => {
                    for item in arr.raw_iter() {
                        if let MaybeRef::NotRef(Object::Name(n)) = item {
                            if is_bad_name(n.as_ref()) {
                                return true;
                            }
                        }
                    }
                }
                MaybeRef::NotRef(Object::Dict(d)) if depth < 4 => {
                    if check_dict_entries(d, depth + 1) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
    match obj {
        Object::Name(n) => is_bad_name(n.as_ref()),
        Object::Dict(dict) => check_dict_entries(dict, 0),
        Object::Stream(s) => check_dict_entries(s.dict(), 0),
        _ => false,
    }
}

/// Check implementation limits: array capacity ≤ 8191 (§6.1.13).
pub fn check_array_capacity_limit(pdf: &Pdf, report: &mut ComplianceReport) {
    check_array_capacity_limit_cached(&ObjectCache::new(pdf), report);
}

pub fn check_array_capacity_limit_cached(cache: &ObjectCache<'_>, report: &mut ComplianceReport) {
    use pdf_syntax::object::MaybeRef;
    for obj in cache.iter() {
        match obj {
            Object::Array(arr) => {
                let count = arr.raw_iter().count();
                if count > 8191 {
                    error(
                        report,
                        "6.1.13",
                        format!("Array capacity ({count}) exceeded 8191"),
                    );
                    return;
                }
            }
            Object::Dict(dict) => {
                for (_, val) in dict.entries() {
                    if let MaybeRef::NotRef(Object::Array(inner_arr)) = val {
                        let count = inner_arr.raw_iter().count();
                        if count > 8191 {
                            error(
                                report,
                                "6.1.13",
                                format!("Array capacity ({count}) exceeded 8191"),
                            );
                            return;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Check CID values don't exceed 65535 (§6.1.13 test 10).
pub fn check_cid_value_limit(pdf: &Pdf, report: &mut ComplianceReport) {
    for_each_font(pdf, |name, font_dict, page_idx| {
        // Check CIDFont descendants for /W entries with large CID values
        if let Some(descendants) = font_dict.get::<Array<'_>>(keys::DESCENDANT_FONTS) {
            for cid_font in descendants.iter::<Dict<'_>>() {
                if let Some(w_arr) = cid_font.get::<Array<'_>>(keys::W) {
                    for item in w_arr.iter::<Object<'_>>() {
                        if let Object::Number(n) = &item {
                            let val = n.as_f64() as i64;
                            if val > 65535 {
                                error_at(
                                    report,
                                    "6.1.13",
                                    format!("CID value ({val}) exceeded 65535 in font {name}"),
                                    format!("page {}", page_idx + 1),
                                );
                                return;
                            }
                        }
                    }
                }
            }
        }
        // Check CMap encoding streams for CID values > 65535 in cidrange/cidchar.
        // CMap streams contain entries like: <startCode> <endCode> startCID
        // where startCID is a decimal number that must be <= 65535.
        if let Some(enc_stream) = font_dict.get::<Stream<'_>>(keys::ENCODING) {
            if let Ok(decoded) = enc_stream.decoded() {
                if let Ok(text) = std::str::from_utf8(&decoded) {
                    check_cmap_cid_values(text, name, page_idx, report);
                }
            }
        }
    });
}

/// Scan decoded CMap text for CID values > 65535 in cidrange/cidchar sections.
fn check_cmap_cid_values(
    text: &str,
    font_name: &str,
    page_idx: usize,
    report: &mut ComplianceReport,
) {
    // Look for decimal numbers after hex-coded ranges: <xxxx> <xxxx> DECIMAL
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        // Skip to after '>' (end of hex code)
        if bytes[i] == b'>' {
            i += 1;
            // Skip whitespace
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            // Check if next token is a decimal number (CID value)
            if i < bytes.len() && bytes[i].is_ascii_digit() {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if let Ok(val) = text[start..i].parse::<u64>() {
                    if val > 65535 {
                        error_at(
                            report,
                            "6.1.13",
                            format!("CMap CID value ({val}) exceeds 65535 in font {font_name}"),
                            format!("page {}", page_idx + 1),
                        );
                        return;
                    }
                }
            }
        } else {
            i += 1;
        }
    }
}

// ─── §6.1.12 — Real value limits ────────────────────────────────────────────

/// Check that real values are within the PDF/A limits (§6.1.12).
///
/// Absolute real values must be <= 32767.0.
/// Non-recursive: checks top-level objects and one level of dict/array nesting.
pub fn check_real_value_limits(pdf: &Pdf, report: &mut ComplianceReport) {
    check_real_value_limits_cached(&ObjectCache::new(pdf), report);
}

pub fn check_real_value_limits_cached(cache: &ObjectCache<'_>, report: &mut ComplianceReport) {
    for obj in cache.iter() {
        if check_real_limit_obj(obj) {
            error(report, "6.1.12", "Real value out of range (exceeds 32767)");
            return;
        }
    }
}

// PDF implementation limit: real (floating-point) values must not exceed 32767 in magnitude.
// Integers are NOT subject to this limit — /Flags 262177, date values etc. are legal.
// Only fire when n.is_real() to avoid FPs on integer dict values. Fixes #FP-6.1.12.
fn is_real_over_limit(n: &pdf_syntax::object::Number) -> bool {
    n.is_real() && n.as_f64().abs() > 32767.0
}

fn check_real_limit_obj(obj: &Object<'_>) -> bool {
    use pdf_syntax::object::MaybeRef;
    match obj {
        Object::Number(n) => is_real_over_limit(n),
        Object::Dict(dict) => {
            for (_, val) in dict.entries() {
                match val {
                    MaybeRef::NotRef(Object::Number(n)) => {
                        if is_real_over_limit(&n) {
                            return true;
                        }
                    }
                    MaybeRef::NotRef(Object::Array(arr)) => {
                        for item in arr.raw_iter() {
                            if let MaybeRef::NotRef(Object::Number(n)) = item {
                                if is_real_over_limit(&n) {
                                    return true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            false
        }
        Object::Array(arr) => {
            for item in arr.raw_iter() {
                if let MaybeRef::NotRef(Object::Number(n)) = item {
                    if is_real_over_limit(&n) {
                        return true;
                    }
                }
            }
            false
        }
        Object::Stream(s) => {
            for (_, val) in s.dict().entries() {
                if let MaybeRef::NotRef(Object::Number(n)) = val {
                    if is_real_over_limit(&n) {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Check for non-zero real values too close to 0 (§6.1.13 test 5).
///
/// PDF implementation limit: non-zero real values must have absolute value >= ~1.175e-38.
pub fn check_near_zero_reals(pdf: &Pdf, report: &mut ComplianceReport) {
    check_near_zero_reals_cached(&ObjectCache::new(pdf), report);
}

pub fn check_near_zero_reals_cached(cache: &ObjectCache<'_>, report: &mut ComplianceReport) {
    const MIN_POSITIVE: f64 = 1.175e-38;
    for obj in cache.iter() {
        if check_near_zero_obj(obj, MIN_POSITIVE) {
            error(report, "6.1.13", "Non-zero real value too close to 0.0");
            return;
        }
    }
}

fn check_near_zero_obj(obj: &Object<'_>, min: f64) -> bool {
    use pdf_syntax::object::MaybeRef;
    match obj {
        Object::Number(n) => {
            let v = n.as_f64();
            v != 0.0 && v.abs() < min
        }
        Object::Dict(dict) => {
            for (_, val) in dict.entries() {
                if let MaybeRef::NotRef(Object::Number(n)) = val {
                    let v = n.as_f64();
                    if v != 0.0 && v.abs() < min {
                        return true;
                    }
                }
            }
            false
        }
        Object::Array(arr) => {
            for val in arr.raw_iter() {
                if let MaybeRef::NotRef(Object::Number(n)) = val {
                    let v = n.as_f64();
                    if v != 0.0 && v.abs() < min {
                        return true;
                    }
                }
            }
            false
        }
        // Stream dict entries (e.g. Pattern /XStep, /YStep) may contain subnormal
        // floats — check them too. Fixes TWG A018 §6.1.13 false negative. (#467)
        Object::Stream(s) => {
            for (_, val) in s.dict().entries() {
                if let MaybeRef::NotRef(Object::Number(n)) = val {
                    let v = n.as_f64();
                    if v != 0.0 && v.abs() < min {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Check integer values are within 32-bit signed range (§6.1.13 test 1).
///
/// PDF integers must be in [-2147483648, 2147483647].
pub fn check_integer_range(pdf: &Pdf, report: &mut ComplianceReport) {
    check_integer_range_cached(&ObjectCache::new(pdf), report);
}

pub fn check_integer_range_cached(cache: &ObjectCache<'_>, report: &mut ComplianceReport) {
    const MAX_INT: f64 = 2_147_483_647.0;
    const MIN_INT: f64 = -2_147_483_648.0;
    for obj in cache.iter() {
        if check_integer_range_obj(obj, MIN_INT, MAX_INT) {
            error(report, "6.1.13", "Integer value out of range");
            return;
        }
    }
}

fn is_int_out_of_range(v: f64, min: f64, max: f64) -> bool {
    v.fract() == 0.0 && (v > max || v < min)
}

fn check_integer_range_obj(obj: &Object<'_>, min: f64, max: f64) -> bool {
    use pdf_syntax::object::MaybeRef;
    match obj {
        Object::Number(n) => is_int_out_of_range(n.as_f64(), min, max),
        Object::Dict(dict) => {
            for (_, val) in dict.entries() {
                match val {
                    MaybeRef::NotRef(Object::Number(n)) => {
                        if is_int_out_of_range(n.as_f64(), min, max) {
                            return true;
                        }
                    }
                    MaybeRef::NotRef(Object::Array(arr)) => {
                        for item in arr.raw_iter() {
                            if let MaybeRef::NotRef(Object::Number(n)) = item {
                                if is_int_out_of_range(n.as_f64(), min, max) {
                                    return true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            false
        }
        Object::Array(arr) => {
            for item in arr.raw_iter() {
                if let MaybeRef::NotRef(Object::Number(n)) = item {
                    if is_int_out_of_range(n.as_f64(), min, max) {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

// ─── §6.3.2 — Font program format ───────────────────────────────────────────

/// Check font file stream /Subtype is valid for PDF/A-1 (§6.3.2).
///
/// Valid font file subtypes: Type1C, CIDFontType0C.
/// (OpenType added in PDF/A-2+).
pub fn check_font_file_subtype(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    check_font_file_subtype_cached(&ObjectCache::new(pdf), part, report);
}

pub fn check_font_file_subtype_cached(
    cache: &ObjectCache<'_>,
    part: u8,
    report: &mut ComplianceReport,
) {
    for obj in cache.iter() {
        let Object::Stream(s) = obj else { continue };
        let dict = s.dict();
        let Some(subtype) = dict.get::<Name>(keys::SUBTYPE) else {
            continue;
        };
        let st = subtype.as_ref();
        let is_font_stream = st == b"Type1C" || st == b"CIDFontType0C" || st == b"OpenType";
        if !is_font_stream {
            continue;
        }
        if part == 1 && st == b"OpenType" {
            let st_str = std::str::from_utf8(st).unwrap_or("?");
            error(
                report,
                "6.3.2",
                format!("Font file stream has Subtype {st_str}, not allowed in PDF/A-1"),
            );
        }
    }
}

// ─── §6.2.2 — Explicit Resources ────────────────────────────────────────────

/// Check that content streams have explicitly associated Resources (§6.2.2).
///
/// In PDF/A, resources used by a content stream must be defined in an
/// explicitly associated Resources dict, not inherited from parent Pages.
pub fn check_explicit_resources(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let loc = format!("page {}", page_idx + 1);

        // Check if page has its own /Resources entry (not inherited)
        let has_own_resources = page_dict.contains_key(keys::RESOURCES);

        // If page has content stream but no own Resources, check if it
        // would need to inherit them.
        //
        // Note: page.resources() returns a Resources where the page's own dict
        // entries are empty (since the page has no /Resources). Inherited entries
        // live in res.parent(). We must check the parent chain for any resource
        // that would be inherited. Fixes #467.
        if !has_own_resources && page.page_stream().is_some() {
            let res = page.resources();
            // Check own (page-level) resources first
            let has_own_res = res.fonts.entries().next().is_some()
                || res.x_objects.entries().next().is_some()
                || res.ext_g_states.entries().next().is_some()
                || res.color_spaces.entries().next().is_some()
                || res.patterns.entries().next().is_some()
                || res.shadings.entries().next().is_some();
            // Check inherited resources from Pages parent nodes
            let has_inherited_res = {
                let mut p = res.parent();
                let mut found = false;
                while let Some(parent) = p {
                    if parent.fonts.entries().next().is_some()
                        || parent.x_objects.entries().next().is_some()
                        || parent.ext_g_states.entries().next().is_some()
                        || parent.color_spaces.entries().next().is_some()
                        || parent.patterns.entries().next().is_some()
                        || parent.shadings.entries().next().is_some()
                    {
                        found = true;
                        break;
                    }
                    p = parent.parent();
                }
                found
            };

            if has_own_res || has_inherited_res {
                error_at(
                    report,
                    "6.2.2",
                    "Content stream uses resources not defined in an explicitly associated Resources dictionary",
                    loc.clone(),
                );
            }
        }

        // Check Form XObjects for missing Resources
        if let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) {
            if let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) {
                for (xname, _) in xobj_dict.entries() {
                    let Some(stream) = xobj_dict.get::<Stream<'_>>(xname.as_ref()) else {
                        continue;
                    };
                    let dict = stream.dict();
                    let is_form = dict
                        .get::<Name>(keys::SUBTYPE)
                        .is_some_and(|s| s.as_ref() == b"Form");
                    if !is_form {
                        continue;
                    }
                    // Form XObjects must define all their resources explicitly.
                    // §6.2.2 T2: `inheritedResourceNames == ''` — no resource
                    // name used by the stream may be inherited from a parent dict.
                    if let Ok(decoded) = stream.decoded() {
                        let xn = std::str::from_utf8(xname.as_ref()).unwrap_or("?");
                        if !dict.contains_key(keys::RESOURCES) {
                            // No Resources dict at all but stream uses resource operators.
                            if stream_references_resources(&decoded) {
                                error_at(
                                    report,
                                    "6.2.2",
                                    format!("Form XObject {xn} references resources but has no explicit Resources dictionary"),
                                    loc.clone(),
                                );
                            }
                        } else {
                            // Has Resources dict, but check it actually covers every
                            // name the stream uses (empty or partial dicts still cause
                            // inherited-resource violations). (#FN-6.2.2)
                            let own_names: std::collections::HashSet<Vec<u8>> = dict
                                .get::<Dict<'_>>(keys::RESOURCES)
                                .map(|res| {
                                    let mut names = std::collections::HashSet::new();
                                    for sub_key in [
                                        keys::COLORSPACE,
                                        keys::FONT,
                                        keys::XOBJECT,
                                        keys::EXT_G_STATE,
                                        keys::PATTERN,
                                        keys::SHADING,
                                        keys::PROPERTIES,
                                    ] {
                                        if let Some(sub) = res.get::<Dict<'_>>(sub_key) {
                                            for (k, _) in sub.entries() {
                                                names.insert(k.as_ref().to_vec());
                                            }
                                        }
                                    }
                                    names
                                })
                                .unwrap_or_default();
                            if stream_has_inherited_resource_refs(&decoded, &own_names) {
                                error_at(
                                    report,
                                    "6.2.2",
                                    format!("Form XObject {xn} references resource names not in its own Resources dictionary (would be inherited)"),
                                    loc.clone(),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Check that resource names referenced in content streams exist in the
/// Resources dictionary (§6.2.2 test 2).
///
/// Only checks `Do` (XObject) and `Tf` (Font) operators — these are the most
/// unambiguous cases. Colorspace/ExtGState/Shading/Pattern operators are
/// intentionally skipped because they have more edge cases (inherited resources
/// in complex page trees, inline image keywords, Separation alternates, etc.)
/// that lead to false positives.
///
/// Uses the parent-chain-aware `get_x_object` / `get_font` accessors so that
/// resources inherited from ancestor Pages nodes are not incorrectly flagged.
pub fn check_resource_names_exist(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let Some(content) = page.page_stream() else {
            continue;
        };
        let res = page.resources();
        let loc = format!("page {}", page_idx + 1);
        check_do_tf_refs_in_stream(content, res, &loc, report);
    }
}

/// Check only `Do` (XObject invocation) and `Tf` (font selection) operators in
/// a content stream.  Uses parent-chain-aware lookups so inherited resources
/// from ancestor Pages nodes are not incorrectly flagged as missing.
fn check_do_tf_refs_in_stream(
    content: &[u8],
    res: &Resources<'_>,
    location: &str,
    report: &mut ComplianceReport,
) {
    let text = String::from_utf8_lossy(content);
    let tokens: Vec<&str> = text.split_ascii_whitespace().collect();
    let mut i = 0;
    let mut in_inline = false;
    while i < tokens.len() {
        let tok = tokens[i];
        // Skip inline image data between ID and EI
        if tok == "ID" {
            in_inline = true;
            i += 1;
            continue;
        }
        if tok == "EI" {
            in_inline = false;
            i += 1;
            continue;
        }
        if in_inline {
            i += 1;
            continue;
        }

        match tok {
            "Do" => {
                // /Name Do — XObject invocation; name is 1 token before.
                // Use contains_key rather than get_x_object (which returns Option<Stream>
                // and returns None for streams with /F <indirect-ref> that pdf-syntax
                // can't parse as a Stream, even though the entry exists). Fixes FP=6.2.2
                // on PDFs where a Form XObject has an external file reference (/F n 0 R).
                if i >= 1 {
                    if let Some(name) = tokens[i - 1].strip_prefix('/') {
                        let in_own = res.x_objects.contains_key(name.as_bytes());
                        let in_parent = !in_own
                            && res
                                .parent()
                                .is_some_and(|p| p.x_objects.contains_key(name.as_bytes()));
                        if !in_own && !in_parent {
                            error_at(
                                report,
                                "6.2.2",
                                format!("/{name} referenced by Do but not in Resources/XObject"),
                                location.to_string(),
                            );
                        }
                    }
                }
            }
            "Tf" => {
                // /Name size Tf — font selection; name is 2 tokens before.
                // Use contains_key for the same reason as Do above.
                if i >= 2 {
                    if let Some(name) = tokens[i - 2].strip_prefix('/') {
                        let in_own = res.fonts.contains_key(name.as_bytes());
                        let in_parent = !in_own
                            && res
                                .parent()
                                .is_some_and(|p| p.fonts.contains_key(name.as_bytes()));
                        if !in_own && !in_parent {
                            error_at(
                                report,
                                "6.2.2",
                                format!("Font /{name} referenced by Tf but not in Resources/Font"),
                                location.to_string(),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
}

// ─── §6.1.3 — Trailer Info key for PDF/A-4 ──────────────────────────────────

/// Check trailer requirements (§6.1.3).
///
/// PDF/A-1: trailer must have /ID keyword.
/// PDF/A-4: trailer must not have /Info unless catalog has /PieceInfo.
pub fn check_trailer_requirements(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    let data = pdf.data().as_ref();

    // All PDF/A parts require /ID in trailer (§6.1.3).
    // Check ALL trailer dicts — linearized PDFs have multiple trailers and
    // ALL must contain /ID (veraPDF §6.1.3 t1/t4).
    if part >= 1 {
        let id_status = if data.windows(7).any(|w| w == b"trailer") {
            // Scan every "trailer" occurrence in the file
            let mut any_missing = false;
            let mut any_empty = false;
            let mut search = 0;
            while let Some(rel) = data[search..].windows(7).position(|w| w == b"trailer") {
                let abs = search + rel;
                // Must be followed by whitespace or << to be a real trailer keyword
                let next = data.get(abs + 7).copied().unwrap_or(0);
                if next != b'\n' && next != b'\r' && next != b' ' && next != b'<' {
                    search = abs + 7;
                    continue;
                }
                let end = data.len().min(abs + 2000);
                let region = &data[abs..end];
                // Must have dict start
                if !region.windows(2).any(|w| w == b"<<") {
                    search = abs + 7;
                    continue;
                }
                if !region.windows(3).any(|w| w == b"/ID") {
                    any_missing = true;
                } else if let Some(id_off) = region.windows(4).position(|w| w == b"/ID ") {
                    let after = &region[id_off + 4..];
                    let stripped: Vec<_> = after
                        .iter()
                        .skip_while(|&&b| b == b'[' || b == b' ' || b == b'\n' || b == b'\r')
                        .collect();
                    if stripped.first() == Some(&&b'<') && stripped.get(1) == Some(&&b'>') {
                        any_empty = true;
                    }
                }
                search = abs + 7;
            }
            if any_missing {
                0u8
            } else if any_empty {
                2u8
            } else {
                1u8
            }
        } else {
            // Cross-reference stream (PDF 1.5+): /ID is embedded in the XRef stream dict.
            // The XRef stream object itself is NOT listed in its own /Index, so pdf.objects()
            // does not yield it. Scan raw bytes instead: look for "/Type /XRef" within a 2 KB
            // window that also contains "/ID". (#FP-6.1.3)
            let found = {
                let needle_xref = b"/Type /XRef";
                let needle_id = b"/ID";
                let mut ok = false;
                let mut search = 0;
                while let Some(off) = data[search..]
                    .windows(needle_xref.len())
                    .position(|w| w == needle_xref)
                {
                    let abs = search + off;
                    let window_start = abs.saturating_sub(512);
                    let window_end = (abs + 2048).min(data.len());
                    let window = &data[window_start..window_end];
                    if window.windows(needle_id.len()).any(|w| w == needle_id) {
                        ok = true;
                        break;
                    }
                    search = abs + needle_xref.len();
                }
                ok
            };
            if found {
                1u8
            } else {
                0u8
            }
        };
        if id_status == 0 {
            error(
                report,
                "6.1.3",
                "Trailer dictionary missing required /ID key",
            );
        } else if id_status == 2 {
            error(
                report,
                "6.1.3",
                "Trailer /ID contains empty identifiers — both ID values must be non-empty",
            );
        }
        // §6.1.3 t4: In linearized PDFs, /ID in all trailers must be identical.
        check_linearized_id_mismatch(data, report);
    }

    if part == 4 {
        check_trailer_info_key(pdf, report);
    }
}

/// §6.1.3 t4: If a linearized PDF has /ID in multiple trailers, all must match.
///
/// Operates on raw bytes (not str) to avoid char-boundary panics on non-UTF-8
/// content (e.g. binary comment `%PDF-1.x%\xE2\xE3...`). Fixes crash on
/// linearized PDFs with high-byte binary content. (#panic-6.1.3)
fn check_linearized_id_mismatch(data: &[u8], report: &mut ComplianceReport) {
    let mut id_values: Vec<Vec<u8>> = Vec::new();
    let mut search = 0;
    while let Some(rel) = data[search..].windows(7).position(|w| w == b"trailer") {
        let abs = search + rel;
        let next = data.get(abs + 7).copied().unwrap_or(0);
        if next != b'\n' && next != b'\r' && next != b' ' && next != b'<' {
            search = abs + 7;
            continue;
        }
        let end = data.len().min(abs + 2000);
        let region = &data[abs..end];
        if !region.windows(2).any(|w| w == b"<<") {
            search = abs + 7;
            continue;
        }
        // Extract first hex string from /ID [<hex1><hex2>]
        if let Some(id_pos) = region.windows(3).position(|w| w == b"/ID") {
            let after = &region[id_pos + 3..];
            if let Some(open) = after.iter().position(|&b| b == b'<') {
                // Skip `<<` dict delimiters — we want the first `<hex>` scalar
                if after.get(open + 1) != Some(&b'<') {
                    if let Some(close) = after[open + 1..].iter().position(|&b| b == b'>') {
                        let hex = after[open + 1..open + 1 + close]
                            .iter()
                            .map(|b| b.to_ascii_lowercase())
                            .collect::<Vec<u8>>();
                        id_values.push(hex);
                    }
                }
            }
        }
        search = abs + 7;
    }
    if id_values.len() >= 2 {
        let first = &id_values[0];
        if id_values.iter().any(|v| v != first) {
            error(
                report,
                "6.1.3",
                "Linearized PDF has different /ID values across trailers",
            );
        }
    }
}

/// Check that Info key is not present in trailer for PDF/A-4 (§6.1.3).
///
/// Unless there's a PieceInfo entry in the document catalog.
/// When PieceInfo is present (exemption), the Info dict must contain only /ModDate. (#467)
fn check_trailer_info_key(pdf: &Pdf, report: &mut ComplianceReport) {
    // Check if trailer has /Info by scanning raw bytes
    let data = pdf.data().as_ref();
    let has_info = if let Some(trailer_pos) = data.windows(7).rposition(|w| w == b"trailer") {
        let end = data.len().min(trailer_pos + 2000);
        let trailer_region = &data[trailer_pos..end];
        trailer_region.windows(5).any(|w| w == b"/Info")
    } else {
        false
    };

    if !has_info {
        return;
    }

    // Check if catalog has /PieceInfo (exemption)
    let has_piece_info = catalog(pdf).is_some_and(|cat| cat.contains_key(b"PieceInfo" as &[u8]));

    if !has_piece_info {
        error(
            report,
            "6.1.3",
            "Info key present in trailer without PieceInfo in catalog (forbidden in PDF/A-4)",
        );
        return;
    }

    // PDF/A-4 §6.1.3: when /Info is allowed (PieceInfo present), the Info
    // dictionary shall only contain the ModDate entry. Entries like Author,
    // Creator, Producer, CreationDate etc. are all forbidden. (#467)
    let meta = pdf.metadata();
    let forbidden: &[(&str, bool)] = &[
        ("Title", meta.title.is_some()),
        ("Author", meta.author.is_some()),
        ("Subject", meta.subject.is_some()),
        ("Keywords", meta.keywords.is_some()),
        ("Creator", meta.creator.is_some()),
        ("Producer", meta.producer.is_some()),
        ("CreationDate", meta.creation_date.is_some()),
    ];
    for (key, present) in forbidden {
        if *present {
            error(
                report,
                "6.1.3",
                format!(
                    "Info dictionary contains /{key} which is forbidden in PDF/A-4 (only ModDate allowed)"
                ),
            );
        }
    }
}

// ─── §6.1.7 — Stream Length verification ─────────────────────────────────────

/// Check that declared /Length of streams matches actual byte count (§6.1.7).
///
/// Scans raw PDF bytes for `stream` / `endstream` pairs and compares
/// the actual byte count with the declared /Length value.
pub fn check_stream_length(pdf: &Pdf, report: &mut ComplianceReport) {
    let data = pdf.data().as_ref();
    let len = data.len();
    let mut pos = 0;

    while pos + 6 < len {
        // Find "stream" keyword followed by CR, LF, or CRLF
        let remaining = &data[pos..];
        let Some(stream_off) = find_keyword(remaining, b"stream") else {
            break;
        };
        let abs_stream = pos + stream_off;

        // Guard: 'stream' must be a standalone keyword, not part of a longer word
        // (e.g. avoid matching 'stream' inside '/InputStream' or binary data).
        // A valid stream keyword is ALWAYS preceded by '>>' (end of stream dict),
        // possibly with whitespace between the '>>' and 'stream'. The word "stream"
        // appearing inside a string literal (e.g. bookmark title text) is preceded
        // only by spaces/letters and has no '>' behind the whitespace. (#FP-6.1.7.1)
        {
            let mut scan = abs_stream;
            while scan > 0
                && (data[scan - 1] == b' '
                    || data[scan - 1] == b'\t'
                    || data[scan - 1] == b'\r'
                    || data[scan - 1] == b'\n')
            {
                scan -= 1;
            }
            if scan == 0 || data[scan - 1] != b'>' {
                pos = abs_stream + 6;
                continue;
            }
        }

        // stream keyword must be followed IMMEDIATELY by \r\n or \n (§6.1.7.1).
        // Spaces between 'stream' and the EOL are a violation — veraPDF counts them
        // as stream data, causing a Length mismatch AND an EOL-compliance failure.
        // Do NOT skip spaces: any non-EOL character after 'stream' is an error. (#FN-6.1.7)
        let after_keyword = abs_stream + 6; // skip "stream"
        if after_keyword >= len {
            break;
        }
        let eol_start = after_keyword;
        let data_start = if eol_start < len
            && data[eol_start] == b'\r'
            && eol_start + 1 < len
            && data[eol_start + 1] == b'\n'
        {
            eol_start + 2
        } else if eol_start < len && data[eol_start] == b'\n' {
            eol_start + 1
        } else {
            // No EOL after 'stream' (with or without whitespace).
            // Guard: a real stream keyword always has /Length in the preceding dict.
            // If /Length is absent, 'stream' is inside a string literal or comment
            // — not a keyword — so skip without error. (#FP-6.1.7)
            // Use has_length_key (not find_length_value) so indirect /Length refs
            // like `/Length 5 0 R` are also detected. (#FN-6.1.7, #FN-6.1.7.1)
            if has_length_key(data, abs_stream) {
                error(
                    report,
                    "6.1.7.1",
                    "Stream keyword not followed by required CR LF or LF end-of-line",
                );
            }
            pos = after_keyword;
            continue;
        };

        // Find "endstream" after the stream data
        let search_from = if data_start + 10 < len {
            data_start
        } else {
            break;
        };
        let remaining = &data[search_from..];
        let Some(endstream_off) = find_keyword(remaining, b"endstream") else {
            break;
        };
        let abs_endstream = search_from + endstream_off;

        // §6.1.7: endstream should be preceded by an EOL marker.
        // ISO 32000-1 §7.3.8.1 uses "should" (not "shall") for EOL before endstream.
        // PDF/A-1 §6.1.7 prohibits lone \r after "stream" (not before "endstream").
        // veraPDF accepts \r\n, \n, and lone \r before endstream. (#FP-6.1.7.1)
        // Fixes #467.
        let eol_before_endstream = if abs_endstream >= 2
            && data[abs_endstream - 2] == b'\r'
            && data[abs_endstream - 1] == b'\n'
        {
            true // \r\n — valid
        } else {
            abs_endstream >= 1
                && (data[abs_endstream - 1] == b'\n' || data[abs_endstream - 1] == b'\r')
            // lone \n or lone \r — valid for endstream; no EOL → false
        };
        if !eol_before_endstream {
            error(
                report,
                "6.1.7.1",
                "endstream keyword not preceded by required end-of-line marker",
            );
            return; // one violation is enough
        }

        // Actual length is bytes between data_start and endstream.
        // The EOL before endstream (\r\n, \n, or \r) is NOT included in /Length.
        // Track whether we stripped a \r after stripping \n: if so, the \r might
        // be the last byte of the content rather than part of a \r\n EOL.
        // (e.g. lopdf always writes just \n before endstream; if the content
        // ends in \r the byte sequence is ...\r\nendstream and we must not count
        // the \r as part of the EOL.) (#FP-6.1.7.1-len)
        let mut actual_end = abs_endstream;
        let mut stripped_cr_after_lf = false;
        if actual_end > data_start && data[actual_end - 1] == b'\n' {
            actual_end -= 1;
            if actual_end > data_start && data[actual_end - 1] == b'\r' {
                actual_end -= 1;
                stripped_cr_after_lf = true;
            }
        } else if actual_end > data_start && data[actual_end - 1] == b'\r' {
            actual_end -= 1;
        }
        let actual_len = actual_end - data_start;

        // Find the /Length value by scanning backwards from "stream" to find the dict
        let declared = find_length_value(data, abs_stream);
        if let Some(declared_len) = declared {
            if declared_len != actual_len {
                // If we stripped \r\n as the EOL and declared == actual+1, the \r
                // is actually the last byte of the content (not part of the EOL).
                // The writer (lopdf) added \n as EOL; /Length correctly includes the
                // \r. This is not a mismatch — avoid the FP. (#FP-6.1.7.1-len)
                if stripped_cr_after_lf && declared_len == actual_len + 1 {
                    pos = abs_endstream + 9;
                    continue;
                }
                // Use "6.1.7.1-len" (distinct from stream-EOL "6.1.7.1") so
                // remap_clause_numbers can map this specifically to §6.1.6.1 for PDF/A-4.
                error(
                    report,
                    "6.1.7.1-len",
                    format!("Stream Length mismatch: declared {declared_len}, actual {actual_len}"),
                );
                return; // One violation is enough
            }
        }

        pos = abs_endstream + 9;
    }
}

/// Find a keyword in data that is not part of a longer word.
fn find_keyword(data: &[u8], keyword: &[u8]) -> Option<usize> {
    let klen = keyword.len();
    let mut pos = 0;
    while pos + klen <= data.len() {
        if let Some(off) = data[pos..].windows(klen).position(|w| w == keyword) {
            let abs = pos + off;
            // Check it's not part of "endstream" when looking for "stream"
            if keyword == b"stream" && abs > 0 && data[abs - 1] == b'd' {
                // This is "endstream", skip
                pos = abs + klen;
                continue;
            }
            return Some(abs);
        }
        break;
    }
    None
}

/// Extract the /Length integer value from the stream dictionary, by scanning
/// backwards from the "stream" keyword to find `/Length <number>`.
fn find_length_value(data: &[u8], stream_pos: usize) -> Option<usize> {
    let start = stream_pos.saturating_sub(500);
    let region = &data[start..stream_pos];
    // Search for /Length as bytes (not UTF-8) to handle binary content
    let length_key = b"/Length";
    let mut last_idx = None;
    let mut search = 0;
    while search + length_key.len() <= region.len() {
        if let Some(off) = region[search..]
            .windows(length_key.len())
            .position(|w| w == length_key)
        {
            let abs = search + off;
            let next_pos = abs + length_key.len();
            // Only accept `/Length` when the next byte is whitespace (not a name-
            // continuation character). `/Length1`, `/Length2`, etc. are different
            // keys — their trailing digit would otherwise be parsed as the value.
            // Fixes false positive: `/Length1 15312` parsed as declared=1.
            let next_is_name_char = region
                .get(next_pos)
                .map(|b| b.is_ascii_alphanumeric())
                .unwrap_or(false);
            if !next_is_name_char {
                last_idx = Some(abs);
            }
            search = next_pos;
        } else {
            break;
        }
    }
    let idx = last_idx?;
    let after = &region[idx + length_key.len()..];
    // Skip whitespace
    let skip = after
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(after.len());
    let after = &after[skip..];
    // Parse digits
    let end = after
        .iter()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(after.len());
    if end == 0 {
        return None; // /Length might be an indirect reference
    }
    // Detect indirect reference: `/Length N M R` pattern.
    // After the digits, skip whitespace and check for another integer followed
    // by whitespace and 'R'. If found, the length is an indirect reference and
    // we cannot determine the declared length by raw scanning. (#FP-6.1.7)
    let rest = &after[end..];
    let ws = rest
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(rest.len());
    let rest = &rest[ws..];
    if rest.first().map(|b| b.is_ascii_digit()).unwrap_or(false) {
        // There is a second number after the first; check if followed by R
        let gen_end = rest
            .iter()
            .position(|b| !b.is_ascii_digit())
            .unwrap_or(rest.len());
        let rest2 = &rest[gen_end..];
        let ws2 = rest2
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .unwrap_or(rest2.len());
        if rest2.get(ws2).copied() == Some(b'R') {
            return None; // Indirect reference — skip length check
        }
    }
    std::str::from_utf8(&after[..end]).ok()?.parse().ok()
}

/// Check if `/Length` key (followed by a non-alphanumeric character) exists in
/// the 500 bytes preceding `stream_pos`. Unlike `find_length_value`, this
/// returns true even when `/Length` has an indirect-reference value like
/// `/Length 5 0 R`. Fixes #FN-6.1.7 on streams with indirect /Length.
fn has_length_key(data: &[u8], stream_pos: usize) -> bool {
    let start = stream_pos.saturating_sub(500);
    let region = &data[start..stream_pos];
    let needle = b"/Length";
    let mut search = 0;
    while search + needle.len() <= region.len() {
        if let Some(off) = region[search..]
            .windows(needle.len())
            .position(|w| w == needle)
        {
            let next_pos = search + off + needle.len();
            // Reject `/Length1`, `/Length2`, etc. — only accept `/Length` followed
            // by whitespace, digit, or end-of-region.
            let next_is_name_char = region
                .get(next_pos)
                .is_some_and(|b| b.is_ascii_alphanumeric());
            if !next_is_name_char {
                return true;
            }
            search = next_pos;
        } else {
            break;
        }
    }
    false
}

// ─── §6.1.8 / §6.1.9 — Object syntax spacing checks ────────────────────────

/// Check spacing around obj/endobj keywords (§6.1.8, §6.1.9).
///
/// Requirements:
/// - Object number and generation number separated by single white-space
/// - Generation number and "obj" separated by single white-space
/// - "obj" followed by EOL marker
/// - "endobj" preceded and followed by EOL marker
pub fn check_object_syntax_spacing(pdf: &Pdf, pdfa_part: u8, report: &mut ComplianceReport) {
    // §6.1.8 in PDF/A-1 and PDF/A-4; §6.1.9 in PDF/A-2 and PDF/A-3
    // (clause numbering shifted: PDF/A-4 renumbered inline-image filter clause to 6.1.9
    // and reused 6.1.8 for object syntax, matching PDF/A-1)
    let rule_id = match pdfa_part {
        1 | 4 => "6.1.8",
        _ => "6.1.9",
    };
    let data = pdf.data().as_ref();
    let len = data.len();

    // Use regex-like pattern matching: find `<digits><ws><digits><ws>obj`
    // and verify spacing is exactly single space/whitespace
    let mut pos = 0;
    while pos + 5 < len {
        // Find "obj" keyword (but not "endobj")
        let remaining = &data[pos..];
        let Some(obj_off) = remaining.windows(3).position(|w| w == b"obj") else {
            break;
        };
        let abs_obj = pos + obj_off;

        // Skip if part of "endobj"
        if abs_obj >= 3 && &data[abs_obj - 3..abs_obj] == b"end" {
            pos = abs_obj + 3;
            continue;
        }

        // Skip if not preceded by whitespace (must have `<gen> obj`)
        if abs_obj == 0 || !data[abs_obj - 1].is_ascii_whitespace() {
            pos = abs_obj + 3;
            continue;
        }

        // Scan backwards: expect single-ws + digit(s) + single-ws + digit(s) + EOL
        let before = &data[abs_obj.saturating_sub(30)..abs_obj];
        if before.is_empty() {
            pos = abs_obj + 3;
            continue;
        }

        // Parse backwards: whitespace, then gen number, then whitespace, then obj number
        let mut idx = before.len() - 1;

        // Count whitespace before "obj"
        let ws1_end = idx + 1;
        while idx > 0 && before[idx].is_ascii_whitespace() {
            idx -= 1;
        }
        let ws1_count = ws1_end - idx - 1;

        // Check: must be exactly 1 whitespace char before "obj"
        if ws1_count != 1 {
            error(
                report,
                rule_id,
                format!(
                    "Extra spacing before 'obj' keyword ({ws1_count} whitespace chars, expected 1)"
                ),
            );
            return;
        }

        // Parse generation number
        let gen_end = idx + 1;
        while idx > 0 && before[idx].is_ascii_digit() {
            idx -= 1;
        }
        let gen_start = idx + 1;
        if gen_start == gen_end {
            pos = abs_obj + 3;
            continue; // Not a valid object header
        }

        // Count whitespace between obj number and gen number
        let ws2_end = gen_start;
        while idx > 0 && before[idx].is_ascii_whitespace() {
            idx -= 1;
        }
        let ws2_count = ws2_end - idx - 1;

        // Check: must be exactly 1 whitespace between obj num and gen num
        if ws2_count > 1 {
            error(
                report,
                rule_id,
                format!("Extra spacing between object number and generation number ({ws2_count} whitespace chars, expected 1)"),
            );
            return;
        }

        // Parse object number digits
        let obj_num_end = idx + 1;
        while idx > 0 && before[idx].is_ascii_digit() {
            idx -= 1;
        }
        let obj_num_start = idx + 1;
        if obj_num_start == obj_num_end {
            pos = abs_obj + 3;
            continue; // Not a valid object header
        }

        // Check: object number must be preceded by EOL marker
        // (except for the very first object which may follow the header)
        let abs_obj_num_start = abs_obj.saturating_sub(30) + obj_num_start;
        if abs_obj_num_start > 0 {
            let before_obj = data[abs_obj_num_start - 1];
            if before_obj != b'\n' && before_obj != b'\r' {
                error(report, rule_id, "Object number not preceded by EOL marker");
                return;
            }
        }

        // Check "obj" is followed by EOL or whitespace
        let after_obj = abs_obj + 3;
        if after_obj < len {
            let c = data[after_obj];
            if c != b'\n' && c != b'\r' && c != b' ' && c != b'\t' {
                error(
                    report,
                    rule_id,
                    "Keyword 'obj' not followed by proper whitespace/EOL",
                );
                return;
            }
        }

        pos = abs_obj + 3;
    }

    // Check endobj spacing
    pos = 0;
    while pos + 6 < len {
        let remaining = &data[pos..];
        let Some(eobj_off) = remaining.windows(6).position(|w| w == b"endobj") else {
            break;
        };
        let abs_eobj = pos + eobj_off;

        // endobj must be preceded by EOL
        if abs_eobj > 0 {
            let before = data[abs_eobj - 1];
            if before != b'\n' && before != b'\r' {
                error(
                    report,
                    rule_id,
                    "Keyword 'endobj' not preceded by EOL marker",
                );
                return;
            }
        }

        // endobj must be followed by EOL or EOF
        let after = abs_eobj + 6;
        if after < len {
            let c = data[after];
            if c != b'\n' && c != b'\r' {
                error(
                    report,
                    rule_id,
                    "Keyword 'endobj' not followed by EOL marker",
                );
                return;
            }
        }

        pos = abs_eobj + 6;
    }
}

// ─── §6.7.8 — XMP extension schema validation ──────────────────────────────

/// Validate XMP extension schemas (§6.7.8).
///
/// Extension schemas must use correct namespace prefixes and have required fields:
/// - pdfaSchema:schema, pdfaSchema:namespaceURI, pdfaSchema:prefix
/// - pdfaProperty:name, pdfaProperty:valueType, pdfaProperty:category, pdfaProperty:description
/// - pdfaType:type, pdfaType:namespaceURI, pdfaType:description
pub fn check_xmp_extension_schema(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(xmp) = get_xmp_metadata(pdf) else {
        return;
    };
    let xmp_str = String::from_utf8_lossy(&xmp);

    // §6.6.2.3.1 — Check for non-predefined namespace usage without extension schema.
    // The XMP Dynamic Media namespace (xmpDM:) is defined in XMP Part 3 and is NOT
    // listed as a predefined namespace in ISO 19005-2 (which only references XMP Part 1).
    // When xmpDM: properties are used without a pdfaExtension:schemas declaration, it
    // violates §6.6.2.3.1. (#FN-6.6.2.3.1)
    let has_extension_schemas =
        xmp_str.contains("pdfaExtension:schemas") || xmp_str.contains("pdfaSchema:");
    if !has_extension_schemas {
        // Detect xmpDM: property usage (element names or attribute values using xmpDM:
        // prefix, but NOT xmlns: declarations which are just namespace bindings).
        let has_xmpdm_property = xmp_str
            .split("xmlns:")
            .skip(1) // skip the first chunk before any xmlns:
            .fold(xmp_str.as_ref(), |_, _| ""); // trick: just check below
        let _ = has_xmpdm_property; // suppress unused warning
                                    // Simpler: check if the XMP text contains xmpDM: USED as element/attribute
                                    // (i.e. appears as <xmpDM: or ` xmpDM:` but not just in xmlns: declarations).
        let has_xmpdm = {
            let mut found = false;
            let bytes = xmp_str.as_bytes();
            for i in 0..bytes.len().saturating_sub(6) {
                if &bytes[i..i + 6] == b"xmpDM:" {
                    // Check it's NOT inside an xmlns: declaration (xmlns:xmpDM)
                    let before_end = i.saturating_sub(7);
                    let prefix_before = if i >= 7 { &xmp_str[before_end..i] } else { "" };
                    if !prefix_before.ends_with("xmlns:") {
                        found = true;
                        break;
                    }
                }
            }
            found
        };
        if has_xmpdm {
            error(
                report,
                "6.6.2.3.1",
                "XMP uses xmpDM: (Dynamic Media) namespace without pdfaExtension:schemas \
                 declaration (xmpDM: is not a predefined namespace in ISO 19005-2)",
            );
        }
    }

    // Check if extension schemas are present (for format validation below)
    if !has_extension_schemas {
        return; // No extension schemas — nothing more to validate
    }

    // Use string-based parsing for extension schema validation
    // (avoid roxmltree dependency complexity)
    check_xmp_extension_schema_text(&xmp_str, report);
}

/// String-based XMP extension schema validation.
fn check_xmp_extension_schema_text(xmp: &str, report: &mut ComplianceReport) {
    // Check 1: Extension schema container must use prefix "pdfaExtension"
    // The container element should be <pdfaExtension:schemas>
    if xmp.contains("pdfaExt:schemas") && !xmp.contains("pdfaExtension:schemas") {
        error(
            report,
            "6.7.8",
            "Extension schema container uses wrong prefix (expected 'pdfaExtension')",
        );
        return;
    }

    // Check 2: Schema fields must use prefix "pdfaSchema"
    // Look for common fields with wrong prefix
    for field in ["schema", "namespaceURI", "prefix", "property"] {
        // Check if the field exists with a non-standard pdfaSchema prefix
        let correct = format!("pdfaSchema:{field}");
        // Check for variations like nonpdfaSchema:field or pdfaSch:field
        if !xmp.contains(&correct) {
            // If the field doesn't appear at all with correct prefix, check for wrong prefix
            // by looking for ":field" after a pdfaSchema-like prefix
            continue;
        }
    }

    // Check 3: Look for schema fields with wrong prefixes
    // Pattern: <wrongprefix:schema>, <wrongprefix:namespaceURI>, etc.
    let schema_fields = ["schema", "namespaceURI", "prefix"];
    for field in schema_fields {
        let correct_prefix = format!("pdfaSchema:{field}");
        // Find all occurrences of this field with any prefix
        let search = format!(":{field}>");
        for (idx, _) in xmp.match_indices(&search) {
            // Look backwards for '<' to find the tag
            let before = &xmp[..idx];
            if let Some(tag_start) = before.rfind('<') {
                let tag = &xmp[tag_start..idx + search.len()];
                // Skip closing tags
                if tag.starts_with("</") {
                    continue;
                }
                let elem_name = &xmp[tag_start + 1..idx + 1 + field.len()];
                if !elem_name.starts_with(&correct_prefix) && elem_name.contains(':') {
                    let prefix = elem_name.split(':').next().unwrap_or("?");
                    if prefix.starts_with("pdfa") && prefix != "pdfaSchema" {
                        // §6.6.2.3.3 (PDF/A-2/3): schema definition fields must use
                        // "pdfaSchema" prefix. Remapped to §6.7.8 for PDF/A-1. (#476)
                        error(
                            report,
                            "6.6.2.3.3",
                            format!(
                                "Extension schema field '{field}' uses wrong prefix '{prefix}' (expected 'pdfaSchema')"
                            ),
                        );
                        return;
                    }
                }
            }
        }
    }

    // Check 4: Property fields must use prefix "pdfaProperty"
    let property_fields = ["name", "valueType", "category", "description"];
    for field in property_fields {
        let correct_prefix = format!("pdfaProperty:{field}");
        let search = format!(":{field}>");
        for (idx, _) in xmp.match_indices(&search) {
            let before = &xmp[..idx];
            if let Some(tag_start) = before.rfind('<') {
                let tag = &xmp[tag_start..idx + search.len()];
                if tag.starts_with("</") {
                    continue;
                }
                let elem_name = &xmp[tag_start + 1..idx + 1 + field.len()];
                if !elem_name.starts_with(&correct_prefix) && elem_name.contains(':') {
                    let prefix = elem_name.split(':').next().unwrap_or("?");
                    if prefix.starts_with("pdfa") && prefix != "pdfaProperty" {
                        error(
                            report,
                            "6.7.8",
                            format!(
                                "Extension schema property field '{field}' uses wrong prefix '{prefix}' (expected 'pdfaProperty')"
                            ),
                        );
                        return;
                    }
                }
            }
        }
    }

    // Check 5: valueType definitions
    let standard_types = [
        "Text",
        "URI",
        "URL",
        "Boolean",
        "Integer",
        "Real",
        "Date",
        "MIMEType",
        "AgentName",
        "RenditionClass",
        "ResourceEvent",
        "ResourceRef",
        "Version",
        "Rational",
        "Lang Alt",
        "Bag Text",
        "Seq Text",
        "Bag ProperName",
        "GUID",
        "Locale",
        "XPath",
        "Part",
        "GPSCoordinate",
        "bag Text",
        "seq Text",
        "Bag Choice",
        "InternalRef",
        "ExternalRef",
        "Field",
        "Dimensions",
    ];

    for (tag_start, tag_end) in find_xml_element_values(xmp, "pdfaProperty:valueType") {
        let val = xmp[tag_start..tag_end].trim();
        if val.is_empty() {
            continue;
        }
        if !standard_types.contains(&val) {
            let type_defined =
                find_xml_element_values(xmp, "pdfaType:type").any(|(s, e)| xmp[s..e].trim() == val);
            if !type_defined {
                error(
                    report,
                    "6.7.8",
                    format!("Extension schema property valueType '{val}' is not defined"),
                );
                return;
            }
        }
    }

    // Check 6: pdfaType fields must use prefix "pdfaType"
    let has_value_types = xmp.contains("pdfaType:type") || xmp.contains("<pdfaType:");
    if has_value_types && !xmp.contains("pdfaType:namespaceURI") {
        error(
            report,
            "6.7.8",
            "Extension schema value type missing required pdfaType:namespaceURI",
        );
    }
}

/// Find XML element text content values: yields (start, end) byte offsets for each
/// `<tag>value</tag>` occurrence.
fn find_xml_element_values<'a>(
    xml: &'a str,
    element: &'a str,
) -> impl Iterator<Item = (usize, usize)> + 'a {
    let open_tag = format!("<{element}>");
    let close_tag = format!("</{element}>");
    let open_len = open_tag.len();
    let mut pos = 0;
    std::iter::from_fn(move || {
        let rest = &xml[pos..];
        let open_off = rest.find(&open_tag)?;
        let val_start = pos + open_off + open_len;
        let rest2 = &xml[val_start..];
        let close_off = rest2.find(&close_tag)?;
        let val_end = val_start + close_off;
        pos = val_end + close_tag.len();
        Some((val_start, val_end))
    })
}

// ─── §6.2.5/6.2.9 — Image XObject rendering intent ─────────────────────────

/// Check /Intent on Image XObjects for valid rendering intent values.
///
/// This complements `check_rendering_intents` which checks content stream `ri`
/// operators and ExtGState /RI keys.
pub fn check_image_xobject_intent(pdf: &Pdf, report: &mut ComplianceReport) {
    let valid_intents: &[&[u8]] = &[
        b"RelativeColorimetric",
        b"AbsoluteColorimetric",
        b"Perceptual",
        b"Saturation",
    ];

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) else {
            continue;
        };
        let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) else {
            continue;
        };
        for (name, _) in xobj_dict.entries() {
            let Some(stream) = xobj_dict.get::<Stream<'_>>(name.as_ref()) else {
                continue;
            };
            let dict = stream.dict();
            if dict
                .get::<Name>(keys::SUBTYPE)
                .is_none_or(|s| s.as_ref() != keys::IMAGE)
            {
                continue;
            }

            if let Some(intent) = dict.get::<Name>(b"Intent" as &[u8]) {
                if !valid_intents.iter().any(|v| *v == intent.as_ref()) {
                    let intent_str = std::str::from_utf8(intent.as_ref()).unwrap_or("?");
                    // veraPDF uses §6.2.6 for ALL rendering intent violations: content
                    // stream ri, ExtGState /RI, and Image XObject /Intent alike.
                    // Use "6.2.6" which remaps to §6.2.9 for PDF/A-1.
                    error_at(
                        report,
                        "6.2.6",
                        format!("Image XObject has invalid rendering intent '{intent_str}'"),
                        format!("page {}", page_idx + 1),
                    );
                    return;
                }
            }
        }
    }
}

// ─── §6.1.4 — Cross-reference table syntax ──────────────────────────────────

/// Check xref keyword EOL markers (§6.1.4).
pub fn check_xref_syntax(pdf: &Pdf, report: &mut ComplianceReport) {
    let data = pdf.data().as_ref();
    let len = data.len();
    let mut pos = 0;

    while pos + 4 < len {
        if &data[pos..pos + 4] != b"xref" {
            pos += 1;
            continue;
        }
        // Skip "startxref"
        if pos >= 5 && data[pos - 5..pos] == *b"start" {
            pos += 4;
            continue;
        }
        // Found standalone "xref"
        let after = pos + 4;
        if after < len {
            let c = data[after];
            if c != b'\n' && c != b'\r' {
                error(
                    report,
                    "6.1.4",
                    "Keyword 'xref' not followed by proper EOL marker",
                );
                return;
            }
        }
        break;
    }
}

// ─── §6.9 — Embedded file specification keys ────────────────────────────────

/// Check embedded file specifications have required F and UF keys (§6.9).
/// Check if a dict key is present with a non-null value.
fn is_present_nonnull(dict: &Dict<'_>, key: &[u8]) -> bool {
    matches!(dict.get::<Object<'_>>(key), Some(obj) if !matches!(obj, Object::Null(_)))
}

pub fn check_embedded_file_spec_keys(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    if part < 3 {
        return;
    }

    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(names) = cat.get::<Dict<'_>>(keys::NAMES) else {
        return;
    };
    let Some(ef_tree) = names.get::<Dict<'_>>(keys::EMBEDDED_FILES) else {
        return;
    };

    if let Some(names_arr) = ef_tree.get::<Array<'_>>(keys::NAMES) {
        // Names array is [name1 spec1 name2 spec2 ...].
        // iter::<Dict>() stops on the first string, so collect as Object and
        // take every second item (the spec dict). Fixes #467.
        let items: Vec<Object<'_>> = names_arr.iter::<Object<'_>>().collect();
        for chunk in items.chunks(2) {
            let spec = match chunk.get(1) {
                Some(Object::Dict(d)) => d,
                _ => continue,
            };
            let has_ef = spec.contains_key(keys::EF);
            if !has_ef {
                continue;
            }
            let rule = if part == 4 { "6.9" } else { "6.8" };
            // F/UF must be present AND non-null (veraPDF t2: F=null counts as missing)
            if !is_present_nonnull(spec, keys::F) {
                error(report, rule, "File specification missing /F key");
            }
            if !is_present_nonnull(spec, b"UF" as &[u8]) {
                error(report, rule, "File specification missing /UF key");
            }
            if part >= 3 && !is_present_nonnull(spec, b"AFRelationship" as &[u8]) {
                error(
                    report,
                    rule,
                    "File specification missing /AFRelationship key",
                );
            }
            // Check embedded file stream has valid MIME type (§6.9 test 1)
            if let Some(ef_dict) = spec.get::<Dict<'_>>(keys::EF) {
                if let Some(f_stream) = ef_dict.get::<Stream<'_>>(keys::F) {
                    match f_stream.dict().get::<Name>(keys::SUBTYPE) {
                        None => {
                            error(
                                report,
                                rule,
                                "Embedded file stream missing /Subtype (MIME type)",
                            );
                        }
                        Some(mime_name) => {
                            // MIME type must contain "/" (e.g. "application/pdf")
                            let mime = std::str::from_utf8(mime_name.as_ref()).unwrap_or("");
                            if !mime.contains('/') {
                                error(
                                    report,
                                    rule,
                                    format!("Embedded file stream has invalid MIME type '{mime}' (missing subtype)"),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Check that embedded file specs are associated via /AF arrays (§6.8 test 4).
///
/// In PDF/A-3+, every file specification dictionary with /EF must be
/// referenced from an /AF array in the catalog, a page, or an annotation.
pub fn check_embedded_file_af_association(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    if part < 3 {
        return;
    }

    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(names) = cat.get::<Dict<'_>>(keys::NAMES) else {
        return;
    };
    let Some(ef_tree) = names.get::<Dict<'_>>(keys::EMBEDDED_FILES) else {
        return;
    };

    let rule = if part == 4 { "6.9" } else { "6.8" };

    // Walk the name tree and check each file spec with /EF
    if let Some(names_arr) = ef_tree.get::<Array<'_>>(keys::NAMES) {
        let items: Vec<Object<'_>> = names_arr.iter::<Object<'_>>().collect();
        for chunk in items.chunks(2) {
            if chunk.len() == 2 {
                if let Object::Dict(ref spec) = chunk[1] {
                    if !spec.contains_key(keys::EF) {
                        continue;
                    }
                    // The catalog must have an /AF array referencing this file spec
                    if cat.get::<Array<'_>>(b"AF" as &[u8]).is_none() {
                        error(
                            report,
                            rule,
                            "Embedded file specification not associated with document (no /AF array in catalog)",
                        );
                        return;
                    }
                }
            }
        }
    }
}

/// Check for non-embedded file specifications in PDF/A-2 (§6.9).
///
/// PDF/A-2 §6.9 requires all file references to be embedded (/EF present).
/// A /Type /Filespec dict without /EF is an external file reference, which
/// violates §6.9 for PDF/A-2. (#467)
pub fn check_filespec_without_ef(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    if part != 2 {
        return;
    }
    for obj in pdf.objects() {
        let dict = match &obj {
            Object::Dict(d) => d,
            _ => continue,
        };
        // Check /Type = /Filespec (PDF uses lowercase 's')
        let is_filespec = match dict.get::<Name>(keys::TYPE) {
            Some(t) => t.as_ref().eq_ignore_ascii_case(b"Filespec"),
            None => false,
        };
        if !is_filespec {
            continue;
        }
        // If the FileSpec has no /EF key it references an external file.
        // External file references are forbidden in PDF/A-2 (§6.9). (#467)
        if !dict.contains_key(keys::EF) {
            error(
                report,
                "6.9",
                "File specification has no /EF key (external file reference not allowed in PDF/A)",
            );
            return; // report once per document
        }
    }
}

/// Check that embedded file streams are accessible via /Names/EmbeddedFiles (§6.9/§6.8). (#467)
///
/// PDF/A-3+ requires that any embedded file (FileSpec with /EF) be registered
/// in the document catalog's /Names/EmbeddedFiles name tree. Files embedded
/// inside RichMedia or other annotations but absent from this tree violate
/// §6.9 (PDF/A-4) or §6.8 (PDF/A-3).
pub fn check_embedded_files_in_names_tree(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    if part < 3 {
        return;
    }

    // Check if any FileSpec with /EF exists in the document.
    let mut has_ef_filespec = false;
    for obj in pdf.objects() {
        let dict = match &obj {
            Object::Dict(d) => d,
            _ => continue,
        };
        let is_filespec = dict
            .get::<Name>(keys::TYPE)
            .is_some_and(|t| t.as_ref().eq_ignore_ascii_case(b"Filespec"));
        if is_filespec && dict.contains_key(keys::EF) {
            has_ef_filespec = true;
            break;
        }
    }
    if !has_ef_filespec {
        return;
    }

    // Check that /Names/EmbeddedFiles exists in the document catalog.
    let has_names_ef_tree = catalog(pdf)
        .and_then(|cat| cat.get::<Dict<'_>>(keys::NAMES))
        .and_then(|names| names.get::<Object<'_>>(keys::EMBEDDED_FILES))
        .is_some();

    if !has_names_ef_tree {
        let rule = if part >= 4 { "6.9" } else { "6.8" };
        error(
            report,
            rule,
            "Document has embedded file streams not registered in /Names/EmbeddedFiles",
        );
    }
}

// ─── §6.1.6 — Hex string validation ──────────────────────────────────────────

/// Scan `data` for invalid hex strings `<...>`.
///
/// Returns `Some((odd_count, is_invalid_char))` describing the first violation
/// found, or `None` if all hex strings in `data` are valid.
/// `skip_streams`: when true, bytes between `stream\n/\r` and `endstream` are
/// skipped (used for raw PDF bytes where streams may contain binary data).
fn scan_for_invalid_hex_string(data: &[u8], skip_streams: bool) -> Option<(bool, bool)> {
    let len = data.len();
    let mut pos = 0;
    let mut in_stream = false;
    // Track nesting depth of parenthesized strings ((...)) to avoid treating
    // '<' inside /RC or /Contents strings as hex string delimiters. (#FP-6.1.6)
    let mut paren_depth: i32 = 0;

    while pos < len {
        if skip_streams {
            // Track stream/endstream to skip binary content.
            // Accept optional spaces/tabs between 'stream' and EOL, matching PDF
            // parsers that allow `stream \n` (technically a §6.1.7 violation but
            // still needs to be treated as a stream body for hex-string scanning).
            if !in_stream && pos + 6 < len && &data[pos..pos + 6] == b"stream" {
                let mut skip = pos + 6;
                while skip < len && (data[skip] == b' ' || data[skip] == b'\t') {
                    skip += 1;
                }
                let eol = data.get(skip).copied().unwrap_or(0);
                if eol == b'\n' || eol == b'\r' {
                    in_stream = true;
                    pos = skip + 1;
                    continue;
                }
            }
            if in_stream {
                if pos + 9 < len && &data[pos..pos + 9] == b"endstream" {
                    in_stream = false;
                    pos += 9;
                } else {
                    pos += 1;
                }
                continue;
            }
        }
        // Track parenthesized string depth so '<' inside literal strings
        // like /RC (<?xml...><body...>) is not treated as a hex string start.
        // Backslash escapes inside strings (e.g. \() must be skipped too. (#FP-6.1.6)
        if !in_stream {
            if data[pos] == b'\\' && paren_depth > 0 {
                pos += 2; // skip escape + next char
                continue;
            }
            if data[pos] == b'(' {
                paren_depth += 1;
                pos += 1;
                continue;
            }
            if data[pos] == b')' {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
                pos += 1;
                continue;
            }
            if paren_depth > 0 {
                // Inside a literal string — skip everything, including '<'
                pos += 1;
                continue;
            }
        }
        if data[pos] != b'<' {
            pos += 1;
            continue;
        }
        // Skip dict markers <<
        if pos + 1 < len && data[pos + 1] == b'<' {
            pos += 2;
            continue;
        }
        // Found potential hex string start
        let start = pos + 1;
        let mut end = start;
        let mut hex_count = 0;
        let mut invalid_char = false;

        while end < len && data[end] != b'>' {
            let c = data[end];
            if c.is_ascii_hexdigit() {
                hex_count += 1;
            } else if c.is_ascii_whitespace() {
                // whitespace is allowed
            } else {
                invalid_char = true;
                break;
            }
            end += 1;
        }

        if end >= len {
            break;
        }

        // Only check if we actually found a closing >
        if data[end] == b'>' && !invalid_char && hex_count > 0 && hex_count % 2 != 0 {
            return Some((true, false));
        }
        if invalid_char && hex_count > 0 {
            return Some((false, true));
        }

        pos = end + 1;
    }
    None
}

/// Check hex strings for validity (§6.1.6 / §6.1.5).
///
/// Hex strings must contain only valid hex characters (0-9, a-f, A-F)
/// and whitespace. Also checks for odd-length hex strings.
/// PDF/A-4 renumbers this as §6.1.5; all other parts use §6.1.6.
pub fn check_hex_strings(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    // §6.1.5 in ISO 19005-4 (PDF/A-4), §6.1.6 in ISO 19005-1/2/3.
    let rule = if level.part() >= 4 { "6.1.5" } else { "6.1.6" };

    // Scan structural (non-stream) PDF bytes first.
    if let Some((odd, invalid)) = scan_for_invalid_hex_string(pdf.data().as_ref(), true) {
        if odd {
            error(
                report,
                rule,
                "Hexadecimal string contains odd number of non-whitespace characters",
            );
        } else if invalid {
            error(
                report,
                rule,
                "Hexadecimal string contains non-hex characters",
            );
        }
        return;
    }

    // Also scan decoded page content streams — hex strings used as text operands
    // (e.g. `<48455> Tj`) are subject to §6.1.6/§6.1.5 too. Content streams are
    // decoded here so we avoid FPs from binary/compressed stream data.
    for page in pdf.pages().iter() {
        let Some(content) = page.page_stream() else {
            continue;
        };
        if let Some((odd, invalid)) = scan_for_invalid_hex_string(content, false) {
            if odd {
                error(report, rule, "Hexadecimal string in content stream contains odd number of non-whitespace characters");
            } else if invalid {
                error(
                    report,
                    rule,
                    "Hexadecimal string in content stream contains non-hex characters",
                );
            }
            return;
        }
    }
}

// ─── §6.2.3 — Output intent restrictions ─────────────────────────────────────

/// Check OutputIntent has no forbidden DestOutputProfileRef key (§6.2.3 test 3).
pub fn check_output_intent_destref(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(intents) = cat.get::<Array<'_>>(keys::OUTPUT_INTENTS) else {
        return;
    };
    for intent in intents.iter::<Dict<'_>>() {
        if intent.contains_key(b"DestOutputProfileRef" as &[u8]) {
            error(
                report,
                "6.2.3",
                "OutputIntent contains forbidden /DestOutputProfileRef entry",
            );
            return;
        }
    }
}

// ─── §6.3.3.1 — CIDSystemInfo compatibility ──────────────────────────────────

/// Check CIDSystemInfo compatibility between CIDFont and CMap (§6.3.3.1 / §6.2.11.3.1).
///
/// For Type0 fonts, the Registry and Ordering entries in the CIDFont's
/// CIDSystemInfo must be compatible with those in the CMap dictionary.
pub fn check_cidsysteminfo_compat(pdf: &Pdf, report: &mut ComplianceReport) {
    for_each_font(pdf, |name, font_dict, page_idx| {
        let Some(subtype) = font_dict.get::<Name>(keys::SUBTYPE) else {
            return;
        };
        if subtype.as_ref() != b"Type0" {
            return;
        }

        // Get the CMap's CIDSystemInfo — stream dict or predefined CMap name.
        let (cmap_registry, cmap_ordering): (Option<String>, Option<String>) =
            if let Some(enc_stream) = font_dict.get::<Stream<'_>>(keys::ENCODING) {
                let d = enc_stream.dict();
                if let Some(csi) = d.get::<Dict<'_>>(keys::CIDSYSTEMINFO) {
                    let r = csi
                        .get::<pdf_syntax::object::String>(keys::REGISTRY)
                        .map(|v| String::from_utf8_lossy(v.as_bytes()).to_string());
                    let o = csi
                        .get::<pdf_syntax::object::String>(keys::ORDERING)
                        .map(|v| String::from_utf8_lossy(v.as_bytes()).to_string());
                    (r, o)
                } else {
                    (None, None)
                }
            } else if let Some(enc_name) = font_dict.get::<Name>(keys::ENCODING) {
                // Identity-H/V CMaps are exempt from Registry/Ordering compatibility
                // per ISO 32000-1 §9.10.3: "For CIDFont dictionaries with a CMap that
                // is not an Identity CMap, the Registry and Ordering values shall be
                // the same." Skip the comparison for Identity CMaps. (#FP-6.2.11.3.1)
                if enc_name.as_ref() == keys::IDENTITY_H || enc_name.as_ref() == keys::IDENTITY_V {
                    (None, None)
                } else {
                    // Predefined CMap: "Registry-Ordering-Supplement" e.g. "Adobe-Japan1-2"
                    let s = std::str::from_utf8(enc_name.as_ref()).unwrap_or("");
                    let parts: Vec<&str> = s.splitn(3, '-').collect();
                    if parts.len() >= 2 {
                        (Some(parts[0].to_string()), Some(parts[1].to_string()))
                    } else {
                        (None, None)
                    }
                }
            } else {
                (None, None)
            };

        // Get CIDFont's CIDSystemInfo
        let Some(descendants) = font_dict.get::<Array<'_>>(keys::DESCENDANT_FONTS) else {
            return;
        };
        for cid_font in descendants.iter::<Dict<'_>>() {
            let Some(cid_si) = cid_font.get::<Dict<'_>>(keys::CIDSYSTEMINFO) else {
                continue;
            };
            let font_ordering = cid_si
                .get::<pdf_syntax::object::String>(keys::ORDERING)
                .map(|o| String::from_utf8_lossy(o.as_bytes()).to_string());
            let font_registry = cid_si
                .get::<pdf_syntax::object::String>(keys::REGISTRY)
                .map(|r| String::from_utf8_lossy(r.as_bytes()).to_string());

            // If Encoding is a non-standard, non-embedded CMap name, the CIDSystemInfo
            // cannot be verified → flag as violation (veraPDF §6.2.10.3.1 t1).
            // Use "6.2.10.3.1" (internal rule); remap_clause_numbers maps it to the
            // correct clause per PDF/A part: §6.3.3.1 (part 1), §6.2.11.3.1 (parts 2/3),
            // §6.2.10.3.1 (part 4). (#FP-6.2.11.3.1)
            if let Some(enc_name) = font_dict.get::<Name>(keys::ENCODING) {
                if !is_standard_cmap(enc_name.as_ref())
                    && font_dict.get::<Stream<'_>>(keys::ENCODING).is_none()
                {
                    let enc_str = std::str::from_utf8(enc_name.as_ref()).unwrap_or("?");
                    error_at(
                        report,
                        "6.2.10.3.1",
                        format!("Font {name} uses non-embedded CMap '{enc_str}'; CIDSystemInfo cannot be verified"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }

            // When CIDFont has a stream CIDToGIDMap, it provides an explicit CID→GID
            // mapping that overrides the CIDSystemInfo-based matching. veraPDF does NOT
            // fire §6.3.3.1 / §6.2.11.3.1 / §6.2.10.3.1 for Registry/Ordering/Supplement
            // mismatches in this case. Skip to avoid FPs (e.g. pdfbox-3017.pdf).
            // Fixes #FP-6.3.3.1.
            if cid_font.get::<Stream<'_>>(keys::CID_TO_GID_MAP).is_some() {
                continue;
            }

            if let (Some(ref co), Some(ref fo)) = (&cmap_ordering, &font_ordering) {
                if co != fo {
                    error_at(
                        report,
                        "6.2.10.3.1",
                        format!("CIDSystemInfo Ordering mismatch: CMap='{co}', CIDFont='{fo}' in font {name}"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
            if let (Some(ref cr), Some(ref fr)) = (&cmap_registry, &font_registry) {
                if cr != fr {
                    error_at(
                        report,
                        "6.2.10.3.1",
                        format!("CIDSystemInfo Registry mismatch: CMap='{cr}', CIDFont='{fr}' in font {name}"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }

            // §6.3.3.1 / §6.2.11.3.1 / §6.2.10.3.1: CIDFont Supplement must be ≤ CMap
            // Supplement. This is a CIDSystemInfo compatibility issue, not CMap embedding.
            let cmap_supp = font_dict
                .get::<Stream<'_>>(keys::ENCODING)
                .and_then(|s| s.dict().get::<Dict<'_>>(keys::CIDSYSTEMINFO))
                .and_then(|csi| csi.get::<i32>(b"Supplement" as &[u8]));
            let font_supp = cid_si.get::<i32>(b"Supplement" as &[u8]);
            if let (Some(cs), Some(fs)) = (cmap_supp, font_supp) {
                if fs > cs {
                    error_at(
                        report,
                        "6.2.10.3.1",
                        format!(
                            "CIDFont Supplement ({fs}) > CMap Supplement ({cs}) for font {name}"
                        ),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    });
}

// ─── Combined content stream checks (shared page stream cache) ──────────────

/// Run all three page-content-stream checks in a single page loop, avoiding
/// redundant decompression of the same streams.  Replaces separate calls to
/// `check_undefined_operators`, `check_marked_content_sequences`, and
/// `check_inline_image_filters`.
pub fn check_page_content_streams_cached(pdf: &Pdf, pdfa_part: u8, report: &mut ComplianceReport) {
    // ── valid operators set — HashSet for O(1) lookup in scan_for_undefined_ops. (#perf) ──
    let valid_ops: std::collections::HashSet<&'static str> = [
        "w", "J", "j", "M", "d", "ri", "i", "gs", "q", "Q", "cm", "m", "l", "c", "v", "y", "h",
        "re", "S", "s", "f", "F", "f*", "B", "B*", "b", "b*", "n", "W", "W*", "BT", "ET", "Tc",
        "Tw", "Tz", "TL", "Tf", "Tr", "Ts", "Td", "TD", "Tm", "T*", "Tj", "TJ", "'", "\"", "d0",
        "d1", "CS", "cs", "SC", "SCN", "sc", "scn", "G", "g", "RG", "rg", "K", "k", "sh", "BI",
        "ID", "EI", "Do", "MP", "DP", "BMC", "BDC", "EMC", "BX", "EX",
    ]
    .iter()
    .copied()
    .collect();

    // PDF/A-1: §6.2.10 (undefined operators); PDF/A-2/3/4: §6.2.7.1 (operators)
    // veraPDF uses §6.2.10 for PDF/A-1, not §6.2.2 (which is OutputIntent related).
    let undef_op_rule = if pdfa_part >= 2 { "6.2.7.1" } else { "6.2.10" };

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let loc = format!("page {}", page_idx + 1);

        // Decompress page content stream ONCE
        if let Some(content) = page.page_stream() {
            if content.len() <= MAX_CONTENT_STREAM_SCAN_SIZE {
                // 1. Undefined operators
                if scan_for_undefined_ops(content, &valid_ops) {
                    error_at(
                        report,
                        undef_op_rule,
                        "Content stream contains undefined operator",
                        loc.clone(),
                    );
                }

                // 2. Marked content BMC/EMC nesting
                check_bmc_emc_nesting(content, page_idx, report);

                // 3. Inline image filters
                check_inline_images_in_content(content, page_idx, pdfa_part, report);

                // 4. §6.2.10.8 — BDC property list /ActualText with PUA codepoints
                if scan_bdc_actualtext_pua(content) {
                    error_at(
                        report,
                        "6.2.10.8",
                        "BDC marked content /ActualText contains PUA codepoint",
                        loc.clone(),
                    );
                }
            }
        }

        // Annotation appearance streams — only for undefined operators
        let page_dict = page.raw();
        if let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) {
            for annot in annots.iter::<Dict<'_>>() {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    if let Some(n_stream) = ap.get::<Stream<'_>>(keys::N) {
                        if let Ok(decoded) = n_stream.decoded() {
                            if scan_for_undefined_ops(&decoded, &valid_ops) {
                                error_at(
                                    report,
                                    undef_op_rule,
                                    "Annotation appearance stream contains undefined operator",
                                    loc.clone(),
                                );
                            }
                        }
                    }
                }
            }
        }

        // Form XObjects — undefined operators + BDC /ActualText PUA.
        let xobjects = &page.resources().x_objects;
        for (name, _) in xobjects.entries() {
            let Some(stream) = xobjects.get::<Stream<'_>>(name.as_ref()) else {
                continue;
            };
            let is_form = stream
                .dict()
                .get::<Name>(keys::SUBTYPE)
                .is_some_and(|s| s.as_ref() == b"Form");
            if !is_form {
                continue;
            }
            if let Ok(decoded) = stream.decoded() {
                if scan_for_undefined_ops(&decoded, &valid_ops) {
                    let xn = std::str::from_utf8(name.as_ref()).unwrap_or("?");
                    error_at(
                        report,
                        undef_op_rule,
                        format!("Form XObject {xn} contains undefined operator"),
                        loc.clone(),
                    );
                }
                // §6.2.10.8 — BDC /ActualText PUA in Form XObject content. (#FN-6.2.10.8)
                if scan_bdc_actualtext_pua(&decoded) {
                    error_at(
                        report,
                        "6.2.10.8",
                        "BDC marked content /ActualText in Form XObject contains PUA codepoint",
                        loc.clone(),
                    );
                }
            }
        }
    }
}

/// Check BMC/EMC nesting for a single page content stream.
fn check_bmc_emc_nesting(content: &[u8], page_idx: usize, report: &mut ComplianceReport) {
    let text = String::from_utf8_lossy(content);

    // Split on whitespace first, then further split each token on ">>" to handle
    // inline dict closings like `0>>BDC` where the dict closer is glued to the operator.
    // Without this, `split_ascii_whitespace()` yields "0>>BDC" as a single token and
    // the "BDC" operator is not recognised, causing depth miscounts. (#FP-6.8.3.4)
    let raw_tokens: Vec<&str> = text.split_ascii_whitespace().collect();
    let tokens: Vec<&str> = raw_tokens.iter().flat_map(|t| t.split(">>")).collect();

    let mut depth: i32 = 0;
    for tok in &tokens {
        // After splitting on ">>" a hex-string close followed by a dict close produces
        // ">>>BDC" → ["<hexdata>", ">BDC"]. The residual leading '>' is from the odd
        // number of consecutive '>' characters. Strip it before matching operators.
        // PDF operators never start with '>'. (#FP-6.8.3.4)
        let tok = tok.trim_start_matches('>');
        match tok {
            "BMC" | "BDC" => depth += 1,
            "EMC" => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            error_at(
                report,
                "6.8.3.4",
                "EMC without matching BMC/BDC",
                format!("page {}", page_idx + 1),
            );
            return;
        }
    }
    if depth > 0 {
        error_at(
            report,
            "6.8.3.4",
            format!("{depth} unclosed marked content sequence(s) (BMC/BDC without EMC)"),
            format!("page {}", page_idx + 1),
        );
    }
}

/// §6.2.10.8 — scan a page content stream for BDC property-list /ActualText values
/// that contain Unicode PUA codepoints.
///
/// Pattern: `/Name << /ActualText <hexstring> >> BDC`
/// The hex string is typically UTF-16BE (starts with FEFF BOM).
fn scan_bdc_actualtext_pua(content: &[u8]) -> bool {
    let needle = b"/ActualText";
    let mut i = 0;
    while i + needle.len() <= content.len() {
        if !content[i..].starts_with(needle) {
            i += 1;
            continue;
        }
        let mut j = i + needle.len();
        // Skip whitespace after /ActualText
        while j < content.len() && content[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= content.len() {
            break;
        }
        // Hex string: <hexdigits> (not <<)
        if content[j] == b'<' && content.get(j + 1).copied() != Some(b'<') {
            let start = j + 1;
            if let Some(rel) = content[start..].iter().position(|&b| b == b'>') {
                let hex = &content[start..start + rel];
                if contains_pua_in_utf16be_hex(hex) {
                    return true;
                }
                i = start + rel + 1;
                continue;
            }
        }
        i = j + 1;
    }
    false
}

/// Decode a sequence of hex-digit bytes (e.g. from a PDF `<hexstring>`) into raw bytes.
/// Whitespace inside the hex data is ignored (valid per PDF spec).
fn decode_hex_to_bytes(hex: &[u8]) -> Vec<u8> {
    let nibbles: Vec<u8> = hex
        .iter()
        .filter(|&&b| !b.is_ascii_whitespace())
        .cloned()
        .collect();
    let mut out = Vec::with_capacity(nibbles.len() / 2);
    let mut k = 0;
    while k + 1 < nibbles.len() {
        let hi = (nibbles[k] as char).to_digit(16);
        let lo = (nibbles[k + 1] as char).to_digit(16);
        if let (Some(h), Some(l)) = (hi, lo) {
            out.push((h * 16 + l) as u8);
        }
        k += 2;
    }
    out
}

/// Scan a content stream for `/Lang` values in BDC inline property dicts and return
/// the decoded language tags found (UTF-8 or UTF-16BE strings).
///
/// Pattern: `/Lang <hexstring>` or `/Lang (literal)` inside `<< … >> BDC`.
/// Used to fix §6.7.4 / §6.8.4 FNs where /Lang is in a content-stream BDC property
/// dict instead of in the catalog or structure tree. (#FN-6.8.4 / #FN-6.7.4)
fn scan_bdc_lang_values(content: &[u8]) -> Vec<String> {
    let needle = b"/Lang";
    let mut result = Vec::new();
    let mut i = 0;
    while i + needle.len() <= content.len() {
        if !content[i..].starts_with(needle) {
            i += 1;
            continue;
        }
        // Verify it's a complete PDF name token — not /Language or similar.
        // PDF name ends at whitespace or any of the delimiter characters.
        let after = i + needle.len();
        let is_name_end = after >= content.len() || {
            let b = content[after];
            b.is_ascii_whitespace()
                || matches!(
                    b,
                    b'<' | b'>' | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
                )
        };
        if !is_name_end {
            i += 1;
            continue;
        }
        // Skip whitespace after the name
        let mut j = after;
        while j < content.len() && content[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= content.len() {
            break;
        }
        if content[j] == b'<' && content.get(j + 1).copied() != Some(b'<') {
            // Hex string: <hexdigits>
            let start = j + 1;
            if let Some(rel) = content[start..].iter().position(|&b| b == b'>') {
                let hex = &content[start..start + rel];
                let bytes = decode_hex_to_bytes(hex);
                if let Some(tag) = decode_pdf_string(&bytes) {
                    result.push(tag);
                }
                i = start + rel + 1;
            } else {
                i = j + 1;
            }
        } else if content[j] == b'(' {
            // Literal string — scan for balanced closing paren, honouring backslash escapes.
            let start = j + 1;
            let mut depth = 1usize;
            let mut k = start;
            while k < content.len() {
                match content[k] {
                    b'\\' => k += 2, // skip escaped character
                    b'(' => {
                        depth += 1;
                        k += 1;
                    }
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        k += 1;
                    }
                    _ => k += 1,
                }
            }
            let bytes = content[start..k].to_vec();
            if let Some(tag) = decode_pdf_string(&bytes) {
                result.push(tag);
            }
            i = k + 1;
        } else {
            i = j + 1;
        }
    }
    result
}

/// Decode a PDF hex string as UTF-16BE and return true if any codepoint is in the PUA.
fn contains_pua_in_utf16be_hex(hex: &[u8]) -> bool {
    // Collect hex nibbles (skip whitespace)
    let nibbles: Vec<u8> = hex
        .iter()
        .filter(|&&b| !b.is_ascii_whitespace())
        .cloned()
        .collect();
    if nibbles.len() < 4 {
        return false;
    }
    // Decode pairs of nibbles into bytes
    let mut bytes = Vec::with_capacity(nibbles.len() / 2);
    let mut k = 0;
    while k + 1 < nibbles.len() {
        let hi = (nibbles[k] as char).to_digit(16);
        let lo = (nibbles[k + 1] as char).to_digit(16);
        if let (Some(h), Some(l)) = (hi, lo) {
            bytes.push((h * 16 + l) as u8);
        }
        k += 2;
    }
    // Require UTF-16BE BOM (0xFE 0xFF)
    if bytes.len() < 2 || bytes[0] != 0xFE || bytes[1] != 0xFF {
        return false;
    }
    let mut i = 2; // skip BOM
    while i + 1 < bytes.len() {
        let hi = bytes[i] as u32;
        let lo = bytes[i + 1] as u32;
        let cp = (hi << 8) | lo;
        // BMP PUA: U+E000–U+F8FF
        if (0xE000..=0xF8FF).contains(&cp) {
            return true;
        }
        // Surrogate pair: high surrogate D800-DBFF + low DC00-DFFF → supplementary PUA
        if (0xD800..=0xDBFF).contains(&cp) && i + 3 < bytes.len() {
            let lo2 = (bytes[i + 2] as u32) << 8 | bytes[i + 3] as u32;
            if (0xDC00..=0xDFFF).contains(&lo2) {
                let full = 0x10000 + ((cp - 0xD800) << 10) + (lo2 - 0xDC00);
                if full >= 0xF0000 {
                    return true;
                }
                i += 2; // consumed surrogate pair extra word
            }
        }
        i += 2;
    }
    false
}

/// Check inline image filters for a single page content stream.
fn check_inline_images_in_content(
    content: &[u8],
    page_idx: usize,
    pdfa_part: u8,
    report: &mut ComplianceReport,
) {
    let text = String::from_utf8_lossy(content);
    let loc = format!("page {}", page_idx + 1);
    let mut pos = 0;
    let mut inline_count = 0usize;
    while let Some(bi_pos) = text[pos..].find("BI") {
        let abs_bi = pos + bi_pos;
        let before_ok = abs_bi == 0 || text.as_bytes()[abs_bi - 1].is_ascii_whitespace();
        let after_ok = abs_bi + 2 >= text.len()
            || text.as_bytes()[abs_bi + 2].is_ascii_whitespace()
            || text.as_bytes()[abs_bi + 2] == b'/';
        if !before_ok || !after_ok {
            pos = abs_bi + 2;
            continue;
        }
        inline_count += 1;
        if inline_count > 200 {
            break;
        }
        let search_region = &text[abs_bi..];
        let id_pos = search_region
            .find(" ID")
            .or_else(|| search_region.find("\nID"))
            .or_else(|| search_region.find("\rID"))
            .or_else(|| search_region.find("\tID"));
        let Some(id_pos) = id_pos else {
            pos = abs_bi + 2;
            continue;
        };
        let header = &text[abs_bi..abs_bi + id_pos];
        let rule = if pdfa_part == 1 { "6.1.10" } else { "6.1.9" };
        let header_lower = header.to_ascii_lowercase();
        let has_lzw = header_lower.contains("/f /lzw")
            || header_lower.contains("/f/lzw")
            || header_lower.contains("/filter /lzw")
            || header_lower.contains("/filter/lzw")
            || header_lower.contains("/f[/lzw")
            || header_lower.contains("/filter[/lzw")
            || header_lower.contains("/f [/lzw")
            || header_lower.contains("/filter [/lzw");
        let has_crypt = header_lower.contains("/f /cr")
            || header_lower.contains("/f/cr")
            || header_lower.contains("/filter /cr")
            || header_lower.contains("/filter/cr")
            || header_lower.contains("/f[/cr")
            || header_lower.contains("/filter[/cr");
        if has_lzw {
            error_at(
                report,
                rule,
                "Inline image uses forbidden LZWDecode filter",
                loc.clone(),
            );
        }
        if has_crypt {
            error_at(
                report,
                rule,
                "Inline image uses forbidden Crypt filter",
                loc.clone(),
            );
        }
        pos = abs_bi + id_pos;
    }
}

// ─── §6.2.10.4.1 — TrueType simple-font cmap requirements (PDF/A-4) ─────────

/// Check that TrueType simple fonts (non-CID) do not have invalid Mac Roman
/// cmap entries in their embedded font programs (§6.2.10.4.1 PDF/A-4).
///
/// ISO 19005-4 §6.2.10.4.1 requires that if a Platform 1 (Mac Roman) cmap
/// subtable is present, all non-zero entries must use character codes defined
/// in the Mac Roman encoding.  Codes 0x80-0x9F are undefined in Mac Roman
/// (they are defined in Windows-1252 but not in Mac OS Roman), so a non-zero
/// glyph mapping at those positions is a violation.
///
/// We emit rule `"6.2.10.4.1-tt"` which `remap_clause_numbers` in pdfa.rs
/// translates to `"6.2.10.4.1"` for PDF/A-4. (#467)
pub fn check_truetype_cmap_pdfa4(pdf: &Pdf, report: &mut ComplianceReport) {
    let xref = pdf.xref();
    for_each_font(pdf, |name, font_dict, page_idx| {
        // Only simple (non-CID, non-Type0) TrueType fonts
        let subtype = font_dict.get::<Name>(keys::SUBTYPE);
        let is_truetype = matches!(
            subtype.as_ref().map(|s| s.as_ref()),
            Some(b"TrueType") | Some(b"Type1")
        );
        if !is_truetype {
            return;
        }

        // FontDescriptor is almost always an indirect ref. (#FN-6.2.10.4.1)
        let Some(desc) = font_dict.get::<Dict<'_>>(keys::FONT_DESC).or_else(|| {
            font_dict
                .get_ref(keys::FONT_DESC)
                .and_then(|r| xref.get::<Dict<'_>>(r.into()))
        }) else {
            return;
        };
        // FontFile2 may also be an indirect ref.
        let Some(ff2) = desc.get::<Stream<'_>>(keys::FONT_FILE2).or_else(|| {
            desc.get_ref(keys::FONT_FILE2)
                .and_then(|r| xref.get::<Stream<'_>>(r.into()))
        }) else {
            return;
        };
        let Ok(font_data) = ff2.decoded() else {
            return;
        };
        if font_data.len() < 12 {
            return;
        }

        // Parse TrueType offset table and find cmap
        let num_tables = u16::from_be_bytes([font_data[4], font_data[5]]) as usize;
        let mut cmap_offset: Option<usize> = None;
        for i in 0..num_tables {
            let t = 12 + i * 16;
            if t + 16 > font_data.len() {
                break;
            }
            if &font_data[t..t + 4] == b"cmap" {
                let off = u32::from_be_bytes([
                    font_data[t + 8],
                    font_data[t + 9],
                    font_data[t + 10],
                    font_data[t + 11],
                ]) as usize;
                cmap_offset = Some(off);
                break;
            }
        }

        let Some(cmap_off) = cmap_offset else {
            return;
        };
        if cmap_off + 4 > font_data.len() {
            return;
        }

        let num_subtables =
            u16::from_be_bytes([font_data[cmap_off + 2], font_data[cmap_off + 3]]) as usize;

        for j in 0..num_subtables {
            let st = cmap_off + 4 + j * 8;
            if st + 8 > font_data.len() {
                break;
            }
            let platform_id = u16::from_be_bytes([font_data[st], font_data[st + 1]]);
            let encoding_id = u16::from_be_bytes([font_data[st + 2], font_data[st + 3]]);
            let sub_off = u32::from_be_bytes([
                font_data[st + 4],
                font_data[st + 5],
                font_data[st + 6],
                font_data[st + 7],
            ]) as usize;

            // Only check Platform 1 (Mac), Encoding 0 (Mac Roman)
            if platform_id != 1 || encoding_id != 0 {
                continue;
            }

            let abs_off = cmap_off + sub_off;
            if abs_off + 2 > font_data.len() {
                continue;
            }
            let fmt = u16::from_be_bytes([font_data[abs_off], font_data[abs_off + 1]]);

            // Format 4 (segmented): iterate segments
            if fmt == 4 {
                if abs_off + 14 > font_data.len() {
                    continue;
                }
                let seg_count = u16::from_be_bytes([font_data[abs_off + 6], font_data[abs_off + 7]])
                    as usize
                    / 2;
                let end_codes_off = abs_off + 14;
                let start_codes_off = end_codes_off + seg_count * 2 + 2; // skip reservedPad
                if start_codes_off + seg_count * 2 > font_data.len() {
                    continue;
                }
                for s in 0..seg_count {
                    let end_code = u16::from_be_bytes([
                        font_data[end_codes_off + s * 2],
                        font_data[end_codes_off + s * 2 + 1],
                    ]);
                    let start_code = u16::from_be_bytes([
                        font_data[start_codes_off + s * 2],
                        font_data[start_codes_off + s * 2 + 1],
                    ]);
                    // Check if this segment covers 0x80-0x9F
                    if start_code <= 0x9F && end_code >= 0x80 {
                        error_at(
                            report,
                            "6.2.10.4.1-tt",
                            format!(
                                "TrueType font {name} Platform 1 cmap covers codes 0x{:02X}-0x{:02X} \
                                 which are undefined in Mac Roman encoding",
                                start_code.max(0x80),
                                end_code.min(0x9F)
                            ),
                            format!("page {}", page_idx + 1),
                        );
                        return;
                    }
                }
            }

            // Format 6 (trimmed table): first_code + entry_count glyph IDs
            if fmt == 6 {
                if abs_off + 10 > font_data.len() {
                    continue;
                }
                let first_code =
                    u16::from_be_bytes([font_data[abs_off + 6], font_data[abs_off + 7]]) as usize;
                let entry_count =
                    u16::from_be_bytes([font_data[abs_off + 8], font_data[abs_off + 9]]) as usize;
                // Check if any code in 0x80-0x9F has a non-zero glyph ID
                for k in 0..entry_count {
                    let code = first_code + k;
                    if !(0x80..=0x9F).contains(&code) {
                        continue;
                    }
                    let glyph_off = abs_off + 10 + k * 2;
                    if glyph_off + 2 > font_data.len() {
                        break;
                    }
                    let gid = u16::from_be_bytes([font_data[glyph_off], font_data[glyph_off + 1]]);
                    if gid != 0 {
                        error_at(
                            report,
                            "6.2.10.4.1-tt",
                            format!(
                                "TrueType font {name} Platform 1 cmap: code 0x{code:02X} maps to \
                                 glyph {gid} but 0x{code:02X} is undefined in Mac Roman encoding"
                            ),
                            format!("page {}", page_idx + 1),
                        );
                        return; // One violation per font is enough
                    }
                }
            }
        }
    });
}

// ─── Type 1 raw font program (FontFile) width checker ────────────────────────
//
// These helpers are duplicated from pdf-manip/src/pdfa_fonts.rs because
// pdf-compliance cannot depend on pdf-manip (circular dependency). (#467)
// They are intentionally private and minimal — only what check_type1_simple_widths
// needs for §6.3.6 / §6.3.5-fw width consistency checks.

/// §6.3.5-fw — Check raw Type1 font (FontFile) /Widths against charstring widths.
///
/// Parses the PostScript Type1 program embedded via /FontFile, extracts the
/// per-glyph advance widths from the eexec-encrypted charstrings, and compares
/// them against the /Widths array declared in the PDF font dictionary.
/// Emits rule "6.3.5-fw" which pdfa.rs remaps to §6.3.6 for PDF/A-1. (#467)
///
/// Handles /Encoding given as an indirect reference (e.g. 24 0 R) by resolving
/// it via xref to obtain the actual Encoding dict and its /BaseEncoding + /Differences.
/// This fixes FNs where veraPDF fires §6.3.6 but we missed it because we couldn't
/// determine the glyph name for a code whose PDF encoding was specified indirectly.
#[allow(clippy::too_many_arguments)]
fn check_type1_simple_widths(
    font_data: &[u8],
    font_dict: &Dict<'_>,
    xref: &pdf_syntax::xref::XRef,
    name: &str,
    first_char: i32,
    last_char: i32,
    pdf_widths: &[i32],
    missing_width: Option<i32>,
    page_idx: usize,
    report: &mut ComplianceReport,
) {
    let Some(parsed) = t1_parse_program(font_data) else {
        return;
    };

    let first = first_char as usize;
    let last = last_char as usize;
    if last < first || pdf_widths.len() < last - first + 1 {
        return;
    }

    // Resolve /Encoding: may be a Name (direct) or an indirect ref to an Encoding dict.
    // Direct name case (e.g. /WinAnsiEncoding):
    let direct_enc_name: Option<Vec<u8>> = font_dict
        .get::<Name>(keys::ENCODING)
        .map(|n| n.as_ref().to_vec());

    // Encoding dict case: try direct dict, then resolve indirect ref via xref.
    // Fixes FNs where /Encoding = 24 0 R and we couldn't determine the glyph name.
    let enc_dict_opt: Option<Dict<'_>> = font_dict.get::<Dict<'_>>(keys::ENCODING).or_else(|| {
        font_dict
            .get_ref(keys::ENCODING)
            .and_then(|r| xref.get::<Dict<'_>>(r.into()))
    });

    // BaseEncoding from the encoding dict (overrides direct name if dict is present).
    let base_enc_name: Option<Vec<u8>> = enc_dict_opt
        .as_ref()
        .and_then(|d| d.get::<Name>(keys::BASE_ENCODING))
        .map(|n| n.as_ref().to_vec())
        .or_else(|| direct_enc_name.clone());

    // /Differences from the encoding dict: maps specific codes to glyph names.
    // These take highest priority and override both BaseEncoding and the font's
    // internal encoding for the codes they cover. Fixes §6.3.6 FN (cs-isartor-fail-c).
    let mut differences: std::collections::HashMap<u8, String> = std::collections::HashMap::new();
    if let Some(enc_dict) = enc_dict_opt.as_ref() {
        if let Some(diffs) = enc_dict.get::<Array<'_>>(b"Differences" as &[u8]) {
            let mut current_code = 0u8;
            for item in diffs.iter::<Object<'_>>() {
                match item {
                    Object::Number(n) => current_code = n.as_i64() as u8,
                    Object::Name(n) => {
                        if let Ok(s) = std::str::from_utf8(n.as_ref()) {
                            differences.insert(current_code, s.to_string());
                        }
                        current_code = current_code.saturating_add(1);
                    }
                    _ => {}
                }
            }
        }
    }

    let loc = format!("page {}", page_idx + 1);

    for code in first..=last {
        let idx = code - first;
        let pdf_w = pdf_widths[idx];

        // Look up glyph name by priority:
        // 1. /Differences entry for this code (highest priority — PDF spec §9.6.6.1)
        // 2. Internal Type1 encoding (dup…put entries in the font program) — only when
        //    the PDF explicitly declares /Encoding. Symbol fonts (CMSY8 etc.) have
        //    non-AGL built-in encodings; using them for width checking causes FP §6.3.6.
        //    (#FP-6.3.6)
        // 3. BaseEncoding (WinAnsiEncoding / StandardEncoding) from the PDF dict
        // 4. StandardEncoding as final fallback (Type1 default)
        let has_explicit_encoding = direct_enc_name.is_some() || enc_dict_opt.is_some();
        let glyph_name: Option<String> = differences
            .get(&(code as u8))
            .cloned()
            .or_else(|| {
                if has_explicit_encoding {
                    parsed.encoding.get(&(code as u8)).cloned()
                } else {
                    None
                }
            })
            .or_else(|| {
                let enc_name = base_enc_name.as_deref().unwrap_or(b"");
                if enc_name.is_empty() || enc_name == b"StandardEncoding" {
                    t1_standard_encoding_name(code as u8).map(str::to_string)
                } else if enc_name == b"WinAnsiEncoding" {
                    t1_winansi_glyph_name(code as u8).map(str::to_string)
                } else {
                    None
                }
            });

        let Some(glyph_name) = glyph_name else {
            continue;
        };
        if glyph_name.is_empty() || glyph_name == ".notdef" {
            continue;
        }

        if let Some(&cs_width) = parsed.charstring_widths.get(glyph_name.as_str()) {
            // Glyph IS in the font program: compare charstring advance with /Widths.
            // Skip pdf_w==0 — a zero /Widths entry means this code is unused/absent
            // in this font subset. veraPDF does not fire §6.3.6/§6.2.11.5 when the
            // PDF declares width=0 for a glyph that has a non-zero charstring width.
            // (#FP-6.2.11.5-zero, empirically confirmed: isartor tests use non-zero)
            if pdf_w == 0 {
                continue;
            }
            let font_w = (cs_width as f64 * parsed.font_matrix_sx * 1000.0).round() as i32;
            if (font_w - pdf_w).abs() > 1 {
                error_at(
                    report,
                    "6.3.5-fw",
                    format!(
                        "Font {name} code {code} ({glyph_name}): \
                         Type1 charstring width {font_w} != PDF /Widths[{idx}] {pdf_w}"
                    ),
                    loc.clone(),
                );
                return; // First mismatch per font only
            }
        } else if pdf_w != 0 {
            // Glyph is absent from the font program and /Widths claims non-zero advance.
            // The effective advance is MissingWidth; if that disagrees, the entry is wrong.
            // Do NOT check pdf_w==0 cases here: width=0 for absent glyphs is common/valid.
            if let Some(mw) = missing_width {
                if (mw - pdf_w).abs() > 1 {
                    error_at(
                        report,
                        "6.3.5-fw",
                        format!(
                            "Font {name} code {code} ({glyph_name}): \
                             glyph absent from font program, MissingWidth {mw} \
                             != PDF /Widths[{idx}] {pdf_w}"
                        ),
                        loc.clone(),
                    );
                    return; // First mismatch per font only
                }
            }
        }
    }
}

/// Parsed data extracted from a Type 1 font program.
struct T1Parsed {
    font_matrix_sx: f64,
    encoding: std::collections::HashMap<u8, String>,
    charstring_widths: std::collections::HashMap<String, i32>,
}

/// Parse a Type 1 font program (PFB/PFA) to extract FontMatrix, Encoding, and
/// per-glyph advance widths from the eexec-encrypted charstrings.
fn t1_parse_program(data: &[u8]) -> Option<T1Parsed> {
    let (cleartext, eexec_data) = t1_split_sections(data)?;
    let font_matrix_sx = t1_parse_font_matrix(cleartext).unwrap_or(0.001);
    let mut encoding = t1_parse_encoding(cleartext);
    let decrypted = t1_eexec_decrypt(eexec_data);
    let len_iv_cleartext = t1_parse_len_iv(cleartext);
    let len_iv_bytes = t1_parse_len_iv_bytes(&decrypted);
    let len_iv = len_iv_cleartext.or(len_iv_bytes).unwrap_or(4) as usize;
    encoding.extend(t1_parse_encoding_bytes(&decrypted));
    let seac_subrs = t1_parse_seac_subrs(&decrypted, len_iv);
    let charstring_widths = t1_parse_charstrings(&decrypted, len_iv, &seac_subrs);
    Some(T1Parsed {
        font_matrix_sx,
        encoding,
        charstring_widths,
    })
}

/// Split a Type 1 font into (cleartext, eexec-data) slices.
fn t1_split_sections(data: &[u8]) -> Option<(&[u8], &[u8])> {
    if data.first() == Some(&0x80) {
        return t1_split_pfb(data);
    }
    // PFA: find "eexec" keyword.
    let eexec_pos = t1_find_bytes(data, b"eexec")?;
    let cleartext = &data[..eexec_pos];
    let mut pos = eexec_pos + 5;
    while pos < data.len() && matches!(data[pos], b' ' | b'\r' | b'\n' | b'\t') {
        pos += 1;
    }
    if pos >= data.len() {
        return None;
    }
    Some((cleartext, &data[pos..]))
}

/// Split a PFB (binary) Type1 font into (cleartext, eexec-data) slices.
fn t1_split_pfb(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut pos = 0;
    let mut cleartext_end = 0;
    while pos + 6 <= data.len() {
        if data[pos] != 0x80 {
            break;
        }
        let seg_type = data[pos + 1];
        let seg_len =
            u32::from_le_bytes([data[pos + 2], data[pos + 3], data[pos + 4], data[pos + 5]])
                as usize;
        let seg_data_start = pos + 6;
        match seg_type {
            1 => {
                cleartext_end = seg_data_start + seg_len;
            }
            2 => {
                let eexec_end = seg_data_start + seg_len;
                return Some((&data[6..cleartext_end], &data[seg_data_start..eexec_end]));
            }
            3 => break,
            _ => break,
        }
        pos = seg_data_start + seg_len;
    }
    // Fallback: keyword search.
    let eexec_pos = t1_find_bytes(&data[6..], b"eexec")?;
    let cleartext = &data[6..6 + eexec_pos];
    let mut skip = 6 + eexec_pos + 5;
    while skip < data.len() && matches!(data[skip], b' ' | b'\r' | b'\n' | b'\t') {
        skip += 1;
    }
    Some((cleartext, &data[skip..]))
}

fn t1_find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Decrypt eexec-encrypted data (R=55665, c1=52845, c2=22719).
fn t1_eexec_decrypt(data: &[u8]) -> Vec<u8> {
    let is_hex = data
        .iter()
        .take(20)
        .all(|b| b.is_ascii_hexdigit() || matches!(b, b'\r' | b'\n' | b' '));
    let binary_data: Vec<u8>;
    let input: &[u8] = if is_hex {
        let hex_chars: Vec<u8> = data
            .iter()
            .copied()
            .filter(|b| b.is_ascii_hexdigit())
            .collect();
        binary_data = hex_chars
            .chunks(2)
            .filter_map(|pair| {
                if pair.len() == 2 {
                    Some((t1_hex_val(pair[0]) << 4) | t1_hex_val(pair[1]))
                } else {
                    None
                }
            })
            .collect();
        &binary_data
    } else {
        data
    };
    let mut r: u16 = 55665;
    let c1: u16 = 52845;
    let c2: u16 = 22719;
    let mut result = Vec::with_capacity(input.len());
    for &cipher in input {
        let plain = cipher ^ (r >> 8) as u8;
        r = (cipher as u16)
            .wrapping_add(r)
            .wrapping_mul(c1)
            .wrapping_add(c2);
        result.push(plain);
    }
    if result.len() > 4 {
        result.drain(..4);
    }
    result
}

fn t1_hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'A'..=b'F' => b - b'A' + 10,
        b'a'..=b'f' => b - b'a' + 10,
        _ => 0,
    }
}

/// Parse FontMatrix sx (index 0) from Type1 cleartext.
fn t1_parse_font_matrix(cleartext: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(cleartext).ok()?;
    let fm_pos = text.find("/FontMatrix")?;
    let after = &text[fm_pos..];
    let start = after.find('[')? + 1;
    let end = after.find(']')?;
    let values: Vec<f64> = after[start..end]
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    values.into_iter().next()
}

/// Parse Encoding `dup CODE /name put` entries from Type1 cleartext.
fn t1_parse_encoding(cleartext: &[u8]) -> std::collections::HashMap<u8, String> {
    let mut enc = std::collections::HashMap::new();
    let Ok(text) = std::str::from_utf8(cleartext) else {
        return enc;
    };
    for line in text.lines() {
        let t = line.trim();
        if !t.starts_with("dup ") || !t.ends_with(" put") {
            continue;
        }
        let parts: Vec<&str> = t.split_whitespace().collect();
        if parts.len() >= 4 && parts[0] == "dup" && parts[3] == "put" {
            if let Ok(code) = parts[1].parse::<u8>() {
                if let Some(gname) = parts[2].strip_prefix('/') {
                    if gname != ".notdef" {
                        enc.insert(code, gname.to_string());
                    }
                }
            }
        }
    }
    enc
}

/// Parse Encoding from decrypted eexec bytes (before /CharStrings).
fn t1_parse_encoding_bytes(data: &[u8]) -> std::collections::HashMap<u8, String> {
    let end = t1_find_bytes(data, b"/CharStrings").unwrap_or(data.len());
    t1_parse_encoding(&data[..end])
}

/// Parse lenIV from cleartext.
fn t1_parse_len_iv(cleartext: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(cleartext).ok()?;
    let pos = text.find("/lenIV")?;
    text[pos + 6..].split_whitespace().next()?.parse().ok()
}

/// Parse lenIV from raw bytes (searches before /CharStrings).
fn t1_parse_len_iv_bytes(data: &[u8]) -> Option<u32> {
    let search_end = t1_find_bytes(data, b"/CharStrings").unwrap_or(data.len());
    let search_data = &data[..search_end];
    let pos = t1_find_bytes(search_data, b"/lenIV")?;
    let after = &search_data[pos + 6..];
    let start = after.iter().position(|b| !b.is_ascii_whitespace())?;
    let end = after[start..]
        .iter()
        .position(|b| b.is_ascii_whitespace() || *b == b'/')
        .unwrap_or(after.len() - start);
    std::str::from_utf8(&after[start..start + end])
        .ok()?
        .parse()
        .ok()
}

/// Return the set of Subrs indices that contain a seac instruction.
fn t1_parse_seac_subrs(decrypted: &[u8], len_iv: usize) -> std::collections::HashSet<u32> {
    let mut seac_subrs = std::collections::HashSet::new();
    let Some(subrs_pos) = t1_find_bytes(decrypted, b"/Subrs") else {
        return seac_subrs;
    };
    let data = &decrypted[subrs_pos + 6..];
    let mut pos = 0;
    while pos < data.len() {
        let Some(dup_off) = t1_find_bytes(&data[pos..], b"dup") else {
            break;
        };
        pos += dup_off + 3;
        while pos < data.len() && data[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let idx_start = pos;
        while pos < data.len() && data[pos].is_ascii_digit() {
            pos += 1;
        }
        let Ok(subr_idx) = std::str::from_utf8(&data[idx_start..pos])
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .ok_or(())
        else {
            continue;
        };
        while pos < data.len() && data[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let len_start = pos;
        while pos < data.len() && data[pos].is_ascii_digit() {
            pos += 1;
        }
        let Ok(cs_len) = std::str::from_utf8(&data[len_start..pos])
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or(())
        else {
            continue;
        };
        while pos < data.len() && data[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos + 2 > data.len() {
            break;
        }
        let marker = &data[pos..pos + 2];
        if marker != b"RD" && marker != b"-|" {
            continue;
        }
        pos += 2;
        if pos < data.len() && matches!(data[pos], b' ' | b'\t') {
            pos += 1;
        }
        if pos + cs_len > data.len() {
            break;
        }
        if t1_charstring_contains_seac(&data[pos..pos + cs_len], len_iv) {
            seac_subrs.insert(subr_idx);
        }
        pos += cs_len;
    }
    seac_subrs
}

/// Decrypt a charstring and check for inline seac (12 6).
fn t1_charstring_contains_seac(data: &[u8], len_iv: usize) -> bool {
    if data.len() <= len_iv {
        return false;
    }
    let mut r: u16 = 4330;
    let c1: u16 = 52845;
    let c2: u16 = 22719;
    let mut dec = Vec::with_capacity(data.len());
    for &cipher in data {
        let plain = cipher ^ (r >> 8) as u8;
        r = (cipher as u16)
            .wrapping_add(r)
            .wrapping_mul(c1)
            .wrapping_add(c2);
        dec.push(plain);
    }
    let cs = &dec[len_iv..];
    for i in 0..cs.len().saturating_sub(1) {
        if cs[i] == 12 && cs[i + 1] == 6 {
            return true;
        }
    }
    false
}

/// Parse all CharString advance widths from decrypted eexec data.
fn t1_parse_charstrings(
    decrypted: &[u8],
    len_iv: usize,
    seac_subrs: &std::collections::HashSet<u32>,
) -> std::collections::HashMap<String, i32> {
    let mut widths = std::collections::HashMap::new();
    let Some(cs_pos) = t1_find_bytes(decrypted, b"/CharStrings") else {
        return widths;
    };
    let mut pos = cs_pos;
    while pos < decrypted.len() {
        let Some(slash_off) = decrypted[pos..].iter().position(|&b| b == b'/') else {
            break;
        };
        let slash_pos = pos + slash_off;
        let check_start = slash_pos.saturating_sub(20).max(pos);
        if t1_find_bytes(&decrypted[check_start..slash_pos], b"end").is_some()
            && !decrypted[slash_pos..].starts_with(b"/CharStrings")
        {
            break;
        }
        let name_start = slash_pos + 1;
        if name_start >= decrypted.len() {
            break;
        }
        let name_end = decrypted[name_start..]
            .iter()
            .position(|b| b.is_ascii_whitespace())
            .map(|p| name_start + p)
            .unwrap_or(decrypted.len());
        let glyph_name = std::str::from_utf8(&decrypted[name_start..name_end])
            .unwrap_or("")
            .to_string();
        if glyph_name.is_empty() {
            pos = name_end + 1;
            continue;
        }
        let mut p = name_end;
        while p < decrypted.len() && decrypted[p].is_ascii_whitespace() {
            p += 1;
        }
        let num_start = p;
        while p < decrypted.len() && decrypted[p].is_ascii_digit() {
            p += 1;
        }
        let Ok(cs_len) = std::str::from_utf8(&decrypted[num_start..p])
            .unwrap_or("")
            .parse::<usize>()
        else {
            pos = p.max(name_end + 1);
            continue;
        };
        while p < decrypted.len() && decrypted[p].is_ascii_whitespace() {
            p += 1;
        }
        let marker_ok = p + 2 <= decrypted.len()
            && (decrypted[p..p + 2] == *b"RD" || decrypted[p..p + 2] == *b"-|");
        if !marker_ok {
            pos = p.max(name_end + 1);
            continue;
        }
        p += 2;
        if p < decrypted.len() && matches!(decrypted[p], b' ' | b'\t') {
            p += 1;
        }
        if p + cs_len > decrypted.len() {
            break;
        }
        if let Some(w) = t1_decrypt_charstring_width(&decrypted[p..p + cs_len], len_iv, seac_subrs)
        {
            widths.insert(glyph_name, w);
        }
        pos = p + cs_len;
    }
    widths
}

/// Decrypt a Type1 charstring and return the hsbw/sbw advance width.
fn t1_decrypt_charstring_width(
    data: &[u8],
    len_iv: usize,
    seac_subrs: &std::collections::HashSet<u32>,
) -> Option<i32> {
    if data.len() <= len_iv {
        return None;
    }
    let mut r: u16 = 4330;
    let c1: u16 = 52845;
    let c2: u16 = 22719;
    let mut decrypted = Vec::with_capacity(data.len());
    for &cipher in data {
        let plain = cipher ^ (r >> 8) as u8;
        r = (cipher as u16)
            .wrapping_add(r)
            .wrapping_mul(c1)
            .wrapping_add(c2);
        decrypted.push(plain);
    }
    let cs = &decrypted[len_iv..];
    let mut pos = 0;
    let mut values: Vec<i32> = Vec::new();
    let mut found_width_op = false;
    let mut is_sbw = false;
    while pos < cs.len() && values.len() < 8 {
        let b = cs[pos];
        if b == 13 {
            // hsbw
            found_width_op = true;
            break;
        }
        if b == 12 {
            if pos + 1 < cs.len() && cs[pos + 1] == 12 {
                // div
                pos += 2;
                if values.len() >= 2 {
                    let divisor = values.pop().unwrap();
                    let dividend = values.pop().unwrap();
                    values.push(if divisor != 0 {
                        dividend / divisor
                    } else {
                        dividend
                    });
                }
                continue;
            }
            if pos + 1 < cs.len() && cs[pos + 1] == 7 {
                // sbw
                is_sbw = true;
                found_width_op = true;
            }
            break;
        }
        if (32..=246).contains(&b) {
            values.push(b as i32 - 139);
            pos += 1;
        } else if (247..=250).contains(&b) {
            if pos + 1 >= cs.len() {
                break;
            }
            values.push((b as i32 - 247) * 256 + cs[pos + 1] as i32 + 108);
            pos += 2;
        } else if (251..=254).contains(&b) {
            if pos + 1 >= cs.len() {
                break;
            }
            values.push(-(b as i32 - 251) * 256 - cs[pos + 1] as i32 - 108);
            pos += 2;
        } else if b == 255 {
            if pos + 4 >= cs.len() {
                break;
            }
            values.push(i32::from_be_bytes([
                cs[pos + 1],
                cs[pos + 2],
                cs[pos + 3],
                cs[pos + 4],
            ]));
            pos += 5;
        } else {
            break;
        }
    }
    if !found_width_op {
        return None;
    }
    let width = if is_sbw { values.get(2) } else { values.get(1) }.copied()?;
    // Check remainder of charstring for seac (inline or via callsubr).
    pos += if is_sbw { 2 } else { 1 };
    let mut stack: Vec<i32> = Vec::with_capacity(8);
    while pos < cs.len() {
        let b = cs[pos];
        if b == 12 {
            if pos + 1 < cs.len() && cs[pos + 1] == 6 {
                return None; // seac — width unreliable
            }
            pos += 2;
            stack.clear();
        } else if b == 10 {
            // callsubr
            if let Some(&idx) = stack.last() {
                if idx >= 0 && seac_subrs.contains(&(idx as u32)) {
                    return None;
                }
            }
            pos += 1;
            stack.clear();
        } else if (32..=246).contains(&b) {
            stack.push(b as i32 - 139);
            pos += 1;
        } else if (247..=250).contains(&b) {
            if pos + 1 < cs.len() {
                stack.push((b as i32 - 247) * 256 + cs[pos + 1] as i32 + 108);
            }
            pos += 2;
        } else if (251..=254).contains(&b) {
            if pos + 1 < cs.len() {
                stack.push(-((b as i32 - 251) * 256) - cs[pos + 1] as i32 - 108);
            }
            pos += 2;
        } else if b == 255 {
            if pos + 4 < cs.len() {
                stack.push(i32::from_be_bytes([
                    cs[pos + 1],
                    cs[pos + 2],
                    cs[pos + 3],
                    cs[pos + 4],
                ]));
            }
            pos += 5;
        } else {
            pos += 1;
            stack.clear();
        }
    }
    Some(width)
}

/// Map a WinAnsiEncoding (Windows-1252) byte code to an AGL glyph name.
///
/// Source: PDF Reference Annex D.2 + Adobe Glyph List.
fn t1_winansi_glyph_name(code: u8) -> Option<&'static str> {
    match code {
        32 => Some("space"),
        33 => Some("exclam"),
        34 => Some("quotedbl"),
        35 => Some("numbersign"),
        36 => Some("dollar"),
        37 => Some("percent"),
        38 => Some("ampersand"),
        39 => Some("quotesingle"),
        40 => Some("parenleft"),
        41 => Some("parenright"),
        42 => Some("asterisk"),
        43 => Some("plus"),
        44 => Some("comma"),
        45 => Some("hyphen"),
        46 => Some("period"),
        47 => Some("slash"),
        48 => Some("zero"),
        49 => Some("one"),
        50 => Some("two"),
        51 => Some("three"),
        52 => Some("four"),
        53 => Some("five"),
        54 => Some("six"),
        55 => Some("seven"),
        56 => Some("eight"),
        57 => Some("nine"),
        58 => Some("colon"),
        59 => Some("semicolon"),
        60 => Some("less"),
        61 => Some("equal"),
        62 => Some("greater"),
        63 => Some("question"),
        64 => Some("at"),
        65 => Some("A"),
        66 => Some("B"),
        67 => Some("C"),
        68 => Some("D"),
        69 => Some("E"),
        70 => Some("F"),
        71 => Some("G"),
        72 => Some("H"),
        73 => Some("I"),
        74 => Some("J"),
        75 => Some("K"),
        76 => Some("L"),
        77 => Some("M"),
        78 => Some("N"),
        79 => Some("O"),
        80 => Some("P"),
        81 => Some("Q"),
        82 => Some("R"),
        83 => Some("S"),
        84 => Some("T"),
        85 => Some("U"),
        86 => Some("V"),
        87 => Some("W"),
        88 => Some("X"),
        89 => Some("Y"),
        90 => Some("Z"),
        91 => Some("bracketleft"),
        92 => Some("backslash"),
        93 => Some("bracketright"),
        94 => Some("asciicircum"),
        95 => Some("underscore"),
        96 => Some("grave"),
        97 => Some("a"),
        98 => Some("b"),
        99 => Some("c"),
        100 => Some("d"),
        101 => Some("e"),
        102 => Some("f"),
        103 => Some("g"),
        104 => Some("h"),
        105 => Some("i"),
        106 => Some("j"),
        107 => Some("k"),
        108 => Some("l"),
        109 => Some("m"),
        110 => Some("n"),
        111 => Some("o"),
        112 => Some("p"),
        113 => Some("q"),
        114 => Some("r"),
        115 => Some("s"),
        116 => Some("t"),
        117 => Some("u"),
        118 => Some("v"),
        119 => Some("w"),
        120 => Some("x"),
        121 => Some("y"),
        122 => Some("z"),
        123 => Some("braceleft"),
        124 => Some("bar"),
        125 => Some("braceright"),
        126 => Some("asciitilde"),
        128 => Some("Euro"),
        130 => Some("quotesinglbase"),
        131 => Some("florin"),
        132 => Some("quotedblbase"),
        133 => Some("ellipsis"),
        134 => Some("dagger"),
        135 => Some("daggerdbl"),
        136 => Some("circumflex"),
        137 => Some("perthousand"),
        138 => Some("Scaron"),
        139 => Some("guilsinglleft"),
        140 => Some("OE"),
        142 => Some("Zcaron"),
        145 => Some("quoteleft"),
        146 => Some("quoteright"),
        147 => Some("quotedblleft"),
        148 => Some("quotedblright"),
        149 => Some("bullet"),
        150 => Some("endash"),
        151 => Some("emdash"),
        152 => Some("tilde"),
        153 => Some("trademark"),
        154 => Some("scaron"),
        155 => Some("guilsinglright"),
        156 => Some("oe"),
        158 => Some("zcaron"),
        159 => Some("Ydieresis"),
        160 => Some("space"),
        161 => Some("exclamdown"),
        162 => Some("cent"),
        163 => Some("sterling"),
        164 => Some("currency"),
        165 => Some("yen"),
        166 => Some("brokenbar"),
        167 => Some("section"),
        168 => Some("dieresis"),
        169 => Some("copyright"),
        170 => Some("ordfeminine"),
        171 => Some("guillemotleft"),
        172 => Some("logicalnot"),
        173 => Some("hyphen"),
        174 => Some("registered"),
        175 => Some("macron"),
        176 => Some("degree"),
        177 => Some("plusminus"),
        178 => Some("twosuperior"),
        179 => Some("threesuperior"),
        180 => Some("acute"),
        181 => Some("mu"),
        182 => Some("paragraph"),
        183 => Some("periodcentered"),
        184 => Some("cedilla"),
        185 => Some("onesuperior"),
        186 => Some("ordmasculine"),
        187 => Some("guillemotright"),
        188 => Some("onequarter"),
        189 => Some("onehalf"),
        190 => Some("threequarters"),
        191 => Some("questiondown"),
        192 => Some("Agrave"),
        193 => Some("Aacute"),
        194 => Some("Acircumflex"),
        195 => Some("Atilde"),
        196 => Some("Adieresis"),
        197 => Some("Aring"),
        198 => Some("AE"),
        199 => Some("Ccedilla"),
        200 => Some("Egrave"),
        201 => Some("Eacute"),
        202 => Some("Ecircumflex"),
        203 => Some("Edieresis"),
        204 => Some("Igrave"),
        205 => Some("Iacute"),
        206 => Some("Icircumflex"),
        207 => Some("Idieresis"),
        208 => Some("Eth"),
        209 => Some("Ntilde"),
        210 => Some("Ograve"),
        211 => Some("Oacute"),
        212 => Some("Ocircumflex"),
        213 => Some("Otilde"),
        214 => Some("Odieresis"),
        215 => Some("multiply"),
        216 => Some("Oslash"),
        217 => Some("Ugrave"),
        218 => Some("Uacute"),
        219 => Some("Ucircumflex"),
        220 => Some("Udieresis"),
        221 => Some("Yacute"),
        222 => Some("Thorn"),
        223 => Some("germandbls"),
        224 => Some("agrave"),
        225 => Some("aacute"),
        226 => Some("acircumflex"),
        227 => Some("atilde"),
        228 => Some("adieresis"),
        229 => Some("aring"),
        230 => Some("ae"),
        231 => Some("ccedilla"),
        232 => Some("egrave"),
        233 => Some("eacute"),
        234 => Some("ecircumflex"),
        235 => Some("edieresis"),
        236 => Some("igrave"),
        237 => Some("iacute"),
        238 => Some("icircumflex"),
        239 => Some("idieresis"),
        240 => Some("eth"),
        241 => Some("ntilde"),
        242 => Some("ograve"),
        243 => Some("oacute"),
        244 => Some("ocircumflex"),
        245 => Some("otilde"),
        246 => Some("odieresis"),
        247 => Some("divide"),
        248 => Some("oslash"),
        249 => Some("ugrave"),
        250 => Some("uacute"),
        251 => Some("ucircumflex"),
        252 => Some("udieresis"),
        253 => Some("yacute"),
        254 => Some("thorn"),
        255 => Some("ydieresis"),
        _ => None,
    }
}

/// Look up a glyph name in Adobe StandardEncoding for the given byte code.
///
/// Returns the AGL name or None for codes that are undefined in StandardEncoding.
fn t1_standard_encoding_name(code: u8) -> Option<&'static str> {
    // Source: Adobe Standard Encoding (ISO 19005-1 Annex A, Adobe Tech Note #5001).
    match code {
        32 => Some("space"),
        33 => Some("exclam"),
        34 => Some("quotedbl"),
        35 => Some("numbersign"),
        36 => Some("dollar"),
        37 => Some("percent"),
        38 => Some("ampersand"),
        39 => Some("quoteright"),
        40 => Some("parenleft"),
        41 => Some("parenright"),
        42 => Some("asterisk"),
        43 => Some("plus"),
        44 => Some("comma"),
        45 => Some("hyphen"),
        46 => Some("period"),
        47 => Some("slash"),
        48 => Some("zero"),
        49 => Some("one"),
        50 => Some("two"),
        51 => Some("three"),
        52 => Some("four"),
        53 => Some("five"),
        54 => Some("six"),
        55 => Some("seven"),
        56 => Some("eight"),
        57 => Some("nine"),
        58 => Some("colon"),
        59 => Some("semicolon"),
        60 => Some("less"),
        61 => Some("equal"),
        62 => Some("greater"),
        63 => Some("question"),
        64 => Some("at"),
        65 => Some("A"),
        66 => Some("B"),
        67 => Some("C"),
        68 => Some("D"),
        69 => Some("E"),
        70 => Some("F"),
        71 => Some("G"),
        72 => Some("H"),
        73 => Some("I"),
        74 => Some("J"),
        75 => Some("K"),
        76 => Some("L"),
        77 => Some("M"),
        78 => Some("N"),
        79 => Some("O"),
        80 => Some("P"),
        81 => Some("Q"),
        82 => Some("R"),
        83 => Some("S"),
        84 => Some("T"),
        85 => Some("U"),
        86 => Some("V"),
        87 => Some("W"),
        88 => Some("X"),
        89 => Some("Y"),
        90 => Some("Z"),
        91 => Some("bracketleft"),
        92 => Some("backslash"),
        93 => Some("bracketright"),
        94 => Some("asciicircum"),
        95 => Some("underscore"),
        96 => Some("quoteleft"),
        97 => Some("a"),
        98 => Some("b"),
        99 => Some("c"),
        100 => Some("d"),
        101 => Some("e"),
        102 => Some("f"),
        103 => Some("g"),
        104 => Some("h"),
        105 => Some("i"),
        106 => Some("j"),
        107 => Some("k"),
        108 => Some("l"),
        109 => Some("m"),
        110 => Some("n"),
        111 => Some("o"),
        112 => Some("p"),
        113 => Some("q"),
        114 => Some("r"),
        115 => Some("s"),
        116 => Some("t"),
        117 => Some("u"),
        118 => Some("v"),
        119 => Some("w"),
        120 => Some("x"),
        121 => Some("y"),
        122 => Some("z"),
        123 => Some("braceleft"),
        124 => Some("bar"),
        125 => Some("braceright"),
        126 => Some("asciitilde"),
        161 => Some("exclamdown"),
        162 => Some("cent"),
        163 => Some("sterling"),
        164 => Some("fraction"),
        165 => Some("yen"),
        166 => Some("florin"),
        167 => Some("section"),
        168 => Some("currency"),
        169 => Some("quotesingle"),
        170 => Some("quotedblleft"),
        171 => Some("guillemotleft"),
        172 => Some("guilsinglleft"),
        173 => Some("guilsinglright"),
        174 => Some("fi"),
        175 => Some("fl"),
        177 => Some("endash"),
        178 => Some("dagger"),
        179 => Some("daggerdbl"),
        180 => Some("periodcentered"),
        182 => Some("paragraph"),
        183 => Some("bullet"),
        184 => Some("quotesinglbase"),
        185 => Some("quotedblbase"),
        186 => Some("quotedblright"),
        187 => Some("guillemotright"),
        188 => Some("ellipsis"),
        189 => Some("perthousand"),
        191 => Some("questiondown"),
        193 => Some("grave"),
        194 => Some("acute"),
        195 => Some("circumflex"),
        196 => Some("tilde"),
        197 => Some("macron"),
        198 => Some("breve"),
        199 => Some("dotaccent"),
        200 => Some("dieresis"),
        202 => Some("ring"),
        203 => Some("cedilla"),
        205 => Some("hungarumlaut"),
        206 => Some("ogonek"),
        207 => Some("caron"),
        208 => Some("emdash"),
        225 => Some("AE"),
        227 => Some("ordfeminine"),
        232 => Some("Lslash"),
        233 => Some("Oslash"),
        234 => Some("OE"),
        235 => Some("ordmasculine"),
        241 => Some("ae"),
        245 => Some("dotlessi"),
        248 => Some("lslash"),
        249 => Some("oslash"),
        250 => Some("oe"),
        251 => Some("germandbls"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── .notdef hex scan ──

    #[test]
    fn notdef_scan_detects_null_byte_in_tj() {
        // Simple font mode: <00> = code 0 = notdef
        assert!(scan_for_notdef_in_content(
            b"BT /F1 12 Tf <00> Tj ET",
            false
        ));
    }

    #[test]
    fn notdef_scan_detects_null_in_multi_byte_hex_simple() {
        // Simple font mode: <0041> = codes 0 and 65; 0 = notdef
        assert!(scan_for_notdef_in_content(
            b"BT /F1 12 Tf <0041> Tj ET",
            false
        ));
    }

    #[test]
    fn notdef_scan_skips_null_in_multi_byte_hex_cid() {
        // CID mode: <0041> = CID 65 = 'A', NOT notdef — only <0000> would be notdef
        assert!(!scan_for_notdef_in_content(
            b"BT /F1 12 Tf <0041> Tj ET",
            true
        ));
    }

    #[test]
    fn notdef_scan_detects_cid_zero_in_cid_mode() {
        // CID mode: <0000> = CID 0 = notdef
        assert!(scan_for_notdef_in_content(
            b"BT /F1 12 Tf <0000> Tj ET",
            true
        ));
    }

    #[test]
    fn notdef_scan_skips_nonzero_hex() {
        assert!(!scan_for_notdef_in_content(
            b"BT /F1 12 Tf <41> Tj ET",
            false
        ));
    }

    #[test]
    fn notdef_scan_skips_non_text_operators() {
        assert!(!scan_for_notdef_in_content(b"<00> Do", false));
    }

    // ── hex_contains_null_byte ──

    #[test]
    fn hex_null_byte_pair_00() {
        assert!(hex_contains_null_byte(b"00"));
    }

    #[test]
    fn hex_null_byte_in_middle() {
        assert!(hex_contains_null_byte(b"410042"));
    }

    #[test]
    fn hex_no_null_byte() {
        assert!(!hex_contains_null_byte(b"4142"));
    }

    // ── BCP 47 lang tag validation ──

    #[test]
    fn valid_lang_tags() {
        assert!(is_valid_lang_tag("en"));
        assert!(is_valid_lang_tag("en-US"));
        assert!(is_valid_lang_tag("zh-Hant"));
        assert!(is_valid_lang_tag("de-DE"));
        assert!(is_valid_lang_tag("sr-Latn-RS"));
    }

    #[test]
    fn invalid_lang_tags() {
        assert!(!is_valid_lang_tag(""));
        assert!(!is_valid_lang_tag("en-12"));
        assert!(!is_valid_lang_tag("english"));
        assert!(!is_valid_lang_tag("e"));
    }

    // ── is_standard_cmap ──

    #[test]
    fn standard_cmaps_recognized() {
        assert!(is_standard_cmap(b"Identity-H"));
        assert!(is_standard_cmap(b"Identity-V"));
        // Note: is_standard_cmap only checks Identity-H/V.
        // Non-Identity predefined CMaps are handled separately in check_cmap_embedding.
    }

    #[test]
    fn custom_cmaps_rejected() {
        assert!(!is_standard_cmap(b"Adobe-Custom-1"));
        assert!(!is_standard_cmap(b"MyFont-CMap"));
    }

    // ── WinAnsi glyph name mapping ──

    #[test]
    fn winansi_glyph_names() {
        assert_eq!(t1_winansi_glyph_name(32), Some("space"));
        assert_eq!(t1_winansi_glyph_name(65), Some("A"));
        assert_eq!(t1_winansi_glyph_name(97), Some("a"));
        assert_eq!(t1_winansi_glyph_name(48), Some("zero"));
        assert_eq!(t1_winansi_glyph_name(46), Some("period"));
        assert_eq!(t1_winansi_glyph_name(0), None);
    }

    // ── xref header spacing ──

    #[test]
    fn xref_header_double_space_detected() {
        let mut report = ComplianceReport::default();
        check_xref_header_spacing(b"\n0  14\n0000000000 65535 f \r\n", &mut report);
        assert!(report.issues.iter().any(|i| i.rule == "6.1.4"));
    }

    #[test]
    fn xref_header_single_space_ok() {
        let mut report = ComplianceReport::default();
        check_xref_header_spacing(b"\n0 14\n0000000000 65535 f \r\n", &mut report);
        assert!(!report.issues.iter().any(|i| i.rule == "6.1.4"));
    }

    // ── ICC profile identity comparison ──

    #[test]
    fn icc_profiles_identical_ignores_profile_id() {
        let mut a = vec![0u8; 200];
        let mut b = vec![0u8; 200];
        // Set different Profile ID bytes (84-99)
        for i in 84..100 {
            a[i] = 0xAA;
            b[i] = 0xBB;
        }
        assert!(icc_profiles_identical(&a, &b));
    }

    #[test]
    fn icc_profiles_identical_detects_different_content() {
        let a = vec![1u8; 200];
        let mut b = vec![1u8; 200];
        b[0] = 2;
        assert!(!icc_profiles_identical(&a, &b));
    }

    // ── BDC /Lang content-stream scan ──

    #[test]
    fn scan_bdc_lang_utf16be_hex() {
        // `<feff0430043d002d00430041>` = UTF-16BE "ан-CA" (Cyrillic primary = invalid BCP-47)
        // This is the exact pattern from 6-8-4-t01-fail-c.pdf.
        let content = b"/Lang <feff0430043d002d00430041>>>BDC";
        let tags = scan_bdc_lang_values(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0], "\u{0430}\u{043D}-CA"); // "ан-CA"
    }

    #[test]
    fn scan_bdc_lang_ascii_literal() {
        let content = b"<</Lang (en-US)>>BDC";
        let tags = scan_bdc_lang_values(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0], "en-US");
    }

    #[test]
    fn scan_bdc_lang_not_matched_for_language_name() {
        // /Language should NOT be matched as /Lang
        let content = b"/Language <feff0065006e> BDC";
        let tags = scan_bdc_lang_values(content);
        assert!(tags.is_empty());
    }

    #[test]
    fn decode_hex_to_bytes_basic() {
        assert_eq!(
            decode_hex_to_bytes(b"feff0065006e"),
            vec![0xFE, 0xFF, 0x00, 0x65, 0x00, 0x6E]
        );
    }

    // ── has_length_key ──

    #[test]
    fn has_length_key_direct_integer() {
        let data = b"<< /Type /XObject /Length 42 >>stream\n";
        // stream_pos = position of "stream" = 32
        let stream_pos = data.windows(6).position(|w| w == b"stream").unwrap();
        assert!(has_length_key(data, stream_pos));
    }

    #[test]
    fn has_length_key_indirect_ref() {
        let data = b"<< /Type /XObject /Length 5 0 R >>stream\n";
        let stream_pos = data.windows(6).position(|w| w == b"stream").unwrap();
        assert!(has_length_key(data, stream_pos));
    }

    #[test]
    fn has_length_key_rejects_length1() {
        // /Length1 is a different key — should NOT match as /Length
        let data = b"<< /Length1 15312 >>stream\n";
        let stream_pos = data.windows(6).position(|w| w == b"stream").unwrap();
        assert!(!has_length_key(data, stream_pos));
    }

    #[test]
    fn has_length_key_absent() {
        let data = b"<< /Type /XObject >>stream\n";
        let stream_pos = data.windows(6).position(|w| w == b"stream").unwrap();
        assert!(!has_length_key(data, stream_pos));
    }
}
