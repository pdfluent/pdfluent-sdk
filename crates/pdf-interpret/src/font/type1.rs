use crate::font::blob::{CffFontBlob, Type1FontBlob};
use crate::font::glyph_simulator::GlyphSimulator;
use crate::font::standard_font::{StandardFont, StandardKind, select_standard_font};
use crate::font::true_type::{Width, read_encoding, read_widths};
use crate::font::{
    Encoding, FallbackFontQuery, NamedEntry, UNITS_PER_EM, glyph_name_to_bfstring,
    glyph_name_to_unicode, is_control_only, normalized_glyph_name, read_to_unicode,
    synthesize_unicode_map_from_encoding, table_entry,
};
use crate::util::decode_or_warn;
use crate::{CMapResolverFn, CacheKey, FontResolverFn, InterpreterWarning, WarningSinkFn};
use kurbo::BezPath;
use log::warn;
use pdf_font::cmap::{BfString, CMap};
use pdf_syntax::object::Dict;
use pdf_syntax::object::Name;
use pdf_syntax::object::Stream;
use pdf_syntax::object::dict::keys::{
    ASCENT, BASE_FONT, CAP_HEIGHT, DESCENT, FONT_DESC, FONT_FILE, FONT_FILE3, MISSING_WIDTH,
    XHEIGHT,
};
use skrifa::GlyphId;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct Type1Font(u128, Kind, Option<CMap>, Option<[Option<NamedEntry>; 256]>);

impl Type1Font {
    pub(crate) fn new(
        dict: &Dict<'_>,
        resolver: &FontResolverFn,
        cmap_resolver: &CMapResolverFn,
        warning_sink: &WarningSinkFn,
    ) -> Option<Self> {
        let cache_key = dict.cache_key();

        let to_unicode = read_to_unicode(dict, cmap_resolver, warning_sink);
        let encoding_unicode = to_unicode
            .is_none()
            .then(|| synthesize_unicode_map_from_encoding(dict))
            .flatten();

        let fallback = || {
            // TODO: Actually use fallback fonts
            let fallback_query = FallbackFontQuery::new(dict);
            let standard_font = fallback_query.pick_standard_font();

            warn!(
                "unable to load font {}, falling back to {}",
                fallback_query
                    .post_script_name
                    .unwrap_or("(no name)".to_string()),
                standard_font.as_str()
            );
            warning_sink(InterpreterWarning::UnsupportedFont);

            Some(Self(
                cache_key,
                Kind::Standard(StandardKind::new_with_standard(
                    dict,
                    standard_font,
                    true,
                    resolver,
                )?),
                to_unicode.clone(),
                encoding_unicode.clone(),
            ))
        };

        let inner = if is_cff(dict) {
            if let Some(cff) = CffKind::new(dict, warning_sink) {
                Self(cache_key, Kind::Cff(cff), to_unicode, encoding_unicode)
            } else {
                return fallback();
            }
        } else if is_type1(dict) {
            if let Some(f) = Type1Kind::new(dict, warning_sink) {
                Self(cache_key, Kind::Type1(f), to_unicode, encoding_unicode)
            } else {
                return fallback();
            }
        } else if let Some(standard) = StandardKind::new(dict, resolver) {
            Self(
                cache_key,
                Kind::Standard(standard),
                to_unicode,
                encoding_unicode,
            )
        } else {
            return fallback();
        };

        Some(inner)
    }

    pub(crate) fn new_standard(font: StandardFont, resolver: &FontResolverFn) -> Option<Self> {
        let dict = Dict::default();
        let standard = StandardKind::new_with_standard(&dict, font, true, resolver)?;

        Some(Self(0, Kind::Standard(standard), None, None))
    }

    pub(crate) fn map_code(&self, code: u8) -> GlyphId {
        match &self.1 {
            Kind::Standard(s) => s.map_code(code),
            Kind::Type1(s) => s.map_code(code),
            Kind::Cff(c) => c.map_code(code),
        }
    }

