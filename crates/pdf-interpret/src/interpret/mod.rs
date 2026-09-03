use crate::FillRule;
use crate::color::ColorSpace;
use crate::context::Context;
use crate::convert::{convert_line_cap, convert_line_join};
use crate::device::Device;
use crate::font::{Font, FontData, FontQuery, StandardFont};
use crate::interpret::path::{
    close_path, fill_path, fill_path_impl, fill_stroke_path, stroke_path,
};
use crate::interpret::state::{TextStateFont, handle_gs};
use crate::interpret::text::TextRenderingMode;
use crate::pattern::{Pattern, ShadingPattern};
use crate::shading::Shading;
use crate::util::{OptionLog, RectExt};
use crate::x_object::{
    FormXObject, ImageXObject, XObject, draw_form_xobject, draw_image_xobject, draw_xobject,
};
use kurbo::{Affine, Point, Shape};
use log::warn;
use pdf_syntax::content::ops::TypedInstruction;
use pdf_syntax::object::dict::keys::{ANNOTS, AP, AS, F, FT, MCID, N, OC, PARENT, RECT, V};
use pdf_syntax::object::{Array, Dict, Name, Object, Rect, Stream, dict_or_stream};
use pdf_syntax::page::{Page, Resources};
use smallvec::smallvec;
use std::sync::{Arc, OnceLock};

pub(crate) mod path;
pub(crate) mod state;
pub(crate) mod text;

pub use state::ActiveTransferFunction;

