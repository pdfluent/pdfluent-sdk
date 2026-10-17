/*!
A crate for rendering PDF files.

This crate allows you to render pages of a PDF file into bitmaps. It is supposed to be relatively
lightweight, since we do not have any dependencies on the GPU. All the rendering happens on the CPU.

The ultimate goal of this crate is to be a *feature-complete* and *performant* PDF rasterizer.
With that said, we are currently still very far away from reaching that goal: So far, no effort
has been put into performance optimizations, as we are still working on implementing missing features.
However, this crate is currently the most comprehensive and feature-complete
implementation of a PDF rasterizer in pure Rust. This claim is supported by the fact that we currently
include over 1000 PDF files in our regression test suite. The majority of those have been scraped
from the `pdf.js` and `PDFBOX` test suites and therefore represent a very large and diverse sample
of PDF files.

As mentioned, there are still some serious limitations, including lack of support for
encrypted/password-protected PDF files, blending and isolation, knockout groups as well as a range
of smaller features such as color key masking. But you should be able to render the vast majority
of PDF files without too many issues.

## Safety
This crate forbids unsafe code via a crate-level attribute.

## Examples
For usage examples, see the [example](https://github.com/LaurenzV/hayro/tree/master/hayro/examples) in
the GitHub repository.

## Cargo features
This crate has one optional feature:
- `embed-fonts`: See the description of [`pdf-interpret`](https://docs.rs/pdf-interpret/latest/pdf_interpret/#cargo-features) for more information.
*/

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use crate::renderer::Renderer;
use kurbo::{Affine, Rect, Shape};
use pdf_interpret::Device;
use pdf_interpret::FillRule;
use pdf_interpret::InterpreterSettings;
use pdf_interpret::pdf_syntax::Pdf;
use pdf_interpret::pdf_syntax::page::Page;
use pdf_interpret::util::PageExt;
use pdf_interpret::{BlendMode, Context};
use pdf_interpret::{ClipPath, interpret_page};
use std::ops::RangeInclusive;

