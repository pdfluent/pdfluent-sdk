//! Shared compliance checking helpers.

use crate::{ComplianceIssue, ComplianceReport, Severity};
use pdf_syntax::object::dict::keys;
use pdf_syntax::object::{Array, Dict, Name, Object, Stream};
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
        let res_dict = page_dict.get::<Dict<'_>>(keys::RESOURCES);

        // Check if Default color spaces are defined
        let has_default_rgb = has_default_cs(res_dict.as_ref(), keys::DEFAULT_RGB);
        let has_default_cmyk = has_default_cs(res_dict.as_ref(), keys::DEFAULT_CMYK);
        let has_default_gray = has_default_cs(res_dict.as_ref(), keys::DEFAULT_GRAY);

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

        // Scan Form XObject content streams
        if let Some(ref rd) = res_dict {
            scan_form_xobjects_device_colors(rd, rgb_ok, cmyk_ok, gray_ok, &loc, report);
        }

        // Scan annotation appearance streams
        if let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) {
            for annot in annots.iter::<Dict<'_>>() {
                if let Some(ap) = annot.get::<Dict<'_>>(keys::AP) {
                    scan_appearance_dict_colors(&ap, rgb_ok, cmyk_ok, gray_ok, &loc, report);
                }
            }
        }

        // Scan Shading resources for device color spaces
        if let Some(ref rd) = res_dict {
            scan_shading_device_colors(rd, rgb_ok, cmyk_ok, gray_ok, &loc, report);
            scan_pattern_device_colors(rd, rgb_ok, cmyk_ok, gray_ok, &loc, report);
        }

        // Scan Type 3 font CharProcs for device color operators
        if let Some(ref rd) = res_dict {
            scan_type3_font_charprocs(rd, rgb_ok, cmyk_ok, gray_ok, &loc, report);
        }

        // Scan SMask Form XObjects in ExtGState for device color operators
        if let Some(ref rd) = res_dict {
            scan_smask_device_colors(rd, rgb_ok, cmyk_ok, gray_ok, &loc, report);
        }

        // Also check ColorSpace resources for direct device CS references
        if let Some(cs_dict) = res_dict
            .as_ref()
            .and_then(|r| r.get::<Dict<'_>>(keys::COLORSPACE))
        {
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
                }
            }
        }
    }
}

/// Check if a resource dict has a Default color space defined.
fn has_default_cs(res: Option<&Dict<'_>>, key: &[u8]) -> bool {
    res.and_then(|r| r.get::<Dict<'_>>(keys::COLORSPACE))
        .and_then(|cs| cs.get::<Object<'_>>(key))
        .is_some()
}

/// Scan Form XObjects in a resource dict for device color operators.
fn scan_form_xobjects_device_colors(
    res_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
    base_loc: &str,
    report: &mut ComplianceReport,
) {
    let Some(xobj_dict) = res_dict.get::<Dict<'_>>(keys::XOBJECT) else {
        return;
    };
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
    for (name, _) in shading_dict.entries() {
        let sname = std::str::from_utf8(name.as_ref()).unwrap_or("?");
        // Shading can be a dict or stream — both have /ColorSpace
        if let Some(sh) = shading_dict.get::<Dict<'_>>(name.as_ref()) {
            if let Some(cs) = sh.get::<Name>(keys::COLORSPACE) {
                let cs_bytes = cs.as_ref();
                report_device_cs_name(
                    cs_bytes,
                    rgb_ok,
                    cmyk_ok,
                    gray_ok,
                    &format!("{base_loc} Shading {sname}"),
                    report,
                );
            }
        } else if let Some(sh_stream) = shading_dict.get::<Stream<'_>>(name.as_ref()) {
            let sh_dict = sh_stream.dict();
            if let Some(cs) = sh_dict.get::<Name>(keys::COLORSPACE) {
                let cs_bytes = cs.as_ref();
                report_device_cs_name(
                    cs_bytes,
                    rgb_ok,
                    cmyk_ok,
                    gray_ok,
                    &format!("{base_loc} Shading {sname}"),
                    report,
                );
            }
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
    for (name, _) in pat_dict.entries() {
        let pname = std::str::from_utf8(name.as_ref()).unwrap_or("?");
        // Type 2 patterns (shading patterns) have /Shading dict with /ColorSpace
        if let Some(pat) = pat_dict.get::<Dict<'_>>(name.as_ref()) {
            if let Some(shading) = pat.get::<Dict<'_>>(b"Shading" as &[u8]) {
                if let Some(cs) = shading.get::<Name>(keys::COLORSPACE) {
                    let cs_bytes = cs.as_ref();
                    report_device_cs_name(
                        cs_bytes,
                        rgb_ok,
                        cmyk_ok,
                        gray_ok,
                        &format!("{base_loc} Pattern {pname}"),
                        report,
                    );
                }
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
fn scan_type3_font_charprocs(
    res_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
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
                }
            }
        }
    }
}

