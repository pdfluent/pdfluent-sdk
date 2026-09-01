/// This struct represents the data of a font.
///
/// # Why there is no `font_embedding` feature here
///
/// Upstream puts this module and `Document::add_font` behind one. The 0.44.0
/// merge brought the six `#[cfg]` attributes across without the `[features]`
/// entry that names the flag, so the code compiled out silently and the tests
/// behind it could not build at all.
///
/// Declaring the feature fixed that, and then could not be finished: a gated
/// test needs a CI job that enables the flag, and the guard that enforces this
/// (`test_feature_gated_tests_run.py`) reads `.gitlab-ci.yml` statically -- it
/// does not resolve default features, and that file belongs to another
/// territory. So the flag was a permanent "feature no job enables".
///
/// It is gone instead. `ttf-parser` is a hard dependency again, as it was
/// before the merge, and the capability is simply always present -- which is
/// what the owner decision on #292 asked for. The cost is six lines of
/// divergence at the next upstream merge, recorded in docs/UPSTREAM_FORKS.toml.
/// It contains information about the font's bounding box, ascent, descent, cap height, italic angle, and stemV.
/// Reference: https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.5_v6.pdf
#[derive(Debug, Clone)]
pub struct FontData {
    /// (Required) The PostScript name of the font. This should be the same as the value of BaseFont in the font or
    /// CIDFont dictionary that refers to this font descriptor.
    pub font_name: String,
    /// (Required) A collection of flags defining various characteristics of the font.
    pub flags: i64,
    /// (Required, except for Type 3 fonts) A rectangle (see Section 3.8.4, “Rectangles”), expressed in the glyph
    /// coordinate system, specifying the font bounding box. This is the smallest rectangle enclosing the shape that
    /// would result if all of the glyphs of the font were placed with their origins coincident and then filled.
    /// Format as: (x_min, y_min, x_max, y_max).
    pub font_bbox: (i64, i64, i64, i64),
    /// (Required) The angle, expressed in degrees counterclockwise from the vertical, of the dominant vertical strokes
    /// of the font. (For example, the 9-o’clock position is 90 degrees, and the 3-o’clock position is –90 degrees.)
    /// The value is negative for fonts that slope to the right, as almost all italic fonts do.
    pub italic_angle: i64,
    /// (Required, except for Type 3 fonts) The maximum height above the baseline reached by glyphs in this font,
    /// excluding the height of glyphs for accentedc haracters.
    pub ascent: i64,
    /// (Required, except for Type 3 fonts) The maximum depth below the baseline reached by glyphs in this font. The
    /// value is a negative number.
    pub descent: i64,
    /// (Required for fonts that have Latin characters, except for Type 3 fonts) The vertical coordinate of the top of
    /// flat capital letters, measured from the baseline.
    pub cap_height: i64,
    /// (Required, except for Type 3 fonts) The thickness, measured horizontally, of the dominant vertical stems of
    /// glyphs in the font.
    pub stem_v: i64,
    /// (Required) The name of a predefined CMap, or a stream containing a CMap program, that maps character codes to
    /// font numbers and CIDs. If the descendant is a Type 2 CIDFont whose associated TrueType font program is not
    /// embedded in the PDF file, the Encoding entry must be a predefined CMap name Read more (page 422): https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.5_v6.pdf
    pub encoding: String,
    /// Size of the font data in bytes.
    /// This is used to set the `Length1` key in the font stream dictionary.
    font: Vec<u8>,
}

