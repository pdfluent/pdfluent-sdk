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
        forbidden.extend_from_slice(&[b"Hide", b"SetOCGState", b"Rendition", b"Trans", b"GoTo3DView"]);
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
                    .map(|n| {
                        std::str::from_utf8(n.as_ref())
                            .unwrap_or("?")
                            .to_string()
                    })
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
    }
}
