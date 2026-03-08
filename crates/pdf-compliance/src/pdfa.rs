//! PDF/A validation (ISO 19005 parts 1–3).
//!
//! Validates PDF documents against all six conformance levels:
//! - PDF/A-1a, PDF/A-1b (ISO 19005-1)
//! - PDF/A-2a, PDF/A-2b, PDF/A-2u (ISO 19005-2)
//! - PDF/A-3a, PDF/A-3b, PDF/A-3u (ISO 19005-3)

use crate::check;
use crate::{ComplianceReport, PdfALevel};
use pdf_syntax::object::dict::keys;
use pdf_syntax::object::{Dict, Name};
use pdf_syntax::Pdf;

/// Validate a PDF document against a PDF/A conformance level.
pub fn validate(pdf: &Pdf, level: PdfALevel) -> ComplianceReport {
    let mut report = ComplianceReport {
        pdfa_level: Some(level),
        ..Default::default()
    };

    check_xmp_metadata(pdf, level, &mut report);
    crate::xmp::validate_xmp(pdf, level, &mut report);
    check_encryption(pdf, &mut report);
    check_forbidden_actions(pdf, level, &mut report);
    check_output_intent(pdf, level, &mut report);
    check_font_embedding(pdf, &mut report);
    check_color_spaces(pdf, &mut report);
    check_device_colorspaces(pdf, &mut report);
    check_page_dimensions(pdf, &mut report);
    check_annotation_flags(pdf, &mut report);
    check_annotation_types(pdf, level, &mut report);
    check_form_xobjects(pdf, &mut report);
    check_page_boundary_sizes(pdf, &mut report);

    // Batch 2: Color space & graphics state validation (§6.2.x)
    check_icc_profile_version(pdf, level, &mut report);
    check_iccbased_alternate(pdf, &mut report);
    check_devicen_separation_alternate(pdf, &mut report);
    check_rendering_intents(pdf, &mut report);
    check_image_xobjects(pdf, &mut report);
    check_halftone_and_transfer(pdf, &mut report);
    check_extgstate_restrictions(pdf, level, &mut report);
    check_cidfont_embedding(pdf, &mut report);
    check_output_intent_profile(pdf, &mut report);

    // Batch 3: File structure, actions, streams (§6.1.x, §6.6.1)
    check_all_page_boundaries(pdf, &mut report);
    check_stream_filters(pdf, level, &mut report);
    check_embedded_file_streams(pdf, &mut report);
    check_file_header(pdf, &mut report);
    check_xref_format(pdf, &mut report);
    check_actions_deep(pdf, level, &mut report);
    check_form_xobject_geometry(pdf, &mut report);
    check_optional_content(pdf, level, &mut report);
    check_linearization(pdf, &mut report);

    if level.part() == 1 {
        check_transparency_a1(pdf, &mut report);
    }

    if level.requires_tagged() {
        check_tagged_requirements(pdf, &mut report);
    }

    if level.part() == 3 {
        check_embedded_files_a3(pdf, &mut report);
    } else {
        check_no_embedded_files(pdf, level, &mut report);
    }

    report.compliant = report.is_compliant();
    report
}

/// XMP metadata must declare the correct PDF/A part and conformance.
/// PDF/A-1: §6.7.11, PDF/A-2/3: §6.6.4.
fn check_xmp_metadata(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    let rule = if level.part() == 1 { "6.7.11" } else { "6.6.4" };

    let Some(xmp) = check::get_xmp_metadata(pdf) else {
        check::error(report, rule, "No XMP metadata stream in catalog");
        return;
    };

    let Some((part, conformance)) = check::parse_xmp_pdfa(&xmp) else {
        check::error(
            report,
            rule,
            "XMP metadata missing pdfaid:part or pdfaid:conformance",
        );
        return;
    };

    if part != level.part() {
        check::error(
            report,
            rule,
            format!(
                "XMP pdfaid:part={part} does not match expected {}",
                level.part()
            ),
        );
    }

    let expected_conf = level.conformance();
    if !conformance.eq_ignore_ascii_case(expected_conf) {
        check::error(
            report,
            rule,
            format!("XMP pdfaid:conformance={conformance} does not match expected {expected_conf}"),
        );
    }
}

