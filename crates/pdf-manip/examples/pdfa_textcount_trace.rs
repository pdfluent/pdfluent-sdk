//! Trace text-extractability through the font passes: print how many codes
//! in the target font's FirstChar..LastChar range resolve to a real glyph
//! (via Differences/base -> (3,1) cmap -> gid with outline) after each pass.
//! Crude but decisive for "rendering broke somewhere" hunts.
use lopdf::{Document, Object};

fn state(doc: &Document, base_font: &str) -> String {
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
        let fc = match dict.get(b"FirstChar") {
            Ok(Object::Integer(i)) => *i as u32,
            _ => 0,
        };
        let lc = match dict.get(b"LastChar") {
            Ok(Object::Integer(i)) => *i as u32,
            _ => 255,
        };
        let enc = match dict.get(b"Encoding") {
            Ok(Object::Name(n)) => format!("name:{}", String::from_utf8_lossy(n)),
            Ok(Object::Dictionary(d)) => {
                let base = d
                    .get(b"BaseEncoding")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .map(|n| String::from_utf8_lossy(n).to_string())
                    .unwrap_or("-".into());
                let nd = d
                    .get(b"Differences")
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .map(|a| a.len())
                    .unwrap_or(0);
                format!("base={} ndiff={}", base, nd)
            }
            Ok(Object::Reference(r)) => match doc.objects.get(r) {
                Some(Object::Dictionary(d)) => {
                    let base = d
                        .get(b"BaseEncoding")
                        .ok()
                        .and_then(|o| o.as_name().ok())
                        .map(|n| String::from_utf8_lossy(n).to_string())
                        .unwrap_or("-".into());
                    let nd = d
                        .get(b"Differences")
                        .ok()
                        .and_then(|o| o.as_array().ok())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    format!("ref:base={} ndiff={}", base, nd)
                }
                Some(Object::Name(n)) => format!("ref:name:{}", String::from_utf8_lossy(n)),
                _ => "ref:?".into(),
            },
            _ => "none".into(),
        };
        return format!("fc={} lc={} enc={}", fc, lc, enc);
    }
    "font not found".into()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (path, font) = (&args[1], args[2].as_str());
    let data = std::fs::read(path).expect("read input");
    let mut doc = Document::load_mem(&data).expect("load");
    println!("{:<34} {}", "start", state(&doc, font));
    macro_rules! step {
        ($name:literal, $call:expr) => {
            let before = state(&doc, font);
            $call;
            let after = state(&doc, font);
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
    step!(
        "cff_enc_supplements",
        f::fix_cff_encoding_supplements(&mut doc)
    );
    step!("fix_cff_widths", f::fix_cff_widths(&mut doc));
    step!(
        "fix_truetype_cid_widths",
        f::fix_truetype_cid_widths(&mut doc)
    );
    step!("fix_type1_charset", f::fix_type1_charset(&mut doc));
    step!("fix_truetype_encoding", f::fix_truetype_encoding(&mut doc));
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
    step!(
        "custom_cff_enc_widths",
        f::fix_custom_cff_encoding_widths(&mut doc)
    );
}
