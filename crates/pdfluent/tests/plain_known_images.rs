//! Plain known-images on the shared construction fixture (#643).
//!
//! `PdfDocument::page_images` plus `decode_image_pixels` over
//! fixtures/known-images.pdf, checked against
//! fixtures/known-images.expected.json: count, dimensions, colour space,
//! components, bpc, and the decoded-pixel sha256. `extract_image_data`
//! rides along to pin the encoded-vs-pixels split (PNG bytes, not samples).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::PdfDocument;
use sha2::{Digest, Sha256};

const KNOWN_PDF: &[u8] = include_bytes!("../../../fixtures/known-images.pdf");
const EXPECTED_JSON: &str = include_str!("../../../fixtures/known-images.expected.json");

struct Want {
    width: u32,
    height: u32,
    colorspace: String,
    components: u8,
    bpc: u8,
    sha256: String,
}

fn expected() -> Vec<Want> {
    let parsed: serde_json::Value =
        serde_json::from_str(EXPECTED_JSON).expect("expected.json parses");
    let images = parsed["images"].as_array().expect("images array");
    let mut out: Vec<Want> = images
        .iter()
        .map(|el| Want {
            width: el["width"].as_u64().expect("width") as u32,
            height: el["height"].as_u64().expect("height") as u32,
            colorspace: el["colorspace"].as_str().expect("colorspace").to_string(),
            components: el["components"].as_u64().expect("components") as u8,
            bpc: el["bpc"].as_u64().expect("bpc") as u8,
            sha256: el["sha256"].as_str().expect("sha256").to_string(),
        })
        .collect();
    out.sort_by_key(|w| (w.width, w.height));
    assert_eq!(out.len(), 2, "fixture holds two images");
    out
}

fn norm(name: &str) -> String {
    name.to_lowercase()
        .replace("device", "")
        .replace("grey", "gray")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn hex_sha256(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

#[test]
fn page_images_lists_the_shared_fixture() {
    let want = expected();
    let doc = PdfDocument::from_bytes(KNOWN_PDF).expect("fixture opens");
    let mut images = doc.page_images(1).expect("list page 1");
    assert_eq!(images.len(), want.len());
    assert_eq!(images.len(), 2);
    images.sort_by_key(|i| (i.width, i.height));
    for (info, w) in images.iter().zip(want.iter()) {
        assert_eq!(info.width, w.width);
        assert_eq!(info.height, w.height);
        assert_eq!(norm(info.color_space.name()), norm(&w.colorspace));
        assert_eq!(info.components, w.components);
        assert_eq!(info.bits_per_component, w.bpc);
    }
}

#[test]
fn decode_image_pixels_matches_the_expected_sha() {
    let want = expected();
    let doc = PdfDocument::from_bytes(KNOWN_PDF).expect("fixture opens");
    let images = doc.page_images(1).expect("list page 1");
    assert_eq!(images.len(), 2);
    for info in &images {
        let w = want
            .iter()
            .find(|w| w.width == info.width && w.height == info.height)
            .unwrap_or_else(|| panic!("unexpected size {}x{}", info.width, info.height));
        let decoded = doc.decode_image_pixels(info.id).expect("decode");
        assert_eq!(decoded.width, w.width);
        assert_eq!(decoded.height, w.height);
        assert_eq!(decoded.components, w.components);
        assert_eq!(norm(decoded.color_space.name()), norm(&w.colorspace));
        let expect_len = w.width as usize * w.height as usize * w.components as usize;
        assert_eq!(decoded.pixels.len(), expect_len);
        assert_eq!(hex_sha256(&decoded.pixels), w.sha256);
    }
}

#[test]
fn extract_image_data_is_png_not_pixels() {
    let doc = PdfDocument::from_bytes(KNOWN_PDF).expect("fixture opens");
    let images = doc.page_images(1).expect("list page 1");
    assert_eq!(images.len(), 2);
    for info in &images {
        let extracted = doc.extract_image_data(info.id).expect("extract");
        assert_eq!(extracted.encoding, "png");
        assert!(
            extracted.data.starts_with(&[0x89, b'P', b'N', b'G']),
            "extracted bytes are PNG"
        );
        let decoded = doc.decode_image_pixels(info.id).expect("decode");
        assert_ne!(
            extracted.data.len(),
            decoded.pixels.len(),
            "encoded PNG length differs from raw pixels"
        );
    }
}