/// A callback function for resolving font queries.
///
/// The first argument is the raw data, the second argument is the index in case the font
/// is a TTC, otherwise it should be 0.
pub type FontResolverFn = Arc<dyn Fn(&FontQuery) -> Option<(FontData, u32)> + Send + Sync>;
/// A callback function for resolving cmap names to their files.
pub type CMapResolverFn =
    Arc<dyn Fn(pdf_font::cmap::CMapName<'_>) -> Option<&'static [u8]> + Send + Sync>;
/// A callback function for resolving warnings during interpretation.
pub type WarningSinkFn = Arc<dyn Fn(InterpreterWarning) + Send + Sync>;

#[derive(Clone)]
/// Settings that should be applied during the interpretation process.
pub struct InterpreterSettings {
    /// Nearly every PDF contains text. In most cases, PDF files embed the fonts they use, and
    /// pdf-interpret can therefore read the font files and do all the processing needed. However, there
    /// are two problems:
    /// - Fonts don't _have_ to be embedded, it's possible that the PDF file only defines the basic
    ///   metadata of the font, like its name, but relies on the PDF processor to find that font
    ///   in its environment.
    /// - The PDF specification requires a list of 14 fonts that should always be available to a
    ///   PDF processor. These include:
    ///   - Times New Roman (Normal, Bold, Italic, `BoldItalic`)
    ///   - Courier (Normal, Bold, Italic, `BoldItalic`)
    ///   - Helvetica (Normal, Bold, Italic, `BoldItalic`)
    ///   - `ZapfDingBats`
    ///   - Symbol
    ///
    /// Because of this, if any of the above situations occurs, this callback will be called, which
    /// expects the data of an appropriate font to be returned, if available. If no such font is
    /// provided, the text will most likely fail to render.
    ///
    /// For the font data, there are two different formats that are accepted:
    /// - Any valid TTF/OTF font.
    /// - A valid CFF font program.
    ///
    /// The following recommendations are given for the implementation of this callback function.
    ///
    /// For the standard fonts, in case the original fonts are available on the system, you should
    /// just return those. Otherwise, for Helvetica, Courier and Times New Roman, the best alternative
    /// are the corresponding fonts of the [Liberation font family](https://github.com/liberationfonts/liberation-fonts).
    /// If you prefer smaller fonts, you can use the [Foxit CFF fonts](https://github.com/LaurenzV/pdf-interpret/tree/master/assets/standard_fonts),
    /// which are much smaller but are missing glyphs for certain scripts.
    ///
    /// For the `Symbol` and `ZapfDingBats` fonts, you should also prefer the system fonts, and if
    /// not available to you, you can, similarly to above, use the corresponding fonts from Foxit.
    ///
    /// If you don't want having to deal with this, you can just enable the `embed-fonts` feature
    /// and use the default implementation of the callback.
    pub font_resolver: FontResolverFn,
    /// A callback for resolving cmaps that aren't embedded.
    ///
    /// When the PDF requires using a cmap that is not directly embedded in the PDF,
    /// this callback will be called to attempt fetching the data of the file.
    ///
    /// When the `embed-cmaps` feature is enabled, this uses `load_embedded`
    /// method from `pdf-interpret-cmap` by default, which embeds the cmap files for
    /// all 61 predefined cmaps
    /// that the PDF specification requires to be readily available on a system.
    /// Otherwise, you can implement your custom logic for lazily fetching the
    /// data. If you are fine not supporting such PDFs, you can simply pass a closure
    /// that always returns `None`.
    pub cmap_resolver: CMapResolverFn,
    /// In certain cases, `pdf-interpret` will emit a warning in case an issue was encountered while interpreting
    /// the PDF file. Providing a callback allows you to catch those warnings and handle them, if desired.
    pub warning_sink: WarningSinkFn,
    /// Whether annotations should be rendered as well.
    ///
    /// Note that this feature is currently not fully implemented yet, so some
    /// annotations might be missing.
    pub render_annotations: bool,
    /// Whether to skip `/FT /Sig` (signature widget) appearance streams.
    ///
    /// Rendering sets this to `true` to match MuPDF behaviour, but text
    /// extraction should set it to `false` so that signature text is included.
    pub skip_signature_widgets: bool,
    /// Maximum number of content-stream operators to interpret.
    ///
    /// `None` preserves the historical unlimited behavior for callers that do
    /// not configure processing limits.
    pub max_operator_count: Option<u64>,
    /// A shared cache to reuse between page interpretations (specifically for images).
    pub shared_cache: Option<crate::Cache>,
}

/// Known paths for CJK fonts, ordered by preference.
/// Covers macOS, Ubuntu/Debian, Fedora/RHEL, and Alpine Linux.
#[cfg(feature = "embed-fonts")]
const CJK_FONT_CANDIDATE_PATHS: &[&str] = &[
    // macOS — ships with every installation
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    // Noto CJK — most common on Linux
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJKsc-Regular.otf",
    // WenQuanYi — fallback on older Ubuntu/Debian systems
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    // Arphic (traditional)
    "/usr/share/fonts/truetype/arphic/uming.ttc",
    // Alpine Linux
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
];

/// Lazily loaded CJK system font bytes.  `None` means no CJK font was found.
#[cfg(feature = "embed-fonts")]
static SYSTEM_CJK_FONT: OnceLock<Option<Arc<Vec<u8>>>> = OnceLock::new();

/// Try to load a CJK font from the host system, returning its raw bytes.
#[cfg(feature = "embed-fonts")]
fn system_cjk_font() -> Option<FontData> {
    SYSTEM_CJK_FONT
        .get_or_init(|| {
            for path in CJK_FONT_CANDIDATE_PATHS {
                if let Ok(bytes) = std::fs::read(path) {
                    log::debug!("CJK fallback font loaded from {path}");
                    return Some(Arc::new(bytes));
                }
            }
            log::warn!(
                "no system CJK font found; non-embedded CJK fonts will render with a Latin fallback"
            );
            None
        })
        .as_ref()
        .map(|data| -> FontData { data.clone() })
}

impl Default for InterpreterSettings {
    fn default() -> Self {
        Self {
            #[cfg(not(feature = "embed-fonts"))]
            font_resolver: Arc::new(|_| None),
            #[cfg(feature = "embed-fonts")]
            font_resolver: Arc::new(|query| match query {
                FontQuery::Standard(s) => Some(s.get_font_data()),
                FontQuery::Fallback(f) => {
                    // For non-embedded CJK fonts (Adobe-GB1, CNS1, Japan1, Korea1)
                    // try a system CJK font first so characters render correctly.
                    // This avoids the situation where a Latin fallback font is used
                    // and Chinese/Japanese/Korean glyphs appear as "d", "a", etc.
                    if f.character_collection
                        .as_ref()
                        .is_some_and(|cc| cc.family.is_cjk())
                        && let Some(data) = system_cjk_font()
                    {
                        return Some((data, 0));
                    }
                    Some(f.pick_standard_font().get_font_data())
                }
            }),
            #[cfg(feature = "embed-cmaps")]
            cmap_resolver: Arc::new(pdf_font::cmap::load_embedded),
            #[cfg(not(feature = "embed-cmaps"))]
            cmap_resolver: Arc::new(|_| None),
            warning_sink: Arc::new(|_| {}),
            render_annotations: true,
            skip_signature_widgets: true,
            max_operator_count: None,
            shared_cache: None,
        }
    }
}

#[derive(Copy, Clone, Debug)]
/// Warnings that can occur while interpreting a PDF file.
pub enum InterpreterWarning {
    /// An unsupported font kind was encountered.
    ///
    /// Currently, only CID fonts with non-identity encoding are unsupported.
    UnsupportedFont,
    /// An image failed to decode.
    ImageDecodeFailure,
    /// A stream exceeded the configured `max_stream_bytes` cap during
    /// image decode.  Must not be silently discarded — propagate as
    /// `LimitError::StreamTooLarge` / `Error::ResourceLimitExceeded`.
    ///
    /// Both fields are `u64` so the variant remains `Copy`.
    StreamTooLarge {
        /// Observed decompressed size in bytes.
        observed: u64,
        /// Configured limit in bytes.
        limit: u64,
    },
}

/// Resolve the normal (`/N`) appearance stream of an annotation.
///
/// Per ISO 32000 §12.5.5 and Table 168, the `/N` entry of the `/AP`
/// dictionary is either an appearance stream or an appearance *subdictionary*
/// mapping appearance-state names to streams (the latter is used by every
/// checkbox and radio button, e.g. `/N << /Yes <stream> /Off <stream> >>`).
///
/// In the subdictionary case the stream is selected by the annotation's
/// `/AS` entry (Table 168: "The annotation's appearance state, which
/// selects the applicable appearance stream from an appearance
/// subdictionary"). When `/AS` is absent, this follows pdfium's
/// `GetAnnotAPInternal` fallback: the widget's own `/V` value as a name,
/// then the `/Parent`'s `/V` (one level), accepting a candidate only if it
/// exists as a key in the subdictionary. If no candidate resolves to an
/// existing key, `None` is returned and nothing is rendered for the
/// annotation (correct for e.g. `/AS /Off` when the subdictionary has no
/// `/Off` entry).
///
/// All key matching is done on raw name bytes — appearance-state names may
/// contain non-ASCII bytes and must never go through a lossy UTF-8
/// conversion.
fn normal_appearance_stream<'a>(annot: &Dict<'a>) -> Option<Stream<'a>> {
    let ap = annot.get::<Dict<'_>>(AP)?;

    // Single appearance stream: use it directly.
    if let Some(stream) = ap.get::<Stream<'_>>(N) {
        return Some(stream);
    }

    // Appearance subdictionary: select the stream by appearance state.
    let states = ap.get::<Dict<'_>>(N)?;

    if let Some(state) = annot.get::<Name>(AS) {
        // An explicit /AS is authoritative; if its entry is missing, no
        // appearance is rendered.
        return states.get::<Stream<'_>>(state.as_ref());
    }

    // pdfium V-fallback: the widget's own /V, then the parent's /V, the
    // first candidate that exists as a key in the subdictionary wins.
    let candidates = [
        annot.get::<Name>(V),
        annot.get::<Dict<'_>>(PARENT).and_then(|p| p.get::<Name>(V)),
    ];

    candidates
        .into_iter()
        .flatten()
        .find_map(|state| states.get::<Stream<'_>>(state.as_ref()))
}

