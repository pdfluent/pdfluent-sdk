use crate::font::blob::{CffFontBlob, OpenTypeFontBlob, Type1FontBlob};
use crate::font::generated::glyph_names;
use crate::font::standard_font::select_standard_font;
use crate::font::{
    FallbackFontQuery, FontFlags, FontQuery, glyph_name_to_string, glyph_name_to_unicode,
    read_to_unicode, stretch_glyph, strip_subset_prefix,
};
use crate::util::decode_or_warn;
use crate::{CMapResolverFn, CacheKey, FontResolverFn, InterpreterWarning, WarningSinkFn};
use kurbo::{BezPath, Vec2};
use log::warn;
use pdf_font::cmap::{BfString, CMap, CidFamily, WritingMode};
use pdf_syntax::object::Dict;
use pdf_syntax::object::Name;
use pdf_syntax::object::Stream;
use pdf_syntax::object::String as PdfString;
use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Array, Object};
use skrifa::attribute::Style;
use skrifa::raw::TableProvider;
use skrifa::raw::collections::int_set::Domain;
use skrifa::raw::tables::post;
use skrifa::{FontRef, GlyphId, GlyphId16, MetadataProvider};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct Type0Font {
    font_type: FontType,
    horizontal: bool,
    cache_key: u128,
    dw: f32,
    dw2: (f32, f32),
    widths: HashMap<u32, f32>,
    encoding: CMap,
    to_unicode: Option<CMap>,
    widths2: HashMap<u32, [f32; 3]>,
    cid_to_gid_map: CidToGIdMap,
    /// PostScript name from the PDF.
    postscript_name: Option<String>,
    /// Font flags from the font descriptor.
    font_flags: Option<FontFlags>,
    /// Whether this font is using a fallback (non-embedded) font.
    fallback: bool,
    /// Whether the `to_unicode` map is a UCS2 `CMap` (CID-indexed) rather than
    /// a `ToUnicode` `CMap` (code-indexed).
    to_unicode_is_cid_indexed: bool,
    /// Whether the CIDs are glyph indices in the standard Macintosh glyph
    /// ordering, so that a CID names a glyph and the glyph name names a
    /// character. See [`Type0Font::standard_order_unicode`] for when this is
    /// set and what it is for.
    standard_order_cids: bool,
}

