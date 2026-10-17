//! Interacting with the different kinds of PDF fonts.

use crate::cache::Cache;
use crate::context::Context;
use crate::device::Device;
use crate::font::cid::Type0Font;
use crate::font::generated::{
    ams_glyph_names, glyph_names, mac_expert, mac_os_roman, mac_roman, standard, win_ansi,
};
use crate::font::true_type::TrueTypeFont;
use crate::font::type1::Type1Font;
use crate::font::type3::Type3;
use crate::interpret::state::State;
use crate::util::decode_or_warn;
use crate::{CMapResolverFn, CacheKey, FontResolverFn, InterpreterSettings, Paint, WarningSinkFn};
use bitflags::bitflags;
use kurbo::{Affine, BezPath, Vec2};
use log::warn;
use outline::OutlineFont;
use pdf_syntax::object::Name;
use pdf_syntax::object::dict::keys::SUBTYPE;
use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Dict, Stream};
use pdf_syntax::page::Resources;
use pdf_syntax::xref::XRef;
use skrifa::GlyphId;
use std::fmt::Debug;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

mod blob;
mod cid;
mod generated;
mod glyph_simulator;
mod marks;
pub(crate) mod outline;
mod standard_font;
mod true_type;
mod type1;
pub(crate) mod type3;

pub(crate) const UNITS_PER_EM: f32 = 1000.0;

pub(crate) fn stretch_glyph(path: BezPath, expected_width: f32, actual_width: f32) -> BezPath {
    if actual_width != 0.0 && actual_width != expected_width {
        let stretch_factor = expected_width / actual_width;
        Affine::scale_non_uniform(stretch_factor as f64, 1.0) * path
    } else {
        path
    }
}

/// A container for the bytes of a PDF file.
pub type FontData = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// Strip the 6-character subset prefix from a PostScript font name.
///
/// PDF subset fonts use names like "ABCDEF+TimesNewRoman". This function
/// returns `TimesNewRoman` from such a name, or the original name if no
/// valid prefix is found.
pub(crate) fn strip_subset_prefix(name: &str) -> &str {
    match name.split_once('+') {
        Some((prefix, rest)) if prefix.len() == 6 => rest,
        _ => name,
    }
}

use crate::util::hash128;
pub(crate) use marks::{MarkPlacement, NamedEntry, table_entry};
use marks::{
    MarkSite, is_combining_mark, mark_rows_set_aside, may_be_row_mark, placements_held,
    row_mark_of, without_mark_rows,
};
pub use outline::OutlineFontData;
use pdf_font::cmap::{BfString, CMap, CMapName, CharacterCollection};
pub use standard_font::StandardFont;

/// A glyph that can be drawn.
pub enum Glyph<'a> {
    /// A glyph defined by an outline.
    Outline(OutlineGlyph),
    /// A type3 glyph, defined by PDF drawing instructions.
    Type3(Box<Type3Glyph<'a>>),
}

impl Glyph<'_> {
    /// Returns the Unicode code point for this glyph, if available.
    ///
    /// This method attempts to determine the Unicode character that this glyph
    /// represents. The exact fallback chain depends on the font type:
    ///
    /// **For Outline Fonts (Type1, TrueType, CFF):**
    /// 1. `ToUnicode` cmap
    /// 2. Glyph name → Unicode (via Adobe Glyph List)
    /// 3. Unicode naming conventions (e.g., "uni0041", "u0041")
    ///
    /// **For CID Fonts (Type0):**
    /// 1. `ToUnicode` cmap
    /// 2. Font program fallback (`cmap`, `post`/charset/CharStrings glyph
    ///    names via the Adobe Glyph List, Identity CID-as-Unicode)
    ///
    ///
    /// **For Type3 Fonts:**
    /// 1. `ToUnicode` cmap
    ///
    /// Returns `None` if the Unicode value could not be determined.
    ///
    /// Please note that this method is still somewhat experimental and might
    /// not work reliably in all cases.
    pub fn as_unicode(&self) -> Option<BfString> {
        match self {
            Glyph::Outline(g) => g.as_unicode(),
            Glyph::Type3(g) => g.as_unicode(),
        }
    }

    /// The resource name the glyph's font was set from, if the `Tf` handler
    /// stamped one (see [`OutlineGlyph::font_resource_name`]).
    pub fn font_resource_name(&self) -> Option<&str> {
        match self {
            Glyph::Outline(g) => g.font_resource_name(),
            Glyph::Type3(_) => None,
        }
    }
}

/// An identifier that uniquely identifies a glyph, for caching purposes.
#[derive(Clone, Debug)]
pub struct GlyphIdentifier {
    id: GlyphId,
    font: OutlineFont,
}

impl CacheKey for GlyphIdentifier {
    fn cache_key(&self) -> u128 {
        hash128(&(self.id, self.font.cache_key()))
    }
}

/// A glyph defined by an outline.
#[derive(Debug)]
pub struct OutlineGlyph {
    pub(crate) id: GlyphId,
    pub(crate) font: OutlineFont,
    pub(crate) char_code: u32,
    pub(crate) resource_name: Option<Rc<str>>,
    /// The glyph's place in the page's record of combining marks (spec
    /// `specs/text/circlecopyrt.md`, C4 and C5a): `None` for a glyph built
    /// outside a page interpretation.
    pub(crate) marks: Option<MarkSite>,
    /// The code's text as the font gives it with the mark rows standing,
    /// read once when the glyph is made: the interpreter and the device each
    /// read a glyph several times, and the font's answer for one code does
    /// not change. A plain value, no shared state.
    pub(crate) text: Option<BfString>,
}

impl Clone for OutlineGlyph {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            font: self.font.clone(),
            char_code: self.char_code,
            resource_name: self.resource_name.clone(),
            marks: self.marks,
            text: self.text.clone(),
        }
    }
}

/// Where a glyph stands among the glyphs of the page interpretation that
/// drew it: a plain value a device keeps in place of the glyph, to hand to
/// [`OutlineGlyph::reread_previous`] when the next glyph arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphPlace(MarkSite);

impl OutlineGlyph {
    /// Where this glyph stands among the glyphs of its page interpretation;
    /// `None` for a glyph made outside one.
    pub fn place(&self) -> Option<GlyphPlace> {
        self.marks.map(GlyphPlace)
    }