    pub(crate) fn outline_glyph(&self, glyph: GlyphId) -> BezPath {
        match &self.1 {
            Kind::Standard(s) => s.outline_glyph(glyph),
            Kind::Cff(c) => c.outline_glyph(glyph),
            Kind::Type1(t) => t.outline_glyph(glyph),
        }
    }

    pub(crate) fn glyph_width(&self, code: u8) -> Option<f32> {
        match &self.1 {
            Kind::Standard(s) => s.glyph_width(code),
            Kind::Cff(c) => c.glyph_width(code),
            Kind::Type1(t) => t.glyph_width(code),
        }
    }

    /// PostScript name of the font, if determinable.
    ///
    /// - `Kind::Standard` always knows its name (one of the 14
    ///   standard fonts).
    /// - `Kind::Type1` exposes its standard-font fallback name
    ///   (when the embedded Type1 was resolved against a known
    ///   standard font).
    /// - `Kind::Cff` defers to the underlying CFF top-level
    ///   FontName when available; otherwise returns `None`.
    ///
    /// Returns the raw name without subset-prefix stripping; callers
    /// (`pdf-engine::text`) are responsible for normalising.
    pub(crate) fn postscript_name(&self) -> Option<&str> {
        match &self.1 {
            Kind::Standard(s) => Some(s.postscript_name()),
            Kind::Type1(t) => t.postscript_name(),
            Kind::Cff(c) => c.postscript_name(),
        }
    }

    /// The font dictionary's /BaseFont, subset prefix and all (#623).
    ///
    /// [`Self::postscript_name`] answers with the standard face a font fell
    /// back to or was resolved against, which is what metrics and the
    /// renderer need. This is the name the page set its text in: a
    /// CFF-embedded `RotisSansSerif-Light` is `Helvetica` there and itself
    /// here, and its weight is only readable from this one.
    pub(crate) fn declared_name(&self) -> Option<&str> {
        match &self.1 {
            Kind::Standard(s) => s.declared_name(),
            Kind::Type1(t) => t.base_font.as_deref(),
            Kind::Cff(c) => c.base_font.as_deref(),
        }
    }

    /// The name text extraction reports for a span: the font dictionary's
    /// /BaseFont, subset prefix and all (callers strip), for an embedded
    /// font -- the name the page set its text in. A font whose dictionary
    /// names none, and every non-embedded font, still reports the
    /// substitute face, which is what those spans merge and restyle on.
    pub(crate) fn document_name(&self) -> Option<&str> {
        match &self.1 {
            Kind::Standard(s) => Some(s.postscript_name()),
            Kind::Type1(t) => t.base_font.as_deref().or_else(|| t.postscript_name()),
            Kind::Cff(c) => c.base_font.as_deref().or_else(|| c.postscript_name()),
        }
    }

    /// Vertical font metrics in /1000 em. An embedded font reports its
    /// descriptor's /Ascent /Descent first -- the pair the producer asserts
    /// for layout -- and only falls back to the substitute face's
    /// canonical AFM values when the descriptor carries none. Returns
    /// `None` when no metrics source is available (e.g. CFF fonts without
    /// descriptor metrics or a standard-font fallback).
    pub(crate) fn font_metrics(&self) -> Option<(f64, f64, Option<f64>, Option<f64>)> {
        match &self.1 {
            Kind::Standard(s) => s.font_metrics(),
            Kind::Type1(t) => t.font_metrics(),
            Kind::Cff(c) => c.font_metrics(),
        }
    }

    /// The substitute face's AFM pair, ignoring the descriptor: the metrics
    /// [`Self::font_metrics`] reported before E4, kept for the default
    /// extraction path (#643 follow-up). `None` where no standard face was
    /// resolved, exactly as before.
    pub(crate) fn substitute_metrics(&self) -> Option<(f64, f64, Option<f64>, Option<f64>)> {
        match &self.1 {
            Kind::Standard(s) => s.font_metrics(),
            Kind::Type1(t) => t.substitute_metrics(),
            Kind::Cff(c) => c.substitute_metrics(),
        }
    }

