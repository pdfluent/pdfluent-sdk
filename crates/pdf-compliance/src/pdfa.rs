//! PDF/A validation (ISO 19005 parts 1–4).
//!
//! Validates PDF documents against all conformance levels:
//! - PDF/A-1a, PDF/A-1b (ISO 19005-1)
//! - PDF/A-2a, PDF/A-2b, PDF/A-2u (ISO 19005-2)
//! - PDF/A-3a, PDF/A-3b, PDF/A-3u (ISO 19005-3)
//! - PDF/A-4, PDF/A-4f, PDF/A-4e (ISO 19005-4)

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
    check_device_color_vs_output_intent(pdf, &mut report);
    check_page_dimensions(pdf, level, &mut report);
    check_annotation_flags(pdf, level, &mut report);
    check_annotation_types(pdf, level, &mut report);
    check_annotation_color_arrays(pdf, &mut report);
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

    // Iteration 11: Deeper 6.2.x fixes
    check_image_xobject_colorspaces(pdf, &mut report);
    check_output_intent_consistency(pdf, &mut report);
    check_undefined_operators(pdf, &mut report);
    check_transparency_vs_output_intent(pdf, level, &mut report);

    // Batch 4: Font & Annotation deep validation (§6.3.x, §6.5.x)
    check_font_type_key(pdf, &mut report);
    check_font_embedding_deep(pdf, level, &mut report);
    check_tounicode_cmap(pdf, &mut report);
    check_font_widths(pdf, &mut report);
    check_symbolic_truetype_encoding(pdf, &mut report);
    check_cidtogidmap_identity(pdf, &mut report);
    check_cmap_embedding(pdf, &mut report);
    check_annotation_appearance(pdf, &mut report);
    check_annotation_subtypes_deep(pdf, level, &mut report);
    check_annotation_flags_deep(pdf, level, &mut report);

    if level.part() == 1 {
        check_transparency_a1(pdf, &mut report);
    }

    // Batch 5: Transparency deep, tagged PDF, remaining rules
    check_transparency_deep(pdf, level, &mut report);
    check_blending_modes_pdfa(pdf, level, &mut report);
    check_soft_mask(pdf, &mut report);
    check_need_appearances_pdfa(pdf, &mut report);
    check_signature_restrictions_pdfa(pdf, &mut report);
    check_document_structure_pdfa(pdf, &mut report);
    check_marked_content(pdf, &mut report);

    // PDF/A-4 requires tagged PDF for all conformance levels;
    // PDF/A-1a/2a/3a require it only for level 'a'
    if level.requires_tagged() || level.part() == 4 {
        check_tagged_requirements(pdf, level, &mut report);
        check_table_structure_pdfa(pdf, &mut report);
        check_figure_alt(pdf, &mut report);
        check_role_mapping_pdfa(pdf, &mut report);
    }

    match level.part() {
        3 => check_embedded_files_a3(pdf, &mut report),
        4 => {
            // PDF/A-4f allows embedded files; base and 4e do not restrict
            // (PDF/A-4 inherits PDF 2.0 which allows associated files)
        }
        _ => check_no_embedded_files(pdf, level, &mut report),
    }

    // Batch 6: Implementation limits & structural checks
    check_name_length(pdf, &mut report);
    check_real_value_range(pdf, level, &mut report);
    check_font_file_format(pdf, level, &mut report);
    check_explicit_resources(pdf, &mut report);

    check_trailer_requirements(pdf, level, &mut report);

    // Batch 7: Stream/syntax validation, XMP extension, image intent
    check_stream_length_pdfa(pdf, &mut report);
    check_object_syntax(pdf, &mut report);
    check_xmp_extension_schema_pdfa(pdf, &mut report);
    check_image_intent(pdf, &mut report);

    // Post-process: remap clause numbers per PDF/A part.
    // Clause numbering differs between ISO 19005 parts.
    remap_clause_numbers(&mut report, level);

    report.compliant = report.is_compliant();
    report
}