    /// What the glyph drawn directly before this one, at `previous`, reads
    /// as now, when drawing this glyph changed it; `None` when it did not.
    ///
    /// Drawing a combining mark from an AMS name row next to a glyph offers
    /// the mark to that glyph (see [`OutlineGlyph::as_unicode`]), so the
    /// glyph drawn before can read differently once this one is drawn: a
    /// `c` drawn before its circle reads `c⃝`, a slash drawn before the `=`
    /// it strikes reads as nothing. This read is what takes the offer, as
    /// reading that glyph with `as_unicode` would; a caller that does not
    /// make it leaves the offer to be refused. On a page where nothing was
    /// offered it answers `None` at the cost of one load.
    ///
    /// Call it before reading this glyph itself (`as_unicode` and the
    /// like): reading this glyph first decides the offer from this side, and
    /// the glyph before then reads as it did. A `previous` from another page
    /// interpretation, or not directly before this glyph, answers `None`.
    pub fn reread_previous(&self, previous: GlyphPlace) -> Option<Option<BfString>> {
        self.marks?.reread_previous(previous.0)
    }

    /// The resource name the glyph's font was set from (`Tf`'s operand), if
    /// the `Tf` handler stamped one: the name extraction reads back to tell
    /// the engine's own OCR layer apart from author text set in the same face.
    pub fn font_resource_name(&self) -> Option<&str> {
        self.resource_name.as_deref()
    }

    /// Return the outline of the glyph, assuming an upem value of 1000.
    pub fn outline(&self) -> BezPath {
        self.font.outline_glyph(self.id, self.char_code)
    }

    /// Return the identifier of the glyph. You can use this to calculate the cache key
    /// for the glyph.
    ///
    /// Note that the `glyph_transform` attribute is not considered in the cache key of
    /// the identifier, only the glyph ID and the font.
    pub fn identifier(&self) -> GlyphIdentifier {
        GlyphIdentifier {
            id: self.id,
            font: self.font.clone(),
        }
    }

    /// Returns the Unicode code point for this glyph, if available.
    ///
    /// See [`Glyph::as_unicode`] for details on the fallback chain used.
    ///
    /// A combining mark that an AMS name row gives (`circlecopyrt`,
    /// `negationslash`) is never handed out on its own glyph. Where it is
    /// placed, the glyph it marks reads as itself followed by the mark and the
    /// mark's glyph reads as nothing; otherwise the mark's glyph reads as its
    /// code did before the row existed (see `marks.rs`). A placement is
    /// decided by which of the two glyphs is read first once it is offered,
    /// so the glyph drawn before a mark can read differently when read again
    /// after the mark is drawn.
    pub fn as_unicode(&self) -> Option<BfString> {
        // A glyph whose text cannot be a row's mark, read while no placement
        // exists anywhere, reads as the font gives it: the common case costs
        // two tests (spec, "Performance").
        if !may_be_row_mark(&self.text) && !placements_held() {
            return self.text.clone();
        }
        self.placed_text()
    }

    /// [`Self::as_unicode`] where a row's mark or a placement may be
    /// involved.
    #[inline(never)]
    fn placed_text(&self) -> Option<BfString> {
        let text = self.cached_text();
        if row_mark_of(text, || self.font_text()).is_some() {
            if self.marks.as_ref().is_some_and(MarkSite::mark_taken) {
                return None;
            }
            return without_mark_rows(|| self.font_text());
        }
        let text = text.clone();
        let marks = self.marks.as_ref().map(MarkSite::base_marks);
        match marks.filter(|marks| !marks.is_empty()) {
            Some(marks) => {
                let mut placed = match text {
                    Some(BfString::Char(c)) => c.to_string(),
                    Some(BfString::String(s)) => s,
                    None => String::new(),
                };
                placed.push_str(&marks);
                Some(BfString::String(placed))
            }
            None => text,
        }
    }

    /// The text the font gives this glyph's code, before any placement: a
    /// row-derived combining mark is its bare character here, and whether a
    /// row gave it is read beside the text (`marks::row_mark_of`).
    ///
    /// Borrowed from the glyph's one read of its code, made with the glyph.
    /// A read that sets the mark rows aside asks [`Self::font_text`].
    pub(crate) fn cached_text(&self) -> &Option<BfString> {
        &self.text
    }

    /// What the font gives this glyph's code now, as the mark rows stand.
    pub(crate) fn font_text(&self) -> Option<BfString> {
        self.font.char_code_to_unicode(self.char_code)
    }

    /// Get raw font bytes and metadata for downstream use.
    ///
    /// Returns `None` for Type1 fonts.
    pub fn font_data(&self) -> Option<OutlineFontData> {
        self.font.font_data()
    }

    /// PostScript name of the underlying font, if determinable.
    ///
    /// Works even when [`font_data`](Self::font_data) returns
    /// `None` — e.g. non-embedded standard-14 Type1 fonts still
    /// expose their canonical PostScript name through this
    /// accessor. Returns `None` when no name source is available.
    ///
    /// Surfaced to support the WASM `getTextPositions()` `fontName`
    /// field for the editor contract.
    pub fn postscript_name(&self) -> Option<String> {
        self.font.postscript_name()
    }

    /// The font dictionary's /BaseFont as the page named it, subset prefix
    /// and all, where the font carries one (#623). [`Self::postscript_name`]
    /// names the face that draws the glyphs, which for a font standing on a
    /// standard face is that face, not the page's.
    pub fn declared_name(&self) -> Option<String> {
        self.font.declared_name()
    }

    /// The name text extraction reports for a span: the document's own name
    /// for an embedded font, the substitute face otherwise. Subset prefix
    /// and all, like [`Self::declared_name`]; callers strip.
    pub fn document_name(&self) -> Option<String> {
        self.font.document_name()
    }

    /// Get the glyph ID within the font.
    pub fn glyph_id(&self) -> GlyphId {
        self.id
    }

    /// Get the advance width for this glyph.
    ///
    /// The advance width is how far to move horizontally after drawing
    /// this glyph before drawing the next one.
    pub fn advance_width(&self) -> Option<f32> {
        self.font.glyph_advance_width(self.char_code)
    }

    /// Vertical font metrics in /1000 em.
    ///
    /// Returns `(ascent, descent, cap_height, x_height)` from the font's
    /// embedded OS/2 tables (via skrifa) when available, or from the
    /// Standard-14 AFM fallback table for non-embedded standard fonts.
    /// Returns `None` only when no metrics source is reachable (e.g.
    /// CFF fonts without a standard-font fallback, Type3).
    pub fn font_metrics(&self) -> Option<(f64, f64, Option<f64>, Option<f64>)> {
        self.font.font_metrics()
    }

    /// The substitute face's AFM pair for Type1/CFF, ignoring the
    /// descriptor: the pre-E4 answer, kept for the default extraction path
    /// (#643 follow-up).
    pub fn substitute_metrics(&self) -> Option<(f64, f64, Option<f64>, Option<f64>)> {
        self.font.substitute_metrics()
    }