/// Whether per-stage render tracing is enabled (env `PDF_RENDER_TRACE=1`).
/// Read once; zero cost in the hot path when disabled.
fn render_trace_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("PDF_RENDER_TRACE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// Worker-thread count for vello_cpu rasterization. Only has an effect on native
/// targets where the `multithreading` feature is enabled (wasm32 keeps the
/// single-threaded path). `0` = single-threaded. Multi-threaded tiled raster is
/// deterministic and byte-identical to single-threaded (verified by test).
///
/// Default: `available_parallelism` on native; overridable via the
/// `PDF_RENDER_THREADS` env var. Both `0` and `1` select the single-threaded
/// path (`0`): vello takes the multi-threaded path for any nonzero count, so
/// `1` would still build its own pool and wait on it, for no gain.
///
/// A page rendered from inside a rayon worker always uses `0`: the outer pool
/// is already parallel, so nothing is lost, and dispatching to vello's own
/// pool from a worker that then waits lets rayon steal further queued page
/// jobs onto the same stack, which grows with every queued page until the
/// process aborts (#636).
fn render_num_threads() -> u16 {
    // Not cached: the same process renders both inside and outside pools, and
    // only the configured base below is stable across calls.
    #[cfg(not(target_arch = "wasm32"))]
    if rayon::current_thread_index().is_some() {
        return 0;
    }
    use std::sync::OnceLock;
    static N: OnceLock<u16> = OnceLock::new();
    *N.get_or_init(configured_num_threads)
}

/// Configured base thread count: `PDF_RENDER_THREADS` when set and parseable,
/// else the machine default. Both `0` and `1` mean single-threaded (`0`).
fn configured_num_threads() -> u16 {
    configured_num_threads_from_env(std::env::var("PDF_RENDER_THREADS").ok().as_deref())
}

/// Same, with the raw variable value injected so tests can pin the mapping
/// without touching process state.
fn configured_num_threads_from_env(raw: Option<&str>) -> u16 {
    if let Some(n) = raw.and_then(|v| v.parse::<u16>().ok()) {
        return normalize_num_threads(n);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        normalize_num_threads(
            std::thread::available_parallelism()
                .map(|n| n.get().min(u16::MAX as usize) as u16)
                .unwrap_or(1),
        )
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

/// Map a raw thread count onto vello's two paths: `0` and `1` both select the
/// single-threaded path (`0`); anything larger passes through.
fn normalize_num_threads(n: u16) -> u16 {
    if n <= 1 { 0 } else { n }
}

pub use glyph_cache::GlyphOutlineCache;
pub use pdf_interpret;
pub use pdf_interpret::pdf_syntax;
pub use vello_cpu;

use vello_cpu::color::AlphaColor;
use vello_cpu::color::Srgb;
use vello_cpu::color::palette::css::TRANSPARENT;
use vello_cpu::color::palette::css::WHITE;
use vello_cpu::{Level, Pixmap, RenderMode};

mod glyph_cache;
mod renderer;

/// Rasterization precision / speed trade-off for the vello_cpu pipeline.
///
/// vello_cpu ships two compositing pipelines: a higher-precision `f32` pipeline
/// and a faster `u8` pipeline. Both are compiled in; this selects which one a
/// given render uses.
///
/// The default is [`RasterQuality::Quality`] (the `f32` pipeline), which keeps
/// output **byte-identical** to historical PDFluent releases. [`RasterQuality::Speed`]
/// is an explicit, caller-controlled opt-in: on content-heavy pages it renders
/// ~1.4–1.6× faster, at the cost of sub-perceptual rounding differences wherever
/// alpha blending, anti-aliasing or images compose (8-bit vs f32 compositing
/// precision). Pages built only from opaque vector fills are byte-identical in
/// both modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterQuality {
    /// Higher-precision `f32` compositing pipeline. Default; matches historical
    /// output byte-for-byte.
    Quality,
    /// Faster `u8` compositing pipeline (~1.4–1.6× on content-heavy pages).
    /// Opt-in; output differs from [`RasterQuality::Quality`] by sub-perceptual
    /// rounding where blending/AA/images compose.
    Speed,
}

/// The default raster mode everywhere: engine `RenderOptions`, facade
/// `ToImagesOptions`/`RenderPageOptions`, and the CLI when `--raster` is
/// absent all resolve through [`RasterQuality::default`], which returns this.
///
/// Do not change here casually: flipping to [`RasterQuality::Speed`] is the
/// one-line follow-up gated on the T5 measurement (SSIM ≥ 0.999 vs current
/// output on every page).
pub const DEFAULT_RASTER_QUALITY: RasterQuality = RasterQuality::Quality;

impl Default for RasterQuality {
    fn default() -> Self {
        DEFAULT_RASTER_QUALITY
    }
}

impl RasterQuality {
    /// Map to the underlying vello_cpu render mode.
    fn render_mode(self) -> RenderMode {
        match self {
            // OptimizeQuality requires the `f32_pipeline` feature (enabled in Cargo.toml).
            RasterQuality::Quality => RenderMode::OptimizeQuality,
            RasterQuality::Speed => RenderMode::OptimizeSpeed,
        }
    }
}

/// Settings to apply during rendering.
#[derive(Clone, Copy)]
pub struct RenderSettings {
    /// How much the contents should be scaled into the x direction.
    pub x_scale: f32,
    /// How much the contents should be scaled into the y direction.
    pub y_scale: f32,
    /// The width of the viewport. If this is set to `None`, the width will be chosen
    /// automatically based on the scale factor and the dimensions of the PDF.
    pub width: Option<u16>,
    /// The height of the viewport. If this is set to `None`, the height will be chosen
    /// automatically based on the scale factor and the dimensions of the PDF.
    pub height: Option<u16>,
    /// The background color. Determines the color of the base
    /// rectangle during rendering to a pixmap.
    pub bg_color: AlphaColor<Srgb>,
    /// Rasterization precision/speed trade-off (default [`RasterQuality::Quality`],
    /// which is byte-identical to historical output).
    pub quality: RasterQuality,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            x_scale: 1.0,
            y_scale: 1.0,
            width: None,
            height: None,
            bg_color: TRANSPARENT,
            quality: RasterQuality::default(),
        }
    }
}

