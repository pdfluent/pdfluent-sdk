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

/// Check if the document has JavaScript (Names/JavaScript or OpenAction with JS).
pub fn has_javascript(pdf: &Pdf) -> bool {
    let Some(cat) = catalog(pdf) else {
        return false;
    };

    // Check Names → JavaScript
    if let Some(names) = cat.get::<Dict<'_>>(keys::NAMES) {
        if names.get::<Object<'_>>(keys::JAVA_SCRIPT).is_some() {
            return true;
        }
    }

    // Check OpenAction for JS action
    if let Some(action) = cat.get::<Dict<'_>>(keys::OPEN_ACTION) {
        if let Some(s) = action.get::<Name>(keys::S) {
            if s.as_ref() == keys::JAVA_SCRIPT {
                return true;
            }
        }
    }

    // Check AA (Additional Actions) on catalog
    if cat.get::<Dict<'_>>(keys::AA).is_some() {
        return true;
    }

    false
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

/// Validate XMP date strings conform to ISO 8601 / XMP date format.
///
/// Valid formats:
/// - YYYY
/// - YYYY-MM
/// - YYYY-MM-DD
/// - YYYY-MM-DDThh:mmTZD
/// - YYYY-MM-DDThh:mm:ssTZD
/// - YYYY-MM-DDThh:mm:ss.sTZD
///
/// TZD = Z | +hh:mm | -hh:mm
pub fn validate_xmp_dates(xmp: &[u8], report: &mut ComplianceReport) {
    let Ok(text) = std::str::from_utf8(xmp) else {
        return;
    };

    let date_keys = ["xmp:CreateDate", "xmp:ModifyDate", "xmp:MetadataDate"];
    for key in &date_keys {
        if let Some(value) = extract_xmp_value(text, key).or_else(|| extract_xmp_attr(text, key)) {
            if !is_valid_xmp_date(&value) {
                error(
                    report,
                    "6.6.2.3.1",
                    format!("{key} has invalid date format: {value}"),
                );
            }
        }
    }
}

/// Check if a date string is a valid XMP/ISO 8601 date.
fn is_valid_xmp_date(date: &str) -> bool {
    let date = date.trim();
    if date.is_empty() {
        return false;
    }

    // Must start with 4-digit year
    if date.len() < 4 || !date[..4].chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    // YYYY
    if date.len() == 4 {
        return true;
    }

    // YYYY-MM
    if date.len() >= 7 && &date[4..5] == "-" {
        let month = &date[5..7];
        if !month.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        if date.len() == 7 {
            return true;
        }
    } else {
        return false;
    }

    // YYYY-MM-DD
    if date.len() >= 10 && &date[7..8] == "-" {
        let day = &date[8..10];
        if !day.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        if date.len() == 10 {
            return true;
        }
    } else {
        return false;
    }

    // After date, expect T for time
    if date.len() > 10 && &date[10..11] != "T" {
        return false;
    }

    // The time portion after T should have hh:mm at minimum
    if date.len() > 11 {
        let time_part = &date[11..];
        // Basic check: contains digits and valid timezone
        let has_tz = time_part.ends_with('Z')
            || time_part.contains('+')
            || time_part.matches('-').count() >= 1;
        // Must have at least hh:mm (5 chars) + timezone
        return time_part.len() >= 5 && has_tz;
    }

    false
}

/// Check widget annotations for required /AP (appearance) dictionaries.
pub fn check_widget_appearances(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot_dict in annots.iter::<Dict<'_>>() {
            let subtype = annot_dict.get::<Name>(keys::SUBTYPE);
            let is_widget = subtype.as_ref().is_some_and(|s| s.as_ref() == keys::WIDGET);
            if !is_widget {
                continue;
            }
            if annot_dict.get::<Dict<'_>>(keys::AP).is_none() {
                error_at(
                    report,
                    "6.7.9",
                    "Widget annotation missing /AP (appearance dictionary)",
                    format!("page {}", page_idx + 1),
                );
            }
        }
    }
}

