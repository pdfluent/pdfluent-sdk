//! Convert a PDF to PDF/A and save the output.
//! Usage: cargo run -p pdf-manip --example convert_pdfa -- input.pdf output.pdf

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <input.pdf> <output.pdf>", args[0]);
        std::process::exit(1);
    }

    let data = std::fs::read(&args[1]).expect("read input");
    let mut doc = lopdf::Document::load_mem(&data).expect("load");

    // Run conversion pipeline
    match pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false) {
        Ok(r) => eprintln!(
            "Cleanup: js_removed={}, ap_fixes={}",
            r.js_actions_removed, r.ap_fixes
        ),
        Err(e) => eprintln!("Cleanup error: {e}"),
    }

    match pdf_manip::pdfa_fonts::embed_fonts(&mut doc) {
        Ok(r) => eprintln!(
            "Fonts: embedded={}, failed={}",
            r.fonts_embedded,
            r.failed.len()
        ),
        Err(e) => eprintln!("Font error: {e}"),
    }

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

    // Fix .notdef glyph references (6.2.11.8:1).
    let notdef_fixed = pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    eprintln!(".notdef refs: fixed={notdef_fixed}");

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
        "Fixups: stream_lengths={}, cmap_wmode={}, cidtogidmap={}, cidsysteminfo={}, cmap_embedded={}",
        fixup_report.stream_lengths_fixed,
        fixup_report.cmap_wmode_fixed,
        fixup_report.cidtogidmap_fixed,
        fixup_report.cidsysteminfo_fixed,
        fixup_report.cmap_embedded,
    );

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

    std::fs::write(&args[2], &saved).expect("write output");
    eprintln!("Saved to {} ({} bytes)", args[2], saved.len());
}
