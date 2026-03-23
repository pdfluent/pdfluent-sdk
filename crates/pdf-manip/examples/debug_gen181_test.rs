fn main() {
    let data = std::fs::read("/tmp/pdf-test-6.2.11.5/gen-181_181221.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();
    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
    pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
    pdf_manip::pdfa_fonts::fix_simple_font_out_of_range_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    pdf_manip::pdfa_fonts::fix_classic_symbolic_base14_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
    pdf_manip::pdfa_fonts::fix_missing_cidtogidmap(&mut doc);
    let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(
        &mut doc,
        pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        None,
    );
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
    pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);
    std::fs::write("/tmp/gen-181-conv.pdf", &saved).unwrap();
    eprintln!("Saved to /tmp/gen-181-conv.pdf");
}
