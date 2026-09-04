//! Generate synthetic "scanned" PDF fixtures for OCR accuracy testing.
//!
//! Each fixture is created by:
//!   1. Rendering page 0 of a source PDF at 150 DPI → RGBA pixels
//!   2. Converting RGBA → RGB and encoding as JPEG (quality 90)
//!   3. Embedding the JPEG in a new, image-only PDF (no text layer)
//!
//! The resulting PDFs mimic real-world scanned documents.  The source PDFs
//! provide the ground-truth text for `check_ocr_accuracy`.
//!
//! Run with:
//!   cargo run -p xfa-test-runner --example generate_scanned_pdfs
//!
//! Output: fixtures/scanned/scan_NN_<name>.pdf
//!         fixtures/scanned/scan_NN_<name>.source.pdf  (symlink / copy of source)

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::io::Cursor;
use std::path::PathBuf;

use lopdf::{dictionary, Document, Object, Stream};

// ---------------------------------------------------------------------------
// Source PDFs — relative to the workspace root (two levels up from this crate)
// ---------------------------------------------------------------------------

const SOURCES: &[(&str, &str)] = &[
    ("sample", "fixtures/sample.pdf"),
    ("acroform", "fixtures/acroform.pdf"),
    ("multipage", "fixtures/multi-page.pdf"),
    ("signed", "fixtures/signed.pdf"),
    ("fc_arithmetic", "fixtures/formcalc/fc_01_arithmetic.pdf"),
];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.join("../..").canonicalize().unwrap();
    let out_dir = workspace_root.join("fixtures/scanned");
    std::fs::create_dir_all(&out_dir).expect("create fixtures/scanned dir");

    let mut written = 0usize;
    for (i, &(suffix, source_rel)) in SOURCES.iter().enumerate() {
        let source_path = workspace_root.join(source_rel);
        if !source_path.exists() {
            println!("  SKIP {source_rel} — not found");
            continue;
        }

        let source_data = match std::fs::read(&source_path) {
            Ok(d) => d,
            Err(e) => {
                println!("  SKIP {source_rel} — read error: {e}");
                continue;
            }
        };

        let doc = match pdf_engine::PdfDocument::open(source_data) {
            Ok(d) => d,
            Err(e) => {
                println!("  SKIP {source_rel} — PDF open error: {e}");
                continue;
            }
        };

        let opts = pdf_engine::RenderOptions {
            dpi: 150.0,
            ..Default::default()
        };
        let rendered = match doc.render_page(0, &opts) {
            Ok(r) => r,
            Err(e) => {
                println!("  SKIP {source_rel} — render error: {e}");
                continue;
            }
        };

        let jpeg = match rgba_to_jpeg(&rendered.pixels, rendered.width, rendered.height, 90) {
            Some(j) => j,
            None => {
                println!("  SKIP {source_rel} — JPEG encode failed");
                continue;
            }
        };

        let pdf_bytes = build_image_pdf(&jpeg, rendered.width, rendered.height);
        let filename = format!("scan_{:02}_{suffix}.pdf", i + 1);
        let out_path = out_dir.join(&filename);
        std::fs::write(&out_path, &pdf_bytes).expect("write scanned PDF");

        // Copy source alongside so check_ocr_accuracy can find it.
        let source_copy = out_dir.join(format!("scan_{:02}_{suffix}.source.pdf", i + 1));
        std::fs::copy(&source_path, &source_copy).ok();

        println!(
            "  {}  ({} bytes, {}×{}px)  ← {}",
            filename,
            pdf_bytes.len(),
            rendered.width,
            rendered.height,
            source_rel
        );
        written += 1;
    }

    println!(
        "\n{written} scanned fixture PDFs written to {}",
        out_dir.display()
    );
}

// ---------------------------------------------------------------------------
// Image helpers
// ---------------------------------------------------------------------------

fn rgba_to_jpeg(pixels: &[u8], width: u32, height: u32, quality: u8) -> Option<Vec<u8>> {
    let rgba = image::RgbaImage::from_raw(width, height, pixels.to_vec())?;
    let rgb = image::DynamicImage::ImageRgba8(rgba).into_rgb8();
    let mut buf = Vec::new();
    rgb.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Jpeg)
        .ok()?;
    // Re-encode at desired quality (write_to uses default quality for JPEG).
    // Use the codecs API for quality control.
    let mut buf_q = Vec::new();
    {
        use image::ImageEncoder;
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf_q, quality);
        enc.write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .ok()?;
    }
    Some(buf_q)
}

// ---------------------------------------------------------------------------
// PDF builder — single-page image-only PDF (DCTDecode / JPEG)
// ---------------------------------------------------------------------------

fn build_image_pdf(jpeg: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");

    // Image XObject (JPEG via DCTDecode).
    let img_stream = Stream::new(
        dictionary! {
            "Type"             => Object::Name(b"XObject".to_vec()),
            "Subtype"          => Object::Name(b"Image".to_vec()),
            "Width"            => Object::Integer(width as i64),
            "Height"           => Object::Integer(height as i64),
            "ColorSpace"       => Object::Name(b"DeviceRGB".to_vec()),
            "BitsPerComponent" => Object::Integer(8),
            "Filter"           => Object::Name(b"DCTDecode".to_vec()),
            "Length"           => Object::Integer(jpeg.len() as i64),
        },
        jpeg.to_vec(),
    );
    let img_id = doc.add_object(Object::Stream(img_stream));

    let resources_id = doc.add_object(Object::Dictionary(dictionary! {
        "XObject" => Object::Dictionary(dictionary! {
            "Im0" => Object::Reference(img_id),
        }),
    }));

    // Content stream: place image to fill the page.
    let (w, h) = (width as i64, height as i64);
    let content = format!("q {w} 0 0 {h} 0 0 cm /Im0 Do Q\n");
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length" => Object::Integer(content.len() as i64) },
        content.into_bytes(),
    )));

    // Page tree.
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type"      => Object::Name(b"Page".to_vec()),
        "Parent"    => Object::Reference(pages_id),
        "MediaBox"  => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(w), Object::Integer(h),
        ]),
        "Resources" => Object::Reference(resources_id),
        "Contents"  => Object::Reference(content_id),
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type"  => Object::Name(b"Pages".to_vec()),
            "Kids"  => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        }),
    );

    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type"  => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("lopdf save");
    out
}