/// §6.1.1 — PDF/A documents shall not be encrypted.
fn check_encryption(pdf: &Pdf, report: &mut ComplianceReport) {
    if check::is_encrypted(pdf) {
        check::error(
            report,
            "6.1.1",
            "Document is encrypted; PDF/A forbids encryption",
        );
    }
}

/// Forbidden action types. PDF/A-1: §6.6.1, PDF/A-2/3: §6.5.1.
fn check_forbidden_actions(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    let rule = if level.part() == 1 { "6.6.1" } else { "6.5.1" };
    check::check_forbidden_actions_rule(pdf, level.part(), rule, report);
}

/// OutputIntents must include a GTS_PDFA1 entry. PDF/A-1: §6.2.2, PDF/A-2/3: §6.2.3.
fn check_output_intent(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    let rule = if level.part() == 1 { "6.2.2" } else { "6.2.3" };
    if !check::has_output_intent(pdf) {
        check::error(
            report,
            rule,
            "No OutputIntents with GTS_PDFA1 subtype found",
        );
    }
}

/// §6.3.3 — All fonts must be embedded.
fn check_font_embedding(pdf: &Pdf, report: &mut ComplianceReport) {
    check::for_each_font(pdf, |name, font_dict, page_idx| {
        let Some(desc) = font_dict.get::<Dict<'_>>(keys::FONT_DESC) else {
            // Type0 fonts have DescendantFonts instead of a direct FontDescriptor
            if let Some(descendants) =
                font_dict.get::<pdf_syntax::object::Array<'_>>(keys::DESCENDANT_FONTS)
            {
                for desc_font in descendants.iter::<Dict<'_>>() {
                    if let Some(inner_desc) = desc_font.get::<Dict<'_>>(keys::FONT_DESC) {
                        if !check::font_has_embedding(&inner_desc) {
                            check::error_at(
                                report,
                                "6.3.3",
                                format!("Font {name} (CIDFont) is not embedded"),
                                format!("page {}", page_idx + 1),
                            );
                        }
                    }
                }
            } else {
                // No FontDescriptor and no DescendantFonts — cannot verify embedding
                check::error_at(
                    report,
                    "6.3.3",
                    format!("Font {name} has no FontDescriptor; cannot verify embedding"),
                    format!("page {}", page_idx + 1),
                );
            }
            return;
        };
        if !check::font_has_embedding(&desc) {
            check::error_at(
                report,
                "6.3.3",
                format!("Font {name} is not embedded"),
                format!("page {}", page_idx + 1),
            );
        }
    });
}

