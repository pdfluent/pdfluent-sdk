//! PDF/UA-1 conformance checking (ISO 14289-1).
//!
//! Validates PDF documents against PDF/UA-1 requirements
//! for universal accessibility.

use crate::check;
use crate::tagged;
use crate::ComplianceReport;
use pdf_syntax::object::dict::keys;
use pdf_syntax::Pdf;

/// Validate a PDF against PDF/UA-1 (ISO 14289-1).
pub fn validate(pdf: &Pdf) -> ComplianceReport {
    let mut report = ComplianceReport::default();

    check_pdfua_identifier(pdf, &mut report);
    check_document_title(pdf, &mut report);
    check_document_language(pdf, &mut report);
    check_marked_content(pdf, &mut report);
    check_structure_tree(pdf, &mut report);
    check_tab_order(pdf, &mut report);
    check_font_glyph_mapping(pdf, &mut report);

    report.compliant = report.is_compliant();
    report
}

/// Check that XMP metadata contains pdfuaid:part=1.
fn check_pdfua_identifier(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(xmp) = check::get_xmp_metadata(pdf) else {
        check::error(
            report,
            "7.1",
            "No XMP metadata stream; PDF/UA requires pdfuaid:part identifier",
        );
        return;
    };

    let Some(part) = check::parse_xmp_pdfua(&xmp) else {
        check::error(
            report,
            "7.1",
            "XMP metadata missing pdfuaid:part identifier",
        );
        return;
    };

    if part != 1 {
        check::error(
            report,
            "7.1",
            format!("pdfuaid:part={part}, expected 1 for PDF/UA-1"),
        );
    }
}

/// Check ViewerPreferences/DisplayDocTitle = true.
fn check_document_title(pdf: &Pdf, report: &mut ComplianceReport) {
    if !check::display_doc_title(pdf) {
        check::error(
            report,
            "7.1",
            "ViewerPreferences/DisplayDocTitle is not true; PDF/UA requires it",
        );
    }
}

/// Check that the document has a /Lang entry on the catalog.
fn check_document_language(pdf: &Pdf, report: &mut ComplianceReport) {
    if check::document_lang(pdf).is_none() {
        check::error(
            report,
            "7.2",
            "Document catalog missing /Lang entry; PDF/UA requires document language",
        );
    }
}

/// Check that the document is marked (MarkInfo/Marked = true).
fn check_marked_content(pdf: &Pdf, report: &mut ComplianceReport) {
    if !check::is_marked(pdf) {
        check::error(
            report,
            "7.1",
            "Document is not marked (MarkInfo/Marked missing or false)",
        );
    }
}

/// Check structure tree completeness and quality.
fn check_structure_tree(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(tree) = tagged::parse(pdf) else {
        check::error(
            report,
            "7.1",
            "No structure tree found; PDF/UA requires all content to be tagged",
        );
        return;
    };

    if tree.root_elements.is_empty() {
        check::error(report, "7.1", "Structure tree is empty; no tagged content");
        return;
    }

    // Check heading hierarchy
    let hierarchy_issues = tree.heading_hierarchy_issues();
    for issue in &hierarchy_issues {
        check::error(report, "7.4.2", issue.clone());
    }

    // Check Figure alt text
    let missing_alt = tree.figures_without_alt();
    for fig in &missing_alt {
        check::error(
            report,
            "7.3",
            format!("Figure element '{}' missing /Alt text", fig.struct_type),
        );
    }

    // Check table headers
    check_table_headers(&tree, report);
}

/// Check that tables have proper TH elements with Scope.
fn check_table_headers(tree: &tagged::StructureTree, report: &mut ComplianceReport) {
    for table in tree.tables() {
        let has_th = table
            .reading_order()
            .iter()
            .any(|e| e.standard_type == "TH");

        if !has_th {
            check::warning(
                report,
                "7.5",
                "Table element found without TH (table header) cells",
            );
        }
    }
}

/// Check that each page has /Tabs = /S.
fn check_tab_order(pdf: &Pdf, report: &mut ComplianceReport) {
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        if !check::page_has_tab_order_s(page_dict) {
            check::error_at(
                report,
                "7.1",
                "Page missing /Tabs /S; PDF/UA requires tab order follows structure",
                format!("page {}", page_idx + 1),
            );
        }
    }
}

/// Check that fonts have ToUnicode CMaps for glyph mapping.
fn check_font_glyph_mapping(pdf: &Pdf, report: &mut ComplianceReport) {
    check::for_each_font(pdf, |name, font_dict, page_idx| {
        // Symbolic fonts and Type3 fonts don't strictly need ToUnicode,
        // but non-symbolic fonts should have it for PDF/UA.
        let subtype = font_dict.get::<pdf_syntax::object::Name>(keys::SUBTYPE);
        let is_type3 = subtype.as_ref().is_some_and(|s| s.as_ref() == keys::TYPE3);

        if !is_type3 && !check::font_has_tounicode(font_dict) {
            // Check if it's a standard 14 font (they have implicit encoding)
            let base_font = font_dict
                .get::<pdf_syntax::object::Name>(keys::BASE_FONT)
                .map(|n| std::str::from_utf8(n.as_ref()).unwrap_or("").to_string());

            if !is_standard_14(&base_font) {
                check::warning(
                    report,
                    "7.21.3.1",
                    format!(
                        "Font {name} on page {} missing ToUnicode CMap",
                        page_idx + 1
                    ),
                );
            }
        }
    });
}

/// Check if a font name is one of the PDF standard 14 fonts.
fn is_standard_14(name: &Option<String>) -> bool {
    let Some(name) = name else { return false };
    matches!(
        name.as_str(),
        "Courier"
            | "Courier-Bold"
            | "Courier-Oblique"
            | "Courier-BoldOblique"
            | "Helvetica"
            | "Helvetica-Bold"
            | "Helvetica-Oblique"
            | "Helvetica-BoldOblique"
            | "Times-Roman"
            | "Times-Bold"
            | "Times-Italic"
            | "Times-BoldItalic"
            | "Symbol"
            | "ZapfDingbats"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_14_detection() {
        assert!(is_standard_14(&Some("Helvetica".to_string())));
        assert!(is_standard_14(&Some("Times-Roman".to_string())));
        assert!(!is_standard_14(&Some("ArialMT".to_string())));
        assert!(!is_standard_14(&None));
    }

    #[test]
    fn xmp_pdfua_parsing() {
        let xmp = br#"<rdf:Description pdfuaid:part="1"/>"#;
        let part = check::parse_xmp_pdfua(xmp).unwrap();
        assert_eq!(part, 1);
    }

    #[test]
    fn xmp_pdfua_element() {
        let xmp = br#"<pdfuaid:part>1</pdfuaid:part>"#;
        let part = check::parse_xmp_pdfua(xmp).unwrap();
        assert_eq!(part, 1);
    }

    #[test]
    fn empty_pdf_fails_pdfua() {
        let data = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
          2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n\
          xref\n0 3\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \n\
          trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n109\n%%EOF"
            .to_vec();
        if let Ok(pdf) = Pdf::new(data) {
            let report = validate(&pdf);
            assert!(!report.is_compliant());
            assert!(report.error_count() > 0);
        }
    }
}
