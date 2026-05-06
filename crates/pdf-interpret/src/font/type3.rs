use crate::context::Context;
use crate::device::Device;
use crate::font::glyph_simulator::GlyphSimulator;
use crate::font::true_type::{Width, read_encoding, read_widths};
use crate::font::{
    Encoding, Glyph, Type3Glyph, UNITS_PER_EM, glyph_name_to_unicode, normalized_glyph_name,
    read_to_unicode,
};
use crate::interpret::state::TextState;
use crate::soft_mask::SoftMask;
use crate::util::RectExt;
use crate::util::decode_or_warn;
use crate::{BlendMode, interpret};
use crate::{CMapResolverFn, WarningSinkFn};
use crate::{CacheKey, ClipPath, GlyphDrawMode, PathDrawMode};
use crate::{Image, Paint};
use kurbo::{Affine, BezPath, Rect};
use pdf_font::cmap::{BfString, CMap};
use pdf_syntax::content::TypedIter;
use pdf_syntax::content::ops::TypedInstruction;
use pdf_syntax::object::Dict;
use pdf_syntax::object::Stream;
use pdf_syntax::object::dict::keys::{CHAR_PROCS, FONT_BBOX, FONT_MATRIX, RESOURCES};
use pdf_syntax::page::Resources;
use skrifa::GlyphId;
use std::collections::HashMap;

#[derive(Debug)]
pub(crate) struct Type3<'a> {
    widths: Vec<Width>,
    missing_width: f32,
    encoding: Encoding,
    encodings: HashMap<u8, String>,
    dict: Dict<'a>,
    char_procs: HashMap<String, Stream<'a>>,
    glyph_simulator: GlyphSimulator,
    font_bbox: Rect,
    matrix: Affine,
    to_unicode: Option<CMap>,
}

impl<'a> Type3<'a> {
    pub(crate) fn new(
        dict: &Dict<'a>,
        cmap_resolver: &CMapResolverFn,
        warning_sink: &WarningSinkFn,
    ) -> Option<Self> {
        let (encoding, encodings) = read_encoding(dict);
        let (widths, missing_width) = read_widths(dict, dict)?;
        let font_bbox = dict
            .get::<pdf_syntax::object::Rect>(FONT_BBOX)
            .unwrap_or(pdf_syntax::object::Rect::ZERO)
            .to_kurbo();

        let matrix = Affine::new(
            dict.get::<[f64; 6]>(FONT_MATRIX)
                .unwrap_or([0.001, 0.0, 0.0, 0.001, 0.0, 0.0]),
        );

        let char_procs = {
            let mut procs = HashMap::new();
            let dict = dict.get::<Dict<'_>>(CHAR_PROCS).unwrap_or_default();

            for name in dict.keys() {
                if let Some(prog) = dict.get::<Stream<'_>>(name.clone()) {
                    procs.insert(name.as_str().to_string(), prog.clone());
                }
            }

            procs
        };

        let to_unicode = read_to_unicode(dict, cmap_resolver, warning_sink);