/// §6.2.3 — Check color space usage.
fn check_color_spaces(pdf: &Pdf, report: &mut ComplianceReport) {
    let has_intent = check::has_output_intent(pdf);

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(res_dict) = page_dict.get::<Dict<'_>>(keys::RESOURCES) else {
            continue;
        };

        if let Some(cs_dict) = res_dict.get::<Dict<'_>>(keys::COLORSPACE) {
            for (name, _) in cs_dict.entries() {
                if let Some(cs_name) = cs_dict.get::<Name>(name.as_ref()) {
                    let cs_bytes = cs_name.as_ref();
                    if !has_intent
                        && (cs_bytes == keys::DEVICE_RGB
                            || cs_bytes == keys::DEVICE_CMYK
                            || cs_bytes == keys::DEVICE_GRAY)
                    {
                        check::warning(
                            report,
                            "6.2.3",
                            format!(
                                "Device-dependent color space {} on page {} without output intent",
                                std::str::from_utf8(cs_bytes).unwrap_or("?"),
                                page_idx + 1
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// §6.2.4.3 — Device color spaces need Default alternatives or OutputIntent.
fn check_device_colorspaces(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_device_colorspaces(pdf, report);
}

/// §6.1.12 — Absolute real values must not exceed 32767.
fn check_page_dimensions(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_page_dimensions(pdf, report);
}

/// §6.3.2 — Annotations must have /F key with correct flags.
fn check_annotation_flags(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_annotation_flags(pdf, report);
}

/// §6.5.2 — Only specific annotation types are permitted (PDF/A-1).
fn check_annotation_types(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    if level.part() != 1 {
        return; // PDF/A-2/3 has different annotation restrictions
    }

    let allowed: &[&[u8]] = &[
        b"Text",
        b"Link",
        b"FreeText",
        b"Line",
        b"Square",
        b"Circle",
        b"Highlight",
        b"Underline",
        b"Squiggly",
        b"StrikeOut",
        b"Stamp",
        b"Ink",
        b"Popup",
        b"Widget",
        b"PrinterMark",
        b"TrapNet",
    ];

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<pdf_syntax::object::Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            if let Some(subtype) = annot.get::<Name>(keys::SUBTYPE) {
                if !allowed.iter().any(|a| subtype.as_ref() == *a) {
                    let name = std::str::from_utf8(subtype.as_ref()).unwrap_or("?");
                    check::error_at(
                        report,
                        "6.5.2",
                        format!("Annotation type {name} not permitted in PDF/A-1"),
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }
}

/// §6.2.9 — Form XObjects must not contain OPI/PS/Ref keys.
fn check_form_xobjects(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_form_xobjects(pdf, report);
}

/// §6.1.13 — Page boundaries must be 3-14400 units.
fn check_page_boundary_sizes(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_page_boundary_sizes(pdf, report);
}

/// §6.2.3.3 — ICC profile version must match PDF/A part.
fn check_icc_profile_version(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_icc_profile_version(pdf, level.part(), report);
}

/// §6.2.4.2 — ICCBased Alternate CS must be consistent with profile.
fn check_iccbased_alternate(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_iccbased_alternate(pdf, report);
}

/// §6.2.4.4 — DeviceN/Separation alternate CS restrictions.
fn check_devicen_separation_alternate(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_devicen_separation_alternate(pdf, report);
}

/// §6.2.5 — Rendering intents must be valid.
fn check_rendering_intents(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_rendering_intents(pdf, report);
}

/// §6.2.8 — Image XObject restrictions.
fn check_image_xobjects(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_image_xobjects(pdf, report);
}

/// §6.2.10 — Halftone and transfer function restrictions.
fn check_halftone_and_transfer(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_halftone_and_transfer(pdf, report);
}

/// §6.2.10.6-9 — ExtGState blend mode and soft mask restrictions.
fn check_extgstate_restrictions(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_extgstate_restrictions(pdf, level.part(), report);
}

/// §6.2.11 — CIDFont embedding requirements.
fn check_cidfont_embedding(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_cidfont_embedding(pdf, report);
}

/// §6.2.3.2 — OutputIntent must have ICC profile.
fn check_output_intent_profile(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_output_intent_profile(pdf, report);
}

// ─── Batch 3: File structure, actions, streams (§6.1.x, §6.6.1) ────────────

/// §6.1.13 — All page boundaries (BleedBox, TrimBox, ArtBox).
fn check_all_page_boundaries(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_all_page_boundaries(pdf, report);
}

/// §6.1.8, §6.1.9 — Stream filter validation.
fn check_stream_filters(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_stream_filters(pdf, level.part(), report);
}

/// §6.1.7, §6.1.7.1 — Embedded file stream type.
fn check_embedded_file_streams(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_embedded_file_streams(pdf, report);
}

/// §6.1.2 — File header binary comment.
fn check_file_header(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_file_header(pdf, report);
}

/// §6.1.3 — Cross-reference table format.
fn check_xref_format(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_xref_format(pdf, report);
}

/// §6.6.1, §6.1.6.x — Deep recursive action scanner.
fn check_actions_deep(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    let rule = if level.part() == 1 { "6.6.1" } else { "6.5.1" };
    check::check_actions_deep(pdf, level.part(), rule, report);
}

/// §6.1.10 — Form XObject BBox validation.
fn check_form_xobject_geometry(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_form_xobject_geometry(pdf, report);
}

/// §6.1.11 — Optional content restrictions.
fn check_optional_content(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_optional_content(pdf, level.part(), report);
}

/// §6.1.5 — Linearization hints.
fn check_linearization(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_linearization(pdf, report);
}

/// §6.4 — PDF/A-1 forbids transparency.
fn check_transparency_a1(pdf: &Pdf, report: &mut ComplianceReport) {
    if check::has_transparency(pdf) {
        check::error(
            report,
            "6.4",
            "Document uses transparency; PDF/A-1 forbids transparency groups",
        );
    }
}

/// Level "a" requires tagged PDF.
fn check_tagged_requirements(pdf: &Pdf, report: &mut ComplianceReport) {
    if !check::is_marked(pdf) {
        check::error(
            report,
            "6.8",
            "Document is not marked (MarkInfo/Marked missing or false); required for level 'a'",
        );
    }

    if check::struct_tree_root(pdf).is_none() {
        check::error(
            report,
            "6.8",
            "No StructTreeRoot found; required for level 'a'",
        );
    }
}

/// PDF/A-3 allows embedded files; check they have proper AF relationships.
fn check_embedded_files_a3(pdf: &Pdf, report: &mut ComplianceReport) {
    if check::has_embedded_files(pdf) {
        let Some(cat) = check::catalog(pdf) else {
            return;
        };
        if cat.get::<pdf_syntax::object::Array<'_>>(keys::AF).is_none() {
            check::warning(
                report,
                "6.8",
                "Embedded files present but no /AF array on catalog (PDF/A-3 requires it)",
            );
        }
    }
}

/// PDF/A-1 and PDF/A-2 forbid embedded files.
fn check_no_embedded_files(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    if check::has_embedded_files(pdf) {
        check::error(
            report,
            "6.1.7",
            format!(
                "Document contains embedded files; forbidden in PDF/A-{}{}",
                level.part(),
                level.conformance().to_lowercase()
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdfa_level_properties() {
        assert_eq!(PdfALevel::A1a.part(), 1);
        assert_eq!(PdfALevel::A1a.conformance(), "A");
        assert!(PdfALevel::A1a.requires_tagged());

        assert_eq!(PdfALevel::A2b.part(), 2);
        assert_eq!(PdfALevel::A2b.conformance(), "B");
        assert!(!PdfALevel::A2b.requires_tagged());

        assert_eq!(PdfALevel::A3u.part(), 3);
        assert_eq!(PdfALevel::A3u.conformance(), "U");
        assert!(!PdfALevel::A3u.requires_tagged());
    }

    #[test]
    fn xmp_parsing() {
        let xmp = br#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
        <x:xmpmeta xmlns:x="adobe:ns:meta/">
        <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
        <rdf:Description rdf:about=""
            xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">
            <pdfaid:part>2</pdfaid:part>
            <pdfaid:conformance>B</pdfaid:conformance>
        </rdf:Description>
        </rdf:RDF>
        </x:xmpmeta>"#;

        let (part, conf) = check::parse_xmp_pdfa(xmp).unwrap();
        assert_eq!(part, 2);
        assert_eq!(conf, "B");
    }

    #[test]
    fn xmp_parsing_attributes() {
        let xmp = br#"<rdf:Description pdfaid:part="1" pdfaid:conformance="A"/>"#;
        let (part, conf) = check::parse_xmp_pdfa(xmp).unwrap();
        assert_eq!(part, 1);
        assert_eq!(conf, "A");
    }

    #[test]
    fn empty_pdf_fails_validation() {
        let data = minimal_pdf_bytes();
        if let Ok(pdf) = Pdf::new(data) {
            let report = validate(&pdf, PdfALevel::A2b);
            assert!(!report.is_compliant());
            assert!(report.error_count() > 0);
        }
    }

    fn minimal_pdf_bytes() -> Vec<u8> {
        b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
          2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n\
          xref\n0 3\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \n\
          trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n109\n%%EOF"
            .to_vec()
    }
}