    /// Get the cache key for this glyph's font.
    ///
    /// This identifies the font uniquely, even when `font_data()` returns `None`
    /// (e.g., for Type1 fonts). Useful for grouping glyphs by font.
    pub fn font_cache_key(&self) -> u128 {
        self.font.cache_key()
    }
}

/// A type3 glyph.
#[derive(Clone)]
pub struct Type3Glyph<'a> {
    pub(crate) font: Rc<Type3<'a>>,
    pub(crate) glyph_id: GlyphId,
    pub(crate) state: State<'a>,
    pub(crate) parent_resources: Resources<'a>,
    pub(crate) cache: Cache,
    pub(crate) xref: &'a XRef,
    pub(crate) settings: InterpreterSettings,
    pub(crate) char_code: u32,
    /// The depth the interpretation sat at when this glyph was reached (#318).
    /// A Type 3 glyph is a content stream, so it can name a pattern whose cell
    /// draws text in this same font — a fresh Context starting at zero would
    /// never reach the bound.
    pub(crate) nesting_depth: u32,
}

/// A glyph defined by PDF drawing instructions.
impl<'a> Type3Glyph<'a> {
    /// Draw the type3 glyph to the given device.
    pub fn interpret(
        &self,
        device: &mut impl Device<'a>,
        transform: Affine,
        glyph_transform: Affine,
        paint: &Paint<'a>,
    ) {
        self.font
            .render_glyph(self, transform, glyph_transform, paint, device);
    }

    /// Returns the Unicode code point for this glyph, if available.
    ///
    /// Note: Type3 fonts can only provide Unicode via `ToUnicode` cmap.
    ///
    /// A combining mark that an AMS name row gives is not placed on a Type 3
    /// glyph's neighbour: it reads as its code did before the row existed.
    pub fn as_unicode(&self) -> Option<BfString> {
        let text = self.font.char_code_to_unicode(self.char_code);
        if row_mark_of(&text, || self.font.char_code_to_unicode(self.char_code)).is_some() {
            return without_mark_rows(|| self.font.char_code_to_unicode(self.char_code));
        }
        text
    }
}

impl CacheKey for Type3Glyph<'_> {
    fn cache_key(&self) -> u128 {
        hash128(&(self.font.cache_key(), self.glyph_id))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Font<'a>(u128, FontType<'a>, Option<Rc<str>>);

impl<'a> Font<'a> {
    pub(crate) fn new(
        dict: &Dict<'a>,
        font_resolver: &FontResolverFn,
        cmap_resolver: &CMapResolverFn,
        warning_sink: &WarningSinkFn,
    ) -> Option<Self> {
        let f_type = match dict.get::<Name>(SUBTYPE)?.deref() {
            TYPE1 | MM_TYPE1 => FontType::Type1(Rc::new(Type1Font::new(
                dict,
                font_resolver,
                cmap_resolver,
                warning_sink,
            )?)),
            // PDFBOX-5463: PDF viewers seem to accept OpenType as well.
            TRUE_TYPE | OPEN_TYPE => FontType::TrueType(Rc::new(TrueTypeFont::new(
                dict,
                font_resolver,
                cmap_resolver,
                warning_sink,
            )?)),
            TYPE0 => FontType::Type0(Rc::new(Type0Font::new(
                dict,
                font_resolver,
                cmap_resolver,
                warning_sink,
            )?)),
            TYPE3 => FontType::Type3(Rc::new(Type3::new(dict, cmap_resolver, warning_sink)?)),
            f => {
                warn!(
                    "unimplemented font type {:?}",
                    std::str::from_utf8(f).unwrap_or("unknown type")
                );

                return None;
            }
        };

        let cache_key = dict.cache_key();

        Some(Self(cache_key, f_type, None))
    }

    pub(crate) fn new_standard(
        standard_font: StandardFont,
        font_resolver: &FontResolverFn,
    ) -> Option<Self> {
        let font = Type1Font::new_standard(standard_font, font_resolver)?;

        Some(Self(0, FontType::Type1(Rc::new(font)), None))
    }

    /// Name the resource this font was set from (`Tf`'s operand): the name
    /// extraction reads back to tell the engine's own OCR layer apart from
    /// author text set in the same face. Stamped on the clone the `Tf`
    /// handler takes out of the cache, so the cached font stays nameless and
    /// one resource's name never leaks onto another's.
    pub(crate) fn set_resource_name(&mut self, name: &str) {
        self.2 = Some(Rc::from(name));
    }

    pub(crate) fn map_code(&self, code: u32) -> GlyphId {
        match &self.1 {
            FontType::Type1(f) => {
                debug_assert!(code <= u8::MAX as u32);

                f.map_code(code as u8)
            }
            FontType::TrueType(t) => {
                debug_assert!(code <= u8::MAX as u32);

                t.map_code(code as u8)
            }
            FontType::Type0(t) => t.map_code(code),
            FontType::Type3(t) => {
                debug_assert!(code <= u8::MAX as u32);

                t.map_code(code as u8)
            }
        }
    }

    pub(crate) fn get_glyph(
        &self,
        glyph: GlyphId,
        char_code: u32,
        ctx: &mut Context<'a>,
        resources: &Resources<'a>,
        origin_displacement: Vec2,
    ) -> (Glyph<'a>, Affine) {
        let glyph_transform = ctx.get().text_state.full_transform()
            * Affine::scale(1.0 / UNITS_PER_EM as f64)
            * Affine::translate(origin_displacement);

        let glyph = match &self.1 {
            FontType::Type1(t) => {
                let font = OutlineFont::Type1(t.clone());
                let text = font.char_code_to_unicode(char_code);
                Glyph::Outline(OutlineGlyph {
                    id: glyph,
                    font,
                    char_code,
                    resource_name: self.2.clone(),
                    marks: Some(ctx.marks.site()),
                    text,
                })
            }
            FontType::TrueType(t) => {
                let font = OutlineFont::TrueType(t.clone());
                let text = font.char_code_to_unicode(char_code);
                Glyph::Outline(OutlineGlyph {
                    id: glyph,
                    font,
                    char_code,
                    resource_name: self.2.clone(),
                    marks: Some(ctx.marks.site()),
                    text,
                })
            }
            FontType::Type0(t) => {
                let font = OutlineFont::Type0(t.clone());
                let text = font.char_code_to_unicode(char_code);
                Glyph::Outline(OutlineGlyph {
                    id: glyph,
                    font,
                    char_code,
                    resource_name: self.2.clone(),
                    marks: Some(ctx.marks.site()),
                    text,
                })
            }
            FontType::Type3(t) => {
                let shape_glyph = Type3Glyph {
                    nesting_depth: ctx.nesting_depth(),
                    font: t.clone(),
                    glyph_id: glyph,
                    state: ctx.get().clone(),
                    parent_resources: resources.clone(),
                    cache: ctx.object_cache.clone(),
                    xref: ctx.xref,
                    settings: ctx.settings.clone(),
                    char_code,
                };

                Glyph::Type3(Box::new(shape_glyph))
            }
        };

        (glyph, glyph_transform)
    }

