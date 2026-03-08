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
pub fn parse_xmp_pdfa(xmp: &[u8]) -> Option<(u8, String)> {
    let text = std::str::from_utf8(xmp).ok()?;

    let part = extract_xmp_value(text, "pdfaid:part")
        .or_else(|| extract_xmp_attr(text, "pdfaid:part"))?
        .parse::<u8>()
        .ok()?;

    let conformance = extract_xmp_value(text, "pdfaid:conformance")
        .or_else(|| extract_xmp_attr(text, "pdfaid:conformance"))?;

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
    if has_output_intent(pdf) {
        return; // OutputIntent provides the fallback
    }

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let res_dict = page_dict.get::<Dict<'_>>(keys::RESOURCES);

        // Check if Default color spaces are defined
        let has_default_rgb = res_dict
            .as_ref()
            .and_then(|r| r.get::<Dict<'_>>(keys::COLORSPACE))
            .and_then(|cs| cs.get::<Object<'_>>(keys::DEFAULT_RGB))
            .is_some();
        let has_default_cmyk = res_dict
            .as_ref()
            .and_then(|r| r.get::<Dict<'_>>(keys::COLORSPACE))
            .and_then(|cs| cs.get::<Object<'_>>(keys::DEFAULT_CMYK))
            .is_some();
        let has_default_gray = res_dict
            .as_ref()
            .and_then(|r| r.get::<Dict<'_>>(keys::COLORSPACE))
            .and_then(|cs| cs.get::<Object<'_>>(keys::DEFAULT_GRAY))
            .is_some();

        // Scan content stream for device color space operators
        if let Some(content) = page.page_stream() {
            let ops = detect_device_color_ops(content);
            if !has_default_rgb && ops.has_rgb {
                error_at(
                    report,
                    "6.2.4.3",
                    "DeviceRGB used without DefaultRGB color space or OutputIntent",
                    format!("page {}", page_idx + 1),
                );
            }
            if !has_default_cmyk && ops.has_cmyk {
                error_at(
                    report,
                    "6.2.4.3",
                    "DeviceCMYK used without DefaultCMYK color space or OutputIntent",
                    format!("page {}", page_idx + 1),
                );
            }
            if !has_default_gray && ops.has_gray {
                error_at(
                    report,
                    "6.2.4.3",
                    "DeviceGray used without DefaultGray color space or OutputIntent",
                    format!("page {}", page_idx + 1),
                );
            }
        }

        // Also check ColorSpace resources for direct device CS references
        if let Some(cs_dict) = res_dict
            .as_ref()
            .and_then(|r| r.get::<Dict<'_>>(keys::COLORSPACE))
        {
            for (name, _) in cs_dict.entries() {
                if let Some(cs_name) = cs_dict.get::<Name>(name.as_ref()) {
                    let cs = cs_name.as_ref();
                    if !has_default_rgb && cs == keys::DEVICE_RGB {
                        error_at(
                            report,
                            "6.2.4.3",
                            "DeviceRGB referenced in ColorSpace resources without OutputIntent",
                            format!("page {}", page_idx + 1),
                        );
                    }
                    if !has_default_cmyk && cs == b"DeviceCMYK" {
                        error_at(
                            report,
                            "6.2.4.3",
                            "DeviceCMYK referenced in ColorSpace resources without OutputIntent",
                            format!("page {}", page_idx + 1),
                        );
                    }
                    if !has_default_gray && cs == b"DeviceGray" {
                        error_at(
                            report,
                            "6.2.4.3",
                            "DeviceGray referenced in ColorSpace resources without OutputIntent",
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
            }
        }
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
/// and cs/CS with DeviceRGB/DeviceCMYK/DeviceGray operand.
fn detect_device_color_ops(content: &[u8]) -> DeviceColorOps {
    let mut result = DeviceColorOps {
        has_rgb: false,
        has_cmyk: false,
        has_gray: false,
    };

    // Tokenize the content stream by splitting on whitespace/newlines
    let text = String::from_utf8_lossy(content);
    let tokens: Vec<&str> = text.split_ascii_whitespace().collect();

    for (i, &tok) in tokens.iter().enumerate() {
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
pub fn check_halftone_and_transfer(pdf: &Pdf, report: &mut ComplianceReport) {
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

            // §6.2.10: halftone type
            if let Some(ht_dict) = gs.get::<Dict<'_>>(b"HT" as &[u8]) {
                if let Some(ht_type) = ht_dict.get::<i32>(keys::TYPE) {
                    if ht_type != 1 && ht_type != 5 {
                        error_at(
                            report,
                            "6.2.10",
                            format!("ExtGState {gs_str} uses HalftoneType {ht_type} (only 1 and 5 allowed)"),
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
                // §6.2.10.4.1: No HalftoneName
                if ht_dict.contains_key(b"HalftoneName" as &[u8]) {
                    error_at(
                        report,
                        "6.2.10.4.1",
                        format!("ExtGState {gs_str} halftone contains forbidden /HalftoneName"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }

            // §6.2.10.5: TR forbidden
            if gs.contains_key(keys::TR) {
                error_at(
                    report,
                    "6.2.10.5",
                    format!("ExtGState {gs_str} contains forbidden /TR (transfer function)"),
                    format!("page {}", page_idx + 1),
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
                            format!("page {}", page_idx + 1),
                        );
                    }
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

/// Check absolute real values don't exceed 32767 (§6.1.12).
///
/// The PDF/A spec requires that all absolute real values in content streams
/// and page dictionaries must be ≤ 32767. We approximate this by checking
/// MediaBox dimensions (the most common trigger).
pub fn check_page_dimensions(pdf: &Pdf, report: &mut ComplianceReport) {
    const MAX_REAL: f64 = 32767.0;

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let rect = page.media_box();
        // Check all four MediaBox values individually
        for val in [rect.x0, rect.y0, rect.x1, rect.y1] {
            if val.abs() > MAX_REAL {
                error_at(
                    report,
                    "6.1.12",
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
                "6.1.12",
                format!(
                    "Page dimensions {:.0}x{:.0} exceed maximum 32767.0",
                    width, height
                ),
                format!("page {}", page_idx + 1),
            );
        }

        // §6.1.12: Also scan content stream numeric operands
        if let Some(content) = page.page_stream() {
            if scan_content_stream_reals(content, MAX_REAL) {
                error_at(
                    report,
                    "6.1.12",
                    "Content stream contains real value exceeding 32767",
                    format!("page {}", page_idx + 1),
                );
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