impl Type0Font {
    pub(crate) fn new(
        dict: &Dict<'_>,
        font_resolver: &FontResolverFn,
        cmap_resolver: &CMapResolverFn,
        warning_sink: &WarningSinkFn,
    ) -> Option<Self> {
        let cmap = read_encoding(
            &dict.get::<Object<'_>>(ENCODING)?,
            cmap_resolver,
            warning_sink,
        )?;

        let horizontal = cmap.metadata().writing_mode != Some(WritingMode::Vertical);

        let descendant_font = dict
            .get::<Array<'_>>(DESCENDANT_FONTS)?
            .iter::<Dict<'_>>()
            .next()?;
        let font_descriptor = descendant_font
            .get::<Dict<'_>>(FONT_DESC)
            .unwrap_or_default();

        // Whether the file carries the font program at all. `fallback` below is
        // a wider condition -- it is also true when an embedded program failed
        // to parse -- and the difference matters for the standard-order mapping
        // further down: a font whose own program is present but broken still
        // has glyph indices of its own, not of the face its name claims.
        let embedded = font_descriptor.contains_key(FONT_FILE2)
            || font_descriptor.contains_key(FONT_FILE3)
            || font_descriptor.contains_key(FONT_FILE);

        let (font_type, fallback, _is_standard_fallback) = match FontType::new(&font_descriptor) {
            Some(ft) => (ft, false, false),
            None => {
                let (query, is_standard) =
                    if let Some((standard, _)) = select_standard_font(dict, &font_descriptor) {
                        (FontQuery::Standard(standard), true)
                    } else {
                        let mut query = FallbackFontQuery::new(dict);
                        query.character_collection = cmap.metadata().character_collection.clone();

                        warn!(
                            "unable to load CID font {} ({:?}), attempting fallback",
                            query.post_script_name.as_deref().unwrap_or("(no name)"),
                            dict.obj_id()
                        );
                        warning_sink(InterpreterWarning::UnsupportedFont);

                        (FontQuery::Fallback(query), false)
                    };

                let (data, index) = font_resolver(&query)?;
                let blob = OpenTypeFontBlob::new(data.clone(), index)
                    .map(FontType::OpenType)
                    .or_else(|| CffFontBlob::new(data).map(FontType::Cff))?;
                (blob, true, is_standard)
            }
        };

        let default_width = descendant_font.get::<f32>(DW).unwrap_or(1000.0);
        // PDF spec §9.7.4.3 (GL-QA40): default /DW2 when the entry is absent is
        // [880 -1000], where 880 is v_y (the y-coordinate of the position vector
        // origin) and -1000 is w1y (the vertical advance, negative = downward).
        let dw2 = descendant_font
            .get::<[f32; 2]>(DW2)
            .map(|v| (v[0], v[1]))
            .unwrap_or((880.0, -1000.0));

        let widths = descendant_font
            .get::<Array<'_>>(W)
            .and_then(|a| read_widths(&a))
            .unwrap_or_default();
        // PDF spec §9.7.4.3 (GL-QA40): /W2 entries are triples [w1y v_x v_y].
        // read_widths2 handles both the array form [c_first [w1y v_x v_y ...]]
        // and the range form [c_first c_last w1y v_x v_y], mapping each CID to
        // [w1y, v_x, v_y]. code_advance uses w1y; origin_displacement uses v_x/v_y.
        let widths2 = descendant_font
            .get::<Array<'_>>(W2)
            .and_then(|a| read_widths2(&a))
            .unwrap_or_default();
        let cid_to_gid_map = CidToGIdMap::new(&descendant_font, warning_sink).unwrap_or_default();
        let cache_key = dict.cache_key();

        let mut to_unicode = read_to_unicode(dict, cmap_resolver, warning_sink);
        let mut to_unicode_is_cid_indexed = false;

        // Which character collection the CIDs belong to. A predefined encoding
        // CMap names it; `Identity-H` and `Identity-V` name nothing, and then
        // the descendant's /CIDSystemInfo is the only place that says what the
        // CIDs are. noembed-identity-2.pdf is drawn through `Identity-H` from a
        // non-embedded `MS-PGothic` whose descendant declares Adobe-Japan1, so
        // its CIDs are Japan1 CIDs; reading only the encoding we called them
        // Adobe-Identity and emitted each one as a code point (#430).
        let descendant_collection = descendant_character_collection(&descendant_font);
        let character_collection = cmap
            .metadata()
            .character_collection
            .as_ref()
            .map(|cc| cc.family.clone())
            .filter(|family| *family != CidFamily::AdobeIdentity)
            .or_else(|| descendant_collection.clone());

        // If there is no explicit ToUnicode map, try the predefined UCS2
        // CMap for the descendant character collection. This works for both
        // embedded and fallback CID fonts.
        if to_unicode.is_none()
            && let Some(ucs2_name) = character_collection
                .as_ref()
                .and_then(|family| family.ucs2_cmap())
            && let Some(data) = (cmap_resolver)(ucs2_name)
        {
            let resolver = cmap_resolver.clone();
            if let Some(ucs2_cmap) = CMap::parse(data, move |n| (resolver)(n)) {
                to_unicode = Some(ucs2_cmap);
                to_unicode_is_cid_indexed = true;
            }
        }

        let postscript_name = dict
            .get::<Name>(BASE_FONT)
            .map(|n| strip_subset_prefix(n.as_str()).to_string());

        // Extract font flags from descriptor
        let font_flags = font_descriptor
            .get::<u32>(FLAGS)
            .and_then(FontFlags::from_bits);

        // #430. A non-embedded `CIDFontType2` with an `Identity` encoding and
        // no `ToUnicode` map hands us glyph indices into a font we do not have.
        // Asking the substitute font what glyph N means -- what
        // `unicode_from_font_program` does below -- reads the wrong font's
        // glyph order and yields a plausible wrong letter: issue6127.pdf's
        // non-embedded `TimesNewRoman` extracts as `eqpfkvkqpu` for
        // `conditions`. The producer wrote indices of the face it names, and
        // the Latin faces of that tradition begin with the 258 glyphs of the
        // standard Macintosh ordering, so the index names a glyph and the
        // glyph name names a character. MuPDF arrives at the same answer
        // through its substitute font, pdf.js through
        // `getGlyphMapForStandardFonts`, which was added for this very file.
        let standard_order_cids = to_unicode.is_none()
            && !embedded
            && descendant_font
                .get::<Name>(SUBTYPE)
                .is_some_and(|subtype| subtype.as_str() == "CIDFontType2")
            && descendant_collection
                .as_ref()
                .is_none_or(|family| *family == CidFamily::AdobeIdentity)
            && cmap
                .metadata()
                .character_collection
                .as_ref()
                .is_some_and(|cc| cc.family == CidFamily::AdobeIdentity)
            && postscript_name
                .as_deref()
                .is_some_and(face_uses_standard_glyph_order);

        Some(Self {
            cache_key,
            horizontal,
            encoding: cmap,
            to_unicode,
            font_type,
            dw: default_width,
            dw2,
            widths,
            widths2,
            cid_to_gid_map,
            postscript_name,
            font_flags,
            fallback,
            to_unicode_is_cid_indexed,
            standard_order_cids,
        })
    }