/// XMP metadata must declare the correct PDF/A part and conformance.
/// PDF/A-1: §6.7.11, PDF/A-2/3: §6.6.4, PDF/A-4: §6.5.2.
fn check_xmp_metadata(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    let rule = match level.part() {
        1 => "6.7.11",
        4 => "6.5.2",
        _ => "6.6.4",
    };

    let Some(xmp) = check::get_xmp_metadata(pdf) else {
        check::error(report, rule, "No XMP metadata stream in catalog");
        return;
    };

    let Some((part, conformance)) = check::parse_xmp_pdfa(&xmp) else {
        check::error(report, rule, "XMP metadata missing pdfaid:part");
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
    // PDF/A-4 base level has no conformance letter — empty is valid
    if !(conformance.eq_ignore_ascii_case(expected_conf)
        || (expected_conf.is_empty() && conformance.is_empty()))
    {
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

/// Forbidden action types. PDF/A-1: §6.6.1, PDF/A-2/3: §6.5.1, PDF/A-4: §6.6.1 (normalized from 6.4.1).
fn check_forbidden_actions(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    // PDF/A-4 clause 6.4.1 normalizes to 6.6.1 in our common numbering
    let rule = match level.part() {
        1 | 4 => "6.6.1",
        _ => "6.5.1",
    };
    check::check_forbidden_actions_rule(pdf, level.part(), rule, report);
}

/// OutputIntents must include a GTS_PDFA1 entry with DestOutputProfile (§6.2.2).
fn check_output_intent(pdf: &Pdf, _level: PdfALevel, report: &mut ComplianceReport) {
    if !check::has_output_intent(pdf) {
        check::error(
            report,
            "6.2.2",
            "No OutputIntents with GTS_PDFA1 subtype found",
        );
        return;
    }
    // GTS_PDFA1 OutputIntent must have a DestOutputProfile
    if check::output_intent_profile_components(pdf).is_none() {
        check::error(
            report,
            "6.2.2",
            "GTS_PDFA1 OutputIntent has no valid DestOutputProfile",
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

/// §6.2.3.3 — Device colors must match OutputIntent profile color space.
fn check_device_color_vs_output_intent(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_device_color_vs_output_intent(pdf, report);
}

/// §6.1.12/6.1.13 — Implementation limits (real values, name/string lengths, etc.).
fn check_page_dimensions(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_page_dimensions(pdf, level.part(), report);
    // PDF/A-4: catalog Version must match "2.n"
    if level.part() == 4 {
        check::check_catalog_version_pdfa4(pdf, report);
    }
}

/// §6.3.2 — Annotations must have /F key with correct flags.
fn check_annotation_flags(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_annotation_flags(pdf, level.part(), report);
}

/// §6.5.2 — Only specific annotation types are permitted (PDF/A-1).
fn check_annotation_types(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    if level.part() != 1 {
        return; // PDF/A-2/3/4 have different annotation restrictions
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

/// §6.5.3 — Annotation /C and /IC color arrays restricted.
fn check_annotation_color_arrays(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_annotation_color_arrays(pdf, report);
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
    let rule = match level.part() {
        1 => "6.6.1",
        4 => "6.4",
        _ => "6.5.1",
    };
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

/// Tagged PDF requirements (§6.8 / §6.8.1).
///
/// Required for PDF/A-1a, PDF/A-2a, PDF/A-3a (level 'a'), and all PDF/A-4.
/// PDF/A-4 uses clause 6.8.1 (mapped from ISO 19005-4 §6.6.1); parts 1-3 use 6.8.
fn check_tagged_requirements(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    let rule = if level.part() == 4 { "6.8.1" } else { "6.8" };
    if !check::is_marked(pdf) {
        check::error(
            report,
            rule,
            "Document is not marked (MarkInfo/Marked missing or false)",
        );
    }

    if check::struct_tree_root(pdf).is_none() {
        check::error(report, rule, "No StructTreeRoot found");
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

// ─── Iteration 11: Deeper 6.2.x fixes ───────────────────────────────────────

/// §6.2.4.3 — Image XObject device color spaces.
fn check_image_xobject_colorspaces(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_image_xobject_colorspaces(pdf, report);
    check::check_page_group_colorspaces(pdf, report);
}

/// §6.2.2 — Multiple OutputIntents must have identical profiles.
fn check_output_intent_consistency(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_output_intent_consistency(pdf, report);
}

/// §6.2.10 — Undefined operators in content streams.
fn check_undefined_operators(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_undefined_operators(pdf, report);
}

/// §6.2.9/6.2.10 — Transparency groups vs OutputIntent.
fn check_transparency_vs_output_intent(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_transparency_vs_output_intent(pdf, level.part(), report);
}

// ─── Batch 4: Font & Annotation Deep Validation (§6.3.x, §6.5.x) ───────────

/// §6.3.1 — Font /Type key validation.
fn check_font_type_key(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_font_type_key(pdf, report);
}

/// §6.3.3 — Deep font embedding validation.
fn check_font_embedding_deep(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_font_embedding_deep(pdf, level.part(), report);
}

/// §6.3.4 — ToUnicode CMap presence.
fn check_tounicode_cmap(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_tounicode_cmap(pdf, report);
}

/// §6.3.5 — Font /Widths array.
fn check_font_widths(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_font_widths(pdf, report);
}

/// §6.3.6 — Symbolic TrueType encoding.
fn check_symbolic_truetype_encoding(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_symbolic_truetype_encoding(pdf, report);
}

/// §6.3.7 — CIDToGIDMap identity for Type2.
fn check_cidtogidmap_identity(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_cidtogidmap_identity(pdf, report);
}

/// §6.3.8 — CMap embedding for Type0.
fn check_cmap_embedding(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_cmap_embedding(pdf, report);
}

/// §6.5.3 — Annotation appearance streams.
fn check_annotation_appearance(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_annotation_appearance(pdf, report);
}

/// §6.5.2 — Deep annotation subtype validation.
fn check_annotation_subtypes_deep(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_annotation_subtypes_deep(pdf, level.part(), report);
}

/// §6.5.1 — Deep annotation flag validation.
fn check_annotation_flags_deep(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_annotation_flags_deep(pdf, level.part(), report);
}

// ─── Batch 5: Transparency, Tagged PDF, Remaining Rules ─────────────────────

/// §6.4 — Deeper transparency validation.
fn check_transparency_deep(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_transparency_deep(pdf, level.part(), report);
}

/// §6.4.1 — Blending mode validation.
fn check_blending_modes_pdfa(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_blending_modes(pdf, level.part(), report);
}

/// §6.4.2 — Soft mask structure validation.
fn check_soft_mask(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_soft_mask_structure(pdf, report);
}

/// §6.8.2.2 — Table structure element nesting.
fn check_table_structure_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_table_structure(pdf, report);
}

/// §6.8.4 — Figure elements must have Alt text.
fn check_figure_alt(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_figure_alt_text(pdf, report);
}

/// §6.8.3.4 — Marked content sequence matching.
fn check_marked_content(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_marked_content_sequences(pdf, report);
}

/// §6.9 — NeedAppearances and field appearances.
fn check_need_appearances_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_need_appearances(pdf, report);
}

/// §6.10 — Digital signature restrictions.
fn check_signature_restrictions_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_signature_restrictions(pdf, report);
    // §6.1.12 (PDF/A-2/3/4): DocMDP signature reference restrictions
    check::check_docmdp_signature_restriction(pdf, report);
}

/// §6.11 — Document structure requirements.
fn check_document_structure_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_document_structure(pdf, report);
}

/// §6.12 — Role mapping check.
fn check_role_mapping_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_role_mapping(pdf, report);
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

// ─── Batch 6: Implementation limits & structural checks ─────────────────────

/// §6.1.13 — Name length limit.
fn check_name_length(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_name_length_limit(pdf, report);
}

/// §6.1.12 — Real value range.
fn check_real_value_range(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    // Only PDF/A-1 has the strict 32767 limit; later parts relax it
    if level.part() == 1 {
        check::check_real_value_limits(pdf, report);
    }
}

/// §6.3.2 — Font file stream format.
fn check_font_file_format(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_font_file_subtype(pdf, level.part(), report);
}

/// §6.2.2 — Explicit Resources.
fn check_explicit_resources(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_explicit_resources(pdf, report);
}

/// §6.1.3 — Trailer requirements.
fn check_trailer_requirements(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_trailer_requirements(pdf, level.part(), report);
}

// ─── Batch 7: Stream/syntax validation, XMP extension, image intent ─────────

/// §6.1.7 — Stream Length verification.
fn check_stream_length_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_stream_length(pdf, report);
}

/// §6.1.8/6.1.9 — Object syntax spacing checks.
fn check_object_syntax(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_object_syntax_spacing(pdf, report);
}

/// §6.7.8 — XMP extension schema validation.
fn check_xmp_extension_schema_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_xmp_extension_schema(pdf, report);
}

/// §6.2.5/6.2.9 — Image XObject rendering intent.
fn check_image_intent(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_image_xobject_intent(pdf, report);
}

/// Remap clause numbers to match the correct ISO 19005 part numbering.
///
/// Our checks use a canonical clause number (typically from PDF/A-2/4),
/// but each ISO part has its own numbering for the same requirement.
fn remap_clause_numbers(report: &mut ComplianceReport, level: PdfALevel) {
    let part = level.part();
    for issue in &mut report.issues {
        let new_rule = match (part, issue.rule.as_str()) {
            // Device color space restrictions
            // PDF/A-1: §6.2.3.3, PDF/A-2/3/4: §6.2.4.3
            (1, "6.2.4.3") => Some("6.2.3.3"),

            // TR/TR2 transfer function restrictions
            // PDF/A-1: §6.2.8, PDF/A-2/3: §6.2.10.5, PDF/A-4: §6.2.5
            (1, "6.2.10.5") => Some("6.2.8"),
            (4, "6.2.10.5") => Some("6.2.5"),

            // Halftone restrictions
            // PDF/A-4: §6.2.5
            (4, "6.2.10") => Some("6.2.5"),
            (4, "6.2.10.4.1") => Some("6.2.5"),

            // Rendering intents
            // PDF/A-1: §6.2.9, PDF/A-2/3/4: §6.2.5
            (1, "6.2.5") => Some("6.2.9"),

            // Image XObject restrictions (OPI, Alternates, Interpolate)
            // PDF/A-1: §6.2.4 sub-clauses mapped to 6.2.8.x in PDF/A-2/3
            (1, "6.2.8.1") => Some("6.2.4"),
            (1, "6.2.8.2") => Some("6.2.4"),
            (1, "6.2.8.3") => Some("6.2.4"),

            // Object syntax spacing
            // PDF/A-2/3/4: §6.1.9
            (2..=4, "6.1.8") => Some("6.1.9"),

            _ => None,
        };
        if let Some(r) = new_rule {
            issue.rule = r.to_string();
        }
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

        assert_eq!(PdfALevel::A4.part(), 4);
        assert_eq!(PdfALevel::A4.conformance(), "");
        assert!(!PdfALevel::A4.requires_tagged());

        assert_eq!(PdfALevel::A4f.part(), 4);
        assert_eq!(PdfALevel::A4f.conformance(), "F");
        assert!(!PdfALevel::A4f.requires_tagged());

        assert_eq!(PdfALevel::A4e.part(), 4);
        assert_eq!(PdfALevel::A4e.conformance(), "E");
        assert!(!PdfALevel::A4e.requires_tagged());
    }

    #[test]
    fn pdfa4_from_parts() {
        assert_eq!(PdfALevel::from_parts(4, ""), Some(PdfALevel::A4));
        assert_eq!(PdfALevel::from_parts(4, "F"), Some(PdfALevel::A4f));
        assert_eq!(PdfALevel::from_parts(4, "f"), Some(PdfALevel::A4f));
        assert_eq!(PdfALevel::from_parts(4, "E"), Some(PdfALevel::A4e));
        assert_eq!(PdfALevel::from_parts(4, "e"), Some(PdfALevel::A4e));
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