    pub(crate) fn code_advance(&self, code: u32) -> Vec2 {
        match &self.1 {
            FontType::Type1(t) => {
                debug_assert!(code <= u8::MAX as u32);

                Vec2::new(t.glyph_width(code as u8).unwrap_or(0.0) as f64, 0.0)
            }
            FontType::TrueType(t) => {
                debug_assert!(code <= u8::MAX as u32);

                Vec2::new(t.glyph_width(code as u8) as f64, 0.0)
            }
            FontType::Type0(t) => t.code_advance(code),
            FontType::Type3(t) => {
                debug_assert!(code <= u8::MAX as u32);

                Vec2::new(t.glyph_width(code as u8) as f64, 0.0)
            }
        }
    }

    pub(crate) fn origin_displacement(&self, code: u32) -> Vec2 {
        match &self.1 {
            FontType::Type1(_) => Vec2::default(),
            FontType::TrueType(_) => Vec2::default(),
            FontType::Type0(t) => t.origin_displacement(code),
            FontType::Type3(_) => Vec2::default(),
        }
    }

    pub(crate) fn read_code(&self, bytes: &[u8], offset: usize) -> (u32, usize) {
        match &self.1 {
            FontType::Type1(_) => (bytes[offset] as u32, 1),
            FontType::TrueType(_) => (bytes[offset] as u32, 1),
            FontType::Type0(t) => t.read_code(bytes, offset),
            FontType::Type3(_) => (bytes[offset] as u32, 1),
        }
    }

    pub(crate) fn is_horizontal(&self) -> bool {
        match &self.1 {
            FontType::Type1(_) => true,
            FontType::TrueType(_) => true,
            FontType::Type0(t) => t.is_horizontal(),
            FontType::Type3(_) => true,
        }
    }
}

impl CacheKey for Font<'_> {
    fn cache_key(&self) -> u128 {
        self.0
    }
}

#[derive(Clone, Debug)]
enum FontType<'a> {
    Type1(Rc<Type1Font>),
    TrueType(Rc<TrueTypeFont>),
    Type0(Rc<Type0Font>),
    Type3(Rc<Type3<'a>>),
}

#[derive(Debug)]
enum Encoding {
    Standard,
    MacRoman,
    WinAnsi,
    MacExpert,
    BuiltIn,
}

impl Encoding {
    fn map_code(&self, code: u8) -> Option<&'static str> {
        if code == 0 {
            return Some(".notdef");
        }
        match self {
            Self::Standard => standard::get(code),
            Self::MacRoman => mac_roman::get(code).or_else(|| mac_os_roman::get(code)),
            Self::WinAnsi => win_ansi::get(code),
            Self::MacExpert => mac_expert::get(code),
            Self::BuiltIn => None,
        }
    }
}

/// The font stretch.
#[derive(Debug, Copy, Clone)]
pub enum FontStretch {
    /// Normal.
    Normal,
    /// Ultra condensed.
    UltraCondensed,
    /// Extra condensed.
    ExtraCondensed,
    /// Condensed.
    Condensed,
    /// Semi condensed.
    SemiCondensed,
    /// Semi expanded.
    SemiExpanded,
    /// Expanded.
    Expanded,
    /// Extra expanded.
    ExtraExpanded,
    /// Ultra expanded.
    UltraExpanded,
}

impl FontStretch {
    fn from_string(s: &str) -> Self {
        match s {
            "UltraCondensed" => Self::UltraCondensed,
            "ExtraCondensed" => Self::ExtraCondensed,
            "Condensed" => Self::Condensed,
            "SemiCondensed" => Self::SemiCondensed,
            "SemiExpanded" => Self::SemiExpanded,
            "Expanded" => Self::Expanded,
            "ExtraExpanded" => Self::ExtraExpanded,
            "UltraExpanded" => Self::UltraExpanded,
            _ => Self::Normal,
        }
    }
}

bitflags! {
    /// Bitflags describing various characteristics of fonts.
    #[derive(Debug)]
    pub(crate) struct FontFlags: u32 {
        const FIXED_PITCH = 1 << 0;
        const SERIF = 1 << 1;
        const SYMBOLIC = 1 << 2;
        const SCRIPT = 1 << 3;
        const NON_SYMBOLIC = 1 << 5;
        const ITALIC = 1 << 6;
        const ALL_CAP = 1 << 16;
        const SMALL_CAP = 1 << 17;
        const FORCE_BOLD = 1 << 18;
    }
}

/// A query for a font.
pub enum FontQuery {
    /// A query for one of the 14 PDF standard fonts.
    Standard(StandardFont),
    /// A query for a font that is not embedded in the PDF file.
    ///
    /// Note that this type of query is currently not supported,
    /// but will be implemented in the future.
    Fallback(FallbackFontQuery),
}

/// A query for a font with specific properties.
#[derive(Debug, Clone)]
pub struct FallbackFontQuery {
    /// The postscript name of the font.
    pub post_script_name: Option<String>,
    /// The name of the font.
    pub font_name: Option<String>,
    /// The family of the font.
    pub font_family: Option<String>,
    /// The stretch of the font.
    pub font_stretch: FontStretch,
    /// The weight of the font.
    pub font_weight: u32,
    /// Whether the font is monospaced.
    pub is_fixed_pitch: bool,
    /// Whether the font is serif.
    pub is_serif: bool,
    /// Whether the font is italic.
    pub is_italic: bool,
    /// Whether the font is bold.
    pub is_bold: bool,
    /// Whether the font is small cap.
    pub is_small_cap: bool,
    /// The character collection (registry/ordering) if this is a CID font.
    pub character_collection: Option<CharacterCollection>,
}