    pub(crate) fn map_code(&self, code: u32) -> GlyphId {
        let Some(cid) = self.code_to_cid(code) else {
            return GlyphId::NOTDEF;
        };

        // UCS2 maps are indexed by CIDs, while embededd `ToUnicode` maps are
        // indexed by character code.
        let to_unicode_key = if self.to_unicode_is_cid_indexed {
            cid
        } else {
            code
        };

        if self.fallback
            && let Some(glyph) = self.map_via_unicode(to_unicode_key)
        {
            // Yay, Unicode worked!
            return glyph;
        }

        // At this point, not much we can do anymore. Just hope that the
        // selected font has the right glyph order, and map via that.

        match &self.font_type {
            FontType::OpenType(_) => self.cid_to_gid_map.map(cid as u16),
            FontType::Cff(c) => {
                let table = c.table();

                if table.is_cid() {
                    // Very confusing stuff going on here, see https://github.com/mozilla/pdf.js/pull/15563.
                    // The PDF spec makes it sounds like cid-to-gid map should only be used for TrueType fonts,
                    // but Acrobat also seems to support it for CFF fonts with some weird behavior.
                    if matches!(self.cid_to_gid_map, CidToGIdMap::Identity) {
                        table
                            .glyph_index_by_cid(cid as u16)
                            .map(|g| GlyphId::new(g.0 as u32))
                            .unwrap_or(GlyphId::NOTDEF)
                    } else {
                        GlyphId::new(self.cid_to_gid_map.inverse_map(GlyphId::new(cid)) as u32)
                    }
                } else {
                    self.cid_to_gid_map.map(cid as u16)
                }
            }
            // Maybe we need similar processing to CFF fonts? But since
            // Type1 fonts are invalid anyway, let's just ignore for now.
            FontType::Type1(_) => GlyphId::new(cid),
        }
    }

    /// Map a character code (or CID) to a glyph ID
    /// by first getting its Unicode and then looking up the codepoint in the
    /// font's cmap.
    fn map_via_unicode(&self, key: u32) -> Option<GlyphId> {
        let to_unicode = self.to_unicode.as_ref()?;

        let character = to_unicode
            .lookup_bf_string(key)
            .or_else(|| {
                for len in 0..4 {
                    if let Some(code) = to_unicode.lookup_cid_code(key, len)
                        && let Some(code) = char::from_u32(code.to_u32())
                    {
                        return Some(BfString::Char(code));
                    }
                }

                None
            })
            .and_then(|bf| match bf {
                BfString::Char(c) => Some(c),
                BfString::String(_) => None,
            })?;

        match &self.font_type {
            FontType::OpenType(t) => t.font_ref().charmap().map(character),
            FontType::Cff(c) => {
                let table = c.table();

                // Map codepoint to glyph name via AFL, and then look it up.
                if let Some(name) = glyph_names::get_reverse(character)
                    && let Some(gid) = table.glyph_index_by_name(name)
                {
                    Some(GlyphId::new(gid.0 as u32))
                } else {
                    None
                }
            }
            FontType::Type1(t) => {
                let name = glyph_names::get_reverse(character)?;
                let idx = t.table().charstring_index(name)?;
                Some(GlyphId::new(idx as u32))
            }
        }
    }

    fn code_to_cid(&self, code: u32) -> Option<u32> {
        for byte_len in 1..=4_u8 {
            if let Some(cid) = self.encoding.lookup_cid_code(code, byte_len) {
                return Some(cid);
            }
        }

        None
    }

    pub(crate) fn outline_glyph(&self, glyph: GlyphId, code: u32) -> BezPath {
        let path = match &self.font_type {
            FontType::OpenType(t) => t.outline_glyph(glyph),
            FontType::Cff(c) => c.outline_glyph(glyph),
            FontType::Type1(t) => {
                let name = t
                    .table()
                    .charstring_names()
                    .get(glyph.to_u32() as usize)
                    .map(|n| n.as_str())
                    .unwrap_or(".notdef");
                t.outline_glyph(name)
            }
        };

        if self.fallback
            && let FontType::OpenType(t) = &self.font_type
            && let Some(cid) = self.code_to_cid(code)
            && let Some(actual_width) = t.glyph_metrics().advance_width(glyph)
            // Only use an expected width if there is an explicit /W array,
            // not the default width, as it leads to weird results from my testing.
            && let Some(expected_width) = self.widths.get(&cid).copied()
        {
            return stretch_glyph(path, expected_width, actual_width);
        }

        path
    }

    pub(crate) fn font_data(&self) -> Option<crate::font::FontData> {
        match &self.font_type {
            FontType::OpenType(t) => Some(t.font_data()),
            FontType::Cff(c) => Some(c.font_data()),
            FontType::Type1(_) => None,
        }
    }

    /// Get the PostScript name.
    pub(crate) fn postscript_name(&self) -> Option<&str> {
        self.postscript_name.as_deref()
    }

    /// Get the font weight (100-900, 400=normal, 700=bold).
    ///
    /// Returns `None` if weight cannot be determined (CFF fonts or invalid weight).
    pub(crate) fn weight(&self) -> Option<u32> {
        match &self.font_type {
            FontType::OpenType(t) => {
                let weight = t.font_ref().attributes().weight.value().round() as u32;
                if weight > 0 { Some(weight) } else { None }
            }
            FontType::Cff(_) | FontType::Type1(_) => None,
        }
    }