    pub(crate) fn char_code_to_unicode(&self, char_code: u32) -> Option<BfString> {
        if let Some(to_unicode) = &self.2
            && let Some(c) = to_unicode.lookup_bf_string(char_code)
        {
            // Skip null character mappings and fall back to glyph name
            // lookup. Some PDFs have incorrect ToUnicode mappings that map
            // to U+0000.
            if c != BfString::Char('\0') {
                if !is_control_only(&c) {
                    return Some(c);
                }
                // A value of only control characters is no text: the glyph
                // name speaks first, and the value stands when it cannot.
                return self.kind_char_code_to_unicode(char_code as u8).or(Some(c));
            }
        }

        if let Some(map) = &self.3
            && let Some(text) = usize::try_from(char_code)
                .ok()
                .and_then(|code| map.get(code))
                .and_then(table_entry)
        {
            return Some(text);
        }

        let code = char_code as u8;
        if let Some(text) = self.kind_char_code_to_unicode(code) {
            return Some(text);
        }

        // Final-resort fallbacks for fonts that ship no ToUnicode CMap, no
        // /Encoding /Differences, *and* whose built-in encoding we could not
        // translate to a glyph name (typical for Type1C subset fonts that
        // pdffonts labels "Builtin"). Without this, text extraction returns
        // an empty string and the SDK loses any chance to surface the
        // document's content — even though most such fonts layout ASCII-
        // compatible characters at ASCII-compatible code points.
        //
        // 1) Try Adobe Standard Encoding → AGL. A large share of legacy
        //    Type1 fonts ship with Standard-compatible layout even when
        //    they don't declare an /Encoding entry in the PDF.
        if let Some(name) = Encoding::Standard.map_code(code)
            && let Some(ch) = glyph_name_to_unicode(name)
        {
            return Some(BfString::Char(ch));
        }

        // 2) If the code lies in printable ASCII, assume identity. Emitting
        //    the likely-wrong character is strictly better than dropping
        //    the glyph: downstream consumers (pdftotext-style diffs, OCR
        //    sanity checks, NLP) are dramatically more sensitive to missing
        //    characters than to occasional mis-encodings.
        if (0x20..=0x7E).contains(&code) {
            return Some(BfString::Char(code as char));
        }

        None
    }

    /// The text the code's glyph name reads as, through the font's
    /// /Encoding (or its built-in encoding).
    fn kind_char_code_to_unicode(&self, code: u8) -> Option<BfString> {
        match &self.1 {
            Kind::Standard(s) => s.char_code_to_unicode(code),
            Kind::Cff(c) => c.char_code_to_unicode(code),
            Kind::Type1(t) => t.char_code_to_unicode(code),
        }
    }
}

impl CacheKey for Type1Font {
    fn cache_key(&self) -> u128 {
        self.0
    }
}

#[derive(Debug)]
enum Kind {
    Standard(StandardKind),
    Cff(CffKind),
    Type1(Type1Kind),
}

fn is_cff(dict: &Dict<'_>) -> bool {
    dict.get::<Dict<'_>>(FONT_DESC)
        .map(|dict| dict.contains_key(FONT_FILE3))
        .unwrap_or(false)
}

fn is_type1(dict: &Dict<'_>) -> bool {
    dict.get::<Dict<'_>>(FONT_DESC)
        .map(|dict| dict.contains_key(FONT_FILE))
        .unwrap_or(false)
}

/// /Ascent /Descent (/CapHeight /XHeight) from the /FontDescriptor, in
/// /1000 em: the vertical metrics the producer asserts for layout.
#[derive(Debug, Clone, Copy)]
struct DescriptorMetrics {
    ascent: f64,
    descent: f64,
    cap_height: Option<f64>,
    x_height: Option<f64>,
}

impl DescriptorMetrics {
    /// Read the descriptor's metrics. Ascent and descent are a pair (both
    /// required by ISO 32000 except for Type 3): without both, there is no
    /// descriptor source and the caller falls back. Cap and x heights are
    /// optional in the descriptor.
    fn read(descriptor: &Dict<'_>) -> Option<Self> {
        let ascent = descriptor.get::<f64>(ASCENT)?;
        let descent = descriptor.get::<f64>(DESCENT)?;
        Some(Self {
            ascent,
            descent,
            cap_height: descriptor.get::<f64>(CAP_HEIGHT),
            x_height: descriptor.get::<f64>(XHEIGHT),
        })
    }
}