impl FallbackFontQuery {
    pub(crate) fn new(dict: &Dict<'_>) -> Self {
        let post_script_name = dict
            .get::<Name>(BASE_FONT)
            .map(|n| strip_subset_prefix(n.as_str()).to_string());

        let mut data = Self {
            post_script_name,
            ..Default::default()
        };

        if let Some(descriptor) = dict.get::<Dict<'_>>(FONT_DESC) {
            data.font_name = dict
                .get::<Name>(FONT_NAME)
                .map(|n| strip_subset_prefix(n.as_str()).to_string());
            data.font_family = descriptor
                .get::<Name>(FONT_FAMILY)
                .map(|n| n.as_str().to_string());
            data.font_stretch = descriptor
                .get::<Name>(FONT_STRETCH)
                .map(|n| FontStretch::from_string(n.as_str()))
                .unwrap_or(FontStretch::Normal);
            data.font_weight = descriptor.get::<u32>(FONT_WEIGHT).unwrap_or(400);

            if let Some(flags) = descriptor
                .get::<u32>(FLAGS)
                .map(FontFlags::from_bits_truncate)
            {
                data.is_fixed_pitch = flags.contains(FontFlags::FIXED_PITCH);
                data.is_serif = flags.contains(FontFlags::SERIF);
                data.is_italic = flags.contains(FontFlags::ITALIC);
                data.is_small_cap = flags.contains(FontFlags::SMALL_CAP);
            }
        }

        data.is_bold |= data.font_weight >= 700;

        if let Some(name) = &data.post_script_name {
            let lower = name.to_ascii_lowercase();
            data.is_italic |=
                lower.contains("italic") || lower.contains("oblique") || lower.contains("slant");
            data.is_bold |= lower.contains("bold")
                || lower.contains("demi")
                || lower.contains("semibold")
                || lower.contains("heavy")
                || lower.contains("black");
        }

        data
    }

    /// Do a best-effort fallback to the 14 standard fonts based on the query.
    pub fn pick_standard_font(&self) -> StandardFont {
        if self.is_fixed_pitch {
            match (self.is_bold, self.is_italic) {
                (true, true) => StandardFont::CourierBoldOblique,
                (true, false) => StandardFont::CourierBold,
                (false, true) => StandardFont::CourierOblique,
                (false, false) => StandardFont::Courier,
            }
        } else if !self.is_serif {
            match (self.is_bold, self.is_italic) {
                (true, true) => StandardFont::HelveticaBoldOblique,
                (true, false) => StandardFont::HelveticaBold,
                (false, true) => StandardFont::HelveticaOblique,
                (false, false) => StandardFont::Helvetica,
            }
        } else {
            match (self.is_bold, self.is_italic) {
                (true, true) => StandardFont::TimesBoldItalic,
                (true, false) => StandardFont::TimesBold,
                (false, true) => StandardFont::TimesItalic,
                (false, false) => StandardFont::TimesRoman,
            }
        }
    }
}

impl Default for FallbackFontQuery {
    fn default() -> Self {
        Self {
            post_script_name: None,
            font_name: None,
            font_family: None,
            font_stretch: FontStretch::Normal,
            font_weight: 400,
            is_fixed_pitch: false,
            is_serif: false,
            is_italic: false,
            is_bold: false,
            is_small_cap: false,
            character_collection: None,
        }
    }
}

/// Convert a glyph name to a Unicode character, if possible.
/// Implements the Adobe Glyph List Specification with extended fallbacks
/// for non-standard names found in real-world PDFs.
/// <https://github.com/adobe-type-tools/agl-specification>
pub(crate) fn glyph_name_to_unicode(name: &str) -> Option<char> {
    // 1. Direct AGL lookup
    if let Some(unicode_str) = glyph_names::get(name) {
        return unicode_str.chars().next();
    }

    // 2. uni/u prefix convention
    if let Some(c) = unicode_from_name(name) {
        return Some(c);
    }

    // 3. AGL §2.4: strip variant suffix after period (e.g., "A.swash" → "A",
    //    "comma.alt" → "comma"). Only the base name before the first period
    //    maps to Unicode.
    if let Some(dot_pos) = name.find('.') {
        let base = &name[..dot_pos];
        if !base.is_empty() {
            if let Some(c) = glyph_names::get(base).and_then(|s| s.chars().next()) {
                return Some(c);
            }
            if let Some(c) = unicode_from_name(base) {
                return Some(c);
            }
        }
    }

    // 4. Handle "aXX" decimal glyph names (e.g., "a65" → 'A') used by some
    //    TeX/LaTeX generated PDFs and custom encoding vectors.
    if name.starts_with('a')
        && name.len() >= 2
        && let Ok(code) = name[1..].parse::<u32>()
        && let Some(c) = char::from_u32(code)
        && (!c.is_control() || c == ' ')
    {
        return Some(c);
    }

    warn!("failed to map glyph name {} to unicode", name);
    None
}

/// AGL §2.2: resolve a glyph name to one or more Unicode code points. Handles
/// everything `glyph_name_to_unicode` does, plus the `uniXXXX…` sequences of
/// AGLFN §3 and underscore-joined ligature names (e.g. `f_i` → "fi",
/// `f_f_i` → "ffi") which glyph-list-based fonts use for Latin ligatures.
/// Returns the full string, so ligatures surface as their component code
/// points rather than being dropped when the composite name isn't in the AGL.
///
/// Strips the variant suffix (`.ss01`, `.swash`, …) from the base name before
/// splitting, matching AGL §2.4. Bails out if any component fails to resolve
/// so we never produce a partial string (which would be worse than None).
///
/// Only when no AGL rule reads the name is the small AMS math-font table
/// asked, by exact name: it adds names, it never overrides an AGL answer.
///
/// A row that reads as a combining mark reads as no table knows it while the
/// rows are set aside (see `marks.rs`), which is how a reader tells a mark
/// the row gave from the same character given by `ToUnicode` or any other
/// source.
pub(crate) fn glyph_name_to_string(name: &str) -> Option<String> {
    glyph_name_reading(name).map(|(text, _)| text)
}

/// [`glyph_name_to_string`], with whether a combining-mark row of the AMS
/// table gave the text.
fn glyph_name_reading(name: &str) -> Option<(String, bool)> {
    if let Some(text) = agl_name_to_string(name) {
        return Some((text, false));
    }
    let c = ams_glyph_names::get(name)?;
    if is_combining_mark(c) {
        if mark_rows_set_aside() {
            return None;
        }
        return Some((String::from(c), true));
    }
    Some((String::from(c), false))
}

