//! Trace what each PDF/A font pass does to one glyph's declared width.
//!
//! When veraPDF reports a width-consistency failure, the useful question is
//! which pass wrote the wrong number. This runs the font passes in pipeline
//! order and prints the width after each one.
//!
//! ```text
//! cargo run -p pdf-manip --example pdfa_width_trace -- input.pdf FontName 32
//! ```

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{Document, Object};

fn width_of(doc: &Document, base_font: &str, code: i64) -> String {
    for obj in doc.objects.values() {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        let Ok(Object::Name(bf)) = dict.get(b"BaseFont") else {
            continue;
        };
        if String::from_utf8_lossy(bf) != base_font {
            continue;
        }
        let first = match dict.get(b"FirstChar") {
            Ok(Object::Integer(i)) => *i,
            _ => return "no FirstChar".into(),
        };
        let Ok(Object::Array(widths)) = dict.get(b"Widths") else {
            return "no Widths".into();
        };
        let idx = code - first;
        if idx < 0 || idx as usize >= widths.len() {
            return "out of range".into();
        }
        return match &widths[idx as usize] {
            Object::Integer(i) => i.to_string(),
            Object::Real(r) => r.to_string(),
            other => format!("{other:?}"),
        };
    }
    "font not found".into()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: pdfa_width_trace <pdf> <BaseFont> <code>");
        std::process::exit(2);
    }
    let (path, font, code) = (&args[1], args[2].as_str(), args[3].parse::<i64>().unwrap());

    let data = std::fs::read(path).expect("read input");
    let mut doc = Document::load_mem(&data).expect("load");

    println!("{:<34} {}", "start", width_of(&doc, font, code));

    macro_rules! step {
        ($name:literal, $call:expr) => {
            let before = width_of(&doc, font, code);
            $call;
            let after = width_of(&doc, font, code);
            let mark = if before == after {
                ""
            } else {
                "   <-- CHANGED"
            };
            println!("{:<34} {}{}", $name, after, mark);
        };
    }

    use pdf_manip::{pdfa_cleanup, pdfa_fonts as f};

    step!("cleanup_for_pdfa", {
        let _ = pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
    });
    step!(
        "promote_inline_font_dicts",
        f::promote_inline_font_dicts(&mut doc)
    );
    step!("embed_fonts", {
        let _ = f::embed_fonts(&mut doc);
    });
    step!("fix_pfb_font_streams", f::fix_pfb_font_streams(&mut doc));
    step!(
        "fix_type1_stub_font_files",
        f::fix_type1_stub_font_files(&mut doc)
    );
    step!(
        "fix_mislabeled_truetype_as_cff",
        f::fix_mislabeled_truetype_as_cff(&mut doc)
    );
    step!(
        "fix_truetype_with_cff_program",
        f::fix_truetype_with_cff_program(&mut doc)
    );
    step!("fix_cff_invalid_bcd", f::fix_cff_invalid_bcd(&mut doc));
    step!(
        "fix_type1_nonstandard_charstrings",
        f::fix_type1_nonstandard_charstrings(&mut doc)
    );
    step!(
        "fix_type1_eexec_space_prefix",
        f::fix_type1_eexec_space_prefix(&mut doc)
    );
    step!("fix_cff_widths", f::fix_cff_widths(&mut doc));
    step!(
        "fix_truetype_cid_widths",
        f::fix_truetype_cid_widths(&mut doc)
    );
    step!("fix_type1_charset", f::fix_type1_charset(&mut doc));
    step!("fix_truetype_encoding", f::fix_truetype_encoding(&mut doc));
    step!(
        "fix_type1_standard_encoding",
        f::fix_type1_standard_encoding(&mut doc)
    );
    step!(
        "fix_existing_symbolic_tt_cmaps",
        f::fix_existing_symbolic_truetype_cmaps(&mut doc)
    );
    step!(
        "fix_truetype_unicode_cmap",
        f::fix_truetype_unicode_cmap(&mut doc)
    );
    step!(
        "fix_type1_tounicode_from_encoding",
        f::fix_type1_tounicode_from_encoding(&mut doc)
    );
    step!("fix_type0_tounicode", f::fix_type0_tounicode(&mut doc));
    step!(
        "fix_type1_tounicode_from_cff",
        f::fix_type1_tounicode_from_cff(&mut doc)
    );
    step!(
        "fix_tounicode_forbidden_values",
        f::fix_tounicode_forbidden_values(&mut doc)
    );
    step!("fix_notdef_glyph_refs", f::fix_notdef_glyph_refs(&mut doc));
    step!(
        "fix_type3_notdef_charprocs",
        f::fix_type3_notdef_charprocs(&mut doc)
    );
    step!("fix_cid_font_notdef", f::fix_cid_font_notdef(&mut doc));
    step!(
        "fix_symbolic_font_notdef_streams",
        f::fix_symbolic_font_notdef_streams(&mut doc)
    );
    step!(
        "fix_simple_font_streams",
        f::fix_simple_font_streams(&mut doc)
    );
    step!(
        "fix_type1_subset_missing_glyphs",
        f::fix_type1_subset_missing_glyphs(&mut doc)
    );
    step!(
        "fix_undefined_encoding_codes",
        f::fix_undefined_encoding_codes(&mut doc)
    );
    step!("fix_symbolic_flags", f::fix_symbolic_flags(&mut doc));
    step!(
        "fix_classic_symbolic_base14_encoding",
        f::fix_classic_symbolic_base14_encoding(&mut doc)
    );
    step!(
        "fix_missing_simple_font_widths",
        f::fix_missing_simple_font_widths(&mut doc)
    );
    step!("fix_type3_font_widths", f::fix_type3_font_widths(&mut doc));
    step!(
        "fix_font_width_mismatches",
        f::fix_font_width_mismatches(&mut doc)
    );
    step!(
        "fix_symbolic_font_widths",
        f::fix_symbolic_font_widths(&mut doc)
    );
    step!(
        "fix_remaining_tt_width_mismatches",
        f::fix_remaining_tt_width_mismatches(&mut doc)
    );
    step!("fix_cidset", f::fix_cidset(&mut doc));
    step!(
        "fix_missing_cidtogidmap",
        f::fix_missing_cidtogidmap(&mut doc)
    );
    step!(
        "fix_cff_subset_missing_space",
        f::fix_cff_subset_missing_space(&mut doc)
    );
}