/// This struct is used to store font metadata extracted from a TrueType Fonts (TTF) file.
/// # Examples
///
/// ```no_run
/// // Read a TrueType Fonts (TTF) file.
/// let font_file = std::fs::read("./SomeFont.ttf").unwrap();
///
/// // Create a new FontData instance.
/// let font_name = "SomeFont".to_string();
/// let font_data = pdfluent_lopdf::FontData::new(&font_file, font_name);
/// ```
///
/// Also provides methods to set various font properties such as bounding box, italic angle, ascent, descent, and stemV.
/// # Examples
///
/// ```no_run
/// let font_file = std::fs::read("./SomeFont.ttf").unwrap();
///
/// // Create a new FontData instance along custome value.
/// let font_data = pdfluent_lopdf::FontData::new(&font_file, "SomeFont".to_string())
///                     .set_stem_v(100)
///                     .set_italic_angle(10);
/// ```
impl FontData {
    /// Create a new `FontData` instance by parsing the provided TTF file.
    /// The TTF file is expected to be in bytes.
    pub fn new(font_file: &[u8], font_name: String) -> Self {
        // Parse the TTF file using ttf_parser crate
        let font = ttf_parser::Face::parse(font_file, 0).expect("Failed to parse font file");

        // Extract font metadata
        // Note: The ttf_parser crate provides methods to get font bounding box, ascent, descent, cap height, italic
        // angle, and stemV.
        let font_bbox = font.global_bounding_box();
        let ascent = font.ascender();
        let descent = font.descender();
        let cap_height = font.capital_height().unwrap_or(ascent);
        let italic_angle = font.italic_angle();
        let flags = 1; // Default flags, can be modified later if needed

        // Calculate stemV based on the font bounding box
        // Reference: https://stackoverflow.com/questions/35485179/stemv-value-of-the-truetype-font
        // The stemV is typically calculated as 13% of the font's bbox width value.
        let stem_v = (font_bbox.width() as f64 * 0.13).round() as i64;

        Self {
            font_name,
            flags,
            font_bbox: (
                font_bbox.x_min as i64,
                font_bbox.y_min as i64,
                font_bbox.x_max as i64,
                font_bbox.y_max as i64,
            ),
            italic_angle: italic_angle.round() as i64,
            ascent: ascent as i64,
            descent: descent as i64,
            cap_height: cap_height as i64,
            stem_v,
            encoding: "WinAnsiEncoding".to_string(), // Default encoding, can be modified later if needed
            font: font_file.to_vec(),
        }
    }

    pub fn set_flags(&mut self, flags: i64) -> &mut Self {
        self.flags = flags;
        self
    }

    pub fn set_font_bbox(&mut self, font_bbox: (i64, i64, i64, i64)) -> &mut Self {
        self.font_bbox = font_bbox;
        self
    }

    pub fn set_italic_angle(&mut self, italic_angle: i64) -> &mut Self {
        self.italic_angle = italic_angle;
        self
    }

    pub fn set_ascent(&mut self, ascent: i64) -> &mut Self {
        self.ascent = ascent;
        self
    }

    pub fn set_descent(&mut self, descent: i64) -> &mut Self {
        self.descent = descent;
        self
    }

    pub fn set_cap_height(&mut self, cap_height: i64) -> &mut Self {
        self.cap_height = cap_height;
        self
    }

    pub fn set_stem_v(&mut self, stem_v: i64) -> &mut Self {
        self.stem_v = stem_v;
        self
    }

    pub fn set_encoding(&mut self, encoding: String) -> &mut Self {
        self.encoding = encoding;
        self
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.font.clone()
    }
}

/// A TrueType font assembled byte by byte, so the tests below need no asset.
///
/// The fork ships no font files, which is why the one test upstream has for
/// this feature carries `#[ignore]` and a note about a missing Montserrat. An
/// ignored test is indistinguishable from a passing one in the summary line, so
/// the font is built here instead: five tables, each field set to a value that
/// appears nowhere else, so a metric read from the wrong table is visible in the
/// assertion rather than plausible.
#[cfg(test)]
pub(crate) mod synthetic {
    /// Values chosen to be mutually distinguishable: no two are equal, none is
    /// zero, and ascender/descender/capHeight differ from their OS/2 twins so
    /// that reading the wrong table changes the answer.
    pub(crate) struct Metrics {
        pub units_per_em: u16,
        pub x_min: i16,
        pub y_min: i16,
        pub x_max: i16,
        pub y_max: i16,
        pub ascender: i16,
        pub descender: i16,
        pub cap_height: i16,
        /// Degrees, as `post` stores it (16.16 fixed).
        pub italic_angle: f32,
        /// OS/2 table version. `sCapHeight` exists only from version 2, so
        /// version 1 is how a font says it does not state a cap height --
        /// which is not the same as stating zero.
        pub os2_version: u16,
    }

    impl Default for Metrics {
        fn default() -> Self {
            Self {
                units_per_em: 1000,
                x_min: -137,
                y_min: -291,
                x_max: 1543,
                y_max: 983,
                ascender: 811,
                descender: -217,
                cap_height: 709,
                italic_angle: -12.0,
                os2_version: 4,
            }
        }
    }

    fn be16(out: &mut Vec<u8>, v: u16) {
        out.extend_from_slice(&v.to_be_bytes());
    }

    fn be16i(out: &mut Vec<u8>, v: i16) {
        out.extend_from_slice(&v.to_be_bytes());
    }