/// The AGL half of [`glyph_name_to_string`].
fn agl_name_to_string(name: &str) -> Option<String> {
    // Then the `uni`/`u` sequences, which spell their own code points --
    // including the multi-group `uni00410042` form, which the
    // single-character path below cannot hold (#636).
    if let Some(s) = unicode_string_from_name(name) {
        return Some(s);
    }

    // Then the single-code-point path, which also handles `.suffix`
    // stripping and the `aNN` decimal convention.
    if let Some(c) = glyph_name_to_unicode(name) {
        return Some(c.to_string());
    }

    let base = name.split_once('.').map(|(b, _)| b).unwrap_or(name);
    if !base.contains('_') {
        return None;
    }

    let mut out = String::new();
    for part in base.split('_') {
        if part.is_empty() {
            return None;
        }
        let text = unicode_string_from_name(part)
            .or_else(|| glyph_name_to_unicode(part).map(|c| c.to_string()))?;
        out.push_str(&text);
    }
    (!out.is_empty()).then_some(out)
}

/// AGLFN §3: read a `uni`/`u` name as the code points it spells, however many
/// there are.
///
/// `uni` takes one or more groups of four hex digits (`uni20AC` is €,
/// `uni00410042` is `AB`); `u` takes a single group of four to six
/// (`u1F600` is 😀, longer runs join with underscores: `u0041_u0042`). The
/// variant suffix of AGL §2.4 is stripped first, so `uni20AC.tnum` reads like
/// `uni20AC`.
///
/// A name that is not well-formed under those rules -- a short group, a
/// non-hex digit, a surrogate -- is refused rather than guessed at, and the
/// caller falls through to the next step of the chain.
pub(crate) fn unicode_string_from_name(name: &str) -> Option<String> {
    let base = name.split_once('.').map(|(b, _)| b).unwrap_or(name);
    if let Some(rest) = base.strip_prefix("uni") {
        if rest.len() < 4 || !rest.len().is_multiple_of(4) {
            return None;
        }
        let mut out = String::new();
        for chunk in rest.as_bytes().chunks(4) {
            let digits = core::str::from_utf8(chunk).ok()?;
            let value = u32::from_str_radix(digits, 16).ok()?;
            out.push(char::from_u32(value)?);
        }
        return Some(out);
    }
    if let Some(rest) = base.strip_prefix('u') {
        if !(4..=6).contains(&rest.len()) {
            return None;
        }
        let value = u32::from_str_radix(rest, 16).ok()?;
        return Some(char::from_u32(value)?.to_string());
    }
    None
}

pub(crate) fn unicode_from_name(name: &str) -> Option<char> {
    let convert = |input: &str| u32::from_str_radix(input, 16).ok().and_then(char::from_u32);

    name.starts_with("uni")
        .then(|| name.get(3..).and_then(convert))
        .or_else(|| {
            name.starts_with("u")
                .then(|| name.get(1..).and_then(convert))
        })
        .flatten()
}

pub(crate) fn read_to_unicode(
    dict: &Dict<'_>,
    cmap_resolver: &CMapResolverFn,
    warning_sink: &WarningSinkFn,
) -> Option<CMap> {
    dict.get::<Stream<'_>>(TO_UNICODE)
        .and_then(|s| decode_or_warn(&s, warning_sink))
        // See PDFJS-11915, where `Identity-H` is used for `ToUnicode`. I don't
        // believe it's valid, but at least mupdf seems to be able to deal with it.
        .or_else(|| {
            dict.get::<Name>(TO_UNICODE)
                .and_then(|name| (cmap_resolver)(CMapName::from_bytes(name.as_ref())))
                .map(|d| d.to_vec())
        })
        .and_then(|data| {
            let cmap_resolver = cmap_resolver.clone();
            CMap::parse(&data, move |name| (cmap_resolver)(name))
        })
}

/// A glyph name read as the text it spells, in cmap-string form.
///
/// Single-code-point names answer as [`BfString::Char`] so the common case
/// keeps its shape; names that spell several code points (an underscore
/// ligature like `f_i`, a multi-group `uni00410042`, an AGL sequence entry)
/// answer as [`BfString::String`]. `None` when no rule reads the name.
pub(crate) fn glyph_name_to_bfstring(name: &str) -> Option<BfString> {
    glyph_name_to_string(name).map(bfstring_of)
}

/// `text` in cmap-string form: one code point as [`BfString::Char`].
fn bfstring_of(text: String) -> BfString {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => BfString::Char(c),
        _ => BfString::String(text),
    }
}

/// True when a ToUnicode value holds nothing but control characters
/// (U+0000-U+001F, U+007F-U+009F). Such a value carries no text: a simple
/// font asks its glyph name instead, and keeps the value only when the name
/// reads as nothing. An empty value is not control-only.
pub(crate) fn is_control_only(value: &BfString) -> bool {
    let is_control = |c: char| matches!(c, '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}');
    match value {
        BfString::Char(c) => is_control(*c),
        BfString::String(s) => !s.is_empty() && s.chars().all(is_control),
    }
}

/// Build a 256-entry Unicode map from the font's `/Encoding` entry.
/// Used as a fallback when no `/ToUnicode` CMap is present. Each entry
/// carries whether a combining-mark row gave it, so that it can be set aside
/// with the rows (see [`table_entry`]).
pub(crate) fn synthesize_unicode_map_from_encoding(
    dict: &Dict<'_>,
) -> Option<[Option<NamedEntry>; 256]> {
    let (base_encoding, differences) = true_type::read_encoding(dict);
    // Only synthesize if there is something to map (encoding name or differences).
    // For BuiltIn with no differences we cannot reliably guess the mapping.
    if matches!(base_encoding, Encoding::BuiltIn) && differences.is_empty() {
        return None;
    }
    let mut table: [Option<NamedEntry>; 256] = std::array::from_fn(|_| None);
    for code in 0u8..=255 {
        let glyph_name = differences
            .get(&code)
            .map(String::as_str)
            .or_else(|| base_encoding.map_code(code));
        if let Some(name) = glyph_name {
            table[code as usize] =
                glyph_name_reading(name).map(|(text, from_mark_row)| NamedEntry {
                    text: bfstring_of(text),
                    from_mark_row,
                });
        }
    }
    Some(table)
}

// When mapping to glyphs, some fonts might only have a glyph for the "normalized"
// name. These aliases come from the Adobe Glyph List (AGL) and are safe
// synonyms: they are consulted only when the literal lookup already failed, so
// adding new entries broadens recovery without overriding correct mappings.
pub(crate) fn normalized_glyph_name(mut name: &str) -> &str {
    if name == "nbspace" || name == "nonbreakingspace" {
        name = "space";
    }

    if name == "sfthyphen" || name == "softhyphen" {
        name = "hyphen";
    }

    name
}

#[cfg(test)]
mod normalized_glyph_name_tests {
    use super::normalized_glyph_name;

    #[test]
    fn maps_space_aliases() {
        assert_eq!(normalized_glyph_name("nbspace"), "space");
        assert_eq!(normalized_glyph_name("nonbreakingspace"), "space");
    }