/// interpret the contents of the page and render them into the device.
pub fn interpret_page<'a>(
    page: &Page<'a>,
    context: &mut Context<'a>,
    device: &mut impl Device<'a>,
) {
    let resources = page.resources();
    interpret(page.typed_operations(), resources, context, device);

    if context.settings.render_annotations
        && let Some(annot_arr) = page.raw().get::<Array<'_>>(ANNOTS)
    {
        for annot in annot_arr.iter::<Dict<'_>>() {
            let flags = annot.get::<u32>(F).unwrap_or(0);

            // Annotation should be hidden.
            if flags & 2 != 0 {
                continue;
            }

            // MuPDF renders signature widgets (/FT /Sig) with its own built-in
            // "SIGN here" indicator and ignores the custom /AP/N stream, so we
            // skip AP rendering for these annotations to match MuPDF output.
            // Text extraction disables this skip so signature text is included.
            if context.settings.skip_signature_widgets
                && annot
                    .get::<Name>(FT)
                    .as_deref()
                    .is_some_and(|n| n == b"Sig")
            {
                continue;
            }

            if let Some(apx) = normal_appearance_stream(&annot)
                .and_then(|o| FormXObject::new(&o, &context.settings.warning_sink))
            {
                let Some(rect) = annot.get::<Rect>(RECT) else {
                    continue;
                };

                let annot_rect = rect.to_kurbo();
                // 12.5.5. Appearance streams
                // "The algorithm outlined in this subclause shall be used
                // to map from the coordinate system of the appearance XObject."

                // 1) The appearance’s bounding box (specified by its BBox entry)
                // shall be transformed, using Matrix, to produce a
                // quadrilateral with arbitrary orientation. The transformed
                // appearance box is the smallest upright rectangle that
                // encompasses this quadrilateral.
                let transformed_rect = (apx.matrix
                    * kurbo::Rect::new(
                        apx.bbox[0] as f64,
                        apx.bbox[1] as f64,
                        apx.bbox[2] as f64,
                        apx.bbox[3] as f64,
                    )
                    .to_path(0.1))
                .bounding_box();

                // A degenerate (zero-width or zero-height) transformed
                // appearance box would make the scale computation below
                // divide by zero, producing a non-finite (inf/NaN) affine.
                // Skip such annotations entirely.
                let (tw, th) = (transformed_rect.width(), transformed_rect.height());
                if !(tw.is_finite() && tw > 0.0 && th.is_finite() && th > 0.0) {
                    continue;
                }

                // 2) A matrix A shall be computed that scales and translates
                // the transformed appearance box to align with the edges
                // of the annotation’s rectangle (specified by the Rect entry).
                // A maps the lower-left corner (the corner with the smallest
                // x and y coordinates) and the upper-right corner (the
                // corner with the greatest x and y coordinates) of the
                // transformed appearance box to the corresponding corners
                // of the annotation’s rectangle.
                let affine = Affine::new([
                    annot_rect.width() / transformed_rect.width(),
                    0.0,
                    0.0,
                    annot_rect.height() / transformed_rect.height(),
                    annot_rect.x0 - transformed_rect.x0,
                    annot_rect.y0 - transformed_rect.y0,
                ]);

                // 3) Matrix shall be concatenated with A to form a matrix
                // AA that maps from the appearance’s coordinate system to
                // the annotation’s rectangle in default user space.
                context.save_state();
                context.pre_concat_affine(affine);
                context.push_root_transform();

                draw_form_xobject(resources, &apx, context, device);
                context.pop_root_transform();
                context.restore_state(device);
            }
        }
    }
}