/// The reported metrics for an embedded font: the descriptor's pair when it
/// carries one, else the substitute face's AFM pair. Cap and x heights come
/// from the descriptor when present so the keys stay populated as before.
fn embedded_metrics(
    desc: Option<DescriptorMetrics>,
    standard_font: Option<StandardFont>,
) -> Option<(f64, f64, Option<f64>, Option<f64>)> {
    let afm = standard_font.and_then(|sf| sf.canonical_metrics());
    if let Some(d) = desc {
        return Some((
            d.ascent,
            d.descent,
            d.cap_height.or_else(|| afm.and_then(|m| m.2)),
            d.x_height.or_else(|| afm.and_then(|m| m.3)),
        ));
    }
    afm
}

#[derive(Debug)]
struct Type1Kind {
    font: Type1FontBlob,
    encoding: Encoding,
    widths: Vec<Width>,
    missing_width: Option<f32>,
    encodings: HashMap<u8, String>,
    glyph_simulator: GlyphSimulator,
    standard_font: Option<StandardFont>,
    /// The font dictionary's /BaseFont, subset prefix and all.
    base_font: Option<String>,
    /// The descriptor's own metrics, when it carries /Ascent and /Descent.
    desc_metrics: Option<DescriptorMetrics>,
}

impl Type1Kind {
    fn new(dict: &Dict<'_>, warning_sink: &WarningSinkFn) -> Option<Self> {
        let descriptor = dict.get::<Dict<'_>>(FONT_DESC)?;
        let data = descriptor.get::<Stream<'_>>(FONT_FILE)?;
        let font = Type1FontBlob::new(Arc::new(decode_or_warn(&data, warning_sink)?.to_vec()))?;

        let (encoding, encodings) = read_encoding(dict);
        let (widths, missing_width) = read_widths(dict, &descriptor)?;
        let missing_width = descriptor
            .contains_key(MISSING_WIDTH)
            .then_some(missing_width);
        let standard_font = select_standard_font(dict, &descriptor).map(|(f, _)| f);

        let glyph_simulator = GlyphSimulator::new();
        let base_font = dict
            .get::<Name>(BASE_FONT)
            .map(|name| name.as_str().to_string());
        let desc_metrics = DescriptorMetrics::read(&descriptor);

        Some(Self {
            font,
            encoding,
            glyph_simulator,
            widths,
            missing_width,
            encodings,
            standard_font,
            base_font,
            desc_metrics,
        })
    }

    fn string_to_glyph(&self, string: &str) -> GlyphId {
        self.glyph_simulator.string_to_glyph(string)
    }

    fn map_code(&self, code: u8) -> GlyphId {
        let table = self.font.table();

        let get_glyph = |entry: &str| self.string_to_glyph(entry);

        if let Some(entry) = self.encodings.get(&code) {
            Some(get_glyph(entry))
        } else {
            match self.encoding {
                Encoding::BuiltIn => table.code_to_string(code).map(get_glyph),
                _ => self.encoding.map_code(code).map(get_glyph),
            }
        }
        .unwrap_or(GlyphId::NOTDEF)
    }

    fn outline_glyph(&self, glyph: GlyphId) -> BezPath {
        self.font.outline_glyph(
            self.glyph_simulator
                .glyph_to_string(glyph)
                .unwrap()
                .as_str(),
        )
    }

    fn code_to_ps_name(&self, code: u8) -> Option<&str> {
        self.encodings
            .get(&code)
            .map(String::as_str)
            .or_else(|| match self.encoding {
                Encoding::BuiltIn => self.font.table().code_to_string(code),
                _ => self.encoding.map_code(code),
            })
    }