    /// Check if font is italic based on font flags or font attributes.
    pub(crate) fn is_italic(&self) -> bool {
        if let Some(flags) = &self.font_flags
            && flags.contains(FontFlags::ITALIC)
        {
            return true;
        }
        match &self.font_type {
            FontType::OpenType(t) => t.font_ref().attributes().style != Style::Normal,
            FontType::Cff(_) | FontType::Type1(_) => false,
        }
    }

    /// Check if font is serif based on font flags.
    pub(crate) fn is_serif(&self) -> bool {
        self.font_flags
            .as_ref()
            .is_some_and(|f| f.contains(FontFlags::SERIF))
    }

    /// Check if font is monospace based on font flags or font metrics.
    pub(crate) fn is_monospace(&self) -> bool {
        if let Some(flags) = &self.font_flags
            && flags.contains(FontFlags::FIXED_PITCH)
        {
            return true;
        }
        match &self.font_type {
            FontType::OpenType(t) => {
                t.font_ref()
                    .metrics(
                        skrifa::instance::Size::unscaled(),
                        skrifa::instance::LocationRef::default(),
                    )
                    .is_monospace
            }
            FontType::Cff(_) | FontType::Type1(_) => false,
        }
    }

    /// Vertical font metrics `(ascent, descent, cap_height, x_height)` scaled to
    /// /1000 em, from the embedded OpenType CIDFont's OS/2 / hhea tables. `None`
    /// for CFF and Type1 CIDFonts (no skrifa font reference).
    pub(crate) fn font_metrics(&self) -> Option<(f64, f64, Option<f64>, Option<f64>)> {
        match &self.font_type {
            FontType::OpenType(t) => {
                let m = t.font_ref().metrics(
                    skrifa::instance::Size::new(crate::font::UNITS_PER_EM),
                    skrifa::instance::LocationRef::default(),
                );
                Some((
                    m.ascent as f64,
                    m.descent as f64,
                    m.cap_height.map(|v| v as f64),
                    m.x_height.map(|v| v as f64),
                ))
            }
            FontType::Cff(_) | FontType::Type1(_) => None,
        }
    }

    pub(crate) fn code_advance(&self, code: u32) -> Vec2 {
        let cid = self.code_to_cid(code).unwrap_or(0);
        if self.horizontal {
            Vec2::new(self.horizontal_width(cid) as f64, 0.0)
        } else if let Some([w, _, _]) = self.widths2.get(&cid) {
            Vec2::new(0.0, *w as f64)
        } else {
            Vec2::new(0.0, self.dw2.1 as f64)
        }
    }

    fn horizontal_width(&self, cid: u32) -> f32 {
        self.widths.get(&cid).copied().unwrap_or(self.dw)
    }

    pub(crate) fn is_horizontal(&self) -> bool {
        self.horizontal
    }

    pub(crate) fn read_code(&self, bytes: &[u8], offset: usize) -> (u32, usize) {
        let mut code = 0_u32;
        let remaining = bytes.len() - offset;

        for n in 0..4.min(remaining) {
            code = (code << 8) | bytes[offset + n] as u32;

            if self.encoding.lookup_cid_code(code, (n + 1) as u8).is_some() {
                return (code, n + 1);
            }
        }

        (0, 1)
    }

    pub(crate) fn origin_displacement(&self, code: u32) -> Vec2 {
        let cid = self.code_to_cid(code).unwrap_or(0);

        if self.is_horizontal() {
            Vec2::default()
        } else if let Some([_, v1, v2]) = self.widths2.get(&cid) {
            Vec2::new(-*v1 as f64, -*v2 as f64)
        } else {
            Vec2::new(-self.horizontal_width(cid) as f64 / 2.0, -self.dw2.0 as f64)
        }
    }

    pub(crate) fn char_code_to_unicode(&self, code: u32) -> Option<BfString> {
        if let Some(to_unicode) = &self.to_unicode {
            let key = if self.to_unicode_is_cid_indexed {
                match self.code_to_cid(code) {
                    Some(cid) => cid,
                    None => return self.unicode_from_font_program(code),
                }
            } else {
                code
            };

            if let Some(mapped) = to_unicode.lookup_bf_string(key) {
                return Some(mapped);
            }
        }

        self.unicode_from_font_program(code)
    }