/// Interpret the instructions from `ops` and render them into the device.
pub fn interpret<'a, 'b>(
    ops: impl Iterator<Item = TypedInstruction<'b>>,
    resources: &Resources<'a>,
    context: &mut Context<'a>,
    device: &mut impl Device<'a>,
) {
    // One choke point for every nested interpretation, because every one of them
    // arrives here: a Form XObject drawing another, a tiling pattern painting
    // with a pattern, a soft mask, a Type 3 glyph procedure. Guarding the call
    // sites instead would mean four guards and a fifth construct next year.
    //
    // Reproduced before the bound existed: a 639-byte file whose XObject `/X1`
    // contains `q /X1 Do Q` aborts the process with a stack overflow, rc=134.
    // No XFA and no script, so it reaches every binding exporting `render_page`.
    if !context.begin_nested_interpretation() {
        warn!(
            "content stream nesting exceeds {}, stopping interpretation",
            crate::context::MAX_NESTED_INTERPRETATION_DEPTH
        );

        return;
    }

    let num_states = context.num_states();
    let max_operator_count = context.settings.max_operator_count.unwrap_or(u64::MAX);
    let mut operator_count = 0_u64;

    context.save_state();

    for op in ops {
        operator_count = operator_count.saturating_add(1);
        if operator_count > max_operator_count {
            warn!(
                "content stream operator count exceeds {max_operator_count}, stopping interpretation"
            );
            break;
        }

        match op {
            TypedInstruction::SaveState(_) => context.save_state(),
            TypedInstruction::StrokeColorDeviceRgb(s) => {
                context.get_mut().graphics_state.stroke_cs = ColorSpace::device_rgb();
                context.get_mut().graphics_state.stroke_color =
                    smallvec![s.0.as_f32(), s.1.as_f32(), s.2.as_f32()];
            }
            TypedInstruction::StrokeColorDeviceGray(s) => {
                context.get_mut().graphics_state.stroke_cs = ColorSpace::device_gray();
                context.get_mut().graphics_state.stroke_color = smallvec![s.0.as_f32()];
            }
            TypedInstruction::StrokeColorCmyk(s) => {
                context.get_mut().graphics_state.stroke_cs = ColorSpace::device_cmyk();
                context.get_mut().graphics_state.stroke_color =
                    smallvec![s.0.as_f32(), s.1.as_f32(), s.2.as_f32(), s.3.as_f32()];
            }
            TypedInstruction::LineWidth(w) => {
                context.get_mut().graphics_state.stroke_props.line_width = w.0.as_f32();
            }
            TypedInstruction::LineCap(c) => {
                context.get_mut().graphics_state.stroke_props.line_cap = convert_line_cap(c);
            }
            TypedInstruction::LineJoin(j) => {
                context.get_mut().graphics_state.stroke_props.line_join = convert_line_join(j);
            }
            TypedInstruction::MiterLimit(l) => {
                context.get_mut().graphics_state.stroke_props.miter_limit = l.0.as_f32();
            }
            TypedInstruction::Transform(t) => {
                context.pre_concat_transform(t);
            }
            TypedInstruction::RectPath(r) => {
                let rect = kurbo::Rect::new(
                    r.0.as_f64(),
                    r.1.as_f64(),
                    r.0.as_f64() + r.2.as_f64(),
                    r.1.as_f64() + r.3.as_f64(),
                )
                .to_path(0.1);
                context.path_mut().extend(rect);
            }
            TypedInstruction::MoveTo(m) => {
                let p = Point::new(m.0.as_f64(), m.1.as_f64());
                *(context.last_point_mut()) = p;
                *(context.sub_path_start_mut()) = p;
                context.path_mut().move_to(p);
            }
            TypedInstruction::FillPathEvenOdd(_) => {
                fill_path(context, device, FillRule::EvenOdd);
            }
            TypedInstruction::FillPathNonZero(_) => {
                fill_path(context, device, FillRule::NonZero);
            }
            TypedInstruction::FillPathNonZeroCompatibility(_) => {
                fill_path(context, device, FillRule::NonZero);
            }
            TypedInstruction::FillAndStrokeEvenOdd(_) => {
                fill_stroke_path(context, device, FillRule::EvenOdd);
            }
            TypedInstruction::FillAndStrokeNonZero(_) => {
                fill_stroke_path(context, device, FillRule::NonZero);
            }
            TypedInstruction::CloseAndStrokePath(_) => {
                close_path(context);
                stroke_path(context, device);
            }
            TypedInstruction::CloseFillAndStrokeEvenOdd(_) => {
                close_path(context);
                fill_stroke_path(context, device, FillRule::EvenOdd);
            }
            TypedInstruction::CloseFillAndStrokeNonZero(_) => {
                close_path(context);
                fill_stroke_path(context, device, FillRule::NonZero);
            }
            TypedInstruction::NonStrokeColorDeviceGray(s) => {
                context.get_mut().graphics_state.none_stroke_cs = ColorSpace::device_gray();
                context.get_mut().graphics_state.non_stroke_color = smallvec![s.0.as_f32()];
            }
            TypedInstruction::NonStrokeColorDeviceRgb(s) => {
                context.get_mut().graphics_state.none_stroke_cs = ColorSpace::device_rgb();
                context.get_mut().graphics_state.non_stroke_color =
                    smallvec![s.0.as_f32(), s.1.as_f32(), s.2.as_f32()];
            }
            TypedInstruction::NonStrokeColorCmyk(s) => {
                context.get_mut().graphics_state.none_stroke_cs = ColorSpace::device_cmyk();
                context.get_mut().graphics_state.non_stroke_color =
                    smallvec![s.0.as_f32(), s.1.as_f32(), s.2.as_f32(), s.3.as_f32()];
            }
            TypedInstruction::LineTo(m) => {
                if !context.path().elements().is_empty() {
                    let last_point = *context.last_point();
                    let mut p = Point::new(m.0.as_f64(), m.1.as_f64());
                    *(context.last_point_mut()) = p;
                    if last_point == p {
                        // Add a small delta so that zero width lines can still have a round stroke.
                        p.x += 0.0001;
                    }

                    context.path_mut().line_to(p);
                }
            }
            TypedInstruction::CubicTo(c) => {
                if !context.path().elements().is_empty() {
                    let p1 = Point::new(c.0.as_f64(), c.1.as_f64());
                    let p2 = Point::new(c.2.as_f64(), c.3.as_f64());
                    let p3 = Point::new(c.4.as_f64(), c.5.as_f64());

                    *(context.last_point_mut()) = p3;

                    context.path_mut().curve_to(p1, p2, p3);
                }
            }
            TypedInstruction::CubicStartTo(c) => {
                if !context.path().elements().is_empty() {
                    let p1 = *context.last_point();
                    let p2 = Point::new(c.0.as_f64(), c.1.as_f64());
                    let p3 = Point::new(c.2.as_f64(), c.3.as_f64());

                    *(context.last_point_mut()) = p3;

                    context.path_mut().curve_to(p1, p2, p3);
                }
            }
            TypedInstruction::CubicEndTo(c) => {
                if !context.path().elements().is_empty() {
                    let p2 = Point::new(c.0.as_f64(), c.1.as_f64());
                    let p3 = Point::new(c.2.as_f64(), c.3.as_f64());

                    *(context.last_point_mut()) = p3;

                    context.path_mut().curve_to(p2, p3, p3);
                }
            }
            TypedInstruction::ClosePath(_) => {
                close_path(context);
            }
            TypedInstruction::SetGraphicsState(gs) => {
                if let Some(gs) = resources
                    .get_ext_g_state(gs.0.clone())
                    .warn_none(&format!("failed to get extgstate {}", gs.0.as_str()))
                {
                    handle_gs(&gs, context, resources);
                }
            }
            TypedInstruction::StrokePath(_) => {
                stroke_path(context, device);
            }
            TypedInstruction::EndPath(_) => {
                if let Some(clip) = *context.clip()
                    && !context.path().elements().is_empty()
                {
                    let clip_path = context.get().ctm * context.path().clone();
                    context.push_clip_path(clip_path, clip, device);

                    *(context.clip_mut()) = None;
                }

                context.path_mut().truncate(0);
            }
            TypedInstruction::NonStrokeColor(c) => {
                let fill_c = &mut context.get_mut().graphics_state.non_stroke_color;
                fill_c.truncate(0);

                for e in c.0 {
                    fill_c.push(e.as_f32());
                }
            }
            TypedInstruction::StrokeColor(c) => {
                let stroke_c = &mut context.get_mut().graphics_state.stroke_color;
                stroke_c.truncate(0);

                for e in c.0 {
                    stroke_c.push(e.as_f32());
                }
            }
            TypedInstruction::ClipNonZero(_) => {
                *(context.clip_mut()) = Some(FillRule::NonZero);
            }
            TypedInstruction::ClipEvenOdd(_) => {
                *(context.clip_mut()) = Some(FillRule::EvenOdd);
            }
            TypedInstruction::RestoreState(_) => context.restore_state(device),
            TypedInstruction::FlatnessTolerance(_) => {
                // Ignore for now.
            }
            TypedInstruction::ColorSpaceStroke(c) => {
                let cs = if let Some(named) = ColorSpace::new_from_name(c.0.clone()) {
                    named
                } else {
                    context
                        .get_color_space(resources, c.0)
                        .unwrap_or(ColorSpace::device_gray())
                };

                context.get_mut().graphics_state.stroke_color = cs.initial_color();
                context.get_mut().graphics_state.stroke_cs = cs;
            }
            TypedInstruction::ColorSpaceNonStroke(c) => {
                let cs = if let Some(named) = ColorSpace::new_from_name(c.0.clone()) {
                    named
                } else {
                    context
                        .get_color_space(resources, c.0)
                        .unwrap_or(ColorSpace::device_gray())
                };

                context.get_mut().graphics_state.non_stroke_color = cs.initial_color();
                context.get_mut().graphics_state.none_stroke_cs = cs;
            }
            TypedInstruction::DashPattern(p) => {
                context.get_mut().graphics_state.stroke_props.dash_offset = p.1.as_f32();
                // kurbo apparently cannot properly deal with offsets that are exactly 0.
                context.get_mut().graphics_state.stroke_props.dash_array =
                    p.0.iter::<f32>()
                        .map(|n| if n == 0.0 { 0.01 } else { n })
                        .collect();
            }
            TypedInstruction::RenderingIntent(_) => {
                // Ignore for now.
            }
            TypedInstruction::NonStrokeColorNamed(n) => {
                context.get_mut().graphics_state.non_stroke_color =
                    n.0.into_iter().map(|n| n.as_f32()).collect();
                context.get_mut().graphics_state.non_stroke_pattern = n.1.and_then(|name| {
                    resources
                        .get_pattern(name)
                        .and_then(|d| Pattern::new(d, context, resources))
                });
            }
            TypedInstruction::StrokeColorNamed(n) => {
                context.get_mut().graphics_state.stroke_color =
                    n.0.into_iter().map(|n| n.as_f32()).collect();
                context.get_mut().graphics_state.stroke_pattern = n.1.and_then(|name| {
                    resources
                        .get_pattern(name)
                        .and_then(|d| Pattern::new(d, context, resources))
                });
            }
            TypedInstruction::BeginMarkedContentWithProperties(bdc) => {
                // Properties can be either:
                // 1. A Name that references an entry in the Resources/Properties dictionary
                // 2. An inline dictionary with an OC key

                let mcid = dict_or_stream(&bdc.1).and_then(|(props, _)| props.get::<i32>(MCID));

                let oc = bdc
                    .1
                    .clone()
                    .into_name()
                    .and_then(|name| {
                        let r = resources.properties.get_ref(name.clone())?;
                        let d = resources
                            .properties
                            .get::<Dict<'_>>(name)
                            .unwrap_or_default();
                        Some((d, r))
                    })
                    .or_else(|| {
                        let (props, _) = dict_or_stream(&bdc.1)?;
                        let r = props.get_ref(OC)?;
                        let d = props.get::<Dict<'_>>(OC).unwrap_or_default();
                        Some((d, r))
                    });

                if let Some((dict, oc_ref)) = oc {
                    context.ocg_state.begin_ocg(&dict, oc_ref.into());
                } else {
                    context.ocg_state.begin_marked_content();
                }

                device.begin_marked_content(&bdc.0, mcid);
            }
            TypedInstruction::MarkedContentPointWithProperties(_) => {}
            TypedInstruction::EndMarkedContent(_) => {
                context.ocg_state.end_marked_content();
                device.end_marked_content();
            }
            TypedInstruction::MarkedContentPoint(_) => {}
            TypedInstruction::BeginMarkedContent(bmc) => {
                context.ocg_state.begin_marked_content();
                device.begin_marked_content(&bmc.0, None);
            }
            TypedInstruction::BeginText(_) => {
                context.get_mut().text_state.text_matrix = Affine::IDENTITY;
                context.get_mut().text_state.text_line_matrix = Affine::IDENTITY;
            }
            TypedInstruction::SetTextMatrix(m) => {
                let m = Affine::new([
                    m.0.as_f64(),
                    m.1.as_f64(),
                    m.2.as_f64(),
                    m.3.as_f64(),
                    m.4.as_f64(),
                    m.5.as_f64(),
                ]);
                context.get_mut().text_state.text_line_matrix = m;
                context.get_mut().text_state.text_matrix = m;
            }
            TypedInstruction::EndText(_) => {
                let has_outline = context
                    .get()
                    .text_state
                    .clip_paths
                    .segments()
                    .next()
                    .is_some();

                if has_outline {
                    let clip_path = context.get().ctm * context.get().text_state.clip_paths.clone();

                    context.push_clip_path(clip_path, FillRule::NonZero, device);
                }

                context.get_mut().text_state.clip_paths.truncate(0);
            }
            TypedInstruction::TextFont(t) => {
                let name = t.0;

                // In case we are unable to resolve the font, two scenarios:
                // 1) If the font doesn't exist in the first place in the resource dictionary,
                // assume Helvetica (this seems to be what other PDF viewers do).
                // 2) In case it's `None` because we were unable to resolve the font
                // (for whatever reason), leave it as `None`. Better showing no
                // text at all than garbage text.
                let font = if let Some(font_dict) = resources.get_font(name.clone()) {
                    context.resolve_font(&font_dict)
                } else {
                    Font::new_standard(StandardFont::Helvetica, &context.settings.font_resolver)
                        .map(TextStateFont::Fallback)
                };

                context.get_mut().text_state.font_size = t.1.as_f32();
                context.get_mut().text_state.font = font;
            }
            TypedInstruction::ShowText(s) => {
                if context.get().text_state.font.is_none() {
                    // Even if no explicit font was set, we try to assume Helvetica. Acrobat
                    // seems to do the same.
                    context.get_mut().text_state.font = Font::new_standard(
                        StandardFont::Helvetica,
                        &context.settings.font_resolver,
                    )
                    .map(TextStateFont::Fallback);
                }

                text::show_text_string(context, device, resources, s.0);
            }
            TypedInstruction::ShowTexts(s) => {
                if context.get().text_state.font.is_none() {
                    // Even if no explicit font was set, we try to assume Helvetica. Acrobat
                    // seems to do the same.
                    context.get_mut().text_state.font = Font::new_standard(
                        StandardFont::Helvetica,
                        &context.settings.font_resolver,
                    )
                    .map(TextStateFont::Fallback);
                }

                for obj in s.0.iter::<Object<'_>>() {
                    if let Some(adjustment) = obj.clone().into_f32() {
                        // ANN[r17/TEX1] Surface TJ adjustment to the Device
                        // before mutating the text matrix so extractors can
                        // record the word-boundary signal alongside the
                        // spatial gap they'd otherwise have to infer.
                        device.text_adjustment(adjustment);
                        context.get_mut().text_state.apply_adjustment(adjustment);
                    } else if let Some(text) = obj.into_string() {
                        text::show_text_string(context, device, resources, text);
                    }
                }
            }
            TypedInstruction::HorizontalScaling(h) => {
                context.get_mut().text_state.horizontal_scaling = h.0.as_f32();
            }
            TypedInstruction::TextLeading(tl) => {
                context.get_mut().text_state.leading = tl.0.as_f32();
            }
            TypedInstruction::CharacterSpacing(c) => {
                context.get_mut().text_state.char_space = c.0.as_f32();
            }
            TypedInstruction::WordSpacing(w) => {
                context.get_mut().text_state.word_space = w.0.as_f32();
            }
            TypedInstruction::NextLine(n) => {
                let (tx, ty) = (n.0.as_f64(), n.1.as_f64());
                text::next_line(context, tx, ty);
            }
            TypedInstruction::NextLineUsingLeading(_) => {
                text::next_line(context, 0.0, -context.get().text_state.leading as f64);
            }
            TypedInstruction::NextLineAndShowText(n) => {
                text::next_line(context, 0.0, -context.get().text_state.leading as f64);
                text::show_text_string(context, device, resources, n.0);
            }
            TypedInstruction::TextRenderingMode(r) => {
                let mode = match r.0.as_i64() {
                    0 => TextRenderingMode::Fill,
                    1 => TextRenderingMode::Stroke,
                    2 => TextRenderingMode::FillStroke,
                    3 => TextRenderingMode::Invisible,
                    4 => TextRenderingMode::FillAndClip,
                    5 => TextRenderingMode::StrokeAndClip,
                    6 => TextRenderingMode::FillAndStrokeAndClip,
                    7 => TextRenderingMode::Clip,
                    _ => {
                        warn!("unknown text rendering mode {}", r.0.as_i64());

                        TextRenderingMode::Fill
                    }
                };

                context.get_mut().text_state.render_mode = mode;
            }
            TypedInstruction::NextLineAndSetLeading(n) => {
                let (tx, ty) = (n.0.as_f64(), n.1.as_f64());
                context.get_mut().text_state.leading = -ty as f32;
                text::next_line(context, tx, ty);
            }
            // d1: uncolored (shape) glyph header.  The advance width (wx) and
            // bounding-box arguments are intentionally ignored here: the glyph
            // advance is taken from the Type3 font's /Widths array (via
            // Font::code_advance), and the is_shape_glyph flag is determined
            // by the pre-scan in Type3::render_glyph before the stream is
            // interpreted.
            TypedInstruction::ShapeGlyph(_) => {}
            TypedInstruction::XObject(x) => {
                let cache = context.object_cache.clone();
                let transfer_function = context.get().graphics_state.transfer_function.clone();
                if let Some(x_object) = resources.get_x_object(x.0).and_then(|s| {
                    XObject::new(
                        &s,
                        &context.settings.warning_sink,
                        &cache,
                        transfer_function.clone(),
                    )
                }) {
                    draw_xobject(&x_object, resources, context, device);
                }
            }
            TypedInstruction::InlineImage(i) => {
                let warning_sink = context.settings.warning_sink.clone();
                let transfer_function = context.get().graphics_state.transfer_function.clone();
                let cache = context.object_cache.clone();
                if let Some(x_object) = ImageXObject::new(
                    &i.0,
                    |name| context.get_color_space(resources, name.clone()),
                    &warning_sink,
                    &cache,
                    false,
                    transfer_function,
                ) {
                    draw_image_xobject(&x_object, context, device);
                }
            }
            TypedInstruction::TextRise(t) => {
                context.get_mut().text_state.rise = t.0.as_f32();
            }
            TypedInstruction::Shading(s) => {
                if !context.ocg_state.is_visible() {
                    continue;
                }

                let transfer_function = context.get().graphics_state.transfer_function.clone();

                if let Some(sp) = resources
                    .get_shading(s.0)
                    .and_then(|o| dict_or_stream(&o))
                    .and_then(|s| {
                        Shading::new(
                            &s.0,
                            s.1.as_ref(),
                            &context.object_cache,
                            &context.settings.warning_sink,
                        )
                    })
                    .map(|s| {
                        Pattern::Shading(ShadingPattern {
                            shading: Arc::new(s),
                            matrix: Affine::IDENTITY,
                            opacity: context.get().graphics_state.non_stroke_alpha,
                            transfer_function: transfer_function.clone(),
                        })
                    })
                {
                    context.save_state();
                    context.push_root_transform();
                    let st = context.get_mut();
                    st.graphics_state.non_stroke_pattern = Some(sp);
                    st.graphics_state.none_stroke_cs = ColorSpace::pattern();

                    device.set_soft_mask(st.graphics_state.soft_mask.clone());
                    device.set_blend_mode(st.graphics_state.blend_mode);

                    let bbox = context.bbox().to_path(0.1);
                    let inverted_bbox = context.get().ctm.inverse() * bbox;
                    fill_path_impl(context, device, FillRule::NonZero, Some(&inverted_bbox));

                    context.pop_root_transform();
                    context.restore_state(device);
                } else {
                    warn!("failed to process shading");
                }
            }
            TypedInstruction::BeginCompatibility(_) => {}
            TypedInstruction::EndCompatibility(_) => {}
            // d0: colored glyph header.  The advance width (wx) argument is
            // intentionally ignored here for the same reason as d1 above.
            TypedInstruction::ColorGlyph(_) => {}
            TypedInstruction::ShowTextWithParameters(t) => {
                context.get_mut().text_state.word_space = t.0.as_f32();
                context.get_mut().text_state.char_space = t.1.as_f32();
                text::next_line(context, 0.0, -context.get().text_state.leading as f64);
                text::show_text_string(context, device, resources, t.2);
            }
            _ => {
                warn!("failed to read an operator");
            }
        }
    }

    while context.num_states() > num_states {
        context.restore_state(device);
    }

    // Released here rather than on a guard object because every exit from this
    // function is this line: the early `return` above happens before the claim.
    // A counter that only rose would refuse the fifty-first XObject a page draws
    // in sequence, which is not nesting at all.
    context.end_nested_interpretation();
}

