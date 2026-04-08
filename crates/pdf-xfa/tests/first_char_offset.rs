use pdf_xfa::font_bridge::ResolvedFont;

fn make_font_with_first_char_32() -> ResolvedFont {
    let mut widths = vec![500; 34];
    widths[0] = 278; // codepoint 32 (' ')
    widths[33] = 722; // codepoint 65 ('A')

    ResolvedFont {
        name: "TestFont".to_string(),
        data: Vec::new(),
        face_index: 0,
        units_per_em: 1000,
        ascender: 800,
        descender: -200,
        pdf_widths: Some((32, widths)),
    }
}

#[test]
fn pdf_glyph_widths_maps_space_from_first_char_offset() {
    let font = make_font_with_first_char_32();
    let (_first_char, widths) = font.pdf_glyph_widths();

    assert_eq!(
        widths.get(32).copied(),
        Some(278),
        "space (codepoint 32) should map to the first /Widths entry, not zero or an offset glyph"
    );
}

#[test]
fn pdf_glyph_widths_maps_a_from_first_char_offset() {
    let font = make_font_with_first_char_32();
    let (_first_char, widths) = font.pdf_glyph_widths();

    assert_eq!(
        widths.get(65).copied(),
        Some(722),
        "'A' (codepoint 65) should map through FirstChar=32 to the 34th /Widths entry"
    );
}