/// Render the page with the given settings to a pixmap.
///
/// Uses a fresh per-page glyph outline cache; to share outlines across the
/// pages of a document, see [`render_with_glyph_cache`]. Output is identical
/// either way.
pub fn render(
    page: &Page<'_>,
    interpreter_settings: &InterpreterSettings,
    render_settings: &RenderSettings,
) -> Pixmap {
    render_with_glyph_cache(
        page,
        interpreter_settings,
        render_settings,
        &mut GlyphOutlineCache::new(),
    )
}

/// Render the page with the given settings to a pixmap, reusing (and
/// warming) a caller-owned [`GlyphOutlineCache`].
///
/// Pass the same cache for every page of a document to build each distinct
/// glyph outline once per document instead of once per page. The cache only
/// avoids rebuilds — the same path in the same order under the same
/// transform — so output is byte-identical to [`render`], and rendering
/// stays pure per page. [`GlyphOutlineCache::hits`] and
/// [`GlyphOutlineCache::misses`] report how the cache served the render.
///
/// Pattern tiles and soft masks render into private bounded caches; only the
/// page's own glyphs go through `glyph_cache`.
pub fn render_with_glyph_cache(
    page: &Page<'_>,
    interpreter_settings: &InterpreterSettings,
    render_settings: &RenderSettings,
    glyph_cache: &mut GlyphOutlineCache,
) -> Pixmap {
    let (x_scale, y_scale) = (render_settings.x_scale, render_settings.y_scale);
    let (width, height) = page.render_dimensions();
    let (scaled_width, scaled_height) = ((width * x_scale) as f64, (height * y_scale) as f64);
    let initial_transform =
        Affine::scale_non_uniform(x_scale as f64, y_scale as f64) * page.initial_transform(true);

    // Clamp to at least 1 pixel. Pages with zero-area MediaBox (e.g. adversarial
    // PDFs from the poppler fuzzing corpus) produce scaled_width/height = 0.
    // vello_common::Pixmap::new(0, 0) allocates an empty buffer; any subsequent
    // pixel sample then panics with "index out of bounds: the len is 0".
    // Fixes crashes on poppler-327-0.zip-{0,1}.pdf. (#546)
    // Round-half-up (PDFium convention: (int)(size*scale + 0.5)) rather than
    // ceil. PDFium / pdfRest is our AVRT oracle; ceil produced a 0–1 px
    // height/width excess vs pdfrest on 9 non-integer-MediaBox PDFs (e.g.
    // 0273, 0139, 0356, 0368, 0508, 0272, 0568, 0418, 0325), destroying SSIM
    // through pixel-row/column misalignment.
    // For exact integer values round/ceil/floor are identical. (#1001, #544, #558)
    let (pix_width, pix_height) = (
        render_settings
            .width
            .unwrap_or(scaled_width.round() as u16)
            .max(1),
        render_settings
            .height
            .unwrap_or(scaled_height.round() as u16)
            .max(1),
    );
    let trace = render_trace_enabled();
    let t_setup = trace.then(std::time::Instant::now);
    let mut state = Context::new(
        initial_transform,
        Rect::new(0.0, 0.0, pix_width as f64, pix_height as f64),
        page.xref(),
        interpreter_settings.clone(),
    );

    let vc_settings = vello_cpu::RenderSettings {
        level: Level::new(),
        num_threads: render_num_threads(),
        render_mode: render_settings.quality.render_mode(),
    };

    let mut device = Renderer::new(pix_width, pix_height, vc_settings);
    device.set_glyph_cache(std::mem::take(glyph_cache));

    device.ctx.set_paint(render_settings.bg_color);
    device
        .ctx
        .fill_rect(&Rect::new(0.0, 0.0, pix_width as f64, pix_height as f64));
    // Clip to the canvas bounds (integer pixel dimensions) rather than the
    // sub-pixel-precise transformed CropBox rectangle.
    // MuPDF clips to the integer pixel canvas boundary (ceil(crop_box × scale));
    // it does not impose a separate sub-pixel-accurate CropBox clip.  Using the
    // exact transformed CropBox rect causes anti-aliased edge columns/rows that
    // differ from MuPDF at the sub-pixel boundary (e.g. a 25 pt page at 150 DPI
    // = 52.083 px → the last pixel column ends up near-white in our render but
    // fully-painted dark red in MuPDF).  Clipping to the integer canvas bounds
    // reproduces MuPDF's behaviour while still preventing content from bleeding
    // outside the canvas.  For the case where CropBox extends beyond MediaBox
    // (gen-802), content outside the MediaBox is simply unpainted (background
    // colour), so no visible difference results.  (#558, follow-up to #544)
    device.push_clip_path(&ClipPath {
        path: Rect::new(0.0, 0.0, pix_width as f64, pix_height as f64).to_path(0.1),
        fill: FillRule::NonZero,
    });

    device.push_transparency_group(1.0, None, BlendMode::Normal);

    // Stage timing (env-gated; zero cost when disabled): the two dominant phases
    // are (1) `interpret_page` — building the vello scene/display list from the
    // PDF content stream (path/text/image construction), and (2)
    // `render_to_pixmap` — vello_cpu rasterization to RGBA. This split localizes
    // whether render cost is scene-build or rasterization.
    // Setup = Context/Renderer construction + background fill + clip/group push,
    // measured up to (but excluding) interpretation.
    let setup_ms = t_setup.map(|t| t.elapsed().as_secs_f64() * 1000.0);
    let t_interpret = trace.then(std::time::Instant::now);
    interpret_page(page, &mut state, &mut device);
    let interpret_ms = t_interpret.map(|t| t.elapsed().as_secs_f64() * 1000.0);

    device.pop_transparency_group();

    device.pop_clip_path();

    let mut pixmap = Pixmap::new(pix_width, pix_height);
    let t_raster = trace.then(std::time::Instant::now);
    // Multi-threaded rasterization requires an explicit flush before sampling
    // the pixmap; on the single-threaded path flush() is a no-op.
    device.ctx.flush();
    device.ctx.render_to_pixmap(&mut pixmap);
    let raster_ms = t_raster.map(|t| t.elapsed().as_secs_f64() * 1000.0);

    if trace {
        eprintln!(
            "PDF_RENDER_TRACE setup_ms={:.3} interpret_ms={:.2} raster_ms={:.2} w={} h={} threads={}",
            setup_ms.unwrap_or(0.0),
            interpret_ms.unwrap_or(0.0),
            raster_ms.unwrap_or(0.0),
            pix_width,
            pix_height,
            vc_settings.num_threads,
        );
    }

    *glyph_cache = device.take_glyph_cache();

    pixmap
}

