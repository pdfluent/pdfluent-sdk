//! Shared compliance checking helpers.

use crate::{ComplianceIssue, ComplianceReport, Severity};
use pdf_syntax::object::dict::keys;
use pdf_syntax::object::{Array, Dict, Name, Object, Stream};
use pdf_syntax::page::Resources;
use pdf_syntax::Pdf;

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
    for obj in pdf.objects() {
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
    let stream: Stream<'_> = cat.get(keys::METADATA)?;
    stream.decoded().ok()
}

/// Parse XMP metadata to find pdfaid:part and pdfaid:conformance.
///
/// PDF/A-4 (ISO 19005-4) may omit pdfaid:conformance entirely;
/// in that case, conformance defaults to an empty string.
pub fn parse_xmp_pdfa(xmp: &[u8]) -> Option<(u8, String)> {
    let text = std::str::from_utf8(xmp).ok()?;

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
pub fn check_xmp_pdfa_identification(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(xmp) = get_xmp_metadata(pdf) else {
        error(report, "6.7.11", "Document missing XMP metadata stream");
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

/// Check MarkInfo/Marked is present and true (§6.8.2.2).
pub fn check_mark_info(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    match cat.get::<Dict<'_>>(keys::MARK_INFO) {
        Some(mark_info) => {
            match mark_info.get::<Object<'_>>(b"Marked" as &[u8]) {
                Some(Object::Boolean(true)) => {}
                _ => {
                    error(
                        report,
                        "6.8.2.2",
                        "MarkInfo /Marked is not set to true",
                    );
                }
            }
        }
        None => {
            error(
                report,
                "6.8.2.2",
                "MarkInfo dictionary missing from catalog",
            );
        }
    }
}

/// Extract a value from an XMP element like `<ns:key>value</ns:key>`.
fn extract_xmp_value(text: &str, key: &str) -> Option<String> {
    let open = format!("<{key}>");
    let close = format!("</{key}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim().to_string())
}

/// Extract a value from an XMP attribute like `ns:key="value"`.
fn extract_xmp_attr(text: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=\"");
    let start = text.find(&pattern)? + pattern.len();
    let end = text[start..].find('"')? + start;
    Some(text[start..end].trim().to_string())
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
    Some(region[content_start..content_end].trim().to_string())
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
        if idx > 10 { &s[..idx] } else { s }
    } else if let Some(idx) = s.rfind('-') {
        if idx > 10 { &s[..idx] } else { s }
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
fn check_date_equivalence(
    pdf_date: &Option<pdf_syntax::object::DateTime>,
    info_key: &str,
    xmp_key: &str,
    xmp_text: &str,
    report: &mut ComplianceReport,
) {
    let Some(dt) = pdf_date else { return };
    let xmp_val = extract_xmp_value(xmp_text, xmp_key)
        .or_else(|| extract_xmp_attr(xmp_text, xmp_key));
    let Some(xmp_str) = xmp_val else { return };

    if let Some((y, mo, d, h, mi, s)) = parse_xmp_datetime(&xmp_str) {
        if dt.year != y || dt.month != mo || dt.day != d || dt.hour != h || dt.minute != mi || dt.second != s {
            error(
                report,
                "6.7.3",
                format!(
                    "{info_key} mismatch: Info={:04}-{:02}-{:02}T{:02}:{:02}:{:02} vs XMP={xmp_str}",
                    dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
                ),
            );
        }
    }
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
pub fn check_device_color_vs_output_intent(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(profile_components) = output_intent_profile_components(pdf) else {
        return; // No OutputIntent profile — handled by other checks
    };

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let loc = format!("page {}", page_idx + 1);

        // Scan page content stream
        if let Some(content) = page.page_stream() {
            let ops = detect_device_color_ops(content);
            report_color_vs_profile(&ops, profile_components, &loc, report);
        }

        // Scan Form XObject content streams
        let page_dict = page.raw();
        let res_dict = page_dict.get::<Dict<'_>>(keys::RESOURCES);
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
                        report_color_vs_profile(&ops, profile_components, &xloc, report);
                    }
                }
            }
        }

        // Scan annotation appearance streams
        if let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) {
            for annot in annots.iter::<Dict<'_>>() {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    for key in [b"N" as &[u8], b"R", b"D"] {
                        if let Some(stream) = ap.get::<Stream<'_>>(key) {
                            if let Ok(decoded) = stream.decoded() {
                                let ops = detect_device_color_ops(&decoded);
                                let kloc =
                                    format!("{loc} AP/{}", std::str::from_utf8(key).unwrap_or("?"));
                                report_color_vs_profile(&ops, profile_components, &kloc, report);
                            }
                        }
                    }
                }
            }
        }

        // Scan Shading/Pattern resources for device CS vs profile mismatch
        if let Some(ref rd) = res_dict {
            scan_shading_cs_vs_profile(rd, profile_components, &loc, report);
            scan_pattern_cs_vs_profile(rd, profile_components, &loc, report);
            scan_image_cs_vs_profile(rd, profile_components, &loc, report);
            scan_type3_charprocs_vs_profile(rd, profile_components, &loc, report);
            scan_smask_cs_vs_profile(rd, profile_components, &loc, report);
        }
    }
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
            if let Ok(decoded) = pat_stream.decoded() {
                let ops = detect_device_color_ops(&decoded);
                let loc = format!("{base_loc} Pattern {pname}");
                report_color_vs_profile(&ops, profile_components, &loc, report);
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
    profile_components: u32,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    let Some(font_dict) = res_dict.get::<Dict<'_>>(keys::FONT) else {
        return;
    };
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
        let Some(charprocs) = font.get::<Dict<'_>>(b"CharProcs" as &[u8]) else {
            continue;
        };
        let fstr = std::str::from_utf8(fname.as_ref()).unwrap_or("?");
        for (cname, _) in charprocs.entries() {
            if let Some(stream) = charprocs.get::<Stream<'_>>(cname.as_ref()) {
                if let Ok(decoded) = stream.decoded() {
                    let cstr = std::str::from_utf8(cname.as_ref()).unwrap_or("?");
                    let ops = detect_device_color_ops(&decoded);
                    let loc = format!("{base_loc} Type3Font {fstr} CharProc {cstr}");
                    report_color_vs_profile(&ops, profile_components, &loc, report);
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
    if ops.has_rgb && profile_components != 3 {
        error_at(
            report,
            "6.2.3.3",
            "DeviceRGB used but OutputIntent profile is not RGB",
            location.to_string(),
        );
    }
    if ops.has_cmyk && profile_components != 4 {
        error_at(
            report,
            "6.2.3.3",
            "DeviceCMYK used but OutputIntent profile is not CMYK",
            location.to_string(),
        );
    }
    if ops.has_gray && profile_components != 1 && profile_components != 3 && profile_components != 4
    {
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
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let fonts = &page.resources().fonts;
        for (name, _) in fonts.entries() {
            let name_str = std::str::from_utf8(name.as_ref()).unwrap_or("<invalid>");
            if let Some(font_dict) = fonts.get::<Dict<'_>>(name.as_ref()) {
                callback(name_str, &font_dict, page_idx);
            }
        }
    }
}

/// Check if the document has embedded files.
pub fn has_embedded_files(pdf: &Pdf) -> bool {
    let Some(cat) = catalog(pdf) else {
        return false;
    };

    if let Some(names) = cat.get::<Dict<'_>>(keys::NAMES) {
        if names.get::<Object<'_>>(keys::EMBEDDED_FILES).is_some() {
            return true;
        }
    }

    cat.get::<Array<'_>>(keys::AF).is_some()
}

