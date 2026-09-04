//! Trace what each PDF/A font pass does to one font's /Encoding.
//!
//! Companion to pdfa_width_trace: when the width is right but the declared
//! encoding changed underneath it, the useful question is which pass rewrote
//! the dictionary. Prints a compact encoding summary after each pass.
//!
//! ```text
//! cargo run -p pdf-manip --example pdfa_encoding_trace -- input.pdf FontName
//! ```

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{Document, Object};

fn encoding_of(doc: &Document, base_font: &str) -> String {
    for obj in doc.objects.values() {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        let Ok(Object::Name(bf)) = dict.get(b"BaseFont") else {
            continue;
        };
        if String::from_utf8_lossy(bf) != base_font {
            continue;
        };
        return match dict.get(b"Encoding") {
            Err(_) => "none".into(),
            Ok(Object::Name(n)) => format!("name:{}", String::from_utf8_lossy(n)),
            Ok(Object::Dictionary(d)) => summarize_enc_dict(doc, d),
            Ok(Object::Reference(r)) => match doc.objects.get(r) {
                Some(Object::Dictionary(d)) => format!("ref:{}", summarize_enc_dict(doc, d)),
                Some(Object::Name(n)) => format!("ref:name:{}", String::from_utf8_lossy(n)),
                _ => "ref:?".into(),
            },
            Ok(other) => format!("{other:?}"),
        };
    }
    "font not found".into()
}

fn summarize_enc_dict(_doc: &Document, d: &lopdf::Dictionary) -> String {
    let base = match d.get(b"BaseEncoding") {
        Ok(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
        _ => "-".into(),
    };
    let (ndiff, first) = match d.get(b"Differences") {
        Ok(Object::Array(a)) => {
            let head: Vec<String> = a
                .iter()
                .take(5)
                .map(|o| match o {
                    Object::Integer(i) => i.to_string(),
                    Object::Name(n) => format!("/{}", String::from_utf8_lossy(n)),
                    other => format!("{other:?}"),
                })
                .collect();
            (a.len(), head.join(" "))
        }
        _ => (0, String::new()),
    };
    format!("dict base={base} ndiff={ndiff} [{first}]")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: pdfa_encoding_trace <pdf> <BaseFont>");
        std::process::exit(2);
    }
    let (path, font) = (&args[1], args[2].as_str());

    let data = std::fs::read(path).expect("read input");
    let mut doc = Document::load_mem(&data).expect("load");

    println!("{:<34} {}", "start", encoding_of(&doc, font));

    macro_rules! step {
        ($name:literal, $call:expr) => {
            let before = encoding_of(&doc, font);
            $call;
            let after = encoding_of(&doc, font);
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