// Just a convenience method for testing.
#[doc(hidden)]
pub fn render_pdf(
    pdf: &Pdf,
    scale: f32,
    settings: InterpreterSettings,
    range: Option<RangeInclusive<usize>>,
) -> Option<Vec<Pixmap>> {
    let rendered = pdf
        .pages()
        .iter()
        .enumerate()
        .flat_map(|(idx, page)| {
            if range.clone().is_some_and(|range| !range.contains(&idx)) {
                return None;
            }

            let pixmap = render(
                page,
                &settings,
                &RenderSettings {
                    x_scale: scale,
                    y_scale: scale,
                    bg_color: WHITE,
                    ..Default::default()
                },
            );

            Some(pixmap)
        })
        .collect();

    Some(rendered)
}

pub(crate) fn derive_settings(settings: &vello_cpu::RenderSettings) -> vello_cpu::RenderSettings {
    vello_cpu::RenderSettings {
        num_threads: 0,
        ..*settings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_interpret::InterpreterSettings;
    use pdf_syntax::Pdf;

    /// Build a minimal one-page PDF (72×72 pt empty page) using lopdf.
    fn minimal_pdf_bytes() -> Vec<u8> {
        use lopdf::{Document, Object, Stream, dictionary};

        let mut doc = Document::with_version("1.4");

        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();

        // Empty content stream so the page has a valid structure.
        let content = Stream::new(dictionary! {}, b"".to_vec());
        let content_id = doc.add_object(content);

        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type"      => Object::Name(b"Page".to_vec()),
                "Parent"    => Object::Reference(pages_id),
                "MediaBox"  => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(72), Object::Integer(72),
                ]),
                "Contents"  => Object::Reference(content_id),
            }),
        );

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type"  => Object::Name(b"Pages".to_vec()),
                "Kids"  => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );

        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type"  => Object::Name(b"Catalog".to_vec()),
                "Pages" => Object::Reference(pages_id),
            }),
        );

        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("lopdf save should succeed");
        bytes
    }

    #[test]
    fn render_pdf_returns_one_pixmap() {
        let bytes = minimal_pdf_bytes();
        let pdf = Pdf::new(bytes).expect("PDF should load");
        let pixmaps = render_pdf(&pdf, 1.0, InterpreterSettings::default(), None);
        assert!(pixmaps.is_some());
        assert_eq!(pixmaps.unwrap().len(), 1);
    }

    /// Assemble a PDF from numbered object bodies (index `i` becomes object
    /// `i + 1`), computing byte-accurate xref offsets.
    fn raw_pdf_bytes(objects: &[Vec<u8>]) -> Vec<u8> {
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

    /// LaurenzV/hayro 128996e9 (#1285): a downscaled image with a soft mask
    /// must be filtered premultiplied, or semi-transparent edges fringe.
    /// The fixture is a 2x1 image — red opaque next to transparent black —
    /// drawn over a 2x1 pt page and rendered at half scale, so the RGBA
    /// resize arm blends the two pixels into one. Premultiplied (correct),
    /// the blend is red at half alpha, which composites to pinkish-white
    /// over the white background; naive straight-alpha filtering fringes
    /// the red channel down to ~191.
    ///
    /// Our resizer (fast_image_resize, `mul_div_alpha` default on) already
    /// multiplies before convolution and divides after, so the defect never
    /// reached this tree; this test pins the behavior rather than a code
    /// change. It fails if the resize ever filters straight alpha (e.g.
    /// with `use_alpha(false)`).
    #[test]
    fn downscaled_smasked_image_composites_without_fringe() {
        let image = [
            b"<< /Type /XObject /Subtype /Image /Width 2 /Height 1 /BitsPerComponent 8 \
              /ColorSpace /DeviceRGB /SMask 5 0 R /Length 6 >>\nstream\n"
                .as_slice(),
            [255, 0, 0, 0, 0, 0].as_slice(),
            b"\nendstream".as_slice(),
        ]
        .concat();
        let smask = [
            b"<< /Type /XObject /Subtype /Image /Width 2 /Height 1 /BitsPerComponent 8 \
              /ColorSpace /DeviceGray /Length 2 >>\nstream\n"
                .as_slice(),
            [255, 0].as_slice(),
            b"\nendstream".as_slice(),
        ]
        .concat();
        let content = b"<< /Length 22 >>\nstream\n2 0 0 1 0 0 cm /Im0 Do\nendstream".to_vec();

        let bytes = raw_pdf_bytes(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 2 1] \
              /Resources << /XObject << /Im0 4 0 R >> >> /Contents 6 0 R >>"
                .to_vec(),
            image,
            smask,
            content,
        ]);

        let pdf = Pdf::new(bytes).expect("PDF should load");
        let pixmaps = render_pdf(&pdf, 0.5, InterpreterSettings::default(), None).unwrap();
        assert_eq!(pixmaps.len(), 1);
        let pixmap = &pixmaps[0];
        assert_eq!((pixmap.width(), pixmap.height()), (1, 1));

        // Red at half alpha over white: full red, half green/blue. A naive
        // straight-alpha resize would fringe red down to ~191; an undrawn
        // image would leave all-white (green 255).
        let px = pixmap.data_as_u8_slice();
        assert_eq!(px.len(), 4);
        assert!(
            px[0] > 240 && (100..160).contains(&px[1]) && (100..160).contains(&px[2]),
            "expected pinkish-white, got {px:?}"
        );
    }

    #[test]
    fn render_pdf_pixmap_matches_mediabox() {
        let bytes = minimal_pdf_bytes();
        let pdf = Pdf::new(bytes).expect("PDF should load");
        let pixmaps = render_pdf(&pdf, 1.0, InterpreterSettings::default(), None).unwrap();
        let pixmap = &pixmaps[0];
        // MediaBox is [0 0 72 72] → 72×72 pixels at scale 1.0.
        assert_eq!(pixmap.width(), 72);
        assert_eq!(pixmap.height(), 72);
    }

    #[test]
    fn render_pdf_with_scale_2_doubles_dimensions() {
        let bytes = minimal_pdf_bytes();
        let pdf = Pdf::new(bytes).expect("PDF should load");
        let pixmaps = render_pdf(&pdf, 2.0, InterpreterSettings::default(), None).unwrap();
        let pixmap = &pixmaps[0];
        assert_eq!(pixmap.width(), 144);
        assert_eq!(pixmap.height(), 144);
    }

    #[test]
    fn render_pdf_page_range_selects_single_page() {
        let bytes = minimal_pdf_bytes();
        let pdf = Pdf::new(bytes).expect("PDF should load");
        // Range 0..=0 selects only the first (and only) page.
        let pixmaps = render_pdf(&pdf, 1.0, InterpreterSettings::default(), Some(0..=0)).unwrap();
        assert_eq!(pixmaps.len(), 1);
    }

    /// Rasterization must be deterministic and byte-identical across renders,
    /// including under the multi-threaded vello_cpu path (native). This guards
    /// the multithreading-enable change against any nondeterminism regression —
    /// a pixel difference here would be a fidelity regression, not a perf win.
    #[test]
    fn render_pdf_is_byte_deterministic() {
        let bytes = minimal_pdf_bytes();
        let pdf = Pdf::new(bytes).expect("PDF should load");
        let a = render_pdf(&pdf, 2.0, InterpreterSettings::default(), None).unwrap();
        let b = render_pdf(&pdf, 2.0, InterpreterSettings::default(), None).unwrap();
        assert_eq!(a.len(), b.len());
        assert_eq!(
            a[0].data_as_u8_slice(),
            b[0].data_as_u8_slice(),
            "render output must be byte-identical across runs"
        );
    }

    /// Each `RasterQuality` mode must itself be deterministic (byte-identical
    /// across runs) and produce identical dimensions. This guards the opt-in
    /// Speed (u8) pipeline against nondeterminism while leaving the default
    /// Quality (f32) path as the byte-identical baseline.
    #[test]
    fn raster_quality_modes_are_deterministic() {
        let bytes = minimal_pdf_bytes();
        let pdf = Pdf::new(bytes).expect("PDF should load");
        for quality in [RasterQuality::Quality, RasterQuality::Speed] {
            let render_once = || {
                let page = &pdf.pages()[0];
                render(
                    page,
                    &InterpreterSettings::default(),
                    &RenderSettings {
                        x_scale: 2.0,
                        y_scale: 2.0,
                        bg_color: WHITE,
                        quality,
                        ..Default::default()
                    },
                )
            };
            let a = render_once();
            let b = render_once();
            assert_eq!(
                (a.width(), a.height()),
                (b.width(), b.height()),
                "{quality:?} dimensions must be stable"
            );
            assert_eq!(
                a.data_as_u8_slice(),
                b.data_as_u8_slice(),
                "{quality:?} output must be byte-identical across runs"
            );
        }
    }

    /// `RasterQuality::Quality` is the default and must map to the f32 render
    /// mode, keeping default output byte-identical to historical releases.
    #[test]
    fn raster_quality_default_is_quality() {
        assert_eq!(RasterQuality::default(), RasterQuality::Quality);
        assert_eq!(RenderSettings::default().quality, RasterQuality::Quality);
    }

    /// The default flows from the single named constant, so the T5-gated
    /// flip (Quality -> Speed) is one line.
    #[test]
    fn raster_quality_default_is_the_named_constant() {
        assert_eq!(RasterQuality::default(), DEFAULT_RASTER_QUALITY);
        assert_eq!(DEFAULT_RASTER_QUALITY, RasterQuality::Quality);
    }

    /// Seam: each quality maps to its vello_cpu pipeline. `render()` feeds
    /// this mapping into `RenderSettings.render_mode`, so the option reaches
    /// the rasteriser by construction; the pixmap tests below prove both
    /// pipelines render.
    #[test]
    fn raster_quality_maps_to_its_render_mode() {
        assert!(matches!(
            RasterQuality::Quality.render_mode(),
            RenderMode::OptimizeQuality
        ));
        assert!(matches!(
            RasterQuality::Speed.render_mode(),
            RenderMode::OptimizeSpeed
        ));
    }

    /// A one-page fixture of our own with filled + stroked content (the
    /// diagonal stroke exercises anti-aliased edges), generated in-test.
    fn aa_edge_pdf_bytes() -> Vec<u8> {
        let content = b"<< /Length 55 >>\nstream\n1 0 0 rg 10 10 50 30 re f 0 0 1 RG 2 w 0 0 m 72 72 l S\nendstream"
            .to_vec();
        raw_pdf_bytes(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Contents 4 0 R >>".to_vec(),
            content,
        ])
    }

    /// Both modes produce a valid pixmap of the same size on content that
    /// exercises fills, strokes and anti-aliased edges.
    #[test]
    fn raster_quality_modes_produce_same_size_pixmap() {
        let bytes = aa_edge_pdf_bytes();
        let pdf = Pdf::new(bytes).expect("PDF should load");
        let page = &pdf.pages()[0];
        let mut pixmaps = Vec::new();
        for quality in [RasterQuality::Quality, RasterQuality::Speed] {
            let pixmap = render(
                page,
                &InterpreterSettings::default(),
                &RenderSettings {
                    x_scale: 2.0,
                    y_scale: 2.0,
                    bg_color: WHITE,
                    quality,
                    ..Default::default()
                },
            );
            assert_eq!((pixmap.width(), pixmap.height()), (144, 144));
            assert!(!pixmap.data_as_u8_slice().is_empty());
            pixmaps.push(pixmap);
        }
        assert_eq!(
            (pixmaps[0].width(), pixmaps[0].height()),
            (pixmaps[1].width(), pixmaps[1].height()),
            "Quality and Speed must agree on dimensions"
        );
    }

    /// `0` and `1` both select the single-threaded path; anything larger
    /// passes through. Vello takes the multi-threaded path for any nonzero
    /// count, so `1` must not reach it (#636).
    #[test]
    fn normalize_num_threads_maps_zero_and_one_to_single_threaded() {
        assert_eq!(normalize_num_threads(0), 0);
        assert_eq!(normalize_num_threads(1), 0);
        assert_eq!(normalize_num_threads(2), 2);
        assert_eq!(normalize_num_threads(8), 8);
    }

    /// `PDF_RENDER_THREADS=1` resolves to the single-threaded path, like `0`.
    /// An unset or unparseable value falls back to the machine default.
    #[test]
    fn configured_num_threads_treats_one_as_single_threaded() {
        assert_eq!(configured_num_threads_from_env(Some("1")), 0);
        assert_eq!(configured_num_threads_from_env(Some("0")), 0);
        assert_eq!(configured_num_threads_from_env(Some("4")), 4);
        assert_eq!(
            configured_num_threads_from_env(Some("bogus")),
            configured_num_threads_from_env(None),
            "an unparseable value must fall back to the default"
        );
    }

    /// A page rendered from inside a rayon worker must use the single-threaded
    /// path: dispatching to vello's own pool from a worker that then waits
    /// overflows the worker's stack once enough page jobs are queued (#636).
    /// Reads through the real (cached) entry point, because the worker guard
    /// must hold no matter what the base resolved to.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn render_num_threads_is_zero_inside_rayon_worker() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("test pool should build");
        pool.install(|| {
            assert!(rayon::current_thread_index().is_some());
            assert_eq!(render_num_threads(), 0);
        });
    }
}
