//! The vision layer runs a tiny model on a page raster (#636).
//!
//! Infrastructure only: no trained model ships. The identity fixture
//! (`fixtures/vision_identity.onnx`, built by
//! `scripts/vision/make_identity_fixture.py`) echoes its NCHW float input, so
//! parity is exact and any tensor-layout mistake shows up as wrong values
//! rather than a crash. The raster comes from the engine's existing renderer,
//! so the dimension test pins that 72 dpi stays 1:1 with PDF points.
//!
//! Every test here fails without the `vision` feature: the file compiles to
//! nothing, and with it the tests call the runner and the rasterizer
//! directly, so deleting either breaks them.

#![cfg(feature = "vision")]

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::vision::{
    rasterize_page, rasterize_page_with_dpi, VisionPixelFormat, VisionRaster, VisionRunner,
};
use pdf_engine::PdfDocument;

const IDENTITY: &[u8] = include_bytes!("fixtures/vision_identity.onnx");
const STRUCTURED: &[u8] = include_bytes!("../../../fixtures/structured.pdf");

fn runner_from_bytes() -> VisionRunner {
    VisionRunner::from_bytes(IDENTITY).expect("identity fixture loads from bytes")
}

#[test]
fn identity_parity_on_a_gray_raster() {
    let runner = runner_from_bytes();
    let raster = VisionRaster {
        width: 2,
        height: 2,
        format: VisionPixelFormat::Gray8,
        data: vec![0, 64, 128, 255],
    };
    let out = runner.run(&raster).expect("run");
    assert_eq!(out.shape, vec![1, 1, 2, 2]);
    let expected = [0.0, 64.0 / 255.0, 128.0 / 255.0, 1.0];
    assert_eq!(out.data.len(), expected.len());
    for (got, want) in out.data.iter().zip(expected.iter()) {
        assert!((got - want).abs() < 1e-6, "got {got}, want {want}");
    }
}

#[test]
fn identity_parity_on_an_rgb_raster_is_planar() {
    let runner = runner_from_bytes();
    // Two pixels: red, then green. NCHW planar, not interleaved.
    let raster = VisionRaster {
        width: 2,
        height: 1,
        format: VisionPixelFormat::Rgb8,
        data: vec![255, 0, 0, 0, 255, 0],
    };
    let out = runner.run(&raster).expect("run");
    assert_eq!(out.shape, vec![1, 3, 1, 2]);
    assert_eq!(out.data, vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
}

#[test]
fn model_loads_from_a_path() {
    let path = format!(
        "{}/tests/fixtures/vision_identity.onnx",
        env!("CARGO_MANIFEST_DIR")
    );
    let runner = VisionRunner::from_path(&path).expect("identity fixture loads from a path");
    let raster = VisionRaster {
        width: 1,
        height: 1,
        format: VisionPixelFormat::Gray8,
        data: vec![255],
    };
    let out = runner.run(&raster).expect("run");
    assert_eq!(out.shape, vec![1, 1, 1, 1]);
    assert_eq!(out.data, vec![1.0]);
}

#[test]
fn raster_dimensions_at_72dpi_match_the_page() {
    let doc = PdfDocument::open(STRUCTURED.to_vec()).expect("open");
    let gray = rasterize_page(&doc, 0, VisionPixelFormat::Gray8).expect("rasterize");
    assert_eq!((gray.width, gray.height), (612, 792));
    assert_eq!(gray.data.len(), 612 * 792);
    let rgb = rasterize_page_with_dpi(&doc, 0, 72, VisionPixelFormat::Rgb8).expect("rasterize");
    assert_eq!((rgb.width, rgb.height), (612, 792));
    assert_eq!(rgb.data.len(), 612 * 792 * 3);
    // The default-dpi entry point is 72 dpi exactly.
    let explicit =
        rasterize_page_with_dpi(&doc, 0, 72, VisionPixelFormat::Gray8).expect("rasterize");
    assert_eq!(gray.data, explicit.data);
}

#[test]
fn crop_in_points_maps_to_the_right_pixels() {
    let doc = PdfDocument::open(STRUCTURED.to_vec()).expect("open");
    let raster = rasterize_page(&doc, 0, VisionPixelFormat::Gray8).expect("rasterize");
    let geometry = doc.page_geometry(0).expect("geometry");
    let (w, h) = geometry.effective_dimensions();
    // Bottom-left 100x100 pt at 72 dpi is 100x100 px.
    let crop = raster
        .crop_points([0.0, 0.0, 100.0, 100.0], (w, h), 72)
        .expect("crop");
    assert_eq!((crop.width, crop.height), (100, 100));
    // The crop is the raster's bottom rows, not its top ones.
    let row = |y: u32| &raster.data[y as usize * 612..y as usize * 612 + 100];
    assert_eq!(&crop.data[0..100], row(692));
    assert_eq!(&crop.data[99 * 100..100 * 100], row(791));
}

#[test]
fn run_on_crop_returns_the_crop_tensor() {
    let runner = runner_from_bytes();
    let doc = PdfDocument::open(STRUCTURED.to_vec()).expect("open");
    let out = runner
        .run_on_crop(&doc, 0, [0.0, 0.0, 8.0, 4.0], 72, VisionPixelFormat::Gray8)
        .expect("run on crop");
    assert_eq!(out.shape, vec![1, 1, 4, 8]);
    assert_eq!(out.data.len(), 32);
}
