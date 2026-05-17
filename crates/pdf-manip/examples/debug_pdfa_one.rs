use lopdf::Document;
use std::fs;

fn run_pipeline(doc: &mut Document, mode: &str) {
    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(doc, false);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(doc);
    let _ = pdf_manip::pdfa_fonts::fix_cff_widths(doc);
    if mode != "skip_tt_cid" {
        let _ = pdf_manip::pdfa_fonts::fix_truetype_cid_widths(doc);
    }
    let _ = pdf_manip::pdfa_fonts::fix_type1_charset(doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_encoding(doc);
    let _ = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(doc);
    let _ = pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(doc);
    let _ = pdf_manip::pdfa_fonts::fix_cid_font_notdef(doc);
    let _ = pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(doc);
    let _ = pdf_manip::pdfa_fonts::fix_simple_font_out_of_range_codes(doc);
    let _ = pdf_manip::pdfa_fonts::strip_control_chars_from_streams(doc);
    let _ = pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(doc);
    let _ = pdf_manip::pdfa_fonts::fix_symbolic_flags(doc);
    let _ = pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(doc);
    let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(doc);
    let _ = pdf_manip::pdfa_fonts::fix_font_width_mismatches(doc);
    let _ = pdf_manip::pdfa_fonts::fix_symbolic_font_widths(doc);
    let _ = pdf_manip::pdfa_fonts::fix_cidset(doc);
    let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(doc);
    let _ = pdf_manip::pdfa_fixups::run_fixups(doc);
    let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(doc);
    let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(
        doc,
        pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        None,
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "usage: {} <input.pdf> <out.pdf> <mode: full|skip_tt_cid>",
            args[0]
        );
        std::process::exit(1);
    }
    let input = &args[1];
    let output = &args[2];
    let mode = &args[3];

    let pdf_bytes = fs::read(input).expect("read");
    let mut doc = Document::load_mem(&pdf_bytes).expect("load");
    run_pipeline(&mut doc, mode);

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut out);
    pdf_manip::pdfa_cleanup::fix_startxref(&mut out);
    std::fs::write(output, &out).expect("write");

    println!("wrote {} bytes", out.len());
}
