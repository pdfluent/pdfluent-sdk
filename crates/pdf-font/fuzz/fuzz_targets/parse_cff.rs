#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Must never panic regardless of input. #463 / crash resistance audit.
    // Exercise parsing + all glyph metric accessors.
    if let Some(table) = pdf_font::font::cff::Table::parse(data) {
        let n = table.number_of_glyphs();
        for id in 0..n {
            let gid = pdf_font::font::GlyphId(id);
            let _ = table.glyph_width(gid);
            let _ = table.glyph_matrix(gid);
            let _ = table.glyph_name(gid);
            let _ = table.glyph_cid(gid);
        }
    }
});