    fn be32(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_be_bytes());
    }

    fn head(m: &Metrics) -> Vec<u8> {
        let mut t = Vec::new();
        be16(&mut t, 1); // majorVersion
        be16(&mut t, 0); // minorVersion
        be32(&mut t, 0x0001_0000); // fontRevision
        be32(&mut t, 0); // checkSumAdjustment -- ttf-parser does not verify it
        be32(&mut t, 0x5F0F_3CF5); // magicNumber
        be16(&mut t, 0); // flags
        be16(&mut t, m.units_per_em);
        t.extend_from_slice(&0i64.to_be_bytes()); // created
        t.extend_from_slice(&0i64.to_be_bytes()); // modified
        be16i(&mut t, m.x_min);
        be16i(&mut t, m.y_min);
        be16i(&mut t, m.x_max);
        be16i(&mut t, m.y_max);
        be16(&mut t, 0); // macStyle
        be16(&mut t, 8); // lowestRecPPEM
        be16i(&mut t, 2); // fontDirectionHint
        be16i(&mut t, 0); // indexToLocFormat
        be16i(&mut t, 0); // glyphDataFormat
        debug_assert_eq!(t.len(), 54);
        t
    }

    fn hhea(m: &Metrics) -> Vec<u8> {
        let mut t = Vec::new();
        be16(&mut t, 1); // majorVersion
        be16(&mut t, 0); // minorVersion
        be16i(&mut t, m.ascender);
        be16i(&mut t, m.descender);
        be16i(&mut t, 0); // lineGap
        be16(&mut t, 1600); // advanceWidthMax
        be16i(&mut t, m.x_min);
        be16i(&mut t, 0); // minRightSideBearing
        be16i(&mut t, m.x_max); // xMaxExtent
        be16i(&mut t, 1); // caretSlopeRise
        be16i(&mut t, 0); // caretSlopeRun
        be16i(&mut t, 0); // caretOffset
        for _ in 0..4 {
            be16i(&mut t, 0); // reserved
        }
        be16i(&mut t, 0); // metricDataFormat
        be16(&mut t, 1); // numberOfHMetrics
        debug_assert_eq!(t.len(), 36);
        t
    }

    fn maxp() -> Vec<u8> {
        let mut t = Vec::new();
        be32(&mut t, 0x0000_5000); // version 0.5
        be16(&mut t, 1); // numGlyphs
        t
    }

    fn post(m: &Metrics) -> Vec<u8> {
        let mut t = Vec::new();
        be32(&mut t, 0x0003_0000); // version 3.0, no glyph names
        // italicAngle is 16.16 fixed point, and it is signed.
        be32(&mut t, ((m.italic_angle * 65536.0) as i32) as u32);
        be16i(&mut t, -75); // underlinePosition
        be16i(&mut t, 50); // underlineThickness
        be32(&mut t, 0); // isFixedPitch
        for _ in 0..4 {
            be32(&mut t, 0); // minMemType42 .. maxMemType1
        }
        debug_assert_eq!(t.len(), 32);
        t
    }

    fn os2(m: &Metrics) -> Vec<u8> {
        let mut t = Vec::new();
        be16(&mut t, m.os2_version); // sCapHeight exists from version 2
        be16i(&mut t, 600); // xAvgCharWidth
        be16(&mut t, 400); // usWeightClass
        be16(&mut t, 5); // usWidthClass
        be16(&mut t, 0); // fsType
        for _ in 0..10 {
            be16i(&mut t, 0); // ySubscript* .. yStrikeoutPosition
        }
        be16i(&mut t, 0); // sFamilyClass
        t.extend_from_slice(&[0u8; 10]); // panose
        for _ in 0..4 {
            be32(&mut t, 0); // ulUnicodeRange1..4
        }
        t.extend_from_slice(b"TEST"); // achVendID
        // fsSelection bit 7 is USE_TYPO_METRICS. It stays clear, so ascender and
        // descender must come from hhea; the sTypo* fields below are deliberately
        // different values, and a reader that took them would fail the assertion.
        be16(&mut t, 0);
        be16(&mut t, 32); // usFirstCharIndex
        be16(&mut t, 122); // usLastCharIndex
        be16i(&mut t, 700); // sTypoAscender -- not 811
        be16i(&mut t, -300); // sTypoDescender -- not -217
        be16i(&mut t, 0); // sTypoLineGap
        be16(&mut t, 900); // usWinAscent
        be16(&mut t, 250); // usWinDescent
        be32(&mut t, 0); // ulCodePageRange1
        be32(&mut t, 0); // ulCodePageRange2
        debug_assert_eq!(t.len(), 86, "version 1 ends here");
        if m.os2_version >= 2 {
            be16i(&mut t, 520); // sxHeight
            be16i(&mut t, m.cap_height);
            be16(&mut t, 0); // usDefaultChar
            be16(&mut t, 32); // usBreakChar
            be16(&mut t, 0); // usMaxContext
            debug_assert_eq!(t.len(), 96);
        }
        t
    }

    /// Assemble the sfnt container: header, table directory, then the tables.
    pub(crate) fn font(m: &Metrics) -> Vec<u8> {
        // Tag order is ascending by the four bytes, which is what the format
        // requires of the directory: "OS/2" < "head" < "hhea" < "maxp" < "post".
        let tables: [(&[u8; 4], Vec<u8>); 5] = [
            (b"OS/2", os2(m)),
            (b"head", head(m)),
            (b"hhea", hhea(m)),
            (b"maxp", maxp()),
            (b"post", post(m)),
        ];

        let n = tables.len() as u16;
        let mut out = Vec::new();
        be32(&mut out, 0x0001_0000); // sfntVersion: TrueType outlines
        be16(&mut out, n);
        // searchRange, entrySelector and rangeShift. No reader here uses them,
        // but writing them wrong is the kind of thing a stricter parser trips on.
        let entry_selector = (u16::BITS - 1 - n.leading_zeros()) as u16;
        let search_range = (1u16 << entry_selector) * 16;
        be16(&mut out, search_range);
        be16(&mut out, entry_selector);
        be16(&mut out, n * 16 - search_range);

        let mut offset = 12 + 16 * tables.len() as u32;
        let mut body = Vec::new();
        for (tag, data) in &tables {
            out.extend_from_slice(*tag);
            be32(&mut out, 0); // checkSum
            be32(&mut out, offset);
            be32(&mut out, data.len() as u32);
            offset += data.len() as u32;
            body.extend_from_slice(data);
        }
        out.extend_from_slice(&body);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::FontData;
    use super::synthetic::{self, Metrics};

    #[test]
    fn the_synthetic_font_parses_at_all() {
        // If this fails, every other test in this module is testing the builder
        // rather than the code under it, so it is asserted separately.
        let bytes = synthetic::font(&Metrics::default());
        assert!(
            ttf_parser::Face::parse(&bytes, 0).is_ok(),
            "the hand-built font is not a font"
        );
    }

    #[test]
    fn metrics_come_from_the_font_and_not_from_defaults() {
        let m = Metrics::default();
        let data = FontData::new(&synthetic::font(&m), "Synthetic".to_string());

        assert_eq!(data.font_name, "Synthetic");
        assert_eq!(data.font_bbox, (-137, -291, 1543, 983), "bbox came from head");
        // hhea, not OS/2's sTypoAscender of 700: fsSelection bit 7 is clear.
        assert_eq!(data.ascent, 811);
        assert_eq!(data.descent, -217);
        // OS/2 version 4 carries sCapHeight, so the unwrap_or(ascent) fallback
        // must not fire. 709 and 811 differ precisely so that this is visible.
        assert_eq!(data.cap_height, 709);
        assert_eq!(data.italic_angle, -12);
    }

    #[test]
    fn stem_v_is_thirteen_percent_of_the_bounding_box_width() {
        let data = FontData::new(&synthetic::font(&Metrics::default()), "S".to_string());
        // width = 1543 - (-137) = 1680; 1680 * 0.13 = 218.4, rounded to 218.
        assert_eq!(data.stem_v, 218);
    }

    #[test]
    fn cap_height_falls_back_to_the_ascender_when_the_font_omits_it() {
        // An OS/2 version 1 table has no sCapHeight field at all. That, not a
        // stored zero, is how a font declines to state one -- ttf-parser reports
        // Some(0) for a zero, so a test built on that would have proved nothing.
        let m = Metrics {
            os2_version: 1,
            ..Metrics::default()
        };
        let data = FontData::new(&synthetic::font(&m), "S".to_string());
        assert_eq!(data.cap_height, 811);
    }

    #[test]
    fn the_font_bytes_survive_the_round_trip() {
        let bytes = synthetic::font(&Metrics::default());
        let data = FontData::new(&bytes, "S".to_string());
        assert_eq!(data.bytes(), bytes, "the embedded bytes are not the input");
    }
}
