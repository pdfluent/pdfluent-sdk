//! Trace which PDF/A font pass rewrites a font's embedded program bytes.
//!
//! Companion to pdfa_width_trace / pdfa_encoding_trace: prints an md5 of the
//! font program after each pass. Usage:
//!   pdfa_program_trace <pdf> <BaseFont>

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{Document, Object};

fn program_hash(doc: &Document, base_font: &str) -> String {
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
        let fd = match dict.get(b"FontDescriptor") {
            Ok(Object::Reference(r)) => r,
            _ => return "no fd".into(),
        };
        let Some(Object::Dictionary(fd)) = doc.objects.get(fd) else {
            return "no fd obj".into();
        };
        for key in [&b"FontFile2"[..], &b"FontFile3"[..], &b"FontFile"[..]] {
            if let Ok(Object::Reference(sid)) = fd.get(key) {
                if let Some(Object::Stream(s)) = doc.objects.get(sid) {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    let digest = md5_compute(&s.content);
                    return digest;
                }
            }
        }
        return "no fontfile".into();
    }
    "font not found".into()
}

fn md5_compute(data: &[u8]) -> String {
    // minimal md5 via lopdf? No — use a simple FNV-like digest to avoid a dep.
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: pdfa_program_trace <pdf> <BaseFont>");
        std::process::exit(2);
    }
    let (path, font) = (&args[1], args[2].as_str());

    let data = std::fs::read(path).expect("read input");
    let mut doc = Document::load_mem(&data).expect("load");

    println!("{:<34} {}", "start", program_hash(&doc, font));

    macro_rules! step {
        ($name:literal, $call:expr) => {
            let before = program_hash(&doc, font);
            $call;
            let after = program_hash(&doc, font);
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
