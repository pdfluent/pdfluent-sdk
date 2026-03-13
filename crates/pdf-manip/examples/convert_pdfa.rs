//! Convert a PDF to PDF/A and save the output.
//! Usage: cargo run -p pdf-manip --example convert_pdfa -- input.pdf output.pdf

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <input.pdf> <output.pdf>", args[0]);
        std::process::exit(1);
    }

    let data = std::fs::read(&args[1]).expect("read input");
    let mut doc = match lopdf::Document::load_mem(&data) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("ERROR: Failed to parse PDF: {e}");
            std::process::exit(2);
        }
    };

    // Run conversion pipeline
    match pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false) {
        Ok(r) => eprintln!(
            "Cleanup: js_removed={}, ap_fixes={}",
            r.js_actions_removed, r.ap_fixes
        ),
        Err(e) => eprintln!("Cleanup error: {e}"),
    }

    match pdf_manip::pdfa_fonts::embed_fonts(&mut doc) {
        Ok(r) => {
            eprintln!(
                "Fonts: embedded={}, failed={}",
                r.fonts_embedded,
                r.failed.len()
            );
            for (name, reason) in &r.failed {
                eprintln!("  FAIL: {} — {}", name, reason);
            }
        }
        Err(e) => eprintln!("Font error: {e}"),
    }

    // Strip PFB (Printer Font Binary) headers from Type1 FontFile streams.
    // PDF spec requires raw PostScript, not PFB format (veraPDF rejects PFB).
    let pfb_fixed = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    eprintln!("PFB font streams stripped: fixed={pfb_fixed}");

    // Fix Type1 binary eexec sections whose first encrypted byte is a PDF
    // whitespace character. veraPDF skips leading spaces before the binary
    // section, causing it to start decryption from the wrong offset.
    let eexec_fixed = pdf_manip::pdfa_fonts::fix_type1_eexec_space_prefix(&mut doc);
    eprintln!("Type1 eexec space prefix: fixed={eexec_fixed}");

    // NOTE: fix_width_mismatches and fix_font_descriptor_metrics disabled —
    // they cause width regression on already-embedded fonts.
    // CFF-only width fixing is safe (doesn't touch TrueType).
    let cff_widths = pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    eprintln!("CFF widths: fixed={cff_widths}");

    let tt_cid_widths = pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    eprintln!("TrueType CID widths: fixed={tt_cid_widths}");

    let charset_fixed = pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    eprintln!("Type1 CharSet: fixed={charset_fixed}");

    let enc_fixed = pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    eprintln!("TrueType encoding: fixed={enc_fixed}");

    let sym_cmap_fixed = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
    eprintln!("Symbolic TrueType cmap: fixed={sym_cmap_fixed}");

    // Add Unicode (3,1) cmap to TrueType fonts that only have Mac Roman (1,0).
    let cmap_fixed = pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
    eprintln!("TrueType Unicode cmap: fixed={cmap_fixed}");

    // Fix .notdef glyph references (6.2.11.8:1).
    let notdef_fixed = pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    eprintln!(".notdef refs (simple): fixed={notdef_fixed}");

    // Fix .notdef in CID fonts by modifying content streams (6.2.11.8:1).
    let cid_notdef_fixed = pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    eprintln!(".notdef refs (CID): fixed={cid_notdef_fixed}");

    // Fix .notdef in symbolic simple fonts via content stream modification.
    let sym_notdef_fixed = pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
    eprintln!(".notdef refs (symbolic): fixed={sym_notdef_fixed}");

    // Remove simple-font bytes outside FirstChar..LastChar (avoids .notdef).
    let simple_range_fixed = pdf_manip::pdfa_fonts::fix_simple_font_out_of_range_codes(&mut doc);
    eprintln!(".notdef refs (simple range): fixed={simple_range_fixed}");

    // Strip control characters from content streams.
    let control_stripped = pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc);
    eprintln!("Control chars stripped: streams={control_stripped}");

    // Ensure undefined WinAnsi codes have Differences entries (prevents
    // ambiguous glyph mapping between veraPDF and our width fixer).
    let undef_fixed = pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    eprintln!("Undefined encoding codes: fixed={undef_fixed}");

    // Fix incorrect Symbolic flags on non-symbolic CFF fonts.
    let sym_flags = pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    eprintln!("Symbolic flags: fixed={sym_flags}");

    let classic_sym_enc = pdf_manip::pdfa_fonts::fix_classic_symbolic_base14_encoding(&mut doc);
    eprintln!("Classic symbolic encodings: fixed={classic_sym_enc}");

    // Populate missing FirstChar/LastChar/Widths for embedded fonts (6.2.11.2:4-6).
    let missing_widths = pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    eprintln!("Missing font widths: populated={missing_widths}");

    // Fix Type3 widths from CharProc d0/d1 operators (6.2.11.5:1).
    let type3_widths = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);
    eprintln!("Type3 font widths: fixed={type3_widths}");

    // Conservative width mismatch fix: only updates individual mismatched entries
    // where the mapping is unambiguous. Skips fonts with >50% mismatches.
    // Also handles subset fonts (ABCDEF+FontName) whose widths differ slightly.
    let width_fixes = pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    eprintln!("Font width mismatches: fixed={width_fixes}");

    // Fix symbolic font widths (ZapfDingbats, Symbol) with CFF/TT programs.
    let symbolic_fixes = pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    eprintln!("Symbolic font widths: fixed={symbolic_fixes}");

    let cidset_fixed = pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
    eprintln!("CIDSet: fixed={cidset_fixed}");

    match pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc) {
        Ok(r) => eprintln!(
            "Colorspace: had_intent={}, added={}, device_cs={:?}",
            r.had_output_intent, r.output_intent_added, r.device_colorspaces_found
        ),
        Err(e) => eprintln!("Colorspace error: {e}"),
    }

    // Supplementary fixups (small rule fixes).
    let fixup_report = pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    eprintln!(
        "Fixups: lengths={}, opi={}, stream_f={}, ps_xobj={}, ref_xobj={}, overflow={}, long_str={}, op_space={}, tiny_float={}, hex_str={}, inline_interp={}, jpx_cs={}, concat_op={}",
        fixup_report.stream_lengths_fixed,
        fixup_report.opi_keys_removed,
        fixup_report.stream_f_keys_removed,
        fixup_report.postscript_xobjects_removed,
        fixup_report.reference_xobjects_removed,
        fixup_report.overflow_integers_fixed,
        fixup_report.long_strings_fixed,
        fixup_report.operator_spacing_fixed,
        fixup_report.tiny_floats_fixed,
        fixup_report.odd_hex_strings_fixed,
        fixup_report.inline_image_interpolate_fixed,
        fixup_report.jpx_colorspace_fixed,
        fixup_report.concatenated_operators_fixed,
    );

    // Re-run colorspace normalization after fixups. Some fixups touch DeviceN
    // / Colorants structures and can reintroduce 6.2.4.4:2 inconsistencies.
    match pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc) {
        Ok(r) => eprintln!(
            "Colorspace (post-fixups): had_intent={}, added={}, device_cs={:?}",
            r.had_output_intent, r.output_intent_added, r.device_colorspaces_found
        ),
        Err(e) => eprintln!("Colorspace (post-fixups) error: {e}"),
    }

    match pdf_manip::pdfa_xmp::repair_xmp_metadata(
        &mut doc,
        pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        None,
    ) {
        Ok(_) => eprintln!("XMP: repaired"),
        Err(e) => eprintln!("XMP error: {e}"),
    }

    // Save
    let mut saved = Vec::new();
    doc.save_to(&mut saved).expect("save");
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
    pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);

    std::fs::write(&args[2], &saved).expect("write output");
    eprintln!("Saved to {} ({} bytes)", args[2], saved.len());
}