    fn unicode_from_font_program(&self, code: u32) -> Option<BfString> {
        if let Some(c) = self.standard_order_unicode(code) {
            return Some(BfString::Char(c));
        }

        let glyph = self.map_code(code);
        if glyph == GlyphId::NOTDEF {
            return self.identity_unicode_fallback(code).map(BfString::Char);
        }

        match &self.font_type {
            // For an Identity-H CIDFontType2 font whose embedded TrueType has
            // no `cmap` table (common in subset fonts) or whose reverse-cmap
            // has no entry for the resolved glyph, fall back to treating the
            // character code as a Unicode code point. Matches PDFBox/Poppler
            // behaviour for Identity-H fonts lacking a usable ToUnicode map.
            //
            // When that guess has nothing to say either, the embedded
            // program's `post` table names the glyph, and the name is read
            // through the Adobe Glyph List -- the last step that speaks
            // about this glyph rather than about its code (#636).
            FontType::OpenType(t) => t
                .glyph_id_to_unicode(glyph)
                .or_else(|| self.identity_unicode_fallback(code))
                .map(BfString::Char)
                .or_else(|| {
                    if self.fallback {
                        return None;
                    }
                    post_glyph_name_unicode(t, glyph).map(BfString::String)
                }),
            FontType::Cff(c) => {
                let table = c.table();

                if table.is_cid() {
                    // CID CFF fonts have no glyph names — fall back to treating
                    // the CID as a Unicode code point when the encoding is
                    // Identity-H/V (common for modern PDF generators).
                    self.identity_unicode_fallback(code).map(BfString::Char)
                } else {
                    // `glyph_name_to_string` recognises underscore-joined
                    // ligature names (e.g. `f_i`, `f_f_i`) that the single-char
                    // path drops, which shows up in Latin text from CFF fonts
                    // without a ToUnicode map.
                    table
                        .glyph_name(pdf_font::GlyphId(glyph.to_u32() as u16))
                        .and_then(glyph_name_to_string)
                        .map(BfString::String)
                }
            }
            FontType::Type1(t) => t
                .table()
                .charstring_names()
                .get(glyph.to_u32() as usize)
                .map(|n| n.as_str())
                .and_then(glyph_name_to_string)
                .map(BfString::String),
        }
    }

    /// The character a CID names when the CIDs are glyph indices of a Latin
    /// face that follows the standard Macintosh glyph ordering (#430).
    ///
    /// Returns `None` unless the font qualified in [`Type0Font::new`], and
    /// `None` for the three slots of that ordering that name no character
    /// (`.notdef`, `.null`, `nonmarkingreturn`) and for any index past its 258.
    /// The caller then falls through to the substitute font as before.
    ///
    /// What the ordering speaks about is a glyph index, and the CID is that
    /// index only through /CIDToGIDMap. Every document of this class seen so
    /// far leaves the map Identity, where the two are the same number; a file
    /// that carries a map anyway is followed rather than read past, and a CID
    /// its map does not name resolves to `.notdef` and so to nothing.
    fn standard_order_unicode(&self, code: u32) -> Option<char> {
        if !self.standard_order_cids {
            return None;
        }
        let cid = u16::try_from(self.code_to_cid(code)?).ok()?;
        let glyph = self.cid_to_gid_map.map(cid).to_u32();
        let name = post::DEFAULT_GLYPH_NAMES.get(glyph as usize)?;
        glyph_name_to_unicode(name)
    }

    /// For Identity-H/V encoded fonts (AdobeIdentity), the character code
    /// maps directly to CID, and the CID is typically the Unicode code point.
    /// This is extremely common for CJK fonts and modern PDF generators.
    ///
    /// A CID that reads as a control character is not text (#636): subset
    /// fonts number their glyphs from 1, so such a CID reaching here means
    /// the code says nothing and the embedded program's `post` name -- the
    /// next step of the chain -- is what speaks about the glyph. Decline so
    /// the caller falls through to it instead of emitting garbage that would
    /// shadow the name.
    fn identity_unicode_fallback(&self, code: u32) -> Option<char> {
        let is_identity = self
            .encoding
            .metadata()
            .character_collection
            .as_ref()
            .is_some_and(|cc| cc.family == CidFamily::AdobeIdentity);
        if !is_identity {
            return None;
        }
        let cid = self.code_to_cid(code).unwrap_or(code);
        let c = char::from_u32(cid)?;
        (!c.is_control()).then_some(c)
    }
}

/// The name the embedded program's `post` table gives `glyph`, read through
/// the Adobe Glyph List (#636).
///
/// The caller reaches here only when the reverse `cmap` and the Identity
/// guess both missed, and only for an embedded program: a substitute font's
/// names describe its own glyph order, not the page's, so consulting them
/// would trade a missing character for a plausible wrong one (#430).
fn post_glyph_name_unicode(blob: &OpenTypeFontBlob, glyph: GlyphId) -> Option<String> {
    let id = u16::try_from(glyph.to_u32()).ok()?;
    let post = blob.font_ref().post().ok()?;
    let name = post.glyph_name(GlyphId16::new(id))?;
    glyph_name_to_string(name)
}

impl CacheKey for Type0Font {
    fn cache_key(&self) -> u128 {
        self.cache_key
    }
}

#[derive(Debug)]
enum FontType {
    /// An OpenType font.
    OpenType(OpenTypeFontBlob),
    /// A CFF font.
    Cff(CffFontBlob),
    /// A Type1 font. (Yes, there actually exist PDFs that embed a Type1 font in a
    /// CID font :))
    Type1(Type1FontBlob),
}

