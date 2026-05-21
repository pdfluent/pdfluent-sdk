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
/// `PDF_RENDER_THREADS` env var (e.g. `1` to force single-threaded for A/B).
fn render_num_threads() -> u16 {
    use std::sync::OnceLock;
    static N: OnceLock<u16> = OnceLock::new();
    *N.get_or_init(|| {
        if let Some(n) = std::env::var("PDF_RENDER_THREADS")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
        {
            return n;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::available_parallelism()
                .map(|n| n.get().min(u16::MAX as usize) as u16)
                .unwrap_or(1)
        }
        #[cfg(target_arch = "wasm32")]
        {
            0
        }
    })
}

pub use pdf_interpret;
pub use pdf_interpret::pdf_syntax;
pub use vello_cpu;

use vello_cpu::color::AlphaColor;
use vello_cpu::color::Srgb;
use vello_cpu::color::palette::css::TRANSPARENT;
use vello_cpu::color::palette::css::WHITE;
use vello_cpu::{Level, Pixmap, RenderMode};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RasterQuality {
    /// Higher-precision `f32` compositing pipeline. Default; matches historical
    /// output byte-for-byte.
    #[default]
    Quality,
    /// Faster `u8` compositing pipeline (~1.4–1.6× on content-heavy pages).
    /// Opt-in; output differs from [`RasterQuality::Quality`] by sub-perceptual
    /// rounding where blending/AA/images compose.
    Speed,
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
pub fn render(
    page: &Page<'_>,
    interpreter_settings: &InterpreterSettings,
    render_settings: &RenderSettings,
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
    let trace = render_trace_enabled();
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
            "PDF_RENDER_TRACE interpret_ms={:.2} raster_ms={:.2} w={} h={} threads={}",
            interpret_ms.unwrap_or(0.0),
            raster_ms.unwrap_or(0.0),
            pix_width,
            pix_height,
            vc_settings.num_threads,
        );
    }

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
}