        Some(Self {
            glyph_simulator: GlyphSimulator::new(),
            encoding,
            font_bbox,
            char_procs,
            widths,
            missing_width,
            encodings,
            matrix,
            dict: dict.clone(),
            to_unicode,
        })
    }

    pub(crate) fn map_code(&self, code: u8) -> GlyphId {
        self.encodings
            .get(&code)
            .map(|s| s.as_str())
            .or_else(|| self.encoding.map_code(code))
            .map(|g| self.glyph_simulator.string_to_glyph(g))
            .unwrap_or(GlyphId::NOTDEF)
    }

    pub(crate) fn glyph_width(&self, code: u8) -> f32 {
        let w = match self.widths.get(code as usize).copied() {
            Some(Width::Value(w)) => w,
            _ => self.missing_width,
        };
        (w * self.matrix.as_coeffs()[0] as f32) * UNITS_PER_EM
    }

    pub(crate) fn char_code_to_unicode(&self, char_code: u32) -> Option<BfString> {
        // 1) ToUnicode CMap — the only canonical path.
        if let Some(c) = self
            .to_unicode
            .as_ref()
            .and_then(|t| t.lookup_bf_string(char_code))
            && c != BfString::Char('\0')
        {
            return Some(c);
        }

        // Type3 fonts do carry an /Encoding dictionary (read into
        // `self.encoding` and `self.encodings` via `read_encoding`), so the
        // absence of a ToUnicode CMap does not have to be terminal — many
        // embedded OCR/bitmap Type3 fonts label their glyphs with AGL-
        // compatible names ("A", "space", "hyphen") and extraction is
        // recoverable via the same fallback chain we apply for Type1
        // (#937). Without this, a `pdfluent text` on e.g. NTIA/Teledyne
        // telecommunication papers (`general_858_858662.pdf`) comes back
        // as only `--- Page N ---` markers.
        let code = char_code as u8;

        // 2) /Encoding /Differences — a Type3-specific glyph name.
        //    Try the literal name first, then its normalized alias.
        if let Some(name) = self.encodings.get(&code)
            && let Some(ch) = glyph_name_to_unicode(name)
                .or_else(|| glyph_name_to_unicode(normalized_glyph_name(name)))
        {
            return Some(BfString::Char(ch));
        }

        // 3) Base encoding (Standard / WinAnsi / MacRoman / MacExpert) →
        //    AGL. For `Encoding::BuiltIn` this returns None, so we fall
        //    through to the ASCII identity below.  Try normalized alias too.
        if let Some(name) = self.encoding.map_code(code)
            && let Some(ch) = glyph_name_to_unicode(name)
                .or_else(|| glyph_name_to_unicode(normalized_glyph_name(name)))
        {
            return Some(BfString::Char(ch));
        }

        // 4) Adobe Standard as last-resort encoding guess. Most legacy
        //    Type3 OCR fonts layout ASCII-compatible content even with
        //    `Encoding::BuiltIn`.
        if let Some(name) = Encoding::Standard.map_code(code)
            && let Some(ch) = glyph_name_to_unicode(name)
        {
            return Some(BfString::Char(ch));
        }

        // 5) Printable-ASCII identity. Emitting a possibly-wrong character
        //    is strictly better than dropping the glyph.
        if (0x20..=0x7E).contains(&code) {
            return Some(BfString::Char(code as char));
        }

        None
    }

    pub(crate) fn render_glyph(
        &self,
        glyph: &Type3Glyph<'a>,
        transform: Affine,
        glyph_transform: Affine,
        paint: &Paint<'a>,
        device: &mut impl Device<'a>,
    ) -> Option<()> {
        let mut state = glyph.state.clone();
        let root_transform =
            transform * glyph_transform * self.matrix * Affine::scale(UNITS_PER_EM as f64);
        state.ctm = root_transform;

        // Not sure if this is mentioned anywhere, but I do think we need to reset the text state
        // (though the graphics state itself should be preserved).
        state.text_state = TextState::default();

        let name = self.glyph_simulator.glyph_to_string(glyph.glyph_id)?;
        let program = self.char_procs.get(&name)?;
        let decoded = decode_or_warn(program, &glyph.settings.warning_sink)?;
        let iter = TypedIter::new(decoded.as_ref());

        // Every valid Type3 glyph stream must begin with either:
        //   d0  (ColorGlyph)  — colored glyph: the stream defines its own colors.
        //   d1  (ShapeGlyph)  — uncolored/shape glyph: drawn in current fill color.
        //
        // We scan for the first occurrence.  Default is `false` (colored / d0
        // behaviour) so that if the stream is malformed and contains neither
        // operator we still render it with its own colors rather than
        // incorrectly forcing the parent fill color.
        let is_shape_glyph = {
            let iter = iter.clone();
            let mut is_shape_glyph = false;

            for op in iter {
                match op {
                    TypedInstruction::ShapeGlyph(_) => {
                        is_shape_glyph = true;
                        break;
                    }
                    TypedInstruction::ColorGlyph(_) => {
                        // is_shape_glyph stays false
                        break;
                    }
                    _ => {}
                }
            }

            is_shape_glyph
        };

        let mut context = Context::new_with(
            state.ctm,
            self.font_bbox,
            glyph.cache.clone(),
            glyph.xref,
            glyph.settings.clone(),
            state,
        );

        let mut resources = Resources::from_parent(
            self.dict.get(RESOURCES).unwrap_or_default(),
            glyph.parent_resources.clone(),
        );

        // Technically not valid, but also support by Adobe Acrobat. See PDFBOX-5294.
        if let Some(procs_resources) = program.dict().get::<Dict<'_>>(RESOURCES) {
            resources = Resources::from_parent(procs_resources, resources);
        }

        if is_shape_glyph {
            let mut device = Type3ShapeGlyphDevice::new(device, paint.clone());
            interpret(iter, &resources, &mut context, &mut device);
        } else {
            interpret(iter, &resources, &mut context, device);
        }

        Some(())
    }
}

impl CacheKey for Type3<'_> {
    fn cache_key(&self) -> u128 {
        self.dict.cache_key()
    }
}

struct Type3ShapeGlyphDevice<'a, 'b, T: Device<'a>> {
    inner: &'b mut T,
    paint: Paint<'a>,
}

impl<'a, 'b, T: Device<'a>> Type3ShapeGlyphDevice<'a, 'b, T> {
    pub(crate) fn new(device: &'b mut T, paint: Paint<'a>) -> Self {
        Self {
            inner: device,
            paint,
        }
    }
}

// Only filling, stroking of paths and stencil masks are allowed.
impl<'a, T: Device<'a>> Device<'a> for Type3ShapeGlyphDevice<'a, '_, T> {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}

    fn draw_path(
        &mut self,
        path: &BezPath,
        transform: Affine,
        _: &Paint<'_>,
        draw_mode: &PathDrawMode,
    ) {
        self.inner
            .draw_path(path, transform, &self.paint, draw_mode);
    }

    fn push_clip_path(&mut self, clip_path: &ClipPath) {
        self.inner.push_clip_path(clip_path);
    }

    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}

    fn draw_glyph(
        &mut self,
        g: &Glyph<'a>,
        transform: Affine,
        glyph_transform: Affine,
        p: &Paint<'a>,
        draw_mode: &GlyphDrawMode,
    ) {
        self.inner
            .draw_glyph(g, transform, glyph_transform, p, draw_mode);
    }

    fn pop_clip_path(&mut self) {
        self.inner.pop_clip_path();
    }

    fn pop_transparency_group(&mut self) {}

    fn draw_image(&mut self, image: Image<'a, '_>, transform: Affine) {
        if let Image::Stencil(mut s) = image {
            s.paint = self.paint.clone();
            self.inner.draw_image(Image::Stencil(s), transform);
        }
    }

    fn set_blend_mode(&mut self, _: BlendMode) {}
}