#[cfg(test)]
mod tests {
    use crate::device::Device;
    use crate::font::Glyph;
    use crate::soft_mask::SoftMask;
    use crate::util::PageExt;
    use crate::{
        BlendMode, ClipPath, Context, GlyphDrawMode, Image, InterpreterSettings, Paint,
        PathDrawMode, interpret_page,
    };
    use kurbo::{Affine, BezPath, Shape};
    use pdf_syntax::Pdf;

    /// A device that records the bounding-box width (in path coordinates) of
    /// every filled/stroked path, so tests can assert exactly which appearance
    /// stream's marks were interpreted.
    #[derive(Default)]
    struct CountingDevice {
        path_widths: Vec<f64>,
    }

    impl Device<'_> for CountingDevice {
        fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
        fn set_blend_mode(&mut self, _: BlendMode) {}
        fn draw_path(&mut self, path: &BezPath, _: Affine, _: &Paint<'_>, _: &PathDrawMode) {
            self.path_widths.push(path.bounding_box().width());
        }
        fn push_clip_path(&mut self, _: &ClipPath) {}
        fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
        fn draw_glyph(
            &mut self,
            _: &Glyph<'_>,
            _: Affine,
            _: Affine,
            _: &Paint<'_>,
            _: &GlyphDrawMode,
        ) {
        }
        fn draw_image(&mut self, _: Image<'_, '_>, _: Affine) {}
        fn pop_clip_path(&mut self) {}
        fn pop_transparency_group(&mut self) {}
    }

    /// Assemble a PDF from numbered object bodies (index `i` becomes object
    /// `i + 1`), computing byte-accurate xref offsets.
    fn build_pdf(objects: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::with_capacity(objects.len());

        for (i, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        }

        let xref_pos = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );

        out
    }

    /// Build a Form XObject stream object body.
    fn form_stream(bbox: &str, content: &str) -> Vec<u8> {
        format!(
            "<< /Type /XObject /Subtype /Form /BBox {bbox} /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        )
        .into_bytes()
    }

    /// Build a single-page PDF with one widget annotation.
    ///
    /// Object layout: 1 catalog, 2 page tree, 3 page, 4 the annotation
    /// (`annot_body`), 5 the "on" appearance stream (two fills, path widths
    /// 10 and 4), 6 the "off" appearance stream (one fill, path width 7),
    /// 7 empty page contents, 8.. `extra_objects`. The "on" stream's BBox is
    /// `on_bbox` so degenerate-BBox behaviour can be exercised.
    fn checkbox_pdf(annot_body: &[u8], on_bbox: &str, extra_objects: &[Vec<u8>]) -> Vec<u8> {
        let mut objects = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] \
              /Annots [4 0 R] /Contents 7 0 R >>"
                .to_vec(),
            annot_body.to_vec(),
            form_stream(on_bbox, "0 0 10 10 re f\n12 12 4 4 re f"),
            form_stream("[0 0 20 20]", "0 0 7 7 re f"),
            b"<< /Length 0 >>\nstream\n\nendstream".to_vec(),
        ];
        objects.extend_from_slice(extra_objects);
        build_pdf(&objects)
    }

    /// Interpret the first page of `pdf_bytes` and return the recorded path
    /// widths.
    fn interpret_widths(pdf_bytes: Vec<u8>) -> Vec<f64> {
        let pdf = Pdf::new(pdf_bytes).expect("test PDF must parse");
        let pages = pdf.pages();
        let page = pages.first().expect("test PDF must have one page");

        let settings = InterpreterSettings::default();
        let initial_transform = page.initial_transform(true);
        let bbox = kurbo::Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut context = Context::new(initial_transform, bbox, page.xref(), settings);
        let mut device = CountingDevice::default();

        interpret_page(page, &mut context, &mut device);
        device.path_widths
    }

    fn assert_widths(widths: &[f64], expected: &[f64]) {
        assert_eq!(
            widths.len(),
            expected.len(),
            "expected {expected:?}, got {widths:?}"
        );
        for (got, want) in widths.iter().zip(expected) {
            assert!(
                (got - want).abs() < 1e-6,
                "expected {expected:?}, got {widths:?}"
            );
        }
    }

    /// /AP /N substate dictionary with /AS /Yes: the Yes stream (and only the
    /// Yes stream) must be drawn.
    #[test]
    fn widget_substate_as_on_state() {
        let pdf = checkbox_pdf(
            b"<< /Type /Annot /Subtype /Widget /FT /Btn /Rect [10 10 30 30] \
              /AP << /N << /Yes 5 0 R /Off 6 0 R >> >> /AS /Yes >>",
            "[0 0 20 20]",
            &[],
        );
        assert_widths(&interpret_widths(pdf), &[10.0, 4.0]);
    }

    /// Same widget with /AS /Off: the Off stream is drawn, and none of the
    /// Yes stream's marks appear.
    #[test]
    fn widget_substate_as_off_state() {
        let pdf = checkbox_pdf(
            b"<< /Type /Annot /Subtype /Widget /FT /Btn /Rect [10 10 30 30] \
              /AP << /N << /Yes 5 0 R /Off 6 0 R >> >> /AS /Off >>",
            "[0 0 20 20]",
            &[],
        );
        assert_widths(&interpret_widths(pdf), &[7.0]);
    }

    /// /N has only the on-state and /AS is /Off: nothing must be drawn and
    /// nothing must panic (ISO 32000 §12.5.5 — no applicable appearance).
    #[test]
    fn widget_substate_as_off_without_off_entry() {
        let pdf = checkbox_pdf(
            b"<< /Type /Annot /Subtype /Widget /FT /Btn /Rect [10 10 30 30] \
              /AP << /N << /Yes 5 0 R >> >> /AS /Off >>",
            "[0 0 20 20]",
            &[],
        );
        assert_widths(&interpret_widths(pdf), &[]);
    }

    /// /AS absent but /V /Yes on the widget: the pdfium V-fallback selects
    /// the Yes stream.
    #[test]
    fn widget_substate_v_fallback() {
        let pdf = checkbox_pdf(
            b"<< /Type /Annot /Subtype /Widget /FT /Btn /Rect [10 10 30 30] \
              /AP << /N << /Yes 5 0 R /Off 6 0 R >> >> /V /Yes >>",
            "[0 0 20 20]",
            &[],
        );
        assert_widths(&interpret_widths(pdf), &[10.0, 4.0]);
    }

    /// /AS and /V absent but the /Parent field dict carries /V /Yes (radio
    /// button group pattern): the one-level parent V-fallback selects the Yes
    /// stream.
    #[test]
    fn widget_substate_parent_v_fallback() {
        let pdf = checkbox_pdf(
            b"<< /Type /Annot /Subtype /Widget /FT /Btn /Rect [10 10 30 30] \
              /AP << /N << /Yes 5 0 R /Off 6 0 R >> >> /Parent 8 0 R >>",
            "[0 0 20 20]",
            &[b"<< /FT /Btn /V /Yes >>".to_vec()],
        );
        assert_widths(&interpret_widths(pdf), &[10.0, 4.0]);
    }

    /// Non-ASCII appearance-state name: the /N dict key contains raw byte
    /// 0xF6 and /AS spells the identical bytes via a #F6 hex escape. Matching
    /// must happen on raw decoded name bytes, never through lossy UTF-8.
    #[test]
    fn widget_substate_non_ascii_state_name() {
        let annot = b"<< /Type /Annot /Subtype /Widget /FT /Btn /Rect [10 10 30 30] \
              /AP << /N << /Stra\xf6m 5 0 R /Off 6 0 R >> >> /AS /Stra#F6m >>";
        // Sanity: the raw 0xF6 byte really is in the annotation dict bytes.
        assert!(annot.contains(&0xf6));
        let pdf = checkbox_pdf(annot, "[0 0 20 20]", &[]);
        assert_widths(&interpret_widths(pdf), &[10.0, 4.0]);
    }

    /// A degenerate (zero-width) appearance BBox must not produce a
    /// non-finite scale matrix: the annotation is skipped without panicking.
    #[test]
    fn widget_degenerate_bbox_skipped() {
        let pdf = checkbox_pdf(
            b"<< /Type /Annot /Subtype /Widget /FT /Btn /Rect [10 10 30 30] \
              /AP << /N << /Yes 5 0 R /Off 6 0 R >> >> /AS /Yes >>",
            "[0 0 0 20]",
            &[],
        );
        assert_widths(&interpret_widths(pdf), &[]);
    }

    /// Regression guard: a plain (non-substate) /AP /N stream still renders.
    #[test]
    fn widget_direct_stream_still_renders() {
        let pdf = checkbox_pdf(
            b"<< /Type /Annot /Subtype /Widget /FT /Btn /Rect [10 10 30 30] \
              /AP << /N 6 0 R >> >>",
            "[0 0 20 20]",
            &[],
        );
        assert_widths(&interpret_widths(pdf), &[7.0]);
    }
}
