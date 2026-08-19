// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! Dump a CFF program's charset names and per-glyph widths.
//!
//! Answers "does this subset actually carry the glyph?" — the question behind
//! most §6.2.11.5 and §6.2.11.8 failures, and the one that decides whether a
//! code should be re-pointed or the text left alone.
//!
//! ```text
//! cargo run -p pdf-manip --example debug_cff_dump -- font.cff
//! ```
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let mut data = std::fs::read(&path).unwrap();
    // PFB segment header, if the caller handed us a wrapped program.
    if data.len() > 4 && &data[0..2] == b"\x80\x01" {
        // PFB: skip header
        let len = u32::from_le_bytes([data[3], data[4], data[5], data[6]]) as usize;
        data = data[6..6 + len].to_vec();
    }
    let Some(cff) = cff_parser::Table::parse(&data) else {
        println!("no CFF parse");
        return;
    };
    println!("glyphs: {}", cff.number_of_glyphs());
    for gid in 0..cff.number_of_glyphs().min(30) {
        let id = cff_parser::GlyphId(gid);
        let name = cff.glyph_name(id).unwrap_or("<none>");
        let w = cff.glyph_width_f64(id);
        println!("  gid {gid}: {name} w={w:?}");
    }
}