    fn font_program_width(&self, code: u8) -> Option<f32> {
        let name = self.code_to_ps_name(code).unwrap_or(".notdef");
        let width = self
            .font
            .table()
            .glyph_width(name)
            .or_else(|| self.font.table().glyph_width(normalized_glyph_name(name)))
            .or_else(|| self.font.table().glyph_width(".notdef"))?;

        Some(width * font_width_scale(self.font.table().matrix().sx))
    }

    fn glyph_width(&self, code: u8) -> Option<f32> {
        match self.widths.get(code as usize).copied() {
            Some(Width::Value(w)) => Some(w),
            Some(Width::Missing) => self
                .missing_width
                .or_else(|| self.font_program_width(code))
                .or_else(|| {
                    let sf = self.standard_font?;
                    self.code_to_ps_name(code)
                        .and_then(|name| sf.get_width(name))
                }),
            _ => self.font_program_width(code).or_else(|| {
                let sf = self.standard_font?;
                self.code_to_ps_name(code)
                    .and_then(|name| sf.get_width(name))
            }),
        }
    }

    fn char_code_to_unicode(&self, code: u8) -> Option<BfString> {
        self.code_to_ps_name(code).and_then(glyph_name_to_bfstring)
    }

    /// PostScript name fallback for embedded Type1 fonts: the standard
    /// font `select_standard_font` resolved during construction, which
    /// matches how the renderer chose metrics for missing glyphs. The
    /// page's own name is [`Type1Font::declared_name`].
    fn postscript_name(&self) -> Option<&'static str> {
        self.standard_font.map(|f| f.postscript_name())
    }

    fn font_metrics(&self) -> Option<(f64, f64, Option<f64>, Option<f64>)> {
        embedded_metrics(self.desc_metrics, self.standard_font)
    }

    fn substitute_metrics(&self) -> Option<(f64, f64, Option<f64>, Option<f64>)> {
        self.standard_font.and_then(|sf| sf.canonical_metrics())
    }
}

#[derive(Debug)]
struct CffKind {
    font: CffFontBlob,
    encoding: Encoding,
    widths: Vec<Width>,
    missing_width: Option<f32>,
    encodings: HashMap<u8, String>,
    standard_font: Option<StandardFont>,
    /// The font dictionary's /BaseFont, subset prefix and all.
    base_font: Option<String>,
    /// The descriptor's own metrics, when it carries /Ascent and /Descent.
    desc_metrics: Option<DescriptorMetrics>,
}

impl CffKind {
    fn new(dict: &Dict<'_>, warning_sink: &WarningSinkFn) -> Option<Self> {
        let descriptor = dict.get::<Dict<'_>>(FONT_DESC)?;
        let data = descriptor.get::<Stream<'_>>(FONT_FILE3)?;
        let font = CffFontBlob::new(Arc::new(decode_or_warn(&data, warning_sink)?.to_vec()))?;

        let (encoding, encodings) = read_encoding(dict);
        let (widths, missing_width) = read_widths(dict, &descriptor)?;
        let missing_width = descriptor
            .contains_key(MISSING_WIDTH)
            .then_some(missing_width);
        let standard_font = select_standard_font(dict, &descriptor).map(|(f, _)| f);
        let base_font = dict
            .get::<Name>(BASE_FONT)
            .map(|name| name.as_str().to_string());
        let desc_metrics = DescriptorMetrics::read(&descriptor);

        Some(Self {
            font,
            encoding,
            widths,
            missing_width,
            encodings,
            standard_font,
            base_font,
            desc_metrics,
        })
    }

    fn map_code(&self, code: u8) -> GlyphId {
        let table = self.font.table();

        let get_glyph = |entry: &str| {
            table
                .glyph_index_by_name(entry)
                .or_else(|| table.glyph_index_by_name(normalized_glyph_name(entry)))
                .map(|g| GlyphId::new(g.0 as u32))
        };

        if let Some(entry) = self.encodings.get(&code) {
            get_glyph(entry)
        } else {
            match self.encoding {
                Encoding::BuiltIn => table.glyph_index(code).map(|g| GlyphId::new(g.0 as u32)),
                _ => self.encoding.map_code(code).and_then(get_glyph),
            }
        }
        .unwrap_or(GlyphId::NOTDEF)
    }