/// Scan ExtGState /SMask Form XObjects for device color operators (§6.2.4.3).
fn scan_smask_device_colors(
    res_dict: &Dict<'_>,
    rgb_ok: bool,
    cmyk_ok: bool,
    gray_ok: bool,
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
            "DeviceRGB in Shading/Pattern without DefaultRGB or matching OutputIntent",
            location.to_string(),
        );
    }
    if !cmyk_ok && cs_bytes == b"DeviceCMYK" {
        error_at(
            report,
            "6.2.4.3",
            "DeviceCMYK in Shading/Pattern without DefaultCMYK or matching OutputIntent",
            location.to_string(),
        );
    }
    if !gray_ok && cs_bytes == b"DeviceGray" {
        error_at(
            report,
            "6.2.4.3",
            "DeviceGray in Shading/Pattern without DefaultGray or OutputIntent",
            location.to_string(),
        );
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

/// Check Info dict / XMP metadata consistency (§6.7.3).
///
/// Properties in /Info dict must have matching values in XMP metadata.
#[allow(dead_code)]
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
}

/// Check annotation dictionaries have required /F key and correct flags (§6.3.2).
///
/// All annotations (except Popup) must have /F key. When present, Print flag
/// must be set, Hidden/Invisible/ToggleNoView/NoView flags must be clear.
pub fn check_annotation_flags(pdf: &Pdf, report: &mut ComplianceReport) {
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
                        "6.3.2",
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
                    "6.3.2",
                    format!("{subtype_name} annotation missing required /F key"),
                    format!("page {}", page_idx + 1),
                );
            }
        }
    }
}