impl FontType {
    fn new(descriptor: &Dict<'_>) -> Option<Self> {
        // PDF has a distinction between CIDFontType0 and CIDFontType 2, with
        // specific requirements what font type can appear where. However, some
        // PDFs use wrong metadata, so we simply bruteforce without even looking
        // at the metadata.

        let data = descriptor
            .get::<Stream<'_>>(FONT_FILE2)
            .or_else(|| descriptor.get::<Stream<'_>>(FONT_FILE3))
            // See PDFBOX-2599. Apparently, Acrobat accepts this as well.
            .or_else(|| descriptor.get::<Stream<'_>>(FONT_FILE))?
            .decoded()
            .ok()?;

        let data = Arc::new(data);

        let parsed = if let Ok(_font_ref) = FontRef::from_index(data.as_ref(), 0) {
            // It's an OpenType font, either TrueType or CFF.
            Self::OpenType(OpenTypeFontBlob::new(data, 0)?)
        } else if let Some(cff) = CffFontBlob::new(data.clone()) {
            // It's a CFF font.
            Self::Cff(cff)
        } else if let Some(t1) = Type1FontBlob::new(data) {
            // It's a Type1 (PFB) font.
            Self::Type1(t1)
        } else {
            return None;
        };

        Some(parsed)
    }
}

/// The registry and ordering the descendant CIDFont declares, if it declares
/// any. With a predefined encoding CMap this repeats what the CMap already
/// says; with `Identity-H`/`Identity-V` it is the only statement about which
/// character collection the CIDs are drawn from (#430).
fn descendant_character_collection(descendant: &Dict<'_>) -> Option<CidFamily> {
    let info = descendant.get::<Dict<'_>>(CIDSYSTEMINFO)?;
    let registry = info.get::<PdfString>(REGISTRY)?;
    let ordering = info.get::<PdfString>(ORDERING)?;
    Some(CidFamily::from_registry_ordering(
        registry.as_bytes(),
        ordering.as_bytes(),
    ))
}

/// The Latin faces whose glyph indices follow the standard Macintosh ordering,
/// normalised by [`normalised_face_name`].
///
/// Measured rather than assumed: over the copies of these faces shipped with
/// macOS 15, indices 3 to 142 are the standard ordering glyph for glyph in
/// every one of them, and the first disagreement after that is a spelling of
/// the same character (`mu1` for `mu`, `uni00B5` for `mu`). Indices 3 to 142
/// carry ASCII and the Latin-1 letters, which is the text this exists to
/// recover. Arial Unicode is deliberately absent: it diverges at index 98, and
/// its name normalises to `arialunicodems`, which is not in this table.
///
/// A face this table does not name keeps the old behaviour. That is the point
/// of a table rather than a heuristic: the ordering is a property of a known
/// face, and a face we cannot name is as likely to be a symbol font, where a
/// mapping would replace one wrong answer with another that reads like text.
/// The /Flags symbolic bit cannot do this job -- Word marks exactly these
/// fonts symbolic (issue6127.pdf's non-embedded `TimesNewRoman` has Flags 6),
/// so gating on it would refuse the case this exists for.
const STANDARD_GLYPH_ORDER_FACES: &[&str] = &[
    "arial",
    "arialblack",
    "arialnarrow",
    "comicsansms",
    "courier",
    "couriernew",
    "georgia",
    "helvetica",
    "impact",
    "tahoma",
    "times",
    "timesnewroman",
    "timesroman",
    "trebuchetms",
    "verdana",
];

/// Whether `base_font` names a face from [`STANDARD_GLYPH_ORDER_FACES`].
fn face_uses_standard_glyph_order(base_font: &str) -> bool {
    STANDARD_GLYPH_ORDER_FACES.contains(&normalised_face_name(base_font).as_str())
}

/// A /BaseFont value reduced to its family: subset prefix, style suffix and
/// foundry suffix removed, punctuation dropped, lowercased.
///
/// One face reaches us under many names -- `ABCDEF+TimesNewRoman,BoldItalic`,
/// `TimesNewRomanPS-BoldItalicMT`, `TimesNewRomanPSMT` -- and the table above
/// holds families, not the cross product of families and styles.
fn normalised_face_name(base_font: &str) -> String {
    /// Longest first, so that `psmt` is not left as `ps` by an earlier match.
    ///
    /// `ms` is deliberately not here, even though `Comic Sans MS` and
    /// `Trebuchet MS` wear it: `Times` ends in those two letters as part of its
    /// name, and stripping them would leave `time`, which is not a face
    /// anybody has. The two faces that carry the foundry in their name are in
    /// the table with it.
    const SUFFIXES: &[&str] = &[
        "psmt",
        "bolditalic",
        "boldoblique",
        "italic",
        "oblique",
        "regular",
        "bold",
        "mt",
        "ps",
    ];

    let family = strip_subset_prefix(base_font);
    let family = family.split(',').next().unwrap_or(family);
    let mut name: String = family
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();

    while let Some(shorter) = SUFFIXES
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .filter(|shorter| !shorter.is_empty())
    {
        name = shorter.to_string();
    }
    name
}

#[derive(Debug, Default)]
enum CidToGIdMap {
    #[default]
    Identity,
    Mapped {
        forward: HashMap<u16, GlyphId>,
        inverse: HashMap<GlyphId, u16>,
    },
}

impl CidToGIdMap {
    fn new(dict: &Dict<'_>, warning_sink: &WarningSinkFn) -> Option<Self> {
        if let Some(name) = dict.get::<Name>(CID_TO_GID_MAP) {
            if name.deref() == IDENTITY {
                Some(Self::Identity)
            } else {
                None
            }
        } else if let Some(stream) = dict.get::<Stream<'_>>(CID_TO_GID_MAP) {
            let decoded = decode_or_warn(&stream, warning_sink)?;
            let mut forward = HashMap::new();
            let mut inverse = HashMap::new();

            for (cid, gid) in decoded.chunks_exact(2).enumerate() {
                let gid = GlyphId::new(u16::from_be_bytes([gid[0], gid[1]]) as u32);

                forward.insert(cid as u16, gid);
                inverse.insert(gid, cid as u16);
            }

            Some(Self::Mapped { forward, inverse })
        } else {
            None
        }
    }

    fn map(&self, code: u16) -> GlyphId {
        match self {
            Self::Identity => GlyphId::new(code as u32),
            Self::Mapped { forward, .. } => forward.get(&code).copied().unwrap_or(GlyphId::NOTDEF),
        }
    }

    fn inverse_map(&self, gid: GlyphId) -> u16 {
        match self {
            Self::Identity => gid.to_u32() as u16,
            Self::Mapped { inverse, .. } => {
                inverse.get(&gid).copied().unwrap_or(gid.to_u32() as u16)
            }
        }
    }
}

fn read_widths(arr: &Array<'_>) -> Option<HashMap<u32, f32>> {
    let mut map = HashMap::new();
    let mut iter = arr.flex_iter();

    loop {
        if let Some((mut first, range)) = iter.next::<(u32, Array<'_>)>() {
            for width in range.iter::<f32>() {
                map.insert(first, width);
                first = first.checked_add(1)?;
            }
        } else if let Some((first, second, width)) = iter.next::<(u32, u32, f32)>() {
            for i in first..=second {
                map.insert(i, width);
            }
        } else {
            break;
        }
    }

    Some(map)
}

fn read_widths2(arr: &Array<'_>) -> Option<HashMap<u32, [f32; 3]>> {
    let mut map = HashMap::new();
    let mut iter = arr.flex_iter();

    loop {
        if let Some((mut first, range)) = iter.next::<(u32, Array<'_>)>() {
            let mut iter = range.iter::<f32>();

            while let Some(w) = iter.next() {
                let v1 = iter.next()?;
                let v2 = iter.next()?;
                map.insert(first, [w, v1, v2]);
                first = first.checked_add(1)?;
            }
        } else if let Some((first, second, w, v1, v2)) = iter.next::<(u32, u32, f32, f32, f32)>() {
            for i in first..=second {
                map.insert(i, [w, v1, v2]);
            }
        } else {
            break;
        }
    }

    Some(map)
}

fn read_encoding(
    object: &Object<'_>,
    cmap_resolver: &CMapResolverFn,
    warning_sink: &WarningSinkFn,
) -> Option<CMap> {
    // TODO: Support fetching CMaps referenced via `usecmap` in the PDF.
    match object {
        Object::Name(n) => {
            let cmap_type = pdf_font::cmap::CMapName::from_bytes(n.deref());
            match cmap_type {
                pdf_font::cmap::CMapName::IdentityH => Some(CMap::identity_h()),
                pdf_font::cmap::CMapName::IdentityV => Some(CMap::identity_v()),
                _ => {
                    let data = (cmap_resolver)(cmap_type)?;
                    let resolver = cmap_resolver.clone();
                    CMap::parse(data, move |n| (resolver)(n))
                }
            }
        }
        Object::Stream(s) => {
            let decoded = decode_or_warn(s, warning_sink)?;
            let resolver = cmap_resolver.clone();
            CMap::parse(&decoded, move |n| (resolver)(n))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_syntax::object::FromBytes;

    fn parse_widths(bytes: &[u8]) -> HashMap<u32, f32> {
        let arr = Array::from_bytes(bytes).expect("valid array bytes");
        read_widths(&arr).expect("read_widths returned None")
    }

    /// /W [40 60 600] — range form: glyphs 40..=60 all get width 600
    #[test]
    fn test_w_range_form() {
        let map = parse_widths(b"[40 60 600]");
        assert_eq!(map.len(), 21); // 60 - 40 + 1
        for cid in 40..=60 {
            assert_eq!(
                map.get(&cid).copied(),
                Some(600.0),
                "cid {cid} should have width 600"
            );
        }
    }

    /// /W [40 [600 700 800]] — array form: glyph 40=600, 41=700, 42=800
    #[test]
    fn test_w_array_form() {
        let map = parse_widths(b"[40 [600 700 800]]");
        assert_eq!(map.len(), 3);
        assert_eq!(map.get(&40).copied(), Some(600.0));
        assert_eq!(map.get(&41).copied(), Some(700.0));
        assert_eq!(map.get(&42).copied(), Some(800.0));
    }

    /// /DW fallback: glyph not in /W should return dw
    #[test]
    fn test_dw_fallback() {
        // read_widths itself doesn't know about /DW; the fallback is in
        // horizontal_width(). Verify it directly: a map that has no entry
        // for cid 99 should return dw when get() returns None.
        let map = parse_widths(b"[40 [600]]");
        // cid 99 is absent → simulate what horizontal_width() does
        let dw = 1000.0_f32;
        let width = map.get(&99).copied().unwrap_or(dw);
        assert_eq!(width, dw, "missing cid should fall back to /DW");
    }

    /// /W [40 [600 600] 100 200 500] — mixed: array form followed by range form
    #[test]
    fn test_w_mixed_form() {
        let map = parse_widths(b"[40 [600 600] 100 200 500]");
        // array form: cid 40=600, 41=600
        assert_eq!(map.get(&40).copied(), Some(600.0));
        assert_eq!(map.get(&41).copied(), Some(600.0));
        // range form: cids 100..=200 = 500
        for cid in 100..=200 {
            assert_eq!(
                map.get(&cid).copied(),
                Some(500.0),
                "cid {cid} should have width 500"
            );
        }
    }

    /// A single-glyph range form (/W [50 50 250]) is valid per the spec.
    #[test]
    fn test_w_range_single_glyph() {
        let map = parse_widths(b"[50 50 250]");
        assert_eq!(map.get(&50).copied(), Some(250.0));
        assert_eq!(map.len(), 1);
    }

    /// One face reaches us under many names; the table of faces that follow the
    /// standard Macintosh glyph ordering holds families, so every name of a
    /// family has to reduce to the same one (#430).
    #[test]
    fn face_names_reduce_to_their_family() {
        for name in [
            "TimesNewRoman",
            "ABCDEF+TimesNewRoman",
            "TimesNewRoman,Italic",
            "TimesNewRoman,BoldItalic",
            "TimesNewRomanPSMT",
            "TimesNewRomanPS-BoldItalicMT",
        ] {
            assert_eq!(normalised_face_name(name), "timesnewroman", "{name}");
        }
        assert_eq!(normalised_face_name("ArialMT"), "arial");
        assert_eq!(normalised_face_name("Arial-BoldMT"), "arial");
        assert_eq!(normalised_face_name("ArialNarrow-Bold"), "arialnarrow");
        assert_eq!(normalised_face_name("CourierNewPS-BoldMT"), "couriernew");
        // The two the foundry suffix cannot simply be stripped from: `Times`
        // ends in the letters of `MS`, and reducing it to `time` would drop it
        // out of the table and out of the fix.
        assert_eq!(normalised_face_name("Times"), "times");
        assert_eq!(normalised_face_name("Times-Bold"), "times");
        assert_eq!(normalised_face_name("Times-Roman"), "timesroman");
        for name in ["Comic Sans MS", "ComicSansMS"] {
            assert_eq!(normalised_face_name(name), "comicsansms", "{name}");
        }
        assert_eq!(normalised_face_name("TrebuchetMS-Italic"), "trebuchetms");
    }

    /// The guard on the standard-order mapping. A face outside the table keeps
    /// the old behaviour, and Arial Unicode is the reason the table is a list
    /// of faces rather than a prefix match on `Arial`: its glyph order leaves
    /// the standard ordering at index 98.
    #[test]
    fn only_the_named_faces_use_the_standard_glyph_order() {
        for name in [
            "TimesNewRoman,Italic",
            "ArialMT",
            "CourierNewPS-BoldMT",
            "Helvetica",
            "Times",
            "Times-Bold",
            "Comic Sans MS",
        ] {
            assert!(face_uses_standard_glyph_order(name), "{name}");
        }
        for name in [
            "ArialUnicodeMS",
            "Wingdings",
            "Symbol",
            "MS-PGothic",
            "NoSuchFaceAnywhere",
        ] {
            assert!(!face_uses_standard_glyph_order(name), "{name}");
        }
    }

    /// What a CID means once the face is known: the standard Macintosh
    /// ordering starts at `space` on index 3, so printable ASCII sits 29 above
    /// its code point, and the three slots that name no character map to
    /// nothing rather than to a plausible letter.
    #[test]
    fn the_standard_ordering_names_characters() {
        for (index, expected) in [(3usize, ' '), (36, 'A'), (68, 'a'), (71, 'd'), (86, 's')] {
            let name = post::DEFAULT_GLYPH_NAMES[index];
            assert_eq!(glyph_name_to_unicode(name), Some(expected), "index {index}");
        }
        for index in [0usize, 1, 2] {
            let name = post::DEFAULT_GLYPH_NAMES[index];
            assert_eq!(glyph_name_to_unicode(name), None, "index {index}");
        }
    }

    /// Empty /W array should produce an empty map (not an error).
    #[test]
    fn test_w_empty_array() {
        let map = parse_widths(b"[]");
        assert!(map.is_empty());
    }
}