    #[test]
    fn maps_hyphen_aliases() {
        assert_eq!(normalized_glyph_name("sfthyphen"), "hyphen");
        assert_eq!(normalized_glyph_name("softhyphen"), "hyphen");
    }

    #[test]
    fn preserves_unrelated_names() {
        assert_eq!(normalized_glyph_name("A"), "A");
        assert_eq!(normalized_glyph_name(".notdef"), ".notdef");
        assert_eq!(normalized_glyph_name("hyphen"), "hyphen");
        assert_eq!(normalized_glyph_name("space"), "space");
    }
}

#[cfg(test)]
mod glyph_name_to_unicode_tests {
    use super::{
        BfString, glyph_name_to_bfstring, glyph_name_to_string, glyph_name_to_unicode,
        unicode_string_from_name,
    };

    #[test]
    fn standard_agl_name() {
        assert_eq!(glyph_name_to_unicode("A"), Some('A'));
        assert_eq!(glyph_name_to_unicode("space"), Some(' '));
        assert_eq!(glyph_name_to_unicode("hyphen"), Some('-'));
    }

    #[test]
    fn ligature_name_underscore_joined() {
        // AGL maps `fi` directly (U+FB01); the underscore form is what
        // CFF fonts without a ToUnicode map tend to emit for the same
        // ligature. We want the decomposed string so readers see "fi".
        assert_eq!(glyph_name_to_string("f_i"), Some("fi".to_string()));
        assert_eq!(glyph_name_to_string("f_f_i"), Some("ffi".to_string()));
        assert_eq!(glyph_name_to_string("A_B_C"), Some("ABC".to_string()));
    }

    #[test]
    fn ligature_name_with_suffix() {
        // Variant suffix must be stripped before splitting on `_`.
        assert_eq!(glyph_name_to_string("f_i.alt"), Some("fi".to_string()));
    }

    #[test]
    fn ligature_name_falls_back_to_single_char_path() {
        // Direct AGL hit should still win and yield the precomposed char.
        assert_eq!(glyph_name_to_string("fi"), Some("\u{FB01}".to_string()));
        assert_eq!(glyph_name_to_string("A"), Some("A".to_string()));
    }

    #[test]
    fn ligature_name_rejects_unresolvable_component() {
        // If any segment is unknown we return None rather than a partial
        // string — a partial would be more confusing than a drop.
        assert!(glyph_name_to_string("A_totallyUnknownGlyph").is_none());
        assert!(glyph_name_to_string("_").is_none());
        assert!(glyph_name_to_string("A__B").is_none());
    }

    #[test]
    fn uni_prefix() {
        assert_eq!(glyph_name_to_unicode("uni0041"), Some('A'));
        assert_eq!(glyph_name_to_unicode("uni00E9"), Some('é'));
    }

    #[test]
    fn u_prefix() {
        assert_eq!(glyph_name_to_unicode("u0041"), Some('A'));
        assert_eq!(glyph_name_to_unicode("u2022"), Some('•'));
    }

    #[test]
    fn variant_suffix_stripped() {
        assert_eq!(glyph_name_to_unicode("A.swash"), Some('A'));
        assert_eq!(glyph_name_to_unicode("comma.alt"), Some(','));
        assert_eq!(glyph_name_to_unicode("space.narrow"), Some(' '));
    }

    #[test]
    fn variant_suffix_with_uni_prefix() {
        assert_eq!(glyph_name_to_unicode("uni0041.ss01"), Some('A'));
    }

    #[test]
    fn a_decimal_glyph_name() {
        assert_eq!(glyph_name_to_unicode("a65"), Some('A'));
        assert_eq!(glyph_name_to_unicode("a32"), Some(' '));
        assert_eq!(glyph_name_to_unicode("a97"), Some('a'));
    }

    #[test]
    fn a_decimal_rejects_control_chars() {
        assert_eq!(glyph_name_to_unicode("a0"), None);
        assert_eq!(glyph_name_to_unicode("a7"), None);
    }

    #[test]
    fn unknown_name_returns_none() {
        assert_eq!(glyph_name_to_unicode("xyzzynonexistent"), None);
    }

    #[test]
    fn bfstring_keeps_single_chars_and_strings_their_shape() {
        assert_eq!(glyph_name_to_bfstring("A"), Some(BfString::Char('A')));
        assert_eq!(
            glyph_name_to_bfstring("f_i"),
            Some(BfString::String("fi".to_string()))
        );
        assert_eq!(glyph_name_to_bfstring("xyzzynonexistent"), None);
    }

    /// The phase-4 rows (C5c) are plain characters: the name reads as the
    /// mathematical angle bracket with no mark origin, set aside or not.
    /// Without the rows the names read as today: no AGL rule knows them, so
    /// the code keeps its old text (`h`/`i` in cmsy10).
    #[test]
    fn the_angle_bracket_rows_read_as_plain_characters() {
        for (name, bracket) in [
            ("angbracketleft", '\u{27E8}'),
            ("angbracketright", '\u{27E9}'),
        ] {
            assert_eq!(super::agl_name_to_string(name), None, "{name}");
            assert_eq!(
                glyph_name_to_bfstring(name),
                Some(BfString::Char(bracket)),
                "{name}"
            );
            assert_eq!(super::glyph_name_reading(name).map(|r| r.1), Some(false));
            assert_eq!(
                super::without_mark_rows(|| glyph_name_to_bfstring(name)),
                Some(BfString::Char(bracket))
            );
        }
    }

    /// The phase-5 row (C5b) is a plain character: `visiblespace` reads as
    /// U+2423 OPEN BOX with no mark origin, set aside or not. Without the row
    /// the name reads as today: no AGL rule knows it, so the glyph reads as
    /// nothing.
    #[test]
    fn the_visiblespace_row_reads_as_the_open_box() {
        let name = "visiblespace";
        assert_eq!(super::agl_name_to_string(name), None);
        assert_eq!(
            glyph_name_to_bfstring(name),
            Some(BfString::Char('\u{2423}'))
        );
        assert_eq!(super::glyph_name_reading(name).map(|r| r.1), Some(false));
        assert_eq!(
            super::without_mark_rows(|| glyph_name_to_bfstring(name)),
            Some(BfString::Char('\u{2423}'))
        );
    }