/// Check that rendering intents are valid PDF/A values.
///
/// PDF/A allows only: RelativeColorimetric, AbsoluteColorimetric, Perceptual, Saturation.
pub fn check_rendering_intents(pdf: &Pdf, report: &mut ComplianceReport) {
    let valid_intents: &[&[u8]] = &[
        keys::RELATIVE_COLORIMETRIC,
        keys::ABSOLUTE_COLORIMETRIC,
        keys::PERCEPTUAL,
        keys::SATURATION,
    ];

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        // Check page-level RI
        if let Some(ri) = page_dict.get::<Name>(keys::RI) {
            if !valid_intents.iter().any(|v| ri.as_ref() == *v) {
                error_at(
                    report,
                    "6.2.4.3",
                    format!(
                        "Invalid rendering intent: {}",
                        std::str::from_utf8(ri.as_ref()).unwrap_or("?")
                    ),
                    format!("page {}", page_idx + 1),
                );
            }
        }

        // Check ExtGState rendering intents
        let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) else {
            continue;
        };
        let Some(gs_dict) = res_dict.get::<Dict<'_>>(keys::EXT_G_STATE) else {
            continue;
        };
        for (gs_name, _) in gs_dict.entries() {
            if let Some(gs) = gs_dict.get::<Dict<'_>>(gs_name.as_ref()) {
                if let Some(ri) = gs.get::<Name>(keys::RI) {
                    if !valid_intents.iter().any(|v| ri.as_ref() == *v) {
                        error_at(
                            report,
                            "6.2.4.3",
                            format!(
                                "Invalid rendering intent in ExtGState: {}",
                                std::str::from_utf8(ri.as_ref()).unwrap_or("?")
                            ),
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
            }
        }
    }
}

/// Check Info dict dates match XMP metadata dates (§6.6.1).
pub fn check_info_xmp_consistency(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(xmp_data) = get_xmp_metadata(pdf) else {
        return;
    };
    let Ok(xmp_text) = std::str::from_utf8(&xmp_data) else {
        return;
    };

    // Check that if /Info has CreationDate, XMP has xmp:CreateDate (and vice versa)
    let xmp_create = extract_xmp_value(xmp_text, "xmp:CreateDate")
        .or_else(|| extract_xmp_attr(xmp_text, "xmp:CreateDate"));
    let xmp_modify = extract_xmp_value(xmp_text, "xmp:ModifyDate")
        .or_else(|| extract_xmp_attr(xmp_text, "xmp:ModifyDate"));

    let metadata = pdf.metadata();

    if metadata.creation_date.is_some() && xmp_create.is_none() {
        error(
            report,
            "6.6.1",
            "/Info has CreationDate but XMP is missing xmp:CreateDate",
        );
    }
    if metadata.modification_date.is_some() && xmp_modify.is_none() {
        error(
            report,
            "6.6.1",
            "/Info has ModDate but XMP is missing xmp:ModifyDate",
        );
    }
}

/// Check page dimensions don't exceed 14400 user units (§6.1.12).
pub fn check_page_dimensions(pdf: &Pdf, report: &mut ComplianceReport) {
    const MAX_DIMENSION: f64 = 14400.0;

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let rect = page.media_box();
        let width = (rect.x1 - rect.x0).abs();
        let height = (rect.y1 - rect.y0).abs();

        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            error_at(
                report,
                "6.1.12",
                format!(
                    "Page dimensions {:.0}x{:.0} exceed maximum 14400 user units",
                    width, height
                ),
                format!("page {}", page_idx + 1),
            );
        }
    }
}

/// Check for forbidden named actions (§6.7.3).
///
/// Only NextPage, PrevPage, FirstPage, LastPage are allowed.
pub fn check_named_actions(pdf: &Pdf, report: &mut ComplianceReport) {
    let allowed: &[&[u8]] = &[b"NextPage", b"PrevPage", b"FirstPage", b"LastPage"];

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            check_action_dict_named(&annot, allowed, page_idx, report);
        }
    }

    // Also check catalog OpenAction
    if let Some(cat) = catalog(pdf) {
        if let Some(action) = cat.get::<Dict<'_>>(keys::OPEN_ACTION) {
            check_action_dict_named(&action, allowed, 0, report);
        }
    }
}

fn check_action_dict_named(
    dict: &Dict<'_>,
    allowed: &[&[u8]],
    page_idx: usize,
    report: &mut ComplianceReport,
) {
    // Check /A (action) dict
    if let Some(action) = dict.get::<Dict<'_>>(keys::A) {
        if let Some(s) = action.get::<Name>(keys::S) {
            if s.as_ref() == b"Named" {
                if let Some(n) = action.get::<Name>(keys::N) {
                    if !allowed.iter().any(|a| n.as_ref() == *a) {
                        error_at(
                            report,
                            "6.7.3",
                            format!(
                                "Forbidden named action: {}",
                                std::str::from_utf8(n.as_ref()).unwrap_or("?")
                            ),
                            format!("page {}", page_idx + 1),
                        );
                    }
                }
            }
        }
    }
}