    fn outline_glyph(&self, glyph: GlyphId) -> BezPath {
        self.font.outline_glyph(glyph)
    }

    fn code_to_ps_name(&self, code: u8) -> Option<&str> {
        if let Some(entry) = self.encodings.get(&code) {
            Some(entry.as_str())
        } else {
            let table = self.font.table();
            match self.encoding {
                Encoding::BuiltIn => table.glyph_index(code).and_then(|g| table.glyph_name(g)),
                _ => self.encoding.map_code(code),
            }
        }
    }

    fn font_program_width(&self, code: u8) -> Option<f32> {
        let glyph = self.map_code(code);
        let glyph = pdf_font::GlyphId(glyph.to_u32() as u16);
        let width = self.font.table().glyph_width(glyph)?;
        let matrix = self.font.table().glyph_matrix(glyph);
        Some(width as f32 * font_width_scale(matrix.sx))
    }

    fn glyph_width(&self, code: u8) -> Option<f32> {
        match self.widths.get(code as usize).copied() {
            Some(Width::Value(w)) => Some(w),
            Some(Width::Missing) => self
                .missing_width
                .or_else(|| self.font_program_width(code))
                .or_else(|| {
                    let sf = self.standard_font?;
                    self.code_to_ps_name(code)
                        .and_then(|name| sf.get_width(name))
                }),
            _ => self.font_program_width(code).or_else(|| {
                let sf = self.standard_font?;
                self.code_to_ps_name(code)
                    .and_then(|name| sf.get_width(name))
            }),
        }
    }

    fn char_code_to_unicode(&self, code: u8) -> Option<BfString> {
        self.code_to_ps_name(code).and_then(glyph_name_to_bfstring)
    }

    /// PostScript name fallback for embedded CFF Type1C fonts.
    ///
    /// We do not currently parse the CFF top-level FontName; instead
    /// we expose the standard-font fallback that
    /// `select_standard_font` resolved during construction. This
    /// covers the most common case (non-embedded standard-14 CFF
    /// equivalents) and matches how the renderer chose metrics for
    /// missing glyphs. Returns `None` when no standard fallback was
    /// matched.
    fn postscript_name(&self) -> Option<&'static str> {
        self.standard_font.map(|f| f.postscript_name())
    }

    fn font_metrics(&self) -> Option<(f64, f64, Option<f64>, Option<f64>)> {
        embedded_metrics(self.desc_metrics, self.standard_font)
    }

    fn substitute_metrics(&self) -> Option<(f64, f64, Option<f64>, Option<f64>)> {
        self.standard_font.and_then(|sf| sf.canonical_metrics())
    }
}

