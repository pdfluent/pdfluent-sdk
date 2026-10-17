// Copyright (c) 2026 Innovation Trigger B.V.
//
// Licensed under the same terms as this crate: Apache-2.0 OR MIT.

// Hand-written from the approved tables `specs/text/ams-glyph-names.tsv`
// (5 rows), `specs/text/ams-glyph-names-phase2.tsv` (1 row) and
// `specs/text/AMS-GLYPH-NAMES-PHASE3.tsv` (1 row) and
// `specs/text/AMS-GLYPH-NAMES-PHASE4.tsv` (2 rows) and
// `specs/text/AMS-GLYPH-NAMES-PHASE5.tsv` (1 row); provenance in
// generated/PROVENANCE.md. Names as defined in the
// AMSFonts 3.04 Type 1 fonts, values from the Unicode 16.0 code charts.
// Data only: no font program or outline is taken.

/// Look up a glyph name that the AMS math fonts define but the Adobe Glyph
/// List does not. Consulted only after every AGL rule has failed.
pub(crate) fn get(name: &str) -> Option<char> {
    AMS_GLYPH_NAMES
        .iter()
        .find(|(entry, _)| *entry == name)
        .map(|(_, value)| *value)
}

static AMS_GLYPH_NAMES: [(&str, char); 10] = [
    // SQUARED DOT OPERATOR
    ("squaredot", '\u{22A1}'),
    // WHITE SQUARE
    ("square", '\u{25A1}'),
    // UPWARDS PAIRED ARROWS
    ("dblarrowup", '\u{21C8}'),
    // DOWNWARDS PAIRED ARROWS
    ("dblarrowdwn", '\u{21CA}'),
    // WHITE UP-POINTING TRIANGLE
    ("triangle", '\u{25B3}'),
    // COMBINING ENCLOSING CIRCLE (phase 2: cmsy10 draws it over a separate
    // `c` for TeX's \copyright; the interpreter puts it after that `c`)
    ("circlecopyrt", '\u{20DD}'),
    // COMBINING LONG SOLIDUS OVERLAY (phase 3: cmsy10's zero-width slash,
    // drawn over the glyph after it for TeX's \not; the interpreter puts it
    // after that glyph)
    ("negationslash", '\u{0338}'),
    // MATHEMATICAL LEFT ANGLE BRACKET (phase 4: cmsy10's delimiter; a plain
    // character, read where it stands)
    ("angbracketleft", '\u{27E8}'),
    // MATHEMATICAL RIGHT ANGLE BRACKET (phase 4, as above)
    ("angbracketright", '\u{27E9}'),
    // OPEN BOX (phase 5: cmtt10's graphic for a space; a plain character,
    // read where it stands)
    ("visiblespace", '\u{2423}'),
];

#[cfg(test)]
mod tests {
    use super::get;

    #[test]
    fn the_phase_two_row_reads_as_the_enclosing_circle() {
        assert_eq!(get("circlecopyrt"), Some('\u{20DD}'));
    }

    #[test]
    fn the_phase_three_row_reads_as_the_long_solidus_overlay() {
        assert_eq!(get("negationslash"), Some('\u{0338}'));
    }

    #[test]
    fn the_phase_four_rows_read_as_the_mathematical_angle_brackets() {
        assert_eq!(get("angbracketleft"), Some('\u{27E8}'));
        assert_eq!(get("angbracketright"), Some('\u{27E9}'));
    }

    #[test]
    fn the_phase_five_row_reads_as_the_open_box() {
        assert_eq!(get("visiblespace"), Some('\u{2423}'));
    }

    #[test]
    fn the_agl_copyright_name_is_not_in_this_table() {
        assert_eq!(get("copyright"), None);
    }
}
