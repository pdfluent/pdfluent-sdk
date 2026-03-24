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

pub use pdf_interpret;
pub use pdf_interpret::pdf_syntax;
pub use vello_cpu;

use vello_cpu::color::AlphaColor;
use vello_cpu::color::Srgb;
use vello_cpu::color::palette::css::TRANSPARENT;
use vello_cpu::color::palette::css::WHITE;
use vello_cpu::{Level, Pixmap, RenderMode};

mod renderer;

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
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            x_scale: 1.0,
            y_scale: 1.0,
            width: None,
            height: None,
            bg_color: TRANSPARENT,
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
    // Ceil (not floor or round) to match MuPDF canvas sizes for non-integer
    // MediaBox dimensions. MuPDF consistently uses ceil to compute the output
    // pixmap size, e.g. 25pt × 10pt at 150 DPI = 52.08 × 20.83 → 53 × 21.
    // For exact integer values the result is identical to floor/round. (#544, #558)
    let (pix_width, pix_height) = (
        render_settings
            .width
            .unwrap_or(scaled_width.ceil() as u16)
            .max(1),
        render_settings
            .height
            .unwrap_or(scaled_height.ceil() as u16)
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
        num_threads: 0,
        render_mode: RenderMode::OptimizeSpeed,
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
    interpret_page(page, &mut state, &mut device);

    device.pop_transparency_group();

    device.pop_clip_path();

    let mut pixmap = Pixmap::new(pix_width, pix_height);
    device.ctx.render_to_pixmap(&mut pixmap);

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
}

pub(crate) fn derive_settings(settings: &vello_cpu::RenderSettings) -> vello_cpu::RenderSettings {
    vello_cpu::RenderSettings {
        num_threads: 0,
        ..*settings
    }
}