/// Check annotation /C and /IC color arrays (§6.5.3).
///
/// In PDF/A-1, annotation /C (color) and /IC (interior color) arrays must not
/// use device-dependent colors when no matching OutputIntent is present.
/// Annotations must not contain /C or /IC unless the OutputIntent profile
/// matches the color space.
pub fn check_annotation_color_arrays(pdf: &Pdf, report: &mut ComplianceReport) {
    let has_intent = has_output_intent(pdf);
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            let has_c = annot.get::<Array<'_>>(b"C" as &[u8]).is_some();
            let has_ic = annot.get::<Array<'_>>(b"IC" as &[u8]).is_some();
            if (has_c || has_ic) && !has_intent {
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
                        "{subtype_name} annotation has {which} color array without matching OutputIntent"
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

// ─── §6.2.5 — Rendering intent validation ───────────────────────────────────

/// Check rendering intents are valid (§6.2.5).
pub fn check_rendering_intents(pdf: &Pdf, report: &mut ComplianceReport) {
    let valid_intents: &[&[u8]] = &[
        b"RelativeColorimetric",
        b"AbsoluteColorimetric",
        b"Perceptual",
        b"Saturation",
    ];

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        if let Some(content) = page.page_stream() {
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
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
            }
        }

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
            if let Some(ri) = gs.get::<Name>(keys::RI) {
                if !valid_intents.iter().any(|v| *v == ri.as_ref()) {
                    let ri_str = std::str::from_utf8(ri.as_ref()).unwrap_or("?");
                    error_at(
                        report,
                        "6.2.5",
                        format!("Invalid rendering intent '{ri_str}' in ExtGState"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }
}

// ─── §6.2.8 — Image XObject restrictions ────────────────────────────────────

/// Check Image XObject restrictions (§6.2.8).
pub fn check_image_xobjects(pdf: &Pdf, report: &mut ComplianceReport) {
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

            if let Some(subtype) = dict.get::<Name>(keys::SUBTYPE) {
                if subtype.as_ref() != keys::IMAGE {
                    continue;
                }
            } else {
                continue;
            }

            let xobj_name = std::str::from_utf8(name.as_ref()).unwrap_or("?");

            if let Some(Object::Boolean(true)) = dict.get::<Object<'_>>(keys::INTERPOLATE) {
                error_at(
                    report,
                    "6.2.8.1",
                    format!("Image XObject {xobj_name} has /Interpolate true"),
                    format!("page {}", page_idx + 1),
                );
            }

            if dict.contains_key(b"Alternates" as &[u8]) {
                error_at(
                    report,
                    "6.2.8.2",
                    format!("Image XObject {xobj_name} contains forbidden /Alternates key"),
                    format!("page {}", page_idx + 1),
                );
            }

            if dict.contains_key(keys::OPI) {
                error_at(
                    report,
                    "6.2.8.3",
                    format!("Image XObject {xobj_name} contains forbidden /OPI key"),
                    format!("page {}", page_idx + 1),
                );
            }
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
                        if xo.get::<Name>(keys::SUBTYPE).is_some_and(|s| s.as_ref() == b"Form") {
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
                            if let Some(ap_res) =
                                ap_stream.get::<Dict<'_>>(keys::RESOURCES)
                            {
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
fn check_halftone_in_extgstate(
    res_dict: &Dict<'_>,
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
                    format!(
                        "ExtGState {gs_str} halftone contains forbidden /HalftoneName"
                    ),
                    location,
                );
            }
        }

        // §6.2.10.5: TR forbidden
        if gs.contains_key(keys::TR) {
            error_at(
                report,
                "6.2.10.5",
                format!(
                    "ExtGState {gs_str} contains forbidden /TR (transfer function)"
                ),
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
    }
}

// ─── §6.2.10.6-9 — ExtGState blend mode and soft mask ───────────────────────

/// Check ExtGState blend mode and soft mask restrictions (§6.2.10.6-9).
pub fn check_extgstate_restrictions(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
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
        }
    }
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

// ─── Batch 3: §6.1.x and §6.6.1 — File structure, actions, streams ─────────

/// Check all page boundary boxes including BleedBox, TrimBox, ArtBox (§6.1.13).
pub fn check_all_page_boundaries(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let boxes: &[(&[u8], &str)] = &[
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
        error(report, "6.1.8", "LZWDecode filter is forbidden in PDF/A");
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

/// Check PDF header binary comment (§6.1.2).
pub fn check_file_header(pdf: &Pdf, report: &mut ComplianceReport) {
    let data = pdf.data().as_ref();
    if !data.starts_with(b"%PDF-") {
        error(report, "6.1.2", "File does not start with %PDF- header");
        return;
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
        let res_dict = page_dict.get::<Dict<'_>>(keys::RESOURCES);

        let rgb_ok = has_default_cs(res_dict.as_ref(), keys::DEFAULT_RGB) || profile == Some(3);
        let cmyk_ok = has_default_cs(res_dict.as_ref(), keys::DEFAULT_CMYK) || profile == Some(4);
        let gray_ok = has_default_cs(res_dict.as_ref(), keys::DEFAULT_GRAY) || has_intent;

        let Some(ref rd) = res_dict else { continue };
        let Some(xobj_dict) = rd.get::<Dict<'_>>(keys::XOBJECT) else {
            continue;
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

            if let Some(cs_name) = dict.get::<Name>(keys::COLORSPACE) {
                let cs = cs_name.as_ref();
                let xobj_name = std::str::from_utf8(name.as_ref()).unwrap_or("?");
                if cs == keys::DEVICE_RGB && !rgb_ok {
                    error_at(
                        report,
                        "6.2.4.3",
                        format!("Image {xobj_name} uses DeviceRGB without DefaultRGB or matching OutputIntent"),
                        format!("page {}", page_idx + 1),
                    );
                }
                if cs == b"DeviceCMYK" && !cmyk_ok {
                    error_at(
                        report,
                        "6.2.4.3",
                        format!("Image {xobj_name} uses DeviceCMYK without DefaultCMYK or matching OutputIntent"),
                        format!("page {}", page_idx + 1),
                    );
                }
                if cs == b"DeviceGray" && !gray_ok {
                    error_at(
                        report,
                        "6.2.4.3",
                        format!(
                            "Image {xobj_name} uses DeviceGray without DefaultGray or OutputIntent"
                        ),
                        format!("page {}", page_idx + 1),
                    );
                }
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
        let Some(content) = page.page_stream() else {
            continue;
        };
        if scan_for_undefined_ops(content, valid_ops) {
            error_at(
                report,
                "6.2.10",
                "Content stream contains undefined operator",
                format!("page {}", page_idx + 1),
            );
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

        // Check 2: pages using transparency features need /Group
        if !has_page_group && page_uses_transparency(page_dict) {
            error_at(
                report,
                page_group_rule,
                "Page uses transparency but has no /Group entry",
                format!("page {}", page_idx + 1),
            );
        }
    }
}

/// Check if a page uses transparency features via ExtGState resources.
///
/// Looks for: SMask != /None, CA < 1.0, ca < 1.0, BM != /Normal and != /Compatible.
fn page_uses_transparency(page_dict: &Dict<'_>) -> bool {
    let Some(res) = page_dict.get::<Dict<'_>>(keys::RESOURCES) else {
        return false;
    };
    let Some(gs_dict) = res.get::<Dict<'_>>(keys::EXT_G_STATE) else {
        return false;
    };

    for (name, _) in gs_dict.entries() {
        let Some(gs) = gs_dict.get::<Dict<'_>>(name.as_ref()) else {
            continue;
        };

        // Check SMask (not /None)
        if let Some(smask) = gs.get::<Object<'_>>(keys::SMASK) {
            match smask {
                Object::Name(n) if n.as_ref() == b"None" => {}
                Object::Name(_) => return true,
                Object::Dict(_) => return true,
                _ => {}
            }
        }

        // Check BM (blend mode) — not Normal/Compatible means transparency
        if let Some(bm) = gs.get::<Name>(keys::BM) {
            let bm_bytes = bm.as_ref();
            if bm_bytes != b"Normal" && bm_bytes != b"Compatible" {
                return true;
            }
        }

        // Check CA (stroking alpha) < 1.0
        if let Some(Object::Number(ca)) = gs.get::<Object<'_>>(b"CA" as &[u8]) {
            if ca.as_f64() < 1.0 {
                return true;
            }
        }

        // Check ca (non-stroking alpha) < 1.0
        if let Some(Object::Number(ca)) = gs.get::<Object<'_>>(b"ca" as &[u8]) {
            if ca.as_f64() < 1.0 {
                return true;
            }
        }
    }

    false
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
            if let Some(enc_name) = font_dict.get::<Name>(keys::ENCODING) {
                let enc = enc_name.as_ref();
                if enc == keys::WIN_ANSI_ENCODING
                    || enc == keys::MAC_ROMAN_ENCODING
                    || enc == keys::STANDARD_ENCODING
                    || enc == keys::MAC_EXPERT_ENCODING
                    || enc == keys::PDF_DOC_ENCODING
                {
                    error_at(
                        report,
                        "6.3.6",
                        format!("Symbolic TrueType font {name} should not have /Encoding"),
                        format!("page {}", page_idx + 1),
                    );
                }
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
        }
    }
}

