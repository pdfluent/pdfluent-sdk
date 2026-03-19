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
///
/// Uses two-phase validation: structural checks run first (fast, inspect PDF
/// object tree only).  If the document fails critical structural checks
/// (missing XMP, encrypted, invalid header), the expensive content-analysis
/// phase is skipped entirely — saving ~80% of check time for clearly
/// non-compliant PDFs.
///
/// Content-stream checks (undefined operators, marked content, inline image
/// filters) are combined into a single page loop to avoid redundant stream
/// decompression.
pub fn validate(pdf: &Pdf, level: PdfALevel) -> ComplianceReport {
    let mut report = ComplianceReport {
        pdfa_level: Some(level),
        ..Default::default()
    };

    // Pre-collect all objects once to avoid repeated O(n) parsing per pdf.objects() call.
    // Skip caching for very large PDFs (>20K objects) where the parse cost dominates.
    let obj_cache = check::ObjectCache::new_bounded(pdf, 20_000);

    // ═══════════════════════════════════════════════════════════════════════
    // Phase 1 — Structural checks (typically <5ms)
    //
    // Fast checks that only inspect the PDF object tree / trailer / XMP.
    // If the document is encrypted or missing XMP metadata, we already know
    // it cannot be PDF/A compliant — skip the expensive Phase 2.
    // ═══════════════════════════════════════════════════════════════════════

    check_xmp_metadata(pdf, level, &mut report);
    crate::xmp::validate_xmp(pdf, level, &mut report);
    check_encryption(pdf, &obj_cache, &mut report);
    check_file_header(pdf, level, &mut report);
    check_xref_format(pdf, &mut report);
    check_output_intent(pdf, level, &mut report);
    check_forbidden_actions(pdf, level, &mut report);
    check_annotation_flags(pdf, level, &mut report);
    check_annotation_types(pdf, level, &mut report);
    check_trailer_requirements(pdf, level, &mut report);
    check::check_xmp_pdfa_identification(pdf, &mut report);
    check::check_no_data_after_eof(pdf, &mut report);

    // Early exit: if critical structural checks already failed, skip content analysis.
    // "Critical" = missing XMP, encrypted, or wrong file header — these guarantee
    // non-compliance regardless of content.
    if has_critical_structural_failure(&report) {
        remap_clause_numbers(&mut report, level);
        report.compliant = report.is_compliant();
        return report;
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Phase 2 — Content analysis (typically 50-200ms)
    //
    // Expensive checks that decompress page content streams, scan fonts,
    // validate color spaces, etc.
    // ═══════════════════════════════════════════════════════════════════════

    check_font_embedding(pdf, &mut report);
    check_color_spaces(pdf, &mut report);
    check_device_colorspaces(pdf, &mut report);
    check_device_color_vs_output_intent(pdf, &mut report);
    check_page_dimensions(pdf, &obj_cache, level, &mut report);
    check_annotation_color_arrays(pdf, &mut report);
    check_form_xobjects(pdf, &mut report);
    check_page_boundary_sizes(pdf, &mut report);

    // Color space & graphics state validation (§6.2.x)
    check_icc_profile_version(pdf, level, &mut report);
    check_iccbased_alternate(pdf, &mut report);
    check_devicen_separation_alternate(pdf, &mut report);
    check::check_devicen_colorants(pdf, &mut report);
    check_rendering_intents(pdf, &mut report);
    check_image_xobjects(pdf, &mut report);
    check_halftone_and_transfer(pdf, &mut report);
    check_extgstate_restrictions(pdf, level, &mut report);
    check_cidfont_embedding(pdf, &mut report);
    check_cidfont_w_arrays(pdf, &mut report);
    check_cidsystem_info_consistency(pdf, &mut report);
    check_font_base_encoding(pdf, &mut report);
    check_output_intent_profile(pdf, &mut report);

    // File structure, actions, streams (§6.1.x, §6.6.1)
    check_all_page_boundaries(pdf, &mut report);
    check_stream_filters_cached(&obj_cache, level, &mut report);
    check_embedded_file_streams(pdf, &mut report);
    check_actions_deep(pdf, level, &mut report);
    check_form_xobject_geometry(pdf, &mut report);
    check_optional_content(pdf, level, &mut report);
    check_linearization(pdf, &mut report);

    // Deeper 6.2.x / 6.6.x fixes
    check_image_xobject_colorspaces(pdf, &mut report);
    check_output_intent_consistency(pdf, &mut report);
    check::check_output_intent_consistency_pdfa(pdf, level.part(), &mut report);
    check_transparency_vs_output_intent(pdf, level, &mut report);
    check::check_transparency_blending_vs_output_intent(pdf, level.part(), &mut report);
    check::check_output_intent_icc_signature(pdf, &mut report);

    // Combined content stream checks: undefined operators + marked content +
    // inline image filters in a single page loop (avoids 2 redundant
    // page.page_stream() decompression passes per page).
    check::check_page_content_streams_cached(pdf, level.part(), &mut report);

    // Font & Annotation deep validation (§6.3.x, §6.5.x)
    check_font_type_key(pdf, &mut report);
    check_font_embedding_deep(pdf, level, &mut report);
    check_tounicode_cmap(pdf, level, &mut report);
    check::check_tounicode_values(pdf, &mut report);
    check_font_widths(pdf, &mut report);
    check_font_program_widths(pdf, &mut report);
    if level.part() == 4 {
        check_truetype_cmap_pdfa4(pdf, &mut report);
        // §6.2.10.7/§6.2.10.9: ToUnicode CMap must cover all glyphs. (#467)
        check::check_tounicode_glyph_coverage(pdf, level.part(), &mut report);
        // §6.2.10.9: no .notdef glyph (CID 0x0000) in text operators. (#496)
        check::check_notdef_glyph_usage(pdf, &mut report);
    }
    check_symbolic_truetype_encoding(pdf, &mut report);
    check_cidtogidmap_identity(pdf, &mut report);
    check_cmap_embedding(pdf, &mut report);
    check::check_cidsysteminfo_compat(pdf, &mut report);
    check_annotation_appearance(pdf, &mut report);
    check_annotation_subtypes_deep(pdf, level, &mut report);
    check_annotation_flags_deep(pdf, level, &mut report);

    if level.part() == 1 {
        check_transparency_a1(pdf, &mut report);
    }

    // Transparency deep, tagged PDF, remaining rules
    check_transparency_deep(pdf, level, &mut report);
    check_blending_modes_pdfa(pdf, level, &mut report);
    check_soft_mask(pdf, &mut report);
    check_need_appearances_pdfa(pdf, &mut report);
    check::check_acroform_no_xfa(pdf, level.part(), &mut report);
    check_signature_restrictions_pdfa(pdf, &mut report);
    check_document_structure_pdfa(pdf, &mut report);
    check::check_stream_empty_keys_cached(&obj_cache, &mut report);

    // PDF/A-4 requires tagged PDF for all conformance levels;
    // PDF/A-1a/2a/3a require it only for level 'a'
    // Lang validation applies to all PDF/A levels (not just tagged)
    check_lang(pdf, level, &mut report);

    if level.requires_tagged() || level.part() == 4 {
        check_tagged_requirements(pdf, level, &mut report);
        check_table_structure_pdfa(pdf, &mut report);
        check_figure_alt(pdf, &mut report);
        check_role_mapping_pdfa(pdf, &mut report);
        check::check_mark_info(pdf, &mut report);
        check_lang_presence(pdf, &mut report);
    }

    match level.part() {
        3 => check_embedded_files_a3(pdf, &obj_cache, &mut report),
        4 => {
            check::check_pdfa4_conformance_absent(pdf, &mut report);
        }
        _ => check_no_embedded_files(pdf, &obj_cache, level, &mut report),
    }

    // Implementation limits & structural checks
    check_name_length_cached(pdf, &obj_cache, &mut report);
    check_real_value_range_cached(&obj_cache, level, &mut report);
    check_font_file_format_cached(&obj_cache, level, &mut report);
    check_explicit_resources(pdf, &mut report);
    check::check_name_utf8_cached(&obj_cache, &mut report);

    // Info/XMP consistency, stream/syntax, XMP extension, image intent
    check_info_xmp(pdf, &mut report);
    check_stream_length_pdfa(pdf, &mut report);
    check_object_syntax(pdf, level, &mut report);
    check_xmp_extension_schema_pdfa(pdf, &mut report);
    check_image_intent(pdf, &mut report);
    check_xref_syntax_pdfa(pdf, &mut report);
    check_embedded_file_spec(pdf, level, &mut report);
    check_postscript_xobjects_pdfa(pdf, level, &mut report);
    check::check_stream_external_refs_cached(&obj_cache, &mut report);
    check::check_widget_no_action(pdf, level.part(), &mut report);
    // PDF/A-1 §6.6.2: Field dictionary must not have /AA.
    if level.part() == 1 {
        check::check_field_aa_pdfa1(pdf, &mut report);
    }
    // PDF/A-2+ §6.4.2 test 2: Catalog must not contain /NeedsRendering.
    // ISO 19005-2/3/4 §6.4.2 test 2; veraPDF reports clause "6.4.2" for all parts ≥2.
    if level.part() >= 2 {
        check::check_catalog_needs_rendering(pdf, &mut report);
    }
    check::check_output_intent_profile_class(pdf, &mut report);
    check::check_hex_strings(pdf, &mut report);
    check::check_output_intent_destref(pdf, &mut report);

    // Post-process: remap clause numbers per PDF/A part.
    // Clause numbering differs between ISO 19005 parts.
    remap_clause_numbers(&mut report, level);

    report.compliant = report.is_compliant();
    report
}

/// Check if the report contains critical structural failures that make further
/// content analysis pointless.  These are issues that guarantee non-compliance
/// regardless of content: *missing* XMP metadata, encrypted, or invalid header.
///
/// Note: "XMP metadata stream is not valid UTF-8" is NOT critical — veraPDF
/// still reports Phase 2 violations for such files, so we must continue. Only
/// completely absent XMP triggers early exit. Fixes #467 (PDFBOX-1760-11).
fn has_critical_structural_failure(report: &ComplianceReport) -> bool {
    report.issues.iter().any(|issue| {
        issue.severity == crate::Severity::Error
            && (issue.message.contains("No XMP metadata")
                || issue.message.contains("missing XMP metadata stream")
                || issue.message.contains("encrypted")
                || issue.message.contains("Encrypt")
                || issue.message.contains("file header"))
    })
}

/// Validate with a progress tracker that records the name of each check as it
/// starts.  On timeout, the caller can read the tracker to see which check was
/// last running.
pub fn validate_with_progress(
    pdf: &Pdf,
    level: PdfALevel,
    progress: &std::sync::Mutex<String>,
) -> ComplianceReport {
    macro_rules! tracked {
        ($label:expr, $e:expr) => {{
            if let Ok(mut p) = progress.lock() {
                *p = $label.to_string();
            }
            $e;
        }};
    }

    let mut report = ComplianceReport {
        pdfa_level: Some(level),
        ..Default::default()
    };

    let obj_cache = check::ObjectCache::new_bounded(pdf, 20_000);

    // Phase 1 — Structural
    tracked!(
        "check_xmp_metadata",
        check_xmp_metadata(pdf, level, &mut report)
    );
    tracked!(
        "validate_xmp",
        crate::xmp::validate_xmp(pdf, level, &mut report)
    );
    tracked!(
        "check_encryption",
        check_encryption(pdf, &obj_cache, &mut report)
    );
    tracked!(
        "check_file_header",
        check_file_header(pdf, level, &mut report)
    );
    tracked!("check_xref_format", check_xref_format(pdf, &mut report));
    tracked!(
        "check_output_intent",
        check_output_intent(pdf, level, &mut report)
    );
    tracked!(
        "check_forbidden_actions",
        check_forbidden_actions(pdf, level, &mut report)
    );
    tracked!(
        "check_annotation_flags",
        check_annotation_flags(pdf, level, &mut report)
    );
    tracked!(
        "check_annotation_types",
        check_annotation_types(pdf, level, &mut report)
    );
    tracked!(
        "check_trailer_requirements",
        check_trailer_requirements(pdf, level, &mut report)
    );
    tracked!(
        "check_xmp_pdfa_identification",
        check::check_xmp_pdfa_identification(pdf, &mut report)
    );
    tracked!(
        "check_no_data_after_eof",
        check::check_no_data_after_eof(pdf, &mut report)
    );

    if has_critical_structural_failure(&report) {
        remap_clause_numbers(&mut report, level);
        report.compliant = report.is_compliant();
        return report;
    }

    // Phase 2 — Content analysis
    tracked!(
        "check_font_embedding",
        check_font_embedding(pdf, &mut report)
    );
    tracked!("check_color_spaces", check_color_spaces(pdf, &mut report));
    tracked!(
        "check_device_colorspaces",
        check_device_colorspaces(pdf, &mut report)
    );
    tracked!(
        "check_device_color_vs_output_intent",
        check_device_color_vs_output_intent(pdf, &mut report)
    );
    tracked!(
        "check_page_dimensions",
        check_page_dimensions(pdf, &obj_cache, level, &mut report)
    );
    tracked!(
        "check_annotation_color_arrays",
        check_annotation_color_arrays(pdf, &mut report)
    );
    tracked!("check_form_xobjects", check_form_xobjects(pdf, &mut report));
    tracked!(
        "check_page_boundary_sizes",
        check_page_boundary_sizes(pdf, &mut report)
    );
    tracked!(
        "check_icc_profile_version",
        check_icc_profile_version(pdf, level, &mut report)
    );
    tracked!(
        "check_iccbased_alternate",
        check_iccbased_alternate(pdf, &mut report)
    );
    tracked!(
        "check_devicen_separation_alternate",
        check_devicen_separation_alternate(pdf, &mut report)
    );
    tracked!(
        "check_devicen_colorants",
        check::check_devicen_colorants(pdf, &mut report)
    );
    tracked!(
        "check_rendering_intents",
        check_rendering_intents(pdf, &mut report)
    );
    tracked!(
        "check_image_xobjects",
        check_image_xobjects(pdf, &mut report)
    );
    tracked!(
        "check_halftone_and_transfer",
        check_halftone_and_transfer(pdf, &mut report)
    );
    tracked!(
        "check_extgstate_restrictions",
        check_extgstate_restrictions(pdf, level, &mut report)
    );
    tracked!(
        "check_cidfont_embedding",
        check_cidfont_embedding(pdf, &mut report)
    );
    tracked!(
        "check_cidfont_w_arrays",
        check_cidfont_w_arrays(pdf, &mut report)
    );
    tracked!(
        "check_cidsystem_info_consistency",
        check_cidsystem_info_consistency(pdf, &mut report)
    );
    tracked!(
        "check_font_base_encoding",
        check_font_base_encoding(pdf, &mut report)
    );
    tracked!(
        "check_output_intent_profile",
        check_output_intent_profile(pdf, &mut report)
    );
    tracked!(
        "check_all_page_boundaries",
        check_all_page_boundaries(pdf, &mut report)
    );
    tracked!(
        "check_stream_filters",
        check_stream_filters_cached(&obj_cache, level, &mut report)
    );
    tracked!(
        "check_embedded_file_streams",
        check_embedded_file_streams(pdf, &mut report)
    );
    tracked!(
        "check_actions_deep",
        check_actions_deep(pdf, level, &mut report)
    );
    tracked!(
        "check_form_xobject_geometry",
        check_form_xobject_geometry(pdf, &mut report)
    );
    tracked!(
        "check_optional_content",
        check_optional_content(pdf, level, &mut report)
    );
    tracked!("check_linearization", check_linearization(pdf, &mut report));
    tracked!(
        "check_image_xobject_colorspaces",
        check_image_xobject_colorspaces(pdf, &mut report)
    );
    tracked!(
        "check_output_intent_consistency",
        check_output_intent_consistency(pdf, &mut report)
    );
    tracked!(
        "check_output_intent_consistency_pdfa",
        check::check_output_intent_consistency_pdfa(pdf, level.part(), &mut report)
    );
    tracked!(
        "check_transparency_vs_output_intent",
        check_transparency_vs_output_intent(pdf, level, &mut report)
    );
    tracked!(
        "check_transparency_blending_vs_output_intent",
        check::check_transparency_blending_vs_output_intent(pdf, level.part(), &mut report)
    );
    tracked!(
        "check_output_intent_icc_signature",
        check::check_output_intent_icc_signature(pdf, &mut report)
    );
    tracked!(
        "check_page_content_streams_cached",
        check::check_page_content_streams_cached(pdf, level.part(), &mut report)
    );
    tracked!("check_font_type_key", check_font_type_key(pdf, &mut report));
    tracked!(
        "check_font_embedding_deep",
        check_font_embedding_deep(pdf, level, &mut report)
    );
    tracked!(
        "check_tounicode_cmap",
        check_tounicode_cmap(pdf, level, &mut report)
    );
    tracked!(
        "check_tounicode_values",
        check::check_tounicode_values(pdf, &mut report)
    );
    tracked!("check_font_widths", check_font_widths(pdf, &mut report));
    tracked!(
        "check_font_program_widths",
        check_font_program_widths(pdf, &mut report)
    );
    if level.part() == 4 {
        tracked!(
            "check_truetype_cmap_pdfa4",
            check_truetype_cmap_pdfa4(pdf, &mut report)
        );
        tracked!(
            "check_tounicode_glyph_coverage",
            check::check_tounicode_glyph_coverage(pdf, level.part(), &mut report)
        );
        // §6.2.10.9: no .notdef glyph (CID 0x0000) in text operators. (#496)
        tracked!(
            "check_notdef_glyph_usage",
            check::check_notdef_glyph_usage(pdf, &mut report)
        );
    }
    tracked!(
        "check_symbolic_truetype_encoding",
        check_symbolic_truetype_encoding(pdf, &mut report)
    );
    tracked!(
        "check_cidtogidmap_identity",
        check_cidtogidmap_identity(pdf, &mut report)
    );
    tracked!(
        "check_cmap_embedding",
        check_cmap_embedding(pdf, &mut report)
    );
    tracked!(
        "check_cidsysteminfo_compat",
        check::check_cidsysteminfo_compat(pdf, &mut report)
    );
    tracked!(
        "check_annotation_appearance",
        check_annotation_appearance(pdf, &mut report)
    );
    tracked!(
        "check_annotation_subtypes_deep",
        check_annotation_subtypes_deep(pdf, level, &mut report)
    );
    tracked!(
        "check_annotation_flags_deep",
        check_annotation_flags_deep(pdf, level, &mut report)
    );
    if level.part() == 1 {
        tracked!(
            "check_transparency_a1",
            check_transparency_a1(pdf, &mut report)
        );
    }
    tracked!(
        "check_transparency_deep",
        check_transparency_deep(pdf, level, &mut report)
    );
    tracked!(
        "check_blending_modes_pdfa",
        check_blending_modes_pdfa(pdf, level, &mut report)
    );
    tracked!("check_soft_mask", check_soft_mask(pdf, &mut report));
    tracked!(
        "check_need_appearances_pdfa",
        check_need_appearances_pdfa(pdf, &mut report)
    );
    tracked!(
        "check_acroform_no_xfa",
        check::check_acroform_no_xfa(pdf, level.part(), &mut report)
    );
    tracked!(
        "check_signature_restrictions_pdfa",
        check_signature_restrictions_pdfa(pdf, &mut report)
    );
    tracked!(
        "check_document_structure_pdfa",
        check_document_structure_pdfa(pdf, &mut report)
    );
    tracked!(
        "check_stream_empty_keys",
        check::check_stream_empty_keys_cached(&obj_cache, &mut report)
    );
    tracked!("check_lang", check_lang(pdf, level, &mut report));
    if level.requires_tagged() || level.part() == 4 {
        tracked!(
            "check_tagged_requirements",
            check_tagged_requirements(pdf, level, &mut report)
        );
        tracked!(
            "check_table_structure_pdfa",
            check_table_structure_pdfa(pdf, &mut report)
        );
        tracked!("check_figure_alt", check_figure_alt(pdf, &mut report));
        tracked!(
            "check_role_mapping_pdfa",
            check_role_mapping_pdfa(pdf, &mut report)
        );
        tracked!("check_mark_info", check::check_mark_info(pdf, &mut report));
    }
    match level.part() {
        3 => tracked!(
            "check_embedded_files_a3",
            check_embedded_files_a3(pdf, &obj_cache, &mut report)
        ),
        4 => tracked!(
            "check_pdfa4_conformance_absent",
            check::check_pdfa4_conformance_absent(pdf, &mut report)
        ),
        _ => tracked!(
            "check_no_embedded_files",
            check_no_embedded_files(pdf, &obj_cache, level, &mut report)
        ),
    }
    tracked!(
        "check_name_length",
        check_name_length_cached(pdf, &obj_cache, &mut report)
    );
    tracked!(
        "check_real_value_range",
        check_real_value_range_cached(&obj_cache, level, &mut report)
    );
    tracked!(
        "check_font_file_format",
        check_font_file_format_cached(&obj_cache, level, &mut report)
    );
    tracked!(
        "check_explicit_resources",
        check_explicit_resources(pdf, &mut report)
    );
    tracked!(
        "check_name_utf8",
        check::check_name_utf8_cached(&obj_cache, &mut report)
    );
    tracked!("check_info_xmp", check_info_xmp(pdf, &mut report));
    tracked!(
        "check_stream_length_pdfa",
        check_stream_length_pdfa(pdf, &mut report)
    );
    tracked!(
        "check_object_syntax",
        check_object_syntax(pdf, level, &mut report)
    );
    tracked!(
        "check_xmp_extension_schema",
        check_xmp_extension_schema_pdfa(pdf, &mut report)
    );
    tracked!("check_image_intent", check_image_intent(pdf, &mut report));
    tracked!(
        "check_xref_syntax_pdfa",
        check_xref_syntax_pdfa(pdf, &mut report)
    );
    tracked!(
        "check_embedded_file_spec",
        check_embedded_file_spec(pdf, level, &mut report)
    );
    tracked!(
        "check_postscript_xobjects",
        check_postscript_xobjects_pdfa(pdf, level, &mut report)
    );
    tracked!(
        "check_stream_external_refs",
        check::check_stream_external_refs_cached(&obj_cache, &mut report)
    );
    tracked!(
        "check_widget_no_action",
        check::check_widget_no_action(pdf, level.part(), &mut report)
    );
    if level.part() == 1 {
        tracked!(
            "check_field_aa_pdfa1",
            check::check_field_aa_pdfa1(pdf, &mut report)
        );
    }
    // PDF/A-2+ §6.4.2 test 2: Catalog must not contain /NeedsRendering.
    if level.part() >= 2 {
        tracked!(
            "check_catalog_needs_rendering",
            check::check_catalog_needs_rendering(pdf, &mut report)
        );
    }
    tracked!(
        "check_output_intent_profile_class",
        check::check_output_intent_profile_class(pdf, &mut report)
    );
    tracked!(
        "check_hex_strings",
        check::check_hex_strings(pdf, &mut report)
    );
    tracked!(
        "check_output_intent_destref",
        check::check_output_intent_destref(pdf, &mut report)
    );

    remap_clause_numbers(&mut report, level);
    report.compliant = report.is_compliant();
    report
}

/// Validate with per-check timing output to stderr (for profiling).
pub fn validate_timed(pdf: &Pdf, level: PdfALevel) -> ComplianceReport {
    use std::time::Instant;
    let mut report = ComplianceReport {
        pdfa_level: Some(level),
        ..Default::default()
    };

    macro_rules! timed {
        ($label:expr, $e:expr) => {{
            let t = Instant::now();
            $e;
            let d = t.elapsed();
            if d.as_millis() > 10 {
                eprintln!("  {:>8.1?}  {}", d, $label);
            }
        }};
    }

    let t_cache = Instant::now();
    let obj_cache = check::ObjectCache::new_bounded(pdf, 20_000);
    eprintln!(
        "  {:>8.1?}  ObjectCache ({})",
        t_cache.elapsed(),
        if obj_cache.is_empty() {
            "skipped"
        } else {
            "built"
        }
    );

    // Batch 1
    timed!(
        "check_xmp_metadata",
        check_xmp_metadata(pdf, level, &mut report)
    );
    timed!(
        "validate_xmp",
        crate::xmp::validate_xmp(pdf, level, &mut report)
    );
    timed!(
        "check_encryption",
        check_encryption(pdf, &obj_cache, &mut report)
    );
    timed!(
        "check_forbidden_actions",
        check_forbidden_actions(pdf, level, &mut report)
    );
    timed!(
        "check_output_intent",
        check_output_intent(pdf, level, &mut report)
    );
    timed!(
        "check_font_embedding",
        check_font_embedding(pdf, &mut report)
    );
    timed!("check_color_spaces", check_color_spaces(pdf, &mut report));
    timed!(
        "check_device_colorspaces",
        check_device_colorspaces(pdf, &mut report)
    );
    timed!(
        "check_device_color_vs_output_intent",
        check_device_color_vs_output_intent(pdf, &mut report)
    );
    timed!(
        "check_page_dimensions",
        check_page_dimensions(pdf, &obj_cache, level, &mut report)
    );
    timed!(
        "check_annotation_flags",
        check_annotation_flags(pdf, level, &mut report)
    );
    timed!(
        "check_annotation_types",
        check_annotation_types(pdf, level, &mut report)
    );
    timed!(
        "check_annotation_color_arrays",
        check_annotation_color_arrays(pdf, &mut report)
    );
    timed!("check_form_xobjects", check_form_xobjects(pdf, &mut report));
    timed!(
        "check_page_boundary_sizes",
        check_page_boundary_sizes(pdf, &mut report)
    );

    // Batch 2
    timed!(
        "check_icc_profile_version",
        check_icc_profile_version(pdf, level, &mut report)
    );
    timed!(
        "check_iccbased_alternate",
        check_iccbased_alternate(pdf, &mut report)
    );
    timed!(
        "check_devicen_separation_alternate",
        check_devicen_separation_alternate(pdf, &mut report)
    );
    timed!(
        "check_devicen_colorants",
        check::check_devicen_colorants(pdf, &mut report)
    );
    timed!(
        "check_rendering_intents",
        check_rendering_intents(pdf, &mut report)
    );
    timed!(
        "check_image_xobjects",
        check_image_xobjects(pdf, &mut report)
    );
    timed!(
        "check_halftone_and_transfer",
        check_halftone_and_transfer(pdf, &mut report)
    );
    timed!(
        "check_extgstate_restrictions",
        check_extgstate_restrictions(pdf, level, &mut report)
    );
    timed!(
        "check_cidfont_embedding",
        check_cidfont_embedding(pdf, &mut report)
    );
    timed!(
        "check_cidsystem_info_consistency",
        check_cidsystem_info_consistency(pdf, &mut report)
    );
    timed!(
        "check_font_base_encoding",
        check_font_base_encoding(pdf, &mut report)
    );
    timed!(
        "check_output_intent_profile",
        check_output_intent_profile(pdf, &mut report)
    );

    // Batch 3
    timed!(
        "check_all_page_boundaries",
        check_all_page_boundaries(pdf, &mut report)
    );
    timed!(
        "check_stream_filters",
        check_stream_filters_cached(&obj_cache, level, &mut report)
    );
    timed!(
        "check_embedded_file_streams",
        check_embedded_file_streams(pdf, &mut report)
    );
    timed!(
        "check_file_header",
        check_file_header(pdf, level, &mut report)
    );
    timed!("check_xref_format", check_xref_format(pdf, &mut report));
    timed!(
        "check_actions_deep",
        check_actions_deep(pdf, level, &mut report)
    );
    timed!(
        "check_form_xobject_geometry",
        check_form_xobject_geometry(pdf, &mut report)
    );
    timed!(
        "check_optional_content",
        check_optional_content(pdf, level, &mut report)
    );
    timed!("check_linearization", check_linearization(pdf, &mut report));

    // Iteration 11
    timed!(
        "check_image_xobject_colorspaces",
        check_image_xobject_colorspaces(pdf, &mut report)
    );
    timed!(
        "check_output_intent_consistency",
        check_output_intent_consistency(pdf, &mut report)
    );
    timed!(
        "check_output_intent_consistency_pdfa",
        check::check_output_intent_consistency_pdfa(pdf, level.part(), &mut report)
    );
    timed!(
        "check_transparency_vs_output_intent",
        check_transparency_vs_output_intent(pdf, level, &mut report)
    );
    timed!(
        "check_transparency_blending_vs_output_intent",
        check::check_transparency_blending_vs_output_intent(pdf, level.part(), &mut report)
    );
    timed!(
        "check_output_intent_icc_signature",
        check::check_output_intent_icc_signature(pdf, &mut report)
    );
    timed!(
        "check_page_content_streams_cached",
        check::check_page_content_streams_cached(pdf, level.part(), &mut report)
    );

    // Batch 4
    timed!("check_font_type_key", check_font_type_key(pdf, &mut report));
    timed!(
        "check_font_embedding_deep",
        check_font_embedding_deep(pdf, level, &mut report)
    );
    timed!(
        "check_tounicode_cmap",
        check_tounicode_cmap(pdf, level, &mut report)
    );
    timed!(
        "check_tounicode_values",
        check::check_tounicode_values(pdf, &mut report)
    );
    timed!("check_font_widths", check_font_widths(pdf, &mut report));
    timed!(
        "check_font_program_widths",
        check_font_program_widths(pdf, &mut report)
    );
    if level.part() == 4 {
        timed!(
            "check_truetype_cmap_pdfa4",
            check_truetype_cmap_pdfa4(pdf, &mut report)
        );
        timed!(
            "check_tounicode_glyph_coverage",
            check::check_tounicode_glyph_coverage(pdf, level.part(), &mut report)
        );
        // §6.2.10.9: no .notdef glyph (CID 0x0000) in text operators. (#496)
        timed!(
            "check_notdef_glyph_usage",
            check::check_notdef_glyph_usage(pdf, &mut report)
        );
    }
    timed!(
        "check_symbolic_truetype_encoding",
        check_symbolic_truetype_encoding(pdf, &mut report)
    );
    timed!(
        "check_cidtogidmap_identity",
        check_cidtogidmap_identity(pdf, &mut report)
    );
    timed!(
        "check_cmap_embedding",
        check_cmap_embedding(pdf, &mut report)
    );
    timed!(
        "check_cidsysteminfo_compat",
        check::check_cidsysteminfo_compat(pdf, &mut report)
    );
    timed!(
        "check_annotation_appearance",
        check_annotation_appearance(pdf, &mut report)
    );
    timed!(
        "check_annotation_subtypes_deep",
        check_annotation_subtypes_deep(pdf, level, &mut report)
    );
    timed!(
        "check_annotation_flags_deep",
        check_annotation_flags_deep(pdf, level, &mut report)
    );

    if level.part() == 1 {
        timed!(
            "check_transparency_a1",
            check_transparency_a1(pdf, &mut report)
        );
    }

    // Batch 5
    timed!(
        "check_transparency_deep",
        check_transparency_deep(pdf, level, &mut report)
    );
    timed!(
        "check_blending_modes_pdfa",
        check_blending_modes_pdfa(pdf, level, &mut report)
    );
    timed!("check_soft_mask", check_soft_mask(pdf, &mut report));
    timed!(
        "check_need_appearances_pdfa",
        check_need_appearances_pdfa(pdf, &mut report)
    );
    timed!(
        "check_acroform_no_xfa",
        check::check_acroform_no_xfa(pdf, level.part(), &mut report)
    );
    timed!(
        "check_signature_restrictions_pdfa",
        check_signature_restrictions_pdfa(pdf, &mut report)
    );
    timed!(
        "check_document_structure_pdfa",
        check_document_structure_pdfa(pdf, &mut report)
    );
    timed!(
        "check_xmp_pdfa_identification",
        check::check_xmp_pdfa_identification(pdf, &mut report)
    );
    timed!(
        "check_stream_empty_keys",
        check::check_stream_empty_keys_cached(&obj_cache, &mut report)
    );
    timed!("check_lang", check_lang(pdf, level, &mut report));

    // Batch 6
    timed!(
        "check_name_length_limit",
        check::check_name_length_limit_cached(&obj_cache, &mut report)
    );
    timed!(
        "check_array_capacity_limit",
        check::check_array_capacity_limit_cached(&obj_cache, &mut report)
    );
    timed!(
        "check_cid_value_limit",
        check::check_cid_value_limit(pdf, &mut report)
    );
    timed!(
        "check_near_zero_reals",
        check::check_near_zero_reals_cached(&obj_cache, &mut report)
    );
    timed!(
        "check_integer_range",
        check::check_integer_range_cached(&obj_cache, &mut report)
    );
    timed!(
        "check_real_value_range",
        check_real_value_range_cached(&obj_cache, level, &mut report)
    );
    timed!(
        "check_font_file_format",
        check_font_file_format_cached(&obj_cache, level, &mut report)
    );
    timed!(
        "check_explicit_resources",
        check_explicit_resources(pdf, &mut report)
    );
    timed!(
        "check_name_utf8",
        check::check_name_utf8_cached(&obj_cache, &mut report)
    );
    timed!(
        "check_trailer_requirements",
        check_trailer_requirements(pdf, level, &mut report)
    );

    // Batch 7
    timed!("check_info_xmp", check_info_xmp(pdf, &mut report));
    timed!(
        "check_stream_length_pdfa",
        check_stream_length_pdfa(pdf, &mut report)
    );
    timed!(
        "check_object_syntax",
        check_object_syntax(pdf, level, &mut report)
    );
    timed!(
        "check_xmp_extension_schema",
        check_xmp_extension_schema_pdfa(pdf, &mut report)
    );
    timed!("check_image_intent", check_image_intent(pdf, &mut report));
    timed!(
        "check_xref_syntax_pdfa",
        check_xref_syntax_pdfa(pdf, &mut report)
    );
    timed!(
        "check_embedded_file_spec",
        check_embedded_file_spec(pdf, level, &mut report)
    );
    timed!(
        "check_postscript_xobjects",
        check_postscript_xobjects_pdfa(pdf, level, &mut report)
    );
    timed!(
        "check_stream_external_refs",
        check::check_stream_external_refs_cached(&obj_cache, &mut report)
    );
    timed!(
        "check_no_data_after_eof",
        check::check_no_data_after_eof(pdf, &mut report)
    );
    timed!(
        "check_widget_no_action",
        check::check_widget_no_action(pdf, level.part(), &mut report)
    );
    // PDF/A-1 §6.6.2: Field /AA forbidden; PDF/A-4 §6.4.2: NeedsRendering forbidden.
    if level.part() == 1 {
        timed!(
            "check_field_aa_pdfa1",
            check::check_field_aa_pdfa1(pdf, &mut report)
        );
    }
    // PDF/A-2+ §6.4.2 test 2: Catalog must not contain /NeedsRendering.
    if level.part() >= 2 {
        timed!(
            "check_catalog_needs_rendering",
            check::check_catalog_needs_rendering(pdf, &mut report)
        );
    }
    timed!(
        "check_output_intent_profile_class",
        check::check_output_intent_profile_class(pdf, &mut report)
    );
    timed!(
        "check_hex_strings",
        check::check_hex_strings(pdf, &mut report)
    );
    timed!(
        "check_output_intent_destref",
        check::check_output_intent_destref(pdf, &mut report)
    );

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
        // If the catalog has a /Metadata key but the pointed-to object is not a stream
        // (e.g., points to a Font dict — as in PDFBOX-3105-1), and the trailer also has
        // an /Info reference, then metadata synchronization (§6.7.3) is broken.
        // This check must run in Phase 1 because Phase 2 is skipped when XMP is absent.
        // Fixes #467 (PDFBOX-3105-1).
        if let Some(cat) = check::catalog(pdf) {
            use pdf_syntax::object::dict::keys;
            if cat.contains_key(keys::METADATA) {
                let raw = pdf.data().as_ref();
                if raw.windows(5).any(|w| w == b"/Info") {
                    check::error(
                        report,
                        "6.7.3",
                        "XMP Metadata stream pointer is corrupt (does not resolve to a stream)",
                    );
                }
            }
        }
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
    // PDF/A-4 base level has no conformance letter — empty is valid.
    // Case-SENSITIVE check: 'u' vs 'U' is a violation (veraPDF reports §6.6.4 for PDF/A-2/3).
    // Previously used eq_ignore_ascii_case which missed case-only errors. Fixes #467 (ZTESTZUGFERD).
    if !(conformance == expected_conf || (expected_conf.is_empty() && conformance.is_empty())) {
        check::error(
            report,
            rule,
            format!("XMP pdfaid:conformance={conformance} does not match expected {expected_conf}"),
        );
    }
}

/// §6.1.1 — PDF/A documents shall not be encrypted.
fn check_encryption(pdf: &Pdf, cache: &check::ObjectCache<'_>, report: &mut ComplianceReport) {
    if check::is_encrypted_cached(pdf, cache) {
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
fn check_output_intent(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    // PDF/A-4 does not require a GTS_PDFA1 OutputIntent subtype.
    // ISO 19005-4 §6.2.3 only requires that OutputIntent entries are valid ICC profiles
    // (checked separately); emitting "6.2.2" here for PDF/A-4 is a false positive.
    if level.part() == 4 {
        return;
    }
    // §6.6.2 in PDF/A-1 (ISO 19005-1), §6.2.2 in PDF/A-2/3.
    // Use "6.6.2-oi" (OutputIntent variant) so remap_clause_numbers can distinguish
    // this from field/catalog /AA violations that also use "6.6.2" but must NOT be
    // remapped to "6.2.2" (veraPDF reports §6.6.2 for /AA violations in PDF/A-1).
    let rule = if level.part() == 1 { "6.6.2-oi" } else { "6.2.2" };
    if !check::has_output_intent(pdf) {
        check::error(
            report,
            rule,
            "No OutputIntents with GTS_PDFA1 subtype found",
        );
        return;
    }
    // ISO 19005-1 §6.2.2: at most one OutputIntent with S=GTS_PDFA1 is allowed.
    // Having two GTS_PDFA1 entries is a violation even if they carry the same profile.
    // veraPDF reports "6.2.2" for this (confirmed by isartor 6-2-2-t03). (#FN-6.2.2)
    let gts_count = check::count_gts_pdfa1_intents(pdf);
    if gts_count > 1 {
        check::error(
            report,
            rule,
            "OutputIntents array has more than one entry with S=GTS_PDFA1 (at most one allowed)",
        );
        return;
    }
    // GTS_PDFA1 OutputIntent must have a DestOutputProfile
    if check::output_intent_profile_components(pdf).is_none() {
        check::error(
            report,
            rule,
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
                // No FontDescriptor and no DescendantFonts — font metadata entirely absent.
                // A font with no FontDescriptor is implicitly not embedded (§6.2.11.4.1 in
                // PDF/A-2/3, §6.3.4 in PDF/A-1). The missing descriptor itself also violates
                // §6.2.11.4.2. veraPDF emits both rules for this case. Fixes #467/#474.
                check::error_at(
                    report,
                    "6.3.3",
                    format!("Font {name} is not embedded (no FontDescriptor)"),
                    format!("page {}", page_idx + 1),
                );
                check::error_at(
                    report,
                    "6.3.3-nd",
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
fn check_page_dimensions(
    pdf: &Pdf,
    cache: &check::ObjectCache<'_>,
    level: PdfALevel,
    report: &mut ComplianceReport,
) {
    check::check_page_dimensions_with_cache(pdf, cache, level.part(), report);
    // PDF/A-4: catalog Version must be present and match "2.n" (ISO 19005-4 §6.1.12).
    // check_catalog_version_pdfa4 validates the FORMAT when the key is present,
    // but does NOT flag the key's absence. PDF/A-4 §6.1.12 requires Version to be present.
    // Add a supplementary required-presence check. (#FN-6.1.12)
    if level.part() == 4 {
        check::check_catalog_version_pdfa4(pdf, report);
        // Supplement: Version key must be present in the catalog for PDF/A-4.
        if let Some(cat) = check::catalog(pdf) {
            if cat
                .get::<pdf_syntax::object::Object<'_>>(b"Version" as &[u8])
                .is_none()
            {
                check::error(
                    report,
                    "6.1.12",
                    "Catalog dictionary missing required /Version key (PDF/A-4 §6.1.12)",
                );
            }
        }
    }
    // PDF/A-2/3/4 §6.1.13: string literals used as content-stream operands must
    // not exceed 32767 bytes (decoded). check.rs only enforces the 65535-byte
    // object-level limit via check_string_lengths_cached. (#496)
    if level.part() >= 2 {
        for (page_idx, page) in pdf.pages().iter().enumerate() {
            if let Some(content) = page.page_stream() {
                if content_stream_has_long_string(content) {
                    check::error_at(
                        report,
                        "6.1.13",
                        "Content stream contains string literal exceeding 32767 bytes",
                        format!("page {}", page_idx + 1),
                    );
                }
            }
        }
    }
}

/// Scan a decoded content-stream byte slice for string literals > 32767 bytes.
///
/// Both literal `(...)` and hex `<...>` string forms are checked. Returns true
/// on the first offending string so callers can report and bail out quickly.
fn content_stream_has_long_string(data: &[u8]) -> bool {
    const LIMIT: usize = 32767;
    let mut pos = 0;
    let len = data.len();
    // Fast path: if the entire stream is shorter than the limit, no string can exceed it.
    if len <= LIMIT {
        return false;
    }
    while pos < len {
        match data[pos] {
            b'(' => {
                // Literal string: scan to matching ')' counting nesting and escapes;
                // accumulate decoded byte count.
                let mut depth: i32 = 1;
                let mut decoded: usize = 0;
                pos += 1; // skip opening '('
                while pos < len && depth > 0 {
                    match data[pos] {
                        b'\\' => {
                            pos += 1;
                            if pos >= len {
                                break;
                            }
                            match data[pos] {
                                b'0'..=b'7' => {
                                    // Octal escape \ddd (1–3 octal digits) → 1 decoded byte
                                    let mut n = 1usize;
                                    while n < 3
                                        && pos + n < len
                                        && matches!(data[pos + n], b'0'..=b'7')
                                    {
                                        n += 1;
                                    }
                                    pos += n;
                                    decoded += 1;
                                }
                                b'\n' => {
                                    // \<LF> — line continuation, 0 decoded bytes
                                    pos += 1;
                                }
                                b'\r' => {
                                    // \<CR> or \<CRLF> — line continuation, 0 decoded bytes
                                    pos += 1;
                                    if pos < len && data[pos] == b'\n' {
                                        pos += 1;
                                    }
                                }
                                _ => {
                                    // \n, \t, \\, \(, \), \b, \f, etc. → 1 decoded byte
                                    pos += 1;
                                    decoded += 1;
                                }
                            }
                        }
                        b'(' => {
                            depth += 1;
                            pos += 1;
                            decoded += 1;
                        }
                        b')' => {
                            depth -= 1;
                            if depth > 0 {
                                decoded += 1;
                            }
                            pos += 1;
                        }
                        _ => {
                            decoded += 1;
                            pos += 1;
                        }
                    }
                    if decoded > LIMIT {
                        return true;
                    }
                }
            }
            b'<' if pos + 1 < len && data[pos + 1] != b'<' => {
                // Hex string <hexdigits>: decoded length = ceil(hex_digit_count / 2)
                pos += 1; // skip '<'
                let mut hex_count: usize = 0;
                while pos < len && data[pos] != b'>' {
                    if data[pos].is_ascii_hexdigit() {
                        hex_count += 1;
                    }
                    pos += 1;
                }
                if hex_count.div_ceil(2) > LIMIT {
                    return true;
                }
                if pos < len {
                    pos += 1; // skip '>'
                }
            }
            b'%' => {
                // Comment — skip to end of line
                while pos < len && data[pos] != b'\n' && data[pos] != b'\r' {
                    pos += 1;
                }
            }
            _ => {
                pos += 1;
            }
        }
    }
    false
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
    // Supplementary: check ALL page boundaries (CropBox, TrimBox, BleedBox, ArtBox),
    // not just MediaBox. veraPDF §6.1.13 t11 checks all boundary types ≥ 3 / ≤ 14400.
    // check.rs only checks MediaBox via page.media_box(). Also check inheritable
    // boundaries on /Pages parent dicts via raw byte scan.
    check_boundary_sizes_raw(pdf, report);
}

/// Scan raw PDF bytes for boundary rectangles that violate size limits.
/// Catches boundaries on both /Page and /Pages (parent) dicts.
fn check_boundary_sizes_raw(pdf: &Pdf, report: &mut ComplianceReport) {
    let data = pdf.data().as_ref();
    for key in [b"/CropBox" as &[u8], b"/TrimBox", b"/BleedBox", b"/ArtBox"] {
        let key_str = std::str::from_utf8(&key[1..]).unwrap_or("?");
        // Scan for key in raw bytes
        for i in 0..data.len().saturating_sub(key.len()) {
            if &data[i..i + key.len()] != key {
                continue;
            }
            // Skip whitespace after key, find '['
            let mut j = i + key.len();
            while j < data.len()
                && (data[j] == b' ' || data[j] == b'\n' || data[j] == b'\r' || data[j] == b'\t')
            {
                j += 1;
            }
            if j >= data.len() || data[j] != b'[' {
                continue;
            }
            j += 1;
            // Find closing ']'
            let start = j;
            while j < data.len() && data[j] != b']' {
                j += 1;
            }
            if j >= data.len() {
                continue;
            }
            // Parse numbers from the array content
            if let Ok(inner) = std::str::from_utf8(&data[start..j]) {
                let nums: Vec<f64> = inner
                    .split_whitespace()
                    .filter_map(|s| s.parse::<f64>().ok())
                    .collect();
                if nums.len() == 4 {
                    let w = (nums[2] - nums[0]).abs();
                    let h = (nums[3] - nums[1]).abs();
                    if w < 3.0 || h < 3.0 {
                        check::error(
                            report,
                            "6.1.13",
                            format!("/{key_str} {w:.1}x{h:.1} less than minimum 3 units"),
                        );
                        return;
                    }
                    if w > 14400.0 || h > 14400.0 {
                        check::error(
                            report,
                            "6.1.13",
                            format!("/{key_str} {w:.1}x{h:.1} exceeds maximum 14400 units"),
                        );
                        return;
                    }
                }
            }
        }
    }
}

/// §6.2.3.3 — ICC profile version must match PDF/A part.
fn check_icc_profile_version(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_icc_profile_version(pdf, level.part(), report);
}

/// §6.2.4.2 — ICCBased Alternate CS must be consistent with profile, and
/// ICCBased CMYK must not be identical to the OutputIntent or transparency CS.
fn check_iccbased_alternate(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_iccbased_alternate(pdf, report);
    // §6.2.4.2 test 3: ICCBased CMYK profile must not be identical (by indirect
    // object reference) to the OutputIntent DestOutputProfile or current
    // transparency blending colorspace. Fixes #467.
    check::check_iccbased_cmyk_not_identical_to_outputintent(pdf, report);
}

/// §6.2.4.4 — DeviceN/Separation alternate CS restrictions.
fn check_devicen_separation_alternate(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_devicen_separation_alternate(pdf, report);
    // §6.2.4.4: all Separation arrays with the same colorant name must have the
    // same alternateSpace (cross-document consistency check). Fixes #467.
    check::check_separation_consistency(pdf, report);
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

/// §6.2.11.6 — CIDFont must have /W (widths) or /DW (default width).
fn check_cidfont_w_arrays(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_cidfont_w_arrays(pdf, report);
}

/// §6.2.10.3.1 — CIDFont and CMap CIDSystemInfo Registry/Ordering must match.
fn check_cidsystem_info_consistency(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_cidsystem_info_consistency(pdf, report);
}

/// §6.2.11.6 — Font Encoding BaseEncoding must be WinAnsiEncoding or MacRomanEncoding.
fn check_font_base_encoding(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_font_base_encoding(pdf, report);
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
fn check_stream_filters_cached(
    cache: &check::ObjectCache<'_>,
    level: PdfALevel,
    report: &mut ComplianceReport,
) {
    check::check_stream_filters_cached(cache, level.part(), report);
}

/// §6.1.7, §6.1.7.1 — Embedded file stream type.
fn check_embedded_file_streams(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_embedded_file_streams(pdf, report);
}

/// §6.1.2 — File header binary comment and version format.
fn check_file_header(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_file_header(pdf, level.part(), report);
    // Supplementary check: binary comment must immediately follow the header line's EOL.
    // veraPDF §6.1.2 t2: "The aforementioned EOL marker shall be immediately followed by
    // a % character followed by at least four bytes, each > 127."
    // check.rs scans the first 512 bytes for any binary comment, but doesn't verify
    // it's on the second line. This catches the case where there's an extra blank line
    // between the header and the binary comment.
    let data = pdf.data().as_ref();
    if data.starts_with(b"%PDF-") && data.len() >= 9 {
        // veraPDF §6.1.2 t1: header line must be exactly %PDF-M.N followed by EOL.
        // Trailing spaces before EOL are a violation.
        let ver_end = 8; // %PDF- = 5 bytes, M.N = 3 bytes → position 8
        if ver_end < data.len() && data[ver_end] == b' ' {
            check::error(
                report,
                "6.1.2",
                "File header has trailing whitespace after %PDF-M.N version",
            );
        }
        // veraPDF §6.1.2 t2: binary comment must immediately follow header EOL.
        if let Some(eol_pos) = data[5..data.len().min(20)]
            .iter()
            .position(|&b| b == b'\n' || b == b'\r')
        {
            let mut after_eol = 5 + eol_pos + 1;
            // Skip CRLF pair
            if after_eol < data.len()
                && data[5 + eol_pos] == b'\r'
                && data.get(after_eol) == Some(&b'\n')
            {
                after_eol += 1;
            }
            // The byte immediately after the header line's EOL must be '%'
            if after_eol < data.len() && data[after_eol] != b'%' {
                check::error(
                    report,
                    "6.1.2",
                    "Binary comment not immediately after header line EOL",
                );
            }
        }
    }
}

/// §6.1.3 — Cross-reference table format.
fn check_xref_format(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_xref_format(pdf, report);
}

/// §6.6.1, §6.1.6.x — Deep recursive action scanner.
fn check_actions_deep(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    let rule = match level.part() {
        1 => "6.6.1",
        // PDF/A-4: ISO 19005-4 §6.6.1 covers forbidden actions. normalize_pdfa4_clause
        // maps "6.6.1" → "6.8.1" which matches veraPDF's clause "6.6.1" normalized.
        // Previously emitted "6.4" which mapped to "6.6" — wrong. Fixes #482.
        4 => "6.6.1",
        _ => "6.5.1",
    };
    check::check_actions_deep(pdf, level.part(), rule, report);
    // Supplementary: catalog /AA is forbidden in PDF/A-4.
    // veraPDF §6.6.3 (normalizes to §6.8.3): catalog /AA entry prohibited.
    // check.rs checks page/annotation /AA but not catalog /AA.
    if let Some(cat) = check::catalog(pdf) {
        if cat.get::<Dict<'_>>(keys::AA).is_some() {
            check::error(
                report,
                "6.1.6.1",
                "Document Catalog contains forbidden /AA entry",
            );
        }
    }
    // Supplementary: /AA on non-widget annotations is forbidden in PDF/A.
    // check.rs only checks action TYPES within /AA, not the mere presence.
    // veraPDF §6.6.3 (PDF/A-4) / §6.5.2 (PDF/A-2/3) flags /AA on non-widget annots.
    // Emit "6.1.6.1" which gets remapped per part by remap_clause_numbers.
    for page in pdf.pages().iter() {
        if let Some(annots) = page
            .raw()
            .get::<pdf_syntax::object::Array<'_>>(keys::ANNOTS)
        {
            for annot in annots.iter::<Dict<'_>>() {
                let is_widget = annot
                    .get::<Name>(keys::SUBTYPE)
                    .is_some_and(|s| s.as_ref() == b"Widget");
                if !is_widget && annot.get::<Dict<'_>>(keys::AA).is_some() {
                    check::error(
                        report,
                        "6.1.6.1",
                        "Non-widget annotation has /AA entry (forbidden in PDF/A)",
                    );
                    return; // Report once
                }
            }
        }
    }
}

/// §6.1.10 — Form XObject BBox validation.
fn check_form_xobject_geometry(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_form_xobject_geometry(pdf, report);
}

/// §6.1.11 — Optional content restrictions.
fn check_optional_content(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    let before = report.issues.len();
    check::check_optional_content(pdf, level.part(), report);
    // PDF/A-2/3: check.rs emits "6.6.4" for OC violations; veraPDF uses "6.9" for these.
    // Re-tag directly here so the blanket remap isn't needed (and doesn't clobber the
    // XMP-conformance "6.6.4" emitted by check_xmp_metadata). (#FN-6.9, #FN-6.6.4)
    if matches!(level.part(), 2 | 3) {
        for issue in &mut report.issues[before..] {
            if issue.rule == "6.6.4" {
                issue.rule = "6.9".to_string();
            }
        }
    }
    // veraPDF also reports "6.1.11" as the parent OC clause. Emit it too. (#FN-6.1.11)
    if matches!(level.part(), 2 | 3)
        && report.issues.len() > before
        && !report.issues.iter().any(|i| i.rule == "6.1.11")
    {
        check::error(
            report,
            "6.1.11",
            "Optional content (OCG/OCMD) not permitted in PDF/A-2/3 (§6.1.11)",
        );
    }
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
/// PDF/A-4 uses ISO 19005-4 clause 6.6.1 (tagged PDF structure); normalize_pdfa4_clause
/// maps "6.6.1" → "6.8.1" so both sides of the comparison agree. Fixes #482.
fn check_tagged_requirements(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    // PDF/A-4: ISO 19005-4 §6.6.1 covers tagged PDF. normalize_pdfa4_clause("6.6.1")="6.8.1".
    // PDF/A-1/2/3: §6.8 for MarkInfo, §6.8.3.3 for StructTreeRoot. Fixes #482.
    let mark_rule = if level.part() == 4 { "6.6.1" } else { "6.8" };
    // veraPDF uses §6.8.3.3 specifically for missing StructTreeRoot in PDF/A-1/2/3.
    // PDF/A-4: still §6.6.1 (single clause for all tagged requirements).
    let struct_rule = if level.part() == 4 {
        "6.6.1"
    } else {
        "6.8.3.3"
    };
    if !check::is_marked(pdf) {
        check::error(
            report,
            mark_rule,
            "Document is not marked (MarkInfo/Marked missing or false)",
        );
    }

    if check::struct_tree_root(pdf).is_none() {
        check::error(report, struct_rule, "No StructTreeRoot found");
    }
}

/// PDF/A-3 allows embedded files; check they have proper AF relationships.
fn check_embedded_files_a3(
    pdf: &Pdf,
    cache: &check::ObjectCache<'_>,
    report: &mut ComplianceReport,
) {
    if check::has_embedded_files_cached(pdf, cache) {
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
fn check_tounicode_cmap(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_tounicode_cmap(pdf, level.part(), report);
}

/// §6.3.5 — Font /Widths array.
fn check_font_widths(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_font_widths(pdf, report);
}

/// §6.2.11.5 / §6.2.10.5 — Font program widths consistent with /Widths dict.
fn check_font_program_widths(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_font_program_widths(pdf, report);
}

/// §6.2.10.4.1 — TrueType simple-font Mac Roman cmap validity (PDF/A-4 only).
fn check_truetype_cmap_pdfa4(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_truetype_cmap_pdfa4(pdf, report);
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

/// §6.7.4 (PDF/A-2/3) / §6.8.4 (PDF/A-1/4) — Lang values must be valid BCP-47.
///
/// veraPDF uses §6.7.4 for PDF/A-2/3 Lang validation, §6.8.4 for others.
fn check_lang(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    // PDF/A-2/3: veraPDF uses §6.7.4 for Lang entry validation.
    let rule = match level.part() {
        2 | 3 => "6.7.4",
        _ => "6.8.4",
    };
    check::check_lang_values(pdf, rule, report);
}

/// Check that tagged PDFs have a /Lang entry in the catalog.
/// veraPDF §6.8.4 requires this for conformance levels that mandate tagging.
fn check_lang_presence(pdf: &Pdf, report: &mut ComplianceReport) {
    if let Some(cat) = check::catalog(pdf) {
        if cat.get::<pdf_syntax::object::String>(keys::LANG).is_none() {
            check::error(
                report,
                "6.8.4",
                "Catalog does not have a /Lang entry (required for tagged PDF)",
            );
        }
    }
}

/// §6.9 — NeedAppearances and field appearances.
fn check_need_appearances_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_need_appearances(pdf, report);
}

/// §6.10 — Digital signature restrictions.
fn check_signature_restrictions_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_signature_restrictions(pdf, report);
    // §6.4.3 (PDF/A-2/3): ByteRange must cover entire file. Fixes #475.
    check::check_sig_byterange_coverage(pdf, report);
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
fn check_no_embedded_files(
    pdf: &Pdf,
    cache: &check::ObjectCache<'_>,
    level: PdfALevel,
    report: &mut ComplianceReport,
) {
    if check::has_embedded_files_cached(pdf, cache) {
        // PDF/A-1: §6.1.11, PDF/A-2: §6.1.7
        let rule = if level.part() == 1 { "6.1.11" } else { "6.1.7" };
        check::error(
            report,
            rule,
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
#[allow(dead_code)]
fn check_name_length(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_name_length_limit(pdf, report);
    check::check_array_capacity_limit(pdf, report);
    check::check_cid_value_limit(pdf, report);
    check::check_near_zero_reals(pdf, report);
    check::check_integer_range(pdf, report);
}

/// §6.1.13 — Name length limit (using pre-cached objects for the hot path).
fn check_name_length_cached(
    pdf: &Pdf,
    cache: &check::ObjectCache<'_>,
    report: &mut ComplianceReport,
) {
    check::check_name_length_limit_cached(cache, report);
    check::check_array_capacity_limit_cached(cache, report);
    check::check_cid_value_limit(pdf, report);
    check::check_near_zero_reals_cached(cache, report);
    check::check_integer_range_cached(cache, report);
}

/// §6.1.12 — Real value range.
fn check_real_value_range_cached(
    cache: &check::ObjectCache<'_>,
    level: PdfALevel,
    report: &mut ComplianceReport,
) {
    if level.part() == 1 {
        check::check_real_value_limits_cached(cache, report);
    }
}

/// §6.3.2 — Font file stream format.
fn check_font_file_format_cached(
    cache: &check::ObjectCache<'_>,
    level: PdfALevel,
    report: &mut ComplianceReport,
) {
    check::check_font_file_subtype_cached(cache, level.part(), report);
}

/// §6.2.2 — Explicit Resources.
fn check_explicit_resources(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_explicit_resources(pdf, report);
    // Check that resource names used via Do/Tf in content streams exist in Resources (§6.2.2).
    // Only Do and Tf operators are checked — they have fewest false positives.
    // Uses parent-chain-aware lookups to handle inherited resources correctly.
    check::check_resource_names_exist(pdf, report);
}

/// §6.1.3 — Trailer requirements.
///
/// check::check_trailer_requirements validates /ID presence and emptiness for
/// traditional "trailer" sections, but only checks KEY presence for cross-reference
/// stream PDFs (no "trailer" keyword). Supplement with a raw scan to catch empty
/// /ID arrays in xref-stream PDFs. (#FN-6.1.3)
fn check_trailer_requirements(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_trailer_requirements(pdf, level.part(), report);
    // If 6.1.3 already emitted, nothing more to add.
    if report.issues.iter().any(|i| i.rule == "6.1.3") {
        return;
    }
    let data = pdf.data().as_ref();
    // Only supplement for xref-stream PDFs (no traditional "trailer" keyword near EOF).
    let tail_start = data.len().saturating_sub(4096);
    let has_trailer_kw = data[tail_start..].windows(7).any(|w| w == b"trailer");
    if has_trailer_kw {
        return; // Traditional trailer already fully handled.
    }
    // Scan for "/ID" followed by "[<>" which signals an empty first identifier element.
    let mut pos = 0;
    while pos + 6 < data.len() {
        if &data[pos..pos + 3] == b"/ID" {
            let mut after = pos + 3;
            while after < data.len() && matches!(data[after], b' ' | b'\n' | b'\r' | b'\t') {
                after += 1;
            }
            if after < data.len() && data[after] == b'[' {
                after += 1;
                while after < data.len() && matches!(data[after], b' ' | b'\n' | b'\r' | b'\t') {
                    after += 1;
                }
                if after + 1 < data.len() && data[after] == b'<' && data[after + 1] == b'>' {
                    check::error(
                        report,
                        "6.1.3",
                        "Trailer /ID array contains empty identifier (xref-stream PDF)",
                    );
                    return;
                }
            }
            pos += 3;
        } else {
            pos += 1;
        }
    }
}

// ─── Batch 7: Stream/syntax validation, XMP extension, image intent ─────────

/// §6.7.3 — Info dict / XMP metadata consistency.
/// §6.7.9.3 — Lang Alt type requirement for dc:description, dc:rights, xmpRights:UsageTerms.
fn check_info_xmp(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_info_xmp_consistency(pdf, report);
    check::check_xmp_lang_alt_properties(pdf, report);
}

/// §6.1.7 — Stream Length verification.
fn check_stream_length_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_stream_length(pdf, report);
}

/// §6.1.8/6.1.9 — Object syntax spacing checks.
///
/// PDF/A-1 and PDF/A-4 use §6.1.8 for object syntax; PDF/A-2/3 use §6.1.9
/// (clause numbers shifted between PDF/A-2/3 and PDF/A-4).
/// The check function emits the correct rule ID based on the part number.
fn check_object_syntax(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    let before = report.issues.len();
    check::check_object_syntax_spacing(pdf, level.part(), report);
    // For PDF/A-4: check_object_syntax_spacing emits "6.1.8" (object-syntax violations),
    // but the global remap (4, "6.1.8") => Some("6.1.6.2") exists for LZW-filter
    // violations. Object-syntax "6.1.8" must stay as "6.1.8" in veraPDF.
    // Retag to "6.1.8-obj" so the LZW remap doesn't capture it; remap_clause_numbers
    // then maps "6.1.8-obj" back to "6.1.8". (#496)
    if level.part() == 4 {
        for issue in &mut report.issues[before..] {
            if issue.rule == "6.1.8" {
                issue.rule = "6.1.8-obj".to_string();
            }
        }
    }

    // Supplementary: check_object_syntax_spacing allows ' '/'\t' after 'obj',
    // but PDF/A-2/3 §6.1.9 and PDF/A-4 §6.1.8 require EOL after 'obj'.
    // Scan for the gap only if no object-syntax issue was already emitted.
    // Pattern: "<digit> obj<space/tab>" — require digit before the single
    // whitespace preceding "obj" to avoid false matches in binary streams.
    // (#496 = PDF/A-4; #FN-6.1.9 = PDF/A-2/3)
    let rule = match level.part() {
        4 => "6.1.8-obj",
        2 | 3 => "6.1.9",
        _ => return,
    };
    let already_emitted = report.issues[before..].iter().any(|i| i.rule == rule);
    if already_emitted {
        return;
    }
    let data = pdf.data().as_ref();
    let len = data.len();
    let mut pos = 0;
    while pos + 3 < len {
        if &data[pos..pos + 3] == b"obj" {
            let is_endobj = pos >= 3 && &data[pos - 3..pos] == b"end";
            // Require: not "endobj", preceded by exactly one space, preceded
            // by a digit (gen number), then the keyword "obj" must be followed
            // by space or tab (not EOL).
            if !is_endobj
                && pos >= 2
                && data[pos - 1] == b' '
                && data[pos - 2].is_ascii_digit()
                && pos + 3 < len
            {
                let after = data[pos + 3];
                if after == b' ' || after == b'\t' {
                    check::error(
                        report,
                        rule,
                        format!(
                            "Keyword 'obj' not followed by EOL marker (PDF/A §{})",
                            if level.part() == 4 { "6.1.8" } else { "6.1.9" }
                        ),
                    );
                    break;
                }
            }
            pos += 3;
        } else {
            pos += 1;
        }
    }
}

/// §6.7.8 — XMP extension schema validation.
fn check_xmp_extension_schema_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_xmp_extension_schema(pdf, report);
}

/// §6.2.5/6.2.9 — Image XObject rendering intent.
fn check_image_intent(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_image_xobject_intent(pdf, report);
}

/// §6.1.4 — xref keyword syntax.
///
/// check::check_xref_syntax stops after the first valid "xref" (breaks on first
/// match) and misses malformed "xref" keywords in later xref sections (incremental
/// updates). Supplement with a full-file scan that checks ALL standalone "xref"
/// keywords. (#FN-6.1.4)
fn check_xref_syntax_pdfa(pdf: &Pdf, report: &mut ComplianceReport) {
    check::check_xref_syntax(pdf, report);
    // Already emitted — don't double-count.
    if report.issues.iter().any(|i| i.rule == "6.1.4") {
        return;
    }
    // Scan every standalone "xref" occurrence (not inside "startxref") and
    // verify each is followed immediately by CR, LF, or CRLF.
    let data = pdf.data().as_ref();
    let len = data.len();
    let mut pos = 0;
    while pos + 4 <= len {
        if &data[pos..pos + 4] != b"xref" {
            pos += 1;
            continue;
        }
        // Skip "startxref"
        if pos >= 5 && &data[pos - 5..pos] == b"start" {
            pos += 4;
            continue;
        }
        // Found standalone "xref" — verify followed by EOL
        let after = pos + 4;
        if after < len {
            let c = data[after];
            if c != b'\n' && c != b'\r' {
                check::error(
                    report,
                    "6.1.4",
                    "Keyword 'xref' not followed by proper EOL marker",
                );
                return;
            }
        }
        pos += 4;
    }
}

/// §6.9 — Embedded file specification keys.
fn check_embedded_file_spec(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_embedded_file_spec_keys(pdf, level.part(), report);
    check::check_embedded_file_af_association(pdf, level.part(), report);
    // §6.9: non-embedded file specs (no /EF) violate PDF/A-2 requirements. (#467)
    check::check_filespec_without_ef(pdf, level.part(), report);
    // §6.9/§6.8: embedded files must be in /Names/EmbeddedFiles (PDF/A-3/4). (#467)
    check::check_embedded_files_in_names_tree(pdf, level.part(), report);
}

/// §6.2.9/6.2.10 — PostScript XObjects are forbidden.
fn check_postscript_xobjects_pdfa(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    check::check_postscript_xobjects(pdf, level.part(), report);
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
            // veraPDF reports §6.2.4.3 for PDF/A-2/3/4; for PDF/A-1 it uses §6.2.3.3
            // (ISO 19005-1 §6.2.3.3 covers all device-dependent CS, not split like -2/3).
            // Remap is below near other PDF/A-1 mappings.

            // TR/TR2 transfer function restrictions
            // PDF/A-1: §6.2.8, PDF/A-2/3: §6.2.10.5, PDF/A-4: §6.2.5
            (1, "6.2.10.5") => Some("6.2.8"),
            (4, "6.2.10.5") => Some("6.2.5"),

            // Halftone restrictions
            // PDF/A-4: §6.2.5
            (4, "6.2.10") => Some("6.2.5"),
            (4, "6.2.10.4.1") => Some("6.2.5"),

            // TrueType simple-font Mac Roman cmap requirements (internal rule "6.2.10.4.1-tt")
            // PDF/A-4: §6.2.10.4.1 (TrueType font cmap — Platform 1 codes must be valid Mac Roman)
            // check.rs emits "6.2.10.4.1-tt"; remap to exact veraPDF clause. (#467)
            (4, "6.2.10.4.1-tt") => Some("6.2.10.4.1"),

            // Rendering intents
            // PDF/A-1: §6.2.9, PDF/A-2/3/4: §6.2.5
            (1, "6.2.5") => Some("6.2.9"),

            // Optional content restrictions
            // PDF/A-1: veraPDF uses §6.1.11 for OCProperties violations.
            // check.rs emits "6.1.11" — no remap needed (matches veraPDF).
            // PDF/A-2/3 OC checks emit "6.6.4"; no remap needed (correct already)

            // Image XObject restrictions (OPI, Alternates, Interpolate)
            // PDF/A-1: §6.2.4 sub-clauses mapped to 6.2.8.x in PDF/A-2/3
            (1, "6.2.8.1") => Some("6.2.4"),
            (1, "6.2.8.2") => Some("6.2.4"),
            (1, "6.2.8.3") => Some("6.2.4"),
            // PDF/A-4: OPI/Alternates/Interpolate checks use §6.2.7.x
            // ISO 19005-4 renumbered: §6.2.8.1 (Interpolate) → §6.2.7.1 (#FN-6.2.7.1)
            (4, "6.2.8.1") => Some("6.2.7.1"), // Interpolate=true forbidden
            (4, "6.2.8.3") => Some("6.2.7.1"), // OPI key forbidden (#467)

            // Implementation limits
            // PDF/A-1: §6.1.12, PDF/A-2/3/4: §6.1.13
            (1, "6.1.13") => Some("6.1.12"),

            // ── PDF/A-4 6.1.x clause renumbering ──
            // ISO 19005-4 renumbered file structure sub-clauses:
            //   PDF/A-2/3 §6.1.5 (hex/name) → PDF/A-4 §6.1.4 (implementation limits)
            //   PDF/A-2/3 §6.1.6 (hex strings) → PDF/A-4 §6.1.5
            //   PDF/A-2/3 §6.1.7 (streams) → PDF/A-4 §6.1.6
            //   PDF/A-2/3 §6.1.8 (stream filters) → PDF/A-4 §6.1.6.2
            //   PDF/A-2/3 §6.1.13 (impl limits) → PDF/A-4 §6.1.13 (same, no remap)
            (4, "6.1.6") => Some("6.1.5"),    // hex strings
            (4, "6.1.8") => Some("6.1.6.2"),  // stream filters (LZWDecode etc.)
            (4, "6.1.10") => Some("6.1.6.2"), // PDF/A-1 filter rule → PDF/A-4
            // PDF/A-2/3: check_stream_filters emits "6.1.8" for LZW/JBIG2 filter
            // violations; veraPDF uses the sub-clause "6.1.6.2" (§6.1.6.2 of ISO
            // 19005-2/3 covers stream filter restrictions). Remap to match. (#FN-6.1.6.2)
            (2..=3, "6.1.8") => Some("6.1.6.2"),
            // Object-syntax spacing retagged to "6.1.8-obj" in check_object_syntax to
            // avoid collision with the LZW-filter remap above. veraPDF uses "6.1.8". (#496)
            (4, "6.1.8-obj") => Some("6.1.8"),

            // Stream checks: Length, EOL, empty keys, external refs
            // PDF/A-1: §6.1.7, PDF/A-2/3: §6.1.7 (same), PDF/A-4: §6.1.7 (same).
            // veraPDF uses "6.1.7" for stream-length violations in all parts 1-4;
            // normalize_pdfa4_clause does NOT remap "6.1.7" or "6.1.7.1", so
            // these must match veraPDF directly (no remap for PDF/A-4).
            // do NOT remap "6.1.7" to "6.1.7.1" for PDF/A-2/3 (was causing false
            // negatives because veraPDF outputs the parent clause, not the sub-clause).
            // Round19: 4 FNs for "6.1.7" (PDF/A-4 stream length) were caused by
            // the now-removed (4,"6.1.7")=>"6.1.6.1" remap. (#496)
            (1, "6.1.7.1") => Some("6.1.7"),

            // Widget annotation actions / NeedAppearances
            // PDF/A-1: internal §6.4.1 → §6.6.1 (ISO 19005-1 numbering)
            // PDF/A-4: veraPDF emits §6.4.1 directly (ISO 19005-4) — no remap needed. Fixes #467.
            // PDF/A-2/3: §6.4.1 used directly.
            (1, "6.4.1") => Some("6.6.1"),

            // XFA key forbidden in AcroForm (PDF/A-4 §6.4.2).
            // veraPDF emits §6.4.2 directly (ISO 19005-4) — no remap needed. Fixes #467.
            // PDF/A-1/2/3: §6.4.2 is soft-mask structure, no remap needed.

            // Annotation types (forbidden subtypes like Sound, Movie, 3D, RichMedia)
            // §6.3.1 is used by veraPDF for ALL PDF/A parts (1-4) for annotation type
            // violations. PDF/A-1 uses §6.5.2 (check_annotation_types emits "6.5.2"),
            // but PDF/A-2/3/4 all use "6.3.1" directly. No remap needed for PDF/A-4.
            // Fixes #467 (veraPDF test suite 6-3-1-t01-fail-e is PDF/A-4).

            // Annotation flags (/F key, Print=1 etc.)
            // PDF/A-4: our checker emits "6.3.2" (ISO 19005-4 clause). In compare_compliance
            // normalize_pdfa4_clause("6.3.2")="6.5.2" — BOTH our rule and veraPDF's rule
            // go through the same normalization, so no remap is needed here. Removing the
            // old (4,"6.3.2")=>"6.5.2" remap which incorrectly converted "6.3.2" to "6.5.2"
            // causing normalization to produce "6.7.2" instead of "6.5.2". Fixes #482.

            // Annotation appearance (AP dict, /N entry, /CA, Btn subdictionary) — §6.5.3.
            // For PDF/A-4: our checker always emits "6.5.3". normalize_pdfa4_clause("6.5.3")
            // would produce "6.7.3" (metadata), but veraPDF uses ISO 19005-4 clause "6.3.3"
            // which normalizes to "6.5.3". Remap our "6.5.3" → "6.3.3" so normalization
            // matches. Note: "6.3.3" from font embedding hits a separate arm below. Fixes #482.
            (4, "6.5.3") => Some("6.3.3"),

            // Page-level /AA entry (§6.5.2 test 2 in ISO 19005-2/3).
            // check_actions_deep hardcodes "6.1.6.1" for page-level /AA; veraPDF reports
            // "6.5.2" for this violation in PDF/A-2/3. Remap to match. Fixes #482.
            (2..=3, "6.1.6.1") => Some("6.5.2"),
            // PDF/A-4: page/annotation /AA → §6.6.3 (ISO 19005-4 actions on non-widget).
            // normalize_pdfa4_clause("6.6.3") = "6.8.3" — matches veraPDF's "6.6.3" normalized.
            // Safe: stream keyword checks emit "6.1.7.1" (not "6.1.6.1") before remap.
            (4, "6.1.6.1") => Some("6.6.3"),

            // Transparency restrictions — PDF/A-1
            // veraPDF uses §6.4 for ALL transparency violations in PDF/A-1.
            // Our checks use PDF/A-2/3 sub-clauses internally:
            //   check_transparency_vs_output_intent → "6.2.10-tgroup" (distinct from halftone "6.2.10")
            //   check_extgstate_restrictions → "6.2.10.6" (BM) / "6.2.10.7" (SMask in ExtGState)
            //   check_soft_mask_structure → "6.4.2" (SMask in XObject)
            // All map to veraPDF's "6.4" for PDF/A-1. (#FN-6.4)
            // NOTE: "6.2.10" (halftone type) is intentionally NOT remapped for PDF/A-1,
            // because veraPDF reports §6.2.10 for halftone violations even in PDF/A-1. (#FN-6.2.10)
            (1, "6.2.10-tgroup") => Some("6.4"),
            (1, "6.2.10.6") => Some("6.4"),
            (1, "6.2.10.7") => Some("6.4"),
            (1, "6.4.2") => Some("6.4"),

            // Transparency page group violations — PDF/A-2/3
            // veraPDF uses §6.2.10 for transparency page group issues in PDF/A-2/3.
            // Our internal tag "6.2.10-tgroup" maps to veraPDF's "6.2.10". (#FN-6.2.10)
            (2..=3, "6.2.10-tgroup") => Some("6.2.10"),

            // Alternate CS consistency (ICCBased)
            // PDF/A-1: §6.2.3.2, PDF/A-2/3/4: §6.2.4.2
            (1, "6.2.4.2") => Some("6.2.3.2"),

            // DeviceN/Separation alternate CS
            // PDF/A-1: §6.2.3.4, PDF/A-2/3/4: §6.2.4.4
            (1, "6.2.4.4") => Some("6.2.3.4"),

            // Form XObject restrictions (PS, Subtype2, Ref)
            // PDF/A-1: §6.2.5, PDF/A-2/3/4: §6.2.9
            (1, "6.2.9") => Some("6.2.5"),

            // Lang tag validation
            // veraPDF uses §6.8.4 for ALL PDF/A parts — no remap needed.
            // The previous (2..=3,"6.8.4")=>"6.7.4" remap was wrong and caused 4 FNs. (#FN-6.8.4)

            // CIDSystemInfo compatibility
            // PDF/A-2/3: §6.2.11.3.1
            (2..=3, "6.3.3.1") => Some("6.2.11.3.1"),
            // PDF/A-4: §6.2.10.3.1 (same requirement, different clause numbering)
            // Merges with the emission from check_cidsystem_info_consistency (#467)
            (4, "6.3.3.1") => Some("6.2.10.3.1"),

            // CIDToGIDMap must be /Identity or a stream
            // PDF/A-4: §6.2.10.3.2 (non-Identity Name value)
            (4, "6.3.7") => Some("6.2.10.3.2"),

            // Name UTF-8 validation — always maps to 6.1.7 for all PDF/A parts
            (_, "6.1.7-names") => Some("6.1.7"),

            // CIDSet / CharSet for subset fonts
            // PDF/A-1: §6.3.5, PDF/A-2/3: §6.2.11.5
            (2..=3, "6.3.5") => Some("6.2.11.5"),
            // PDF/A-4: §6.3.5 → §6.2.10.5 (font program width consistency
            //   and CIDSet requirements share the same clause in ISO 19005-4)
            (4, "6.3.5") => Some("6.2.10.5"),

            // Font program width consistency (internal rule "6.3.5-fw")
            // PDF/A-1: §6.3.6 (ISO 19005-1 — veraPDF uses §6.3.6 for width consistency)
            // Previously wrongly remapped to §6.3.5; fixed in #467.
            (1, "6.3.5-fw") => Some("6.3.6"),
            // PDF/A-2/3: §6.2.11.5 (glyph width consistency). veraPDF uses §6.2.11.5 t1.
            (2..=3, "6.3.5-fw") => Some("6.2.11.5"),
            // PDF/A-4: §6.2.10.5
            (4, "6.3.5-fw") => Some("6.2.10.5"),

            // Font embedding
            // PDF/A-1: §6.3.3 → §6.3.4 (veraPDF uses 6.3.4 for font embedding in PDF/A-1)
            (1, "6.3.3") => Some("6.3.4"),
            (1, "6.3.3-nd") => Some("6.3.4"), // no-FontDescriptor case, same clause in PDF/A-1
            // PDF/A-1: corrupt/null font file — veraPDF maps this to §6.3.4 (same as
            // missing embedding), not §6.3.2. §6.3.2 is for glyph-presence in the
            // content (annotation/XObject level), not the font program itself. (#467)
            (1, "6.3.2-null") => Some("6.3.4"),
            // PDF/A-2/3: §6.3.4 → §6.2.11.4.1 (font program not embedded)
            (2..=3, "6.3.4") => Some("6.2.11.4.1"),
            (2..=3, "6.3.3") => Some("6.2.11.4.1"),
            // PDF/A-2/3: no-FontDescriptor case → §6.2.11.4.2 (distinct from §6.2.11.4.1).
            // veraPDF uses §6.2.11.4.2 when FontDescriptor is entirely absent. Fixes #467.
            (2..=3, "6.3.3-nd") => Some("6.2.11.4.2"),
            // PDF/A-4: §6.3.4 → §6.2.10.4.1 (different numbering in ISO 19005-4)
            (4, "6.3.4") => Some("6.2.10.4.1"),
            (4, "6.3.3") => Some("6.2.10.4.1"),
            (4, "6.3.3-nd") => Some("6.2.10.4.1"),

            // OutputIntent requirements (missing GTS_PDFA1, multiple GTS_PDFA1 entries).
            // check_output_intent emits "6.6.2-oi" for PDF/A-1 OutputIntent violations.
            // veraPDF uses §6.2.2 for ALL PDF/A-1 OutputIntent violations (confirmed
            // by isartor 6-2-2-t03 FN where multiple GTS_PDFA1 entries → "6.2.2"). (#FN-6.2.2)
            // Note: Field /AA and Catalog /AA violations emit "6.6.2" and must NOT be
            // remapped here (veraPDF uses "6.6.2" for /AA in PDF/A-1).
            (1, "6.6.2-oi") => Some("6.2.2"),

            // OutputIntent ICC profile class (prtr/mntr) check
            // PDF/A-1: veraPDF uses §6.2.2 for all OutputIntent/ICC violations.
            // PDF/A-2/3/4: §6.2.3 → no remap needed (direct internal clause).
            (1, "6.2.3") => Some("6.2.2"),

            // Device color vs OutputIntent: §6.2.3.3 is correct for ALL PDF/A parts.
            // veraPDF emits "6.2.3.3" for PDF/A-1 too — no remap needed. (#467)
            // (Removed wrong (1,"6.2.3.3")=>"6.6.2.3.3" mapping that hid FN.)

            // OutputIntent ICC profile version check (internal rule "6.2.3.3-iccver")
            // PDF/A-1: §6.2.2 (veraPDF groups ICC validity under OutputIntent clause)
            // PDF/A-2/3/4: §6.2.3.3
            (1, "6.2.3.3-iccver") => Some("6.2.2"),
            (_, "6.2.3.3-iccver") => Some("6.2.3.3"),

            // XMP extension schema checks emit "6.6.2.3.1"/"6.6.2.3.3" (PDF/A-2/3 clauses).
            // In PDF/A-1 these map to §6.7.8 (ISO 19005-1 XMP extension schemas). Fixes #476.
            (1, "6.6.2.3.1") => Some("6.7.8"),
            (1, "6.6.2.3.3") => Some("6.7.8"),

            // OutputIntent DestOutputProfile required
            // veraPDF uses §6.2.3.2 for ALL PDF/A parts — no remap needed. Fixes #467.
            // (Removed wrong (1,"6.2.3.2")=>"6.6.2.3.2" mapping that hid FN.)

            // Symbolic TrueType /Encoding must not be present (internal rule "6.3.7-se").
            // PDF/A-1: §6.3.7; PDF/A-2/3: §6.2.11.6 (TrueType encoding). (#483)
            (1, "6.3.7-se") => Some("6.3.7"),
            (2..=3, "6.3.7-se") => Some("6.2.11.6"),

            // CIDToGIDMap must be /Identity or a stream (internal rule "6.3.7").
            // PDF/A-1: veraPDF uses §6.3.7 directly — no remap needed.
            // PDF/A-2/3: §6.2.11.3.2.
            // (PDF/A-4 already handled above as §6.2.10.3.2.) (#483)
            (2..=3, "6.3.7") => Some("6.2.11.3.2"),

            // CIDSystemInfo compatibility (internal rule "6.3.3.1").
            // PDF/A-1: veraPDF uses §6.3.3.1 directly — no remap needed.

            // CIDSystemInfo mismatch (check_cidsystem_info_consistency emits "6.2.10.3.1").
            // PDF/A-1: §6.3.3.1 (clause numbering differs from PDF/A-4). (#FN-6.3.3.1)
            (1, "6.2.10.3.1") => Some("6.3.3.1"),
            // PDF/A-2/3: §6.2.11.3.1. (#483)
            (2..=3, "6.2.10.3.1") => Some("6.2.11.3.1"),

            // Annotation appearance stream required.
            // PDF/A-2/3: §6.3.3. (#483)
            (2..=3, "6.5.3") => Some("6.3.3"),

            // Undefined content-stream operators.
            // PDF/A-2/3: veraPDF uses §6.2.10. (#483)
            (2..=3, "6.2.7.1") => Some("6.2.10"),

            // Image Alternates key prohibited.
            // PDF/A-2/3: §6.2.7.1. (#483)
            (2..=3, "6.2.8.2") => Some("6.2.7.1"),

            // Image Interpolate=true prohibited.
            // PDF/A-2/3: §6.2.8. (#483)
            (2..=3, "6.2.8.1") => Some("6.2.8"),

            // Device color space vs OutputIntent: §6.2.3.3 is correct for ALL PDF/A parts.
            // The isartor test suite (6-2-3-3-t03) confirms veraPDF emits "6.2.3.3" for
            // PDF/A-1 DeviceCMYK inline image violations — NOT "6.2.2". No remap needed.

            // Symbolic TrueType /Encoding for PDF/A-4: §6.2.10.6. (#483)
            (4, "6.3.7-se") => Some("6.2.10.6"),
            // Non-symbolic TrueType BaseEncoding and CIDFont widths: in PDF/A-4
            // §6.2.11.6 (PDF/A-2/3) is renumbered to §6.2.10.6. (#FN-6.2.10.6)
            (4, "6.2.11.6") => Some("6.2.10.6"),

            // PUA codepoints in ToUnicode require ActualText.
            // PDF/A-4: §6.2.10.9 (same requirement as §6.2.11.7.3 in PDF/A-2/3). (#483)
            (4, "6.2.11.7.3") => Some("6.2.10.9"),

            // CMap embedding for Type0 fonts (internal rule "6.3.3.3").
            // PDF/A-1: §6.3.3.3 (already correct); PDF/A-2/3: §6.2.11.3.3;
            // PDF/A-4: §6.2.10.3.3. (#483)
            (2..=3, "6.3.3.3") => Some("6.2.11.3.3"),
            (4, "6.3.3.3") => Some("6.2.10.3.3"),

            // Optional content "6.6.4" → "6.9" was a blanket remap but it also clobbered
            // XMP-conformance "6.6.4" from check_xmp_metadata. Re-tagging is now done in
            // check_optional_content() directly before the global remap. (#FN-6.9, #FN-6.6.4)

            // Forbidden ToUnicode values (U+0000, U+FEFF, U+FFFE) use "6.2.11.7.2".
            // PDF/A-4 §6.2.10.7 covers both missing-ToUnicode AND forbidden-values.
            // veraPDF uses §6.2.10.7 for U+0000/FEFF/FFFE violations too.
            (4, "6.2.11.7.2") => Some("6.2.10.7"),

            // Device colour without DefaultRGB/DefaultCMYK/DefaultGray or OutputIntent.
            // PDF/A-1: veraPDF reports all device-colour violations under §6.2.3.3
            // (ISO 19005-1 §6.2.3.3 covers all device-dependent CS restrictions).
            // PDF/A-2/3/4: §6.2.4.3 is already the correct clause.
            // Fixes 6 FNs from round16 where isartor t03 showed veraPDF emits "6.2.3.3".
            (1, "6.2.4.3") => Some("6.2.3.3"),

            // XMP property type/value violations.
            // check.rs emits "6.7.9.3" (wrong scalar container) and xmp.rs emits
            // "6.7.9.1"/"6.7.9.2"/"6.7.9.3" for various property type violations.
            // veraPDF always uses the parent clause "6.7.9" for ALL such violations
            // in PDF/A-1 and "6.6.2.3.1" for PDF/A-2/3. Sub-clauses cause 237+ FNs.
            (1, "6.7.9.1") => Some("6.7.9"),
            (1, "6.7.9.2") => Some("6.7.9"),
            (1, "6.7.9.3") => Some("6.7.9"),
            // For PDF/A-2/3/4 the same checks emit "6.6.2.3.1" which is already correct.
            // But if any sub-clause slips through for PDF/A-2/3, collapse to parent.
            (2..=3, "6.7.9.1") => Some("6.6.2.3.1"),
            (2..=3, "6.7.9.2") => Some("6.6.2.3.1"),
            (2..=3, "6.7.9.3") => Some("6.6.2.3.1"),

            // ── 6.2.x color/font rule remaps ──

            // Device color vs OutputIntent: veraPDF uses §6.2.4.3 for PDF/A-2/3/4.
            // Our checker emits "6.2.3.3" (internal numbering).
            (2..=3, "6.2.3.3") => Some("6.2.4.3"),
            (4, "6.2.3.3") => Some("6.2.4.3"),

            // Undefined operators: veraPDF uses §6.2.10 for PDF/A-1, §6.2.2 for PDF/A-4.
            // Our check_page_content_streams emits "6.2.10" for PDF/A-1 (direct),
            // "6.2.7.1" for PDF/A-2+. PDF/A-4 needs remap to §6.2.2.
            (4, "6.2.7.1") => Some("6.2.2"),

            // TrueType encoding requirements.
            // PDF/A-1: check.rs emits "6.2.11.6" (PDF/A-2/3 clause numbering); remap to
            // ISO 19005-1 §6.3.7 which veraPDF uses for TrueType encoding violations. (#496)
            (1, "6.2.11.6") => Some("6.3.7"),

            // Role mapping check.
            // PDF/A-2/3: check.rs emits "6.12" (our canonical numbering); veraPDF uses
            // §6.7.3.4 for role-mapping violations in PDF/A-2/3 (not §6.11). (#FN-6.7.3.4)
            (2..=3, "6.12") => Some("6.7.3.4"),

            // CIDSet coverage check emits "6.2.11.4.2" (PDF/A-2/3 numbering).
            // PDF/A-1: veraPDF uses §6.3.5 for CIDSet/CharSet violations.
            // PDF/A-4: §6.2.10.4.2.
            (1, "6.2.11.4.2") => Some("6.3.5"),
            (4, "6.2.11.4.2") => Some("6.2.10.4.2"),

            // Rendering intent: check.rs emits "6.2.5" (our canonical numbering).
            // PDF/A-1: veraPDF uses §6.2.9 (not §6.2.5).
            // Actually our remap (1,"6.2.5")=>"6.2.9" already exists above for form XObjects.
            // For PDF/A-4: rendering intent is §6.2.6.
            (4, "6.2.5") => Some("6.2.6"),

            // Halftone/TransferFunction: check.rs emits "6.2.10" or "6.2.10.5".
            // PDF/A-2/3: veraPDF uses §6.2.5 for halftone+transfer restrictions.
            // Our remap (1,"6.2.10.5")=>"6.2.8" already exists for PDF/A-1.
            (2..=3, "6.2.10.5") => Some("6.2.5"),

            // Font embedding: check.rs emits "6.3.4" for missing font programs.
            // PDF/A-1: veraPDF uses §6.3.4 directly — no remap needed.
            // But for CIDFonts, the embedding check emits "6.3.3" which is remapped above.

            // CMap external reference: check.rs emits "6.3.3.3" for CMap references.
            // PDF/A-1: §6.3.3.3 (already correct).
            // PDF/A-4: already remapped to "6.2.10.3.3" above.

            // ── Image XObject remaps ──
            // (6.2.8.1/6.2.8.2 for PDF/A-1/2/3/4 already handled above at ~line 2277-2505)

            // ── PDF/A-4 font width remaps ──

            // Font width mismatches: our width check emits "6.2.11.5" (PDF/A-2/3).
            // PDF/A-4: veraPDF uses §6.2.10.5.
            (4, "6.2.11.5") => Some("6.2.10.5"),

            // ToUnicode missing: check.rs emits "6.2.11.7.2" for PDF/A-2/3.
            // PDF/A-4: already remapped to "6.2.10.7" above.

            // CIDSet missing: check.rs emits "6.3.5" for PDF/A-1.
            // PDF/A-4: "6.2.11.4.2" → "6.2.10.4.2" already above.

            // Font embedding: "6.2.11.4.1" stays for PDF/A-2/3.
            // PDF/A-4: "6.3.4" → "6.2.10.4.1" already above.

            // ── OutputIntent/ICC remaps ──

            // OutputIntent consistency (multiple DestOutputProfile entries).
            // PDF/A-4: veraPDF uses §6.2.3 (not §6.2.2).
            // Our check_output_intent_consistency emits "6.2.2".
            // Note: "6.2.2" is also used for undefined operators (PDF/A-4 remap from 6.2.7.1).
            // Only remap "6.2.2" that comes from OutputIntent checks (has "OutputIntent" in message).
            // Since we can't distinguish by message in remap, use a separate internal rule.

            // PostScript XObject check: check.rs emits "6.2.9" for PS XObjects.
            // PDF/A-1: veraPDF uses §6.2.7. Our remap (1,"6.2.9")=>"6.2.5" already exists
            // but veraPDF uses "6.2.7" for PS XObjects (not "6.2.5" which is rendering intent).
            // Fix: specific remap for PS XObject rule.
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

    /// Verify that halftone "6.2.10" is NOT remapped to "6.4" for PDF/A-1.
    /// Previously, the shared "6.2.10" tag caused halftone violations to be
    /// remapped to §6.4 (transparency) instead of staying as §6.2.10.
    /// Transparency page-group violations now use "6.2.10-tgroup" internally.
    #[test]
    fn halftone_rule_not_remapped_to_transparency_for_pdfa1() {
        use crate::{ComplianceIssue, ComplianceReport, Severity};
        let mut report = ComplianceReport {
            pdfa_level: Some(PdfALevel::A1b),
            ..Default::default()
        };
        // Halftone type violation (from check_halftone_in_extgstate)
        report.issues.push(ComplianceIssue {
            rule: "6.2.10".to_string(),
            severity: Severity::Error,
            message: "HalftoneType 3 not allowed".to_string(),
            location: None,
        });
        // Transparency page-group violation (from check_transparency_vs_output_intent)
        report.issues.push(ComplianceIssue {
            rule: "6.2.10-tgroup".to_string(),
            severity: Severity::Error,
            message: "Transparency group without CS".to_string(),
            location: None,
        });
        remap_clause_numbers(&mut report, PdfALevel::A1b);
        let rules: Vec<&str> = report.issues.iter().map(|i| i.rule.as_str()).collect();
        // Halftone violation must remain as "6.2.10" (veraPDF §6.2.10 for PDF/A-1)
        assert!(
            rules.contains(&"6.2.10"),
            "halftone rule incorrectly remapped: {rules:?}"
        );
        // Transparency violation must map to "6.4" (veraPDF §6.4 for PDF/A-1)
        assert!(
            rules.contains(&"6.4"),
            "transparency rule not remapped to 6.4: {rules:?}"
        );
        // Must NOT have "6.4" coming from halftone (i.e. only one "6.4" entry max)
        assert_eq!(rules.iter().filter(|&&r| r == "6.4").count(), 1);
    }

    /// Verify that transparency "6.2.10-tgroup" maps to "6.2.10" for PDF/A-2/3.
    #[test]
    fn transparency_tgroup_remapped_to_6210_for_pdfa2() {
        use crate::{ComplianceIssue, ComplianceReport, Severity};
        let mut report = ComplianceReport {
            pdfa_level: Some(PdfALevel::A2b),
            ..Default::default()
        };
        report.issues.push(ComplianceIssue {
            rule: "6.2.10-tgroup".to_string(),
            severity: Severity::Error,
            message: "Transparency group without CS and no OutputIntent".to_string(),
            location: None,
        });
        remap_clause_numbers(&mut report, PdfALevel::A2b);
        let rules: Vec<&str> = report.issues.iter().map(|i| i.rule.as_str()).collect();
        assert_eq!(
            rules,
            vec!["6.2.10"],
            "transparency tgroup rule wrong for PDF/A-2: {rules:?}"
        );
    }
}
