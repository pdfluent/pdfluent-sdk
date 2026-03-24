fn t() -> std::time::Instant { std::time::Instant::now() }
fn e(label: &str, start: std::time::Instant) {
    eprintln!("{}: {:.3}s", label, start.elapsed().as_secs_f64());
}
fn main() {
    let path = std::env::args().nth(1).unwrap_or("/tmp/w6211/gen-348_348434.pdf".to_string());
    let data = std::fs::read(&path).unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();
    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
    let _ = pdf_manip::pdfa_fonts::promote_inline_font_dicts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    pdf_manip::pdfa_fonts::fix_cff_invalid_bcd(&mut doc);
    pdf_manip::pdfa_fonts::fix_type1_nonstandard_charstrings(&mut doc);
    pdf_manip::pdfa_fonts::fix_type1_eexec_space_prefix(&mut doc);
    pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_tounicode_from_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_simple_font_out_of_range_codes(&mut doc);
    let _ = pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc);
    pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_classic_symbolic_base14_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
    pdf_manip::pdfa_fonts::fix_missing_cidtogidmap(&mut doc);
    let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);

    // Instrument run_fixups sub-functions
    let t0 = t(); let _ = pdf_manip::pdfa_fixups::run_fixups(&mut doc); e("run_fixups", t0);
}