fn font_width_scale(matrix_sx: f32) -> f32 {
    let scale = matrix_sx * UNITS_PER_EM;
    if scale.abs() > f32::EPSILON {
        scale
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::blob::Type1FontBlob;
    use std::fmt::Write;

    fn build_widths(entries: &[(u8, f32)]) -> Vec<Width> {
        let mut widths = vec![Width::Missing; 256];
        for (code, width) in entries {
            widths[*code as usize] = Width::Value(*width);
        }
        widths
    }

    fn encode_type1_number(value: i32) -> Vec<u8> {
        match value {
            -107..=107 => vec![(value + 139) as u8],
            108..=1131 => {
                let value = value - 108;
                vec![((value / 256) + 247) as u8, (value % 256) as u8]
            }
            -1131..=-108 => {
                let value = -value - 108;
                vec![((value / 256) + 251) as u8, (value % 256) as u8]
            }
            _ => panic!("unsupported Type1 test number {value}"),
        }
    }

    fn charstring_for_width(width: i32) -> Vec<u8> {
        let mut data = encode_type1_number(0);
        data.extend(encode_type1_number(width));
        data.extend([13, 14]);
        data
    }

    fn eexec_encrypt_hex(data: &[u8]) -> String {
        let mut r: u32 = 55665;
        let mut out = String::new();

        for plain in [0u8, 0, 0, 0].into_iter().chain(data.iter().copied()) {
            let cipher = plain ^ ((r >> 8) as u8);
            r = ((cipher as u32 + r).wrapping_mul(52845) + 22719) & 0xFFFF;
            write!(&mut out, "{cipher:02X}").expect("hex write should succeed");
        }

        out
    }

    fn test_type1_font_data() -> Arc<Vec<u8>> {
        let notdef = charstring_for_width(500);
        let a = charstring_for_width(700);
        let b = charstring_for_width(710);

        let mut eexec = Vec::new();
        eexec.extend_from_slice(b"/Private 8 dict dup begin\n");
        eexec.extend_from_slice(b"/lenIV -1 def\n");
        eexec.extend_from_slice(b"/Subrs 0 array\n");
        eexec.extend_from_slice(b"/CharStrings 3 dict dup begin\n");

        for (name, charstring) in [(".notdef", notdef), ("A", a), ("B", b)] {
            eexec.extend_from_slice(format!("/{name} {} RD ", charstring.len()).as_bytes());
            eexec.extend_from_slice(&charstring);
            eexec.extend_from_slice(b" ND\n");
        }

        eexec.extend_from_slice(b"end\nend\n");

        let hex = eexec_encrypt_hex(&eexec);
        let program = format!(
            "%!PS-AdobeFont-1.0: TestFont 1.0\n\
/FontName /TestFont def\n\
/FontType 1 def\n\
/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n\
/Encoding StandardEncoding def\n\
currentfile eexec\n\
{hex}\n\
cleartomark\n"
        );

        Arc::new(program.into_bytes())
    }

    fn load_type1_kind(widths: Vec<Width>, missing_width: Option<f32>) -> Type1Kind {
        Type1Kind {
            base_font: None,
            font: Type1FontBlob::new(test_type1_font_data())
                .expect("embedded test font should parse"),
            encoding: Encoding::WinAnsi,
            widths,
            missing_width,
            encodings: HashMap::new(),
            glyph_simulator: GlyphSimulator::new(),
            standard_font: None,
            desc_metrics: None,
        }
    }

    #[test]
    fn embedded_type1_uses_font_program_width_without_widths_array() {
        let font = load_type1_kind(Vec::new(), None);

        let width = font
            .glyph_width(b'A')
            .expect("embedded font should provide fallback width");
        assert!(width > 500.0, "expected font-program width, got {width}");
    }

    #[test]
    fn embedded_type1_uses_font_program_width_for_codes_missing_from_widths_array() {
        let font = load_type1_kind(build_widths(&[(b'A', 600.0)]), None);

        assert_eq!(font.glyph_width(b'A'), Some(600.0));

        let width = font
            .glyph_width(b'B')
            .expect("missing-width code should fall back to font program");
        assert!(width > 500.0, "expected font-program width, got {width}");
    }

    #[test]
    fn embedded_type1_respects_explicit_zero_missing_width() {
        let font = load_type1_kind(build_widths(&[(b'A', 600.0)]), Some(0.0));

        assert_eq!(font.glyph_width(b'A'), Some(600.0));
        assert_eq!(font.glyph_width(b'B'), Some(0.0));
    }

    #[test]
    fn substitute_metrics_ignores_the_descriptor() {
        let mut font = load_type1_kind(build_widths(&[(b'A', 600.0)]), None);
        font.standard_font = Some(StandardFont::Helvetica);
        font.desc_metrics = Some(DescriptorMetrics {
            ascent: 952.0,
            descent: -214.0,
            cap_height: Some(714.0),
            x_height: Some(517.0),
        });

        // The descriptor-first answer reads the descriptor ...
        assert_eq!(
            font.font_metrics(),
            Some((952.0, -214.0, Some(714.0), Some(517.0)))
        );
        // ... while the substitute answer stays on the AFM pair.
        assert_eq!(
            font.substitute_metrics(),
            Some((718.0, -207.0, Some(718.0), Some(523.0)))
        );
    }
}