/// Deep annotation subtype validation (§6.5.2).
pub fn check_annotation_subtypes_deep(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
    let forbidden_all: &[&[u8]] = &[b"Sound", b"Movie", b"3D"];

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
                    "6.5.2",
                    format!("Annotation type {name} forbidden in PDF/A-{part}"),
                    format!("page {}", page_idx + 1),
                );
            }

            if st == b"FileAttachment" && part <= 2 {
                error_at(
                    report,
                    "6.5.2",
                    format!("FileAttachment annotation forbidden in PDF/A-{part}"),
                    format!("page {}", page_idx + 1),
                );
            }
        }
    }
}

/// Deep annotation flag validation per PDF/A part (§6.5.1).
pub fn check_annotation_flags_deep(pdf: &Pdf, part: u8, report: &mut ComplianceReport) {
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

            // PDF/A-2/3 §6.5.1: Widget annotations used as form fields
            // must not have both Hidden and Print flags set simultaneously
            if part >= 2 && subtype.as_ref() == b"Widget" {
                let hidden = flags & 0x02 != 0;
                let print = flags & 0x04 != 0;
                if hidden && print {
                    error_at(
                        report,
                        "6.5.1",
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

/// Check interactive form /NeedAppearances must be false or absent (§6.9).
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
            "6.9",
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