/// Check if any page has transparency (Group with /S /Transparency).
pub fn has_transparency(pdf: &Pdf) -> bool {
    for page in pdf.pages().iter() {
        let page_dict = page.raw();
        if let Some(group) = page_dict.get::<Dict<'_>>(keys::GROUP) {
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
pub fn document_lang(pdf: &Pdf) -> Option<String> {
    let cat = catalog(pdf)?;
    let lang = cat.get::<pdf_syntax::object::String>(keys::LANG)?;
    std::string::String::from_utf8(lang.as_bytes().to_vec()).ok()
}

/// Validate that a language tag follows basic BCP-47 format.
/// Primary subtag must be 2-3 ASCII letters.
fn is_valid_lang_tag(tag: &str) -> bool {
    if tag.is_empty() {
        return false;
    }
    let primary = tag.split('-').next().unwrap_or("");
    (primary.len() == 2 || primary.len() == 3)
        && primary.bytes().all(|b| b.is_ascii_alphabetic())
}

/// Check /Lang entries in catalog and structure elements are valid BCP-47 (§6.8.4).
pub fn check_lang_values(pdf: &Pdf, report: &mut ComplianceReport) {
    // Check catalog /Lang
    if let Some(cat) = catalog(pdf) {
        if let Some(lang_str) = cat.get::<pdf_syntax::object::String>(keys::LANG) {
            if let Ok(tag) = std::str::from_utf8(lang_str.as_bytes()) {
                if !is_valid_lang_tag(tag) {
                    error(
                        report,
                        "6.8.4",
                        format!("Catalog /Lang value '{tag}' is not a valid Language-Tag"),
                    );
                }
            }
        }
        // Check structure tree /Lang entries
        if let Some(struct_root) = cat.get::<Dict<'_>>(keys::STRUCT_TREE_ROOT) {
            check_struct_elem_lang(&struct_root, report);
        }
    }
}

fn check_struct_elem_lang(elem: &Dict<'_>, report: &mut ComplianceReport) {
    if let Some(lang_str) = elem.get::<pdf_syntax::object::String>(keys::LANG) {
        if let Ok(tag) = std::str::from_utf8(lang_str.as_bytes()) {
            if !is_valid_lang_tag(tag) {
                error(
                    report,
                    "6.8.4",
                    format!("Structure element /Lang value '{tag}' is not a valid Language-Tag"),
                );
            }
        }
    }
    if let Some(kids) = elem.get::<Array<'_>>(keys::K) {
        for kid in kids.iter::<Dict<'_>>() {
            check_struct_elem_lang(&kid, report);
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

    // Known predefined XMP namespaces (PDF/A-1 §6.7.9, PDF/A-2 §6.6.2.3.1)
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
        let has_default_rgb = cs_dict_ref
            .get::<Object<'_>>(keys::DEFAULT_RGB)
            .is_some();
        let has_default_cmyk = cs_dict_ref
            .get::<Object<'_>>(keys::DEFAULT_CMYK)
            .is_some();
        let has_default_gray = cs_dict_ref
            .get::<Object<'_>>(keys::DEFAULT_GRAY)
            .is_some();

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
                    error_at(report, "6.2.4.3", "DeviceGray in ColorSpace resources without DefaultGray or OutputIntent", loc.clone());
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
                    error_at(report, "6.2.4.3", format!("ColorSpace {nstr} has DeviceRGB alternate/base"), xloc.clone());
                }
                if kind == 2 && !cmyk_ok {
                    error_at(report, "6.2.4.3", format!("ColorSpace {nstr} has DeviceCMYK alternate/base"), xloc.clone());
                }
                if kind == 3 && !gray_ok {
                    error_at(report, "6.2.4.3", format!("ColorSpace {nstr} has DeviceGray alternate/base"), xloc.clone());
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
    let has_painting = tokens.iter().any(|&t| {
        matches!(
            t,
            "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "S" | "s"
        )
    });
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
        "Tf",  // font
        "Do",  // XObject
        "cs", "CS",  // color space
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

/// Check Info dict / XMP metadata consistency (§6.7.3).
///
/// Properties in /Info dict must have matching values in XMP metadata.
pub fn check_info_xmp_consistency(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(xmp_data) = get_xmp_metadata(pdf) else {
        return;
    };
    let Ok(xmp_text) = std::str::from_utf8(&xmp_data) else {
        return;
    };

    let metadata = pdf.metadata();

    // Check Creator (/Info Creator vs xmp:CreatorTool)
    if metadata.creator.is_some() {
        let xmp_creator = extract_xmp_value(xmp_text, "xmp:CreatorTool")
            .or_else(|| extract_xmp_attr(xmp_text, "xmp:CreatorTool"));
        if xmp_creator.is_none() {
            error(
                report,
                "6.7.3",
                "/Info has Creator but XMP is missing xmp:CreatorTool",
            );
        }
    }

    // Check Producer (/Info Producer vs pdf:Producer)
    if metadata.producer.is_some() {
        let xmp_producer = extract_xmp_value(xmp_text, "pdf:Producer")
            .or_else(|| extract_xmp_attr(xmp_text, "pdf:Producer"));
        if xmp_producer.is_none() {
            error(
                report,
                "6.7.3",
                "/Info has Producer but XMP is missing pdf:Producer",
            );
        }
    }

    // Check CreationDate (/Info CreationDate vs xmp:CreateDate)
    if metadata.creation_date.is_some() {
        let xmp_create_date = extract_xmp_value(xmp_text, "xmp:CreateDate")
            .or_else(|| extract_xmp_attr(xmp_text, "xmp:CreateDate"));
        if xmp_create_date.is_none() {
            error(
                report,
                "6.7.3",
                "/Info has CreationDate but XMP is missing xmp:CreateDate",
            );
        }
    }

    // Check ModDate (/Info ModDate vs xmp:ModifyDate)
    if metadata.modification_date.is_some() {
        let xmp_mod_date = extract_xmp_value(xmp_text, "xmp:ModifyDate")
            .or_else(|| extract_xmp_attr(xmp_text, "xmp:ModifyDate"));
        if xmp_mod_date.is_none() {
            error(
                report,
                "6.7.3",
                "/Info has ModDate but XMP is missing xmp:ModifyDate",
            );
        }
    }

    // Check Title (/Info Title vs dc:title)
    if let Some(title) = &metadata.title {
        if !xmp_text.contains("dc:title") {
            error(
                report,
                "6.7.3",
                "/Info has Title but XMP is missing dc:title",
            );
        } else {
            // Extract dc:title value (usually in rdf:Alt/rdf:li)
            let xmp_title = extract_rdf_alt_value(xmp_text, "dc:title");
            if let Some(xmp_val) = &xmp_title {
                let info_val = String::from_utf8_lossy(title);
                if info_val.trim() != xmp_val.trim() {
                    error(
                        report,
                        "6.7.3",
                        format!(
                            "Title mismatch: Info='{}' vs XMP='{}'",
                            info_val.chars().take(50).collect::<String>(),
                            xmp_val.chars().take(50).collect::<String>()
                        ),
                    );
                }
            }
        }
    }

    // Check Author (/Info Author vs dc:creator)
    if let Some(author) = &metadata.author {
        if xmp_text.contains("dc:creator") {
            let (xmp_vals, count) = extract_rdf_seq_values(xmp_text, "dc:creator");
            if count != 1 {
                error(
                    report,
                    "6.7.3",
                    format!("dc:creator has {count} entries, expected exactly 1"),
                );
            }
            if let Some(xmp_val) = xmp_vals.first() {
                let info_val = String::from_utf8_lossy(author);
                if info_val.as_ref() != xmp_val.as_str() {
                    error(
                        report,
                        "6.7.3",
                        format!(
                            "Author mismatch: Info='{}' vs XMP='{}'",
                            info_val.chars().take(50).collect::<String>(),
                            xmp_val.chars().take(50).collect::<String>()
                        ),
                    );
                }
            }
        }
    }

    // Check Keywords (/Info Keywords vs pdf:Keywords)
    if let Some(keywords) = &metadata.keywords {
        let xmp_keywords = extract_xmp_value(xmp_text, "pdf:Keywords")
            .or_else(|| extract_xmp_attr(xmp_text, "pdf:Keywords"));
        if let Some(xmp_val) = &xmp_keywords {
            let info_val = String::from_utf8_lossy(keywords);
            if info_val.as_ref() != xmp_val.as_str() {
                error(
                    report,
                    "6.7.3",
                    format!(
                        "Keywords mismatch: Info='{}' vs XMP='{}'",
                        info_val.chars().take(50).collect::<String>(),
                        xmp_val.chars().take(50).collect::<String>()
                    ),
                );
            }
        } else {
            error(
                report,
                "6.7.3",
                "/Info has Keywords but XMP is missing pdf:Keywords",
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
}

/// Check annotation dictionaries have required /F key and correct flags (§6.3.2).
///
/// All annotations (except Popup) must have /F key. When present, Print flag
/// must be set, Hidden/Invisible/ToggleNoView/NoView flags must be clear.
pub fn check_annotation_flags(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    // PDF/A-4: 6.3.2 → normalized 6.5.2; parts 1/2/3: 6.5.3
    let rule = if part == 4 { "6.5.2" } else { "6.5.3" };

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            // Skip Popup annotations
            if let Some(subtype) = annot.get::<Name>(keys::SUBTYPE) {
                if subtype.as_ref() == b"Popup" {
                    continue;
                }
            }

            if let Some(flags) = annot.get::<i32>(keys::F) {
                // Bit 1 (0x01) = Invisible, Bit 2 (0x02) = Hidden,
                // Bit 3 (0x04) = Print, Bit 6 (0x20) = NoView,
                // Bit 9 (0x100) = ToggleNoView
                let invisible = flags & 0x01 != 0;
                let hidden = flags & 0x02 != 0;
                let print = flags & 0x04 != 0;
                let no_view = flags & 0x20 != 0;
                let toggle_no_view = flags & 0x100 != 0;

                if !print || invisible || hidden || no_view || toggle_no_view {
                    error_at(
                        report,
                        rule,
                        format!(
                            "Annotation /F flags {flags:#x}: Print must be set, Hidden/Invisible/NoView/ToggleNoView must be clear"
                        ),
                        format!("page {}", page_idx + 1),
                    );
                }
            } else {
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
    // C/IC are RGB (3 components); only OK if OutputIntent is also RGB-based
    let rgb_intent = profile_components == Some(3);
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            let has_c = annot.get::<Array<'_>>(b"C" as &[u8]).is_some();
            let has_ic = annot.get::<Array<'_>>(b"IC" as &[u8]).is_some();
            if (has_c || has_ic) && !rgb_intent {
                let subtype_name = annot
                    .get::<Name>(keys::SUBTYPE)
                    .map(|n| std::str::from_utf8(n.as_ref()).unwrap_or("?").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let which = match (has_c, has_ic) {
                    (true, true) => "/C and /IC",
                    (true, false) => "/C",
                    (false, true) => "/IC",
                    _ => unreachable!(),
                };
                error_at(
                    report,
                    "6.5.3",
                    format!(
                        "{subtype_name} annotation has {which} color array without RGB-based OutputIntent"
                    ),
                    format!("page {}", page_idx + 1),
                );
            }
        }
    }
}

/// Check Form XObjects don't contain forbidden keys (§6.2.9).
///
/// Form XObjects must not contain OPI key, PS key, or Subtype2=PS.
/// Reference XObjects (Ref key) are also forbidden.
pub fn check_form_xobjects(pdf: &Pdf, report: &mut ComplianceReport) {
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
            let Some(stream) = xobj_dict.get::<Stream<'_>>(name.as_ref()) else {
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

            if dict.contains_key(keys::OPI) {
                error_at(
                    report,
                    "6.2.9",
                    format!("Form XObject {xobj_name} contains forbidden /OPI key"),
                    format!("page {}", page_idx + 1),
                );
            }
            if dict.contains_key(keys::PS) {
                error_at(
                    report,
                    "6.2.9",
                    format!("Form XObject {xobj_name} contains forbidden /PS key"),
                    format!("page {}", page_idx + 1),
                );
            }
            if let Some(sub2) = dict.get::<Name>(b"Subtype2" as &[u8]) {
                if sub2.as_ref() == keys::PS {
                    error_at(
                        report,
                        "6.2.9",
                        format!("Form XObject {xobj_name} has Subtype2=PS"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
            if dict.contains_key(b"Ref" as &[u8]) {
                error_at(
                    report,
                    "6.2.9",
                    format!("Form XObject {xobj_name} is a reference XObject (contains /Ref)"),
                    format!("page {}", page_idx + 1),
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
            error(report, "6.2.3.3", "ICC profile too short to parse header");
            continue;
        }
        let major = profile_data[8];
        let max_version = if part == 1 { 2 } else { 4 };
        if major > max_version {
            error(
                report,
                "6.2.3.3",
                format!(
                    "ICC profile version {major}.x exceeds maximum v{max_version} for PDF/A-{part}"
                ),
            );
        }
    }
}

// ─── §6.2.4.2 — ICCBased Alternate CS consistency ──────────────────────────

/// Check ICCBased color spaces have consistent Alternate CS (§6.2.4.2).
pub fn check_iccbased_alternate(pdf: &Pdf, report: &mut ComplianceReport) {
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
            if cs_type.as_ref() != keys::ICC_BASED {
                continue;
            }
            // Second item in array is the ICC stream
            let Some(icc_stream) = items.next().and_then(|o| match o {
                Object::Stream(s) => Some(s),
                _ => None,
            }) else {
                continue;
            };
            let icc_dict = icc_stream.dict();
            let n_components: Option<i32> = icc_dict.get(keys::N);

            if let Some(alt_name) = icc_dict.get::<Name>(keys::ALTERNATE) {
                let alt = alt_name.as_ref();
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
                        let cs_name = std::str::from_utf8(name.as_ref()).unwrap_or("?");
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
            for cn in colorant_names.iter::<Name>() {
                let cn_bytes = cn.as_ref();
                if cn_bytes == b"None" || cn_bytes == b"All" {
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
                            "DeviceN CS '{cs_name}' colorant '{cn_str}' not defined in Colorants dictionary"
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

/// Check 'ri' operators in a content stream.
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
                    "6.2.5",
                    format!("Invalid rendering intent '{name}'"),
                    location,
                );
            }
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
                    "6.2.5",
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
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let loc = format!("page {}", page_idx + 1);

        if let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) {
            check_image_restrictions_in_res(&res_dict, &loc, report);
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
                                check_image_restrictions_in_res(&ap_res, &ap_loc, report);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Check image XObject restrictions within a resource dict.
fn check_image_restrictions_in_res(
    res_dict: &Dict<'_>,
    location: &str,
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
            error_at(
                report,
                "6.2.8.2",
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
    }
}

// ─── §6.2.10 — Halftone and transfer function restrictions ──────────────────

/// Check halftone and transfer function restrictions in ExtGState (§6.2.10).
///
/// Scans page resources, annotation appearances, and Form XObjects.
pub fn check_halftone_and_transfer(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let loc = format!("page {}", page_idx + 1);

        // Page-level resources
        if let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) {
            check_halftone_in_extgstate(&res_dict, &loc, report);

            // Form XObjects in page resources
            if let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) {
                for (xo_name, _) in xobj_dict.entries() {
                    if let Some(xo) = xobj_dict.get::<Dict<'_>>(xo_name.as_ref()) {
                        if xo
                            .get::<Name>(keys::SUBTYPE)
                            .is_some_and(|s| s.as_ref() == b"Form")
                        {
                            if let Some(xo_res) = xo.get::<Dict<'_>>(keys::RESOURCES) {
                                let xo_loc = format!("{loc}/XObject");
                                check_halftone_in_extgstate(&xo_res, &xo_loc, report);
                            }
                        }
                    }
                }
            }
        }

        // Annotation appearances
        if let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) {
            for annot in annots.iter::<Dict<'_>>() {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    for (ap_key, _) in ap.entries() {
                        if let Some(ap_stream) = ap.get::<Dict<'_>>(ap_key.as_ref()) {
                            if let Some(ap_res) = ap_stream.get::<Dict<'_>>(keys::RESOURCES) {
                                let ap_loc = format!("{loc}/Annot/AP");
                                check_halftone_in_extgstate(&ap_res, &ap_loc, report);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Check halftone and transfer function restrictions in a resource dict's ExtGState.
fn check_halftone_in_extgstate(res_dict: &Dict<'_>, location: &str, report: &mut ComplianceReport) {
    let Some(gs_dict) = res_dict.get::<Dict<'_>>(keys::EXT_G_STATE) else {
        return;
    };
    for (gs_name, _) in gs_dict.entries() {
        let Some(gs) = gs_dict.get::<Dict<'_>>(gs_name.as_ref()) else {
            continue;
        };
        let gs_str = std::str::from_utf8(gs_name.as_ref()).unwrap_or("?");

        // §6.2.10: halftone type
        if let Some(ht_dict) = gs.get::<Dict<'_>>(b"HT" as &[u8]) {
            if let Some(ht_type) = ht_dict.get::<i32>(keys::TYPE) {
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
            // §6.2.10.5: TransferFunction in halftone must be /Identity or absent
            if let Some(tf) = ht_dict.get::<Object<'_>>(b"TransferFunction" as &[u8]) {
                let is_identity = matches!(tf, Object::Name(n) if n.as_ref() == b"Identity");
                if !is_identity {
                    error_at(
                        report,
                        "6.2.10.5",
                        format!("ExtGState {gs_str} halftone has custom /TransferFunction"),
                        location,
                    );
                }
            }
            // For Type 5 halftones, check sub-halftones too
            for (key, _) in ht_dict.entries() {
                if let Some(sub_ht) = ht_dict.get::<Dict<'_>>(key.as_ref()) {
                    if let Some(tf) = sub_ht.get::<Object<'_>>(b"TransferFunction" as &[u8]) {
                        let is_identity = matches!(tf, Object::Name(n) if n.as_ref() == b"Identity");
                        if !is_identity {
                            let kstr = std::str::from_utf8(key.as_ref()).unwrap_or("?");
                            error_at(
                                report,
                                "6.2.10.5",
                                format!("ExtGState {gs_str} halftone/{kstr} has custom /TransferFunction"),
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

        // §6.2.5: HTO key forbidden (PDF/A-4)
        if gs.contains_key(b"HTO" as &[u8]) {
            error_at(
                report,
                "6.2.5",
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
                error_at(
                    report,
                    "6.2.11",
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

        // Scan content stream numeric operands
        if let Some(content) = page.page_stream() {
            if scan_content_stream_reals(content, MAX_REAL) {
                error_at(
                    report,
                    rule,
                    "Content stream contains real value exceeding 32767",
                    format!("page {}", page_idx + 1),
                );
            }
        }
    }

    // Name objects must not exceed 127 bytes
    check_name_lengths(pdf, rule, report);

    // String objects must not exceed 65535 bytes
    check_string_lengths(pdf, rule, report);

    // Array objects must not exceed 8191 elements
    check_array_sizes(pdf, rule, report);

    // Dictionary objects must not exceed 4095 entries
    check_dict_sizes(pdf, rule, report);

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
fn check_name_lengths(pdf: &Pdf, rule: &str, report: &mut ComplianceReport) {
    for obj in pdf.objects() {
        if let Object::Dict(ref d) = obj {
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
        if let Object::Name(ref n) = obj {
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
    for obj in pdf.objects() {
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
        if let Some(params) = dict.get::<Dict<'_>>(keys::DECODE_PARMS) {
            if params.get::<Stream<'_>>(keys::JBIG2_GLOBALS).is_some() {
                error(report, "6.1.8", "JBIG2Decode with global segments");
            }
        }
    }
    if filter_name == keys::JPX_DECODE && pdfa_part == 1 {
        error(report, "6.1.9", "JPXDecode (JPEG2000) forbidden in PDF/A-1");
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
        let text = String::from_utf8_lossy(content);
        let loc = format!("page {}", page_idx + 1);
        // Find BI ... ID sequences and check /F or /Filter keys within them
        let mut pos = 0;
        while let Some(bi_pos) = text[pos..].find("BI") {
            let abs_bi = pos + bi_pos;
            // Make sure BI is at a word boundary
            let before_ok = abs_bi == 0
                || text.as_bytes()[abs_bi - 1].is_ascii_whitespace();
            let after_ok = abs_bi + 2 >= text.len()
                || text.as_bytes()[abs_bi + 2].is_ascii_whitespace()
                || text.as_bytes()[abs_bi + 2] == b'/';
            if !before_ok || !after_ok {
                pos = abs_bi + 2;
                continue;
            }
            // Find ID marker
            let Some(id_pos) = text[abs_bi..].find(" ID") else {
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
            error(
                report,
                "6.1.3",
                "Data found after last %%EOF marker",
            );
        }
    }
}

/// Check Widget annotations don't have /A or /AA keys (§6.4.1 test 1).
pub fn check_widget_no_action(pdf: &Pdf, report: &mut ComplianceReport) {
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
                    "6.4.1",
                    "Widget annotation contains /AA key (forbidden additional actions)",
                    format!("page {}", page_idx + 1),
                );
            }
        }
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

/// Check embedded file streams have /Type /EmbeddedFile (§6.1.7, §6.1.7.1).
pub fn check_embedded_file_streams(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        return;
    };
    let Some(names) = cat.get::<Dict<'_>>(keys::NAMES) else {
        return;
    };
    if let Some(ef_tree) = names.get::<Dict<'_>>(keys::EMBEDDED_FILES) {
        walk_name_tree_filespec(&ef_tree, report);
    }
}

fn walk_name_tree_filespec(node: &Dict<'_>, report: &mut ComplianceReport) {
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
                                    "6.1.7.1",
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
            walk_name_tree_filespec(&kid, report);
        }
    }
}

/// Check stream dicts for external file reference keys (§6.1.7.1 test 3).
///
/// Stream dictionaries must not contain /F, /FFilter, or /FDecodeParms keys
/// (these reference external files, forbidden in PDF/A).
pub fn check_stream_external_refs(pdf: &Pdf, report: &mut ComplianceReport) {
    for obj in pdf.objects() {
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
            if !is_embedded {
                if let Some(Object::String(_)) = dict.get::<Object<'_>>(keys::F) {
                    error(
                        report,
                        "6.1.7.1",
                        "Stream dictionary contains /F file specification (external file reference)",
                    );
                }
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
    if let Some(eol) = data.iter().position(|&b| b == b'\n' || b == b'\r') {
        let after = &data[eol..];
        let rest = after
            .iter()
            .position(|&b| b != b'\n' && b != b'\r')
            .map(|i| &after[i..])
            .unwrap_or(b"");
        if rest.starts_with(b"%") && rest.len() > 4 {
            let binary_count = rest[1..5].iter().filter(|&&b| b >= 128).count();
            if binary_count < 4 {
                error(
                    report,
                    "6.1.2",
                    "Binary comment must contain at least 4 bytes >= 128",
                );
            }
        } else if !rest.starts_with(b"%") {
            error(report, "6.1.2", "Missing binary comment after %PDF- header");
        }
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
                if let Some(issue) = validate_xref_section(&data[after..]) {
                    error(report, "6.1.3", issue);
                    return;
                }
            }
        }
        pos += 1;
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
        while pos < data.len() && data[pos].is_ascii_digit() {
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
        b"Named",
        b"GoToR",
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
                check_form_fields_actions(&fields, &forbidden, report);
            }
        }
        // Check outline (bookmark) actions
        if let Some(outlines) = cat.get::<Dict<'_>>(keys::OUTLINES) {
            check_outline_actions(&outlines, &forbidden, rule, report, 0);
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
                let is_widget = annot
                    .get::<Name>(keys::SUBTYPE)
                    .is_some_and(|s| s.as_ref() == keys::WIDGET);
                let r = if is_widget { "6.1.6.2" } else { "6.1.6.1" };
                check_aa_triggers(&aa, &forbidden, r, &aloc, report);
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
    report: &mut ComplianceReport,
) {
    for (idx, field) in fields.iter::<Dict<'_>>().enumerate() {
        let loc = format!("form field {}", idx + 1);
        if let Some(action) = field.get::<Dict<'_>>(keys::A) {
            check_action_recursive(&action, forbidden, "6.1.6.2", &loc, report);
        }
        if let Some(aa) = field.get::<Dict<'_>>(keys::AA) {
            check_aa_triggers(&aa, forbidden, "6.1.6.2", &loc, report);
        }
        if let Some(kids) = field.get::<Array<'_>>(keys::KIDS) {
            check_form_fields_actions(&kids, forbidden, report);
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
        if forbidden.contains(&bytes) {
            let name = std::str::from_utf8(bytes).unwrap_or("?");
            error_at(
                report,
                rule,
                format!("Forbidden action type: {name}"),
                location.to_string(),
            );
        }
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
        error(
            report,
            "6.1.11",
            "Optional content (OCProperties) forbidden in PDF/A-1",
        );
        return;
    }
    if let Some(as_arr) = ocprops.get::<Array<'_>>(keys::AS) {
        for as_dict in as_arr.iter::<Dict<'_>>() {
            if let Some(event) = as_dict.get::<Name>(b"Event" as &[u8]) {
                let evt = event.as_ref();
                if evt == b"Export" || evt == b"Print" {
                    let e = std::str::from_utf8(evt).unwrap_or("?");
                    error(report, "6.1.11", format!("OCProperties /AS event '{e}'"));
                }
            }
        }
    }
    // §6.10 (PDF/A-4): OCG configuration dicts must have /Name entry
    if pdfa_part >= 2 {
        let rule = if pdfa_part == 4 { "6.10" } else { "6.1.11" };
        if let Some(d_dict) = ocprops.get::<Dict<'_>>(b"D" as &[u8]) {
            if d_dict.get::<Object<'_>>(keys::NAME).is_none() {
                error(
                    report,
                    rule,
                    "Default OCG configuration dictionary missing /Name entry",
                );
            }
        }
        if let Some(configs) = ocprops.get::<Array<'_>>(b"Configs" as &[u8]) {
            for (idx, cfg) in configs.iter::<Dict<'_>>().enumerate() {
                if cfg.get::<Object<'_>>(keys::NAME).is_none() {
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
fn check_string_lengths(pdf: &Pdf, rule: &str, report: &mut ComplianceReport) {
    for obj in pdf.objects() {
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

/// Array objects must not exceed 8191 elements.
fn check_array_sizes(pdf: &Pdf, rule: &str, report: &mut ComplianceReport) {
    for obj in pdf.objects() {
        if let Object::Array(ref a) = obj {
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
    }
}

/// Dictionary objects must not exceed 4095 entries.
fn check_dict_sizes(pdf: &Pdf, rule: &str, report: &mut ComplianceReport) {
    for obj in pdf.objects() {
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

/// Scan content stream for numeric tokens exceeding max (§6.1.12).
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
        let cmyk_ok = cs_dict.get::<Object<'_>>(keys::DEFAULT_CMYK).is_some()
            || profile == Some(4);
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

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(group) = page_dict.get::<Dict<'_>>(b"Group" as &[u8]) else {
            continue;
        };
        let cs_res = &page.resources().color_spaces;
        let rgb_ok = cs_res.get::<Object<'_>>(keys::DEFAULT_RGB).is_some() || profile == Some(3);
        let cmyk_ok =
            cs_res.get::<Object<'_>>(keys::DEFAULT_CMYK).is_some() || profile == Some(4);
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

/// Check content stream operators are valid PDF operators (§6.2.10).
///
/// Operators not defined in PDF Reference are forbidden even if
/// bracketed by BX/EX compatibility markers.
pub fn check_undefined_operators(pdf: &Pdf, report: &mut ComplianceReport) {
    // All valid PDF content stream operators
    let valid_ops: &[&str] = &[
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
    ];

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let loc = format!("page {}", page_idx + 1);
        if let Some(content) = page.page_stream() {
            if scan_for_undefined_ops(content, valid_ops) {
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
                            if scan_for_undefined_ops(&decoded, valid_ops) {
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
                if scan_for_undefined_ops(&decoded, valid_ops) {
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

fn scan_for_undefined_ops(content: &[u8], valid_ops: &[&str]) -> bool {
    let text = String::from_utf8_lossy(content);
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
        // Check if it's a valid operator
        if !token.is_empty()
            && token
                .bytes()
                .all(|b| b.is_ascii_alphabetic() || b == b'*' || b == b'\'' || b == b'"')
            && !valid_ops.contains(&token)
        {
            return true;
        }
    }
    false
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
    // PDF/A-4 merges transparency checks into 6.2.9; parts 2/3 use 6.2.10
    let page_group_rule = if part == 4 { "6.2.9" } else { "6.2.10" };

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let has_page_group = page_dict
            .get::<Dict<'_>>(b"Group" as &[u8])
            .and_then(|g| g.get::<Name>(keys::S))
            .is_some_and(|s| s.as_ref() == b"Transparency");

        if !has_oi {
            // Check 1: existing transparency group must not use device CS
            if let Some(group) = page_dict.get::<Dict<'_>>(b"Group" as &[u8]) {
                if let Some(s) = group.get::<Name>(keys::S) {
                    if s.as_ref() == b"Transparency" {
                        if let Some(cs) = group.get::<Name>(keys::CS) {
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
                        } else {
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
            if let Some(group) = page_dict.get::<Dict<'_>>(b"Group" as &[u8]) {
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
        // Check appearance stream resources for transparency
        if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
            if let Some(n_stream) = ap.get::<Stream<'_>>(keys::N) {
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

/// Check for PostScript XObjects (forbidden in PDF/A, §6.2.9 test 3).
pub fn check_postscript_xobjects(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    let rule = if part == 4 { "6.2.9" } else { "6.2.10" };
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
    for_each_font(pdf, |name, font_dict, page_idx| {
        // Check direct font descriptor
        if let Some(desc) = font_dict.get::<Dict<'_>>(keys::FONT_DESC) {
            check_fontfile_subtype_match(&desc, name, page_idx, report);
            if is_subset_font(name) {
                let has_cidset = desc.get::<Stream<'_>>(keys::CID_SET).is_some();
                let has_charset = desc.get::<Object<'_>>(keys::CHAR_SET).is_some();
                if !has_cidset && !has_charset && part == 1 {
                    warning(
                        report,
                        "6.3.3",
                        format!("Subset font {name} missing CIDSet/CharSet"),
                    );
                }
            }
        }

        // Check CIDFont descendants
        if let Some(descendants) = font_dict.get::<Array<'_>>(keys::DESCENDANT_FONTS) {
            for desc_font in descendants.iter::<Dict<'_>>() {
                check_cidfont_descriptor_deep(&desc_font, name, page_idx, part, report);
            }
        }
    });
}

fn check_cidfont_descriptor_deep(
    cid_font: &Dict<'_>,
    name: &str,
    page_idx: usize,
    part: u8,
    report: &mut ComplianceReport,
) {
    let Some(desc) = cid_font.get::<Dict<'_>>(keys::FONT_DESC) else {
        return;
    };
    check_fontfile_subtype_match(&desc, name, page_idx, report);

    if is_subset_font(name) && part == 1 && desc.get::<Stream<'_>>(keys::CID_SET).is_none() {
        error_at(
            report,
            "6.3.3",
            format!("Subset CIDFont {name} missing required /CIDSet"),
            format!("page {}", page_idx + 1),
        );
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

/// Check ToUnicode CMap presence for non-symbolic fonts (§6.3.4).
pub fn check_tounicode_cmap(pdf: &Pdf, report: &mut ComplianceReport) {
    for_each_font(pdf, |name, font_dict, _page_idx| {
        if let Some(enc) = font_dict.get::<Name>(keys::ENCODING) {
            if enc.as_ref() == keys::IDENTITY_H || enc.as_ref() == keys::IDENTITY_V {
                return;
            }
        }

        if let Some(desc) = font_dict.get::<Dict<'_>>(keys::FONT_DESC) {
            if let Some(flags) = desc.get::<i32>(keys::FLAGS) {
                if flags & 0x04 != 0 {
                    return;
                }
            }
        }

        if let Some(subtype) = font_dict.get::<Name>(keys::SUBTYPE) {
            if subtype.as_ref() == b"Type0" {
                return;
            }
        }

        if !font_has_tounicode(font_dict) {
            warning(
                report,
                "6.3.4",
                format!("Non-symbolic font {name} missing /ToUnicode CMap"),
            );
        }
    });
}

/// Check ToUnicode CMap values for forbidden Unicode code points (§6.2.11.7.2).
/// U+0000, U+FEFF (BOM), and U+FFFE are forbidden.
pub fn check_tounicode_values(pdf: &Pdf, report: &mut ComplianceReport) {
    for_each_font(pdf, |name, font_dict, page_idx| {
        let Some(cmap_stream) = font_dict.get::<Stream<'_>>(keys::TO_UNICODE) else {
            return;
        };
        let Ok(data) = cmap_stream.decoded() else {
            return;
        };
        let text = String::from_utf8_lossy(&data);
        // Parse "beginbfchar" and "beginbfrange" sections for hex Unicode values
        // Format: <XXXX> <YYYY> where YYYY is the Unicode value
        for cap in text.split('<') {
            let Some(hex_end) = cap.find('>') else {
                continue;
            };
            let hex = &cap[..hex_end];
            if hex.len() != 4 {
                continue;
            }
            let Ok(val) = u16::from_str_radix(hex, 16) else {
                continue;
            };
            if val == 0x0000 || val == 0xFEFF || val == 0xFFFE {
                error_at(
                    report,
                    "6.2.11.7.2",
                    format!("Font {name} ToUnicode CMap contains forbidden U+{val:04X}"),
                    format!("page {}", page_idx + 1),
                );
                return; // one error per font is enough
            }
        }
    });
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

/// Validate symbolic TrueType font encoding (§6.3.6).
pub fn check_symbolic_truetype_encoding(pdf: &Pdf, report: &mut ComplianceReport) {
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
            // Symbolic TrueType must not have ANY /Encoding entry
            if font_dict.get::<Object<'_>>(keys::ENCODING).is_some() {
                error_at(
                    report,
                    "6.3.6",
                    format!("Symbolic TrueType font {name} should not have /Encoding"),
                    format!("page {}", page_idx + 1),
                );
            }
        }
    });
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

/// Validate CMap embedding for Type0 fonts (§6.3.8).
pub fn check_cmap_embedding(pdf: &Pdf, report: &mut ComplianceReport) {
    for_each_font(pdf, |name, font_dict, _page_idx| {
        let Some(subtype) = font_dict.get::<Name>(keys::SUBTYPE) else {
            return;
        };
        if subtype.as_ref() != b"Type0" {
            return;
        }

        if let Some(enc_name) = font_dict.get::<Name>(keys::ENCODING) {
            let enc = enc_name.as_ref();
            if enc == keys::IDENTITY_H || enc == keys::IDENTITY_V {
                return;
            }
            if enc.starts_with(b"90")
                || enc.starts_with(b"ETen")
                || enc.starts_with(b"UniGB")
                || enc.starts_with(b"UniJIS")
                || enc.starts_with(b"UniCNS")
                || enc.starts_with(b"UniKS")
                || enc.starts_with(b"GBK")
                || enc.starts_with(b"B5")
            {
                return;
            }

            let enc_str = std::str::from_utf8(enc).unwrap_or("?");
            warning(
                report,
                "6.3.8",
                format!("Type0 font {name} uses CMap {enc_str}; verify it is embedded"),
            );
        }
    });
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

            // Appearance dict should not have /D or /R entries for Widget annotations (§6.5.3 test 2)
            if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                if ap.get::<Object<'_>>(b"D" as &[u8]).is_some() {
                    error_at(
                        report,
                        "6.5.3",
                        format!("{subtype_name} annotation /AP contains forbidden /D (down) appearance"),
                        format!("page {}", page_idx + 1),
                    );
                }
                if ap.get::<Object<'_>>(b"R" as &[u8]).is_some() {
                    error_at(
                        report,
                        "6.5.3",
                        format!("{subtype_name} annotation /AP contains forbidden /R (rollover) appearance"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }

            // Widget/Btn: /AP /N must be a subdictionary, not a stream (§6.5.3 test 5)
            let is_widget = annot
                .get::<Name>(keys::SUBTYPE)
                .is_some_and(|s| s.as_ref() == b"Widget");
            let is_btn = annot
                .get::<Name>(keys::FT)
                .is_some_and(|s| s.as_ref() == b"Btn");
            if is_widget && is_btn {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    // /N should be a dict (with state names as keys → streams)
                    // NOT a single stream
                    if ap.get::<Stream<'_>>(keys::N).is_some()
                        && ap.get::<Dict<'_>>(keys::N).is_none()
                    {
                        error_at(
                            report,
                            "6.5.3",
                            "Widget/Btn annotation /AP /N must be a subdictionary, not a stream",
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
            }
        }
    }
}

/// Deep annotation subtype validation (§6.5.2).
pub fn check_annotation_subtypes_deep(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    let forbidden_all: &[&[u8]] = &[b"Sound", b"Movie", b"3D"];
    // PDF/A-4 (ISO 19005-4 §6.3.1) also forbids Screen, RichMedia, FileAttachment
    let forbidden_pdfa4: &[&[u8]] = &[b"Screen", b"RichMedia", b"FileAttachment"];

    // PDF/A-4 uses 6.3.1 (normalized to 6.5.1); other parts use 6.5.2
    let rule = if part == 4 { "6.5.1" } else { "6.5.2" };

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
        }
    }
}

/// Deep annotation flag validation per PDF/A part (§6.5.1/§6.5.2).
///
/// PDF/A-4 uses clause 6.3.2 (normalized to 6.5.2); parts 2/3 use 6.5.1.
pub fn check_annotation_flags_deep(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    // PDF/A-4: 6.3.2 → normalized 6.5.2; parts 2/3: 6.5.1
    let rule = if part == 4 { "6.5.2" } else { "6.5.1" };

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
pub fn check_soft_mask_structure(pdf: &Pdf, report: &mut ComplianceReport) {
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

fn check_field_appearances(fields: &Array<'_>, report: &mut ComplianceReport, depth: usize) {
    if depth > 50 {
        return;
    }
    for (idx, field) in fields.iter::<Dict<'_>>().enumerate() {
        // Widget annotations (or fields with widget characteristics) need /AP
        let is_widget = field
            .get::<Name>(keys::SUBTYPE)
            .is_some_and(|s| s.as_ref() == keys::WIDGET);
        let has_ft = field.get::<Name>(b"FT" as &[u8]).is_some();

        if (is_widget || has_ft) && field.get::<Dict<'_>>(keys::AP).is_none() {
            error_at(
                report,
                "6.9",
                format!("Form field {idx} missing required /AP (appearance dictionary)"),
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
                            for key in [
                                &b"DigestLocation"[..],
                                b"DigestMethod",
                                b"DigestValue",
                            ] {
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

        // Non-standard type must be in RoleMap
        let mapped = role_map
            .as_ref()
            .and_then(|rm| rm.get::<Name>(t.as_slice()));

        if mapped.is_none() {
            let type_str = std::str::from_utf8(t).unwrap_or("?");
            error(
                report,
                "6.12",
                format!(
                    "Structure element type '{type_str}' has no role mapping to a standard type"
                ),
            );
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

// ─── §6.1.13 — Name length limit ────────────────────────────────────────────

/// Check implementation limits (§6.1.13): name length ≤ 127 and string length ≤ 32767.
///
/// Non-recursive: checks top-level objects and one level of dict/array nesting
/// to avoid OOM from shared indirect reference resolution.
pub fn check_name_length_limit(pdf: &Pdf, report: &mut ComplianceReport) {
    let mut long_name = false;
    let mut long_string = false;
    for obj in pdf.objects() {
        if !long_name && check_name_length_obj(&obj) {
            long_name = true;
        }
        if !long_string && check_string_length_obj(&obj) {
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
    match obj {
        Object::Name(n) => n.as_ref().len() > 127,
        Object::Dict(dict) => {
            for (key, _) in dict.entries() {
                if key.as_ref().len() > 127 {
                    return true;
                }
            }
            // Check name values one level deep
            for (key, _) in dict.entries() {
                if let Some(Object::Name(n)) = dict.get::<Object<'_>>(key.as_ref()) {
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
    match obj {
        Object::String(s) => s.as_ref().len() > 32767,
        Object::Dict(dict) => {
            for (key, _) in dict.entries() {
                if let Some(Object::String(s)) = dict.get::<Object<'_>>(key.as_ref()) {
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

// ─── §6.1.12 — Real value limits ────────────────────────────────────────────

/// Check that real values are within the PDF/A limits (§6.1.12).
///
/// Absolute real values must be <= 32767.0.
/// Non-recursive: checks top-level objects and one level of dict/array nesting.
pub fn check_real_value_limits(pdf: &Pdf, report: &mut ComplianceReport) {
    let mut found = false;
    for obj in pdf.objects() {
        if check_real_limit_obj(&obj) {
            found = true;
            break;
        }
    }
    if found {
        error(report, "6.1.12", "Real value out of range (exceeds 32767)");
    }
}

fn check_real_limit_obj(obj: &Object<'_>) -> bool {
    match obj {
        Object::Number(n) => n.as_f64().abs() > 32767.0,
        Object::Dict(dict) => {
            for (key, _) in dict.entries() {
                if let Some(Object::Number(n)) = dict.get::<Object<'_>>(key.as_ref()) {
                    if n.as_f64().abs() > 32767.0 {
                        return true;
                    }
                }
            }
            false
        }
        Object::Array(arr) => {
            for item in arr.iter::<Object<'_>>() {
                if let Object::Number(n) = &item {
                    if n.as_f64().abs() > 32767.0 {
                        return true;
                    }
                }
            }
            false
        }
        Object::Stream(s) => {
            for (key, _) in s.dict().entries() {
                if let Some(Object::Number(n)) = s.dict().get::<Object<'_>>(key.as_ref()) {
                    if n.as_f64().abs() > 32767.0 {
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
    for obj in pdf.objects() {
        let Object::Stream(s) = obj else { continue };
        let dict = s.dict();
        // Only check font file streams (identified by having Subtype in
        // a font descriptor's FontFile3 stream)
        let Some(subtype) = dict.get::<Name>(keys::SUBTYPE) else {
            continue;
        };
        let st = subtype.as_ref();
        // FontFile3 streams have subtypes like Type1C, CIDFontType0C, OpenType
        let is_font_stream = st == b"Type1C"
            || st == b"CIDFontType0C"
            || st == b"OpenType";
        if !is_font_stream {
            continue;
        }
        // PDF/A-1 only allows Type1C and CIDFontType0C
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
        // would need to inherit them
        if !has_own_resources && page.page_stream().is_some() {
            // page.resources() returns resolved (possibly inherited) resources
            // If the resolved resources have entries, they must be inherited
            let res = page.resources();
            let has_any_resource = res.fonts.entries().next().is_some()
                || res.x_objects.entries().next().is_some()
                || res.ext_g_states.entries().next().is_some()
                || res.color_spaces.entries().next().is_some()
                || res.patterns.entries().next().is_some()
                || res.shadings.entries().next().is_some();

            if has_any_resource {
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
                    // Form XObjects should have their own Resources if they use any
                    if !dict.contains_key(keys::RESOURCES) {
                        if let Ok(decoded) = stream.decoded() {
                            // Check for any resource-using operators
                            let needs_resources = stream_references_resources(&decoded);
                            if needs_resources {
                                let xn = std::str::from_utf8(xname.as_ref()).unwrap_or("?");
                                error_at(
                                    report,
                                    "6.2.2",
                                    format!("Form XObject {xn} references resources but has no explicit Resources dictionary"),
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
/// Maps operators to their resource sub-dictionary:
/// - Tf → Font, Do → XObject, gs → ExtGState, cs/CS → ColorSpace, sh → Shading
pub fn check_resource_names_exist(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let Some(content) = page.page_stream() else {
            continue;
        };
        let res = page.resources();
        let loc = format!("page {}", page_idx + 1);
        check_resource_refs_in_stream(content, &res.fonts, &res.x_objects, &res.ext_g_states,
            &res.color_spaces, &res.shadings, &res.patterns, &loc, report);
    }
}

#[allow(clippy::too_many_arguments)]
fn check_resource_refs_in_stream(
    content: &[u8],
    fonts: &Dict<'_>,
    xobjects: &Dict<'_>,
    extgstates: &Dict<'_>,
    colorspaces: &Dict<'_>,
    shadings: &Dict<'_>,
    patterns: &Dict<'_>,
    location: &str,
    report: &mut ComplianceReport,
) {
    let text = String::from_utf8_lossy(content);
    let tokens: Vec<&str> = text.split_ascii_whitespace().collect();
    let mut i = 0;
    let mut in_inline = false;
    while i < tokens.len() {
        let tok = tokens[i];
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

        // Match operator and check the preceding name operand
        match tok {
            "Tf" => {
                // /Name size Tf — name is 2 tokens before
                if i >= 2 {
                    if let Some(name) = tokens[i - 2].strip_prefix('/') {
                        if !fonts.contains_key(name.as_bytes()) {
                            error_at(
                                report,
                                "6.2.2",
                                format!("Font /{name} referenced but not in Resources/Font"),
                                location.to_string(),
                            );
                        }
                    }
                }
            }
            "Do" | "gs" | "sh" => {
                // /Name op — name is 1 token before
                if i >= 1 {
                    if let Some(name) = tokens[i - 1].strip_prefix('/') {
                        let (dict, cat) = match tok {
                            "Do" => (xobjects, "XObject"),
                            "gs" => (extgstates, "ExtGState"),
                            "sh" => (shadings, "Shading"),
                            _ => unreachable!(),
                        };
                        if !dict.contains_key(name.as_bytes()) {
                            error_at(
                                report,
                                "6.2.2",
                                format!("/{name} referenced by {tok} but not in Resources/{cat}"),
                                location.to_string(),
                            );
                        }
                    }
                }
            }
            "cs" | "CS" => {
                // /Name cs|CS — name is 1 token before
                if i >= 1 {
                    if let Some(name) = tokens[i - 1].strip_prefix('/') {
                        // Built-in color spaces don't need Resources entry
                        let builtin = matches!(
                            name,
                            "DeviceGray" | "DeviceRGB" | "DeviceCMYK" | "Pattern"
                        );
                        if !builtin && !colorspaces.contains_key(name.as_bytes()) {
                            error_at(
                                report,
                                "6.2.2",
                                format!("ColorSpace /{name} referenced but not in Resources/ColorSpace"),
                                location.to_string(),
                            );
                        }
                    }
                }
            }
            "scn" | "SCN" => {
                // For pattern color space: /Name scn|SCN — name is 1 token before
                // Only check if the operand is a name (starts with /), not a number
                if i >= 1 {
                    if let Some(name) = tokens[i - 1].strip_prefix('/') {
                        // This is a pattern name reference
                        if !patterns.contains_key(name.as_bytes())
                            && !colorspaces.contains_key(name.as_bytes())
                        {
                            error_at(
                                report,
                                "6.2.2",
                                format!("/{name} referenced by {tok} but not in Resources"),
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

    if part == 1 {
        // PDF/A-1: trailer must contain /ID
        let has_id = if let Some(trailer_pos) = data.windows(7).rposition(|w| w == b"trailer") {
            let end = data.len().min(trailer_pos + 2000);
            let trailer_region = &data[trailer_pos..end];
            trailer_region.windows(3).any(|w| w == b"/ID")
        } else {
            // Cross-reference stream — check for /ID in xref stream dicts
            pdf.objects().into_iter().any(|obj| {
                if let Object::Stream(s) = obj {
                    let dict = s.dict();
                    dict.get::<Name>(keys::TYPE)
                        .is_some_and(|t| t.as_ref() == keys::XREF)
                        && dict.contains_key(b"ID" as &[u8])
                } else {
                    false
                }
            })
        };
        if !has_id {
            error(report, "6.1.3", "Trailer dictionary missing required /ID key");
        }
        return;
    }

    if part == 4 {
        check_trailer_info_key(pdf, report);
    }
}

/// Check that Info key is not present in trailer for PDF/A-4 (§6.1.3).
///
/// Unless there's a PieceInfo entry in the document catalog.
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
    if let Some(cat) = catalog(pdf) {
        if cat.contains_key(b"PieceInfo" as &[u8]) {
            return;
        }
    }

    error(
        report,
        "6.1.3",
        "Info key present in trailer without PieceInfo in catalog (forbidden in PDF/A-4)",
    );
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

        // stream keyword must be followed by \r\n or \n
        let data_start = abs_stream + 6; // skip "stream"
        if data_start >= len {
            break;
        }
        let data_start = if data[data_start] == b'\r' && data_start + 1 < len && data[data_start + 1] == b'\n' {
            data_start + 2
        } else if data[data_start] == b'\n' {
            data_start + 1
        } else {
            pos = abs_stream + 6;
            continue;
        };

        // Find "endstream" after the stream data
        let search_from = if data_start + 10 < len { data_start } else { break };
        let remaining = &data[search_from..];
        let Some(endstream_off) = find_keyword(remaining, b"endstream") else {
            break;
        };
        let abs_endstream = search_from + endstream_off;

        // Actual length is bytes between data_start and endstream
        // endstream may be preceded by EOL (\r\n or \n or \r)
        let mut actual_end = abs_endstream;
        if actual_end > data_start && data[actual_end - 1] == b'\n' {
            actual_end -= 1;
            if actual_end > data_start && data[actual_end - 1] == b'\r' {
                actual_end -= 1;
            }
        } else if actual_end > data_start && data[actual_end - 1] == b'\r' {
            actual_end -= 1;
        }
        let actual_len = actual_end - data_start;

        // Find the /Length value by scanning backwards from "stream" to find the dict
        let declared = find_length_value(data, abs_stream);
        if let Some(declared_len) = declared {
            if declared_len != actual_len {
                error(
                    report,
                    "6.1.7",
                    format!(
                        "Stream Length mismatch: declared {declared_len}, actual {actual_len}"
                    ),
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
        if let Some(off) = region[search..].windows(length_key.len()).position(|w| w == length_key) {
            last_idx = Some(search + off);
            search = search + off + length_key.len();
        } else {
            break;
        }
    }
    let idx = last_idx?;
    let after = &region[idx + length_key.len()..];
    // Skip whitespace
    let skip = after.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(after.len());
    let after = &after[skip..];
    // Parse digits
    let end = after.iter().position(|b| !b.is_ascii_digit()).unwrap_or(after.len());
    if end == 0 {
        return None; // /Length might be an indirect reference
    }
    std::str::from_utf8(&after[..end]).ok()?.parse().ok()
}

// ─── §6.1.8 / §6.1.9 — Object syntax spacing checks ────────────────────────

/// Check spacing around obj/endobj keywords (§6.1.8, §6.1.9).
///
/// Requirements:
/// - Object number and generation number separated by single white-space
/// - Generation number and "obj" separated by single white-space
/// - "obj" followed by EOL marker
/// - "endobj" preceded and followed by EOL marker
pub fn check_object_syntax_spacing(pdf: &Pdf, report: &mut ComplianceReport) {
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
                "6.1.8",
                format!("Extra spacing before 'obj' keyword ({ws1_count} whitespace chars, expected 1)"),
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
                "6.1.8",
                format!("Extra spacing between object number and generation number ({ws2_count} whitespace chars, expected 1)"),
            );
            return;
        }

        // Check "obj" is followed by EOL or whitespace
        let after_obj = abs_obj + 3;
        if after_obj < len {
            let c = data[after_obj];
            if c != b'\n' && c != b'\r' && c != b' ' && c != b'\t' {
                error(report, "6.1.8", "Keyword 'obj' not followed by proper whitespace/EOL");
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
                error(report, "6.1.8", "Keyword 'endobj' not preceded by EOL marker");
                return;
            }
        }

        // endobj must be followed by EOL or EOF
        let after = abs_eobj + 6;
        if after < len {
            let c = data[after];
            if c != b'\n' && c != b'\r' {
                error(report, "6.1.8", "Keyword 'endobj' not followed by EOL marker");
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

    // Check if extension schemas are present
    if !xmp_str.contains("pdfaExtension:schemas") && !xmp_str.contains("pdfaSchema:") {
        return; // No extension schemas — nothing to validate
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
                        error(
                            report,
                            "6.7.8",
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
        "Text", "URI", "URL", "Boolean", "Integer", "Real",
        "Date", "MIMEType", "AgentName", "RenditionClass",
        "ResourceEvent", "ResourceRef", "Version", "Rational",
        "Lang Alt", "Bag Text", "Seq Text", "Bag ProperName",
        "GUID", "Locale", "XPath", "Part", "GPSCoordinate",
        "bag Text", "seq Text", "Bag Choice", "InternalRef",
        "ExternalRef", "Field", "Dimensions",
    ];

    for (tag_start, tag_end) in find_xml_element_values(xmp, "pdfaProperty:valueType") {
        let val = xmp[tag_start..tag_end].trim();
        if val.is_empty() {
            continue;
        }
        if !standard_types.contains(&val) {
            let type_defined = find_xml_element_values(xmp, "pdfaType:type")
                .any(|(s, e)| xmp[s..e].trim() == val);
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
            if dict.get::<Name>(keys::SUBTYPE).is_none_or(|s| s.as_ref() != keys::IMAGE) {
                continue;
            }

            if let Some(intent) = dict.get::<Name>(b"Intent" as &[u8]) {
                if !valid_intents.iter().any(|v| *v == intent.as_ref()) {
                    let intent_str = std::str::from_utf8(intent.as_ref()).unwrap_or("?");
                    error_at(
                        report,
                        "6.2.5",
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
        // Names array is [name1 spec1 name2 spec2 ...]
        // Iterate through the file spec dicts
        for spec in names_arr.iter::<Dict<'_>>() {
            let has_ef = spec.contains_key(keys::EF);
            if !has_ef {
                continue;
            }
            let rule = if part == 4 { "6.9" } else { "6.8" };
            if !spec.contains_key(keys::F) {
                error(report, rule, "File specification missing /F key");
            }
            if !spec.contains_key(b"UF" as &[u8]) {
                error(report, rule, "File specification missing /UF key");
            }
            if part >= 3 && !spec.contains_key(b"AFRelationship" as &[u8]) {
                error(report, rule, "File specification missing /AFRelationship key");
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
