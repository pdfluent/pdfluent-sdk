// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! Trace one glyph's declared width across the sub-passes inside `embed_fonts`.
//!
//! `pdfa_width_trace` reports `embed_fonts` as a single step, which is too
//! coarse when the wrong number is written by one of the dozen passes it calls.
//!
//! ```text
//! cargo run -p pdf-manip --example pdfa_embed_substep_trace -- input.pdf FontName 32
//! ```

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
            Object::Integer(w) => w.to_string(),
            Object::Real(r) => r.to_string(),
            other => format!("{other:?}"),
        };
    }
    "font not found".into()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: pdfa_embed_substep_trace <pdf> <BaseFont> <code>");
        std::process::exit(2);
    }
    let (path, font, code) = (&args[1], args[2].as_str(), args[3].parse::<i64>().unwrap());

    let data = std::fs::read(path).expect("read input");
    let mut doc = Document::load_mem(&data).expect("load");

    println!("{:<40} {}", "start", width_of(&doc, font, code));

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
            println!("{:<40} {}{}", $name, after, mark);
        };
    }

    // The sub-passes `embed_fonts` runs, in its order.
    step!(
        "isolate_font_descriptors",
        pdf_manip::pdfa_fonts::isolate_font_descriptors(&mut doc)
    );
    step!(
        "sync_subtypes_from_fontfile",
        pdf_manip::pdfa_fonts::sync_subtypes_from_fontfile(&mut doc)
    );
    step!("enforce_pdfa_font_compliance", {
        let _ = pdf_manip::pdfa_fonts::enforce_pdfa_font_compliance(&mut doc);
    });
    step!("fix_font_descriptor_metrics", {
        let _ = pdf_manip::pdfa_fonts::fix_font_descriptor_metrics(&mut doc);
    });
    step!("fix_type1_charset", {
        let _ = pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    });
    step!("fix_missing_simple_font_widths", {
        let _ = pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    });
    step!("fix_incomplete_tounicode_from_encoding", {
        let _ = pdf_manip::pdfa_fonts::fix_incomplete_tounicode_from_encoding(&mut doc);
    });
    step!("sync_widths_from_embedded_fonts", {
        let _ = pdf_manip::pdfa_fonts::sync_widths_from_embedded_fonts(&mut doc);
    });
}