    /// Each combining-mark row reads as its bare mark, with its origin beside
    /// it: set aside, the name reads as no row at all, while the same mark
    /// from a `uni` name stays (C4 rule 6, C5a, table shape, review (c)). A
    /// plain row reads as its character whether the rows stand or not.
    #[test]
    fn a_combining_mark_row_carries_its_origin() {
        for (name, mark, uni) in [
            ("circlecopyrt", '\u{20DD}', "uni20DD"),
            ("negationslash", '\u{0338}', "uni0338"),
        ] {
            let named = glyph_name_to_bfstring(name);
            assert_eq!(named, Some(BfString::Char(mark)), "{name}");
            assert_eq!(super::glyph_name_reading(name).map(|r| r.1), Some(true));
            assert_eq!(
                super::row_mark_of(&named, || glyph_name_to_bfstring(name)),
                Some(mark)
            );
            let elsewhere = glyph_name_to_bfstring(uni);
            assert_eq!(elsewhere, Some(BfString::Char(mark)));
            assert_eq!(
                super::row_mark_of(&elsewhere, || glyph_name_to_bfstring(uni)),
                None
            );
            assert_eq!(
                super::without_mark_rows(|| glyph_name_to_bfstring(name)),
                None
            );
            // A table entry the row wrote counts as absent while set aside.
            let entry = Some(super::NamedEntry {
                text: BfString::Char(mark),
                from_mark_row: true,
            });
            assert_eq!(super::table_entry(&entry), named);
            assert_eq!(
                super::without_mark_rows(|| super::table_entry(&entry)),
                None
            );
        }
        assert_eq!(
            super::without_mark_rows(|| glyph_name_to_bfstring("square")),
            Some(BfString::Char('\u{25A1}'))
        );
        assert_eq!(
            glyph_name_to_bfstring("square"),
            Some(BfString::Char('\u{25A1}'))
        );
    }

    #[test]
    fn uni_sequence_spells_each_group() {
        assert_eq!(
            unicode_string_from_name("uni00410042"),
            Some("AB".to_string())
        );
        assert_eq!(unicode_string_from_name("uni20AC"), Some("€".to_string()));
        assert_eq!(glyph_name_to_string("uni00410042"), Some("AB".to_string()));
    }

    #[test]
    fn uni_sequence_strips_the_variant_suffix_first() {
        // The suffix itself holds an underscore, so splitting before
        // stripping would resolve `tnum` as a ligature component.
        assert_eq!(
            glyph_name_to_string("uni00410042.tnum_lnum"),
            Some("AB".to_string())
        );
        assert_eq!(glyph_name_to_string("uni20AC.tnum"), Some("€".to_string()));
    }

    #[test]
    fn u_prefix_holds_four_to_six_digits() {
        assert_eq!(unicode_string_from_name("u0041"), Some("A".to_string()));
        assert_eq!(unicode_string_from_name("u1F600"), Some("😀".to_string()));
        assert_eq!(unicode_string_from_name("u01F600"), Some("😀".to_string()));
        assert_eq!(unicode_string_from_name("u041"), None);
        assert_eq!(unicode_string_from_name("u00410042"), None);
        assert_eq!(glyph_name_to_string("u0041_u0042"), Some("AB".to_string()));
    }

    #[test]
    fn malformed_uni_names_are_refused() {
        assert_eq!(unicode_string_from_name("uni"), None);
        assert_eq!(unicode_string_from_name("uni00"), None);
        assert_eq!(unicode_string_from_name("uni00410"), None);
        assert_eq!(unicode_string_from_name("uni00ZZ"), None);
        assert_eq!(unicode_string_from_name("uniD800"), None);
        assert_eq!(glyph_name_to_string("uni00ZZ"), None);
        assert_eq!(glyph_name_to_string("uniD800"), None);
    }

    #[test]
    fn ligature_components_may_spell_sequences() {
        // A component may itself spell several code points -- the
        // single-character path reads neither of these first halves.
        assert_eq!(
            glyph_name_to_string("uni00410042_uni0043"),
            Some("ABC".to_string())
        );
        assert_eq!(
            glyph_name_to_string("uni00410042_X"),
            Some("ABX".to_string())
        );
        assert_eq!(
            glyph_name_to_string("uni0041_uni0042"),
            Some("AB".to_string())
        );
        assert_eq!(glyph_name_to_string("A_uni0042"), Some("AB".to_string()));
        assert_eq!(glyph_name_to_string("A_u0042"), Some("AB".to_string()));
    }
}

#[cfg(test)]
mod fallback_font_query_tests {
    use super::*;

    fn query_with(name: &str, flags: u32, weight: u32) -> FallbackFontQuery {
        let mut q = FallbackFontQuery {
            post_script_name: Some(name.to_string()),
            font_weight: weight,
            ..Default::default()
        };

        let font_flags = FontFlags::from_bits_truncate(flags);
        q.is_fixed_pitch = font_flags.contains(FontFlags::FIXED_PITCH);
        q.is_serif = font_flags.contains(FontFlags::SERIF);
        q.is_italic = font_flags.contains(FontFlags::ITALIC);
        q.is_small_cap = font_flags.contains(FontFlags::SMALL_CAP);

        q.is_bold |= q.font_weight >= 700;

        if let Some(name) = &q.post_script_name {
            let lower = name.to_ascii_lowercase();
            q.is_italic |=
                lower.contains("italic") || lower.contains("oblique") || lower.contains("slant");
            q.is_bold |= lower.contains("bold")
                || lower.contains("demi")
                || lower.contains("semibold")
                || lower.contains("heavy")
                || lower.contains("black");
        }
        q
    }

    #[test]
    fn fixed_pitch_flag_selects_courier() {
        let q = query_with("LetterGothic", FontFlags::FIXED_PITCH.bits(), 400);
        assert!(matches!(q.pick_standard_font(), StandardFont::Courier));
    }

    #[test]
    fn demi_in_name_selects_bold() {
        let q = query_with("FranklinGothic-Demi", FontFlags::SERIF.bits(), 400);
        assert!(q.is_bold);
        assert!(matches!(q.pick_standard_font(), StandardFont::TimesBold));
    }

    #[test]
    fn oblique_detected_as_italic() {
        let q = query_with("HelveticaNeue-LightOblique", 0, 400);
        assert!(q.is_italic);
        assert!(matches!(
            q.pick_standard_font(),
            StandardFont::HelveticaOblique
        ));
    }

    #[test]
    fn font_weight_700_detected_as_bold() {
        let q = query_with("CustomFont", 0, 700);
        assert!(q.is_bold);
        assert!(matches!(
            q.pick_standard_font(),
            StandardFont::HelveticaBold
        ));
    }

    #[test]
    fn semibold_detected_as_bold() {
        let q = query_with("AGaramond-Semibold", FontFlags::SERIF.bits(), 400);
        assert!(q.is_bold);
        assert!(matches!(q.pick_standard_font(), StandardFont::TimesBold));
    }
}
