//! Compat known-images as encoded files, decoded independently (#636).
//!
//! `compat::Document::extract_image` over fixtures/known-images.pdf returns
//! the image FILE (header facts plus encoded bytes), checked against
//! fixtures/known-images.expected.json: `ext` matches the payload's magic
//! bytes, and the payload decoded with the third-party `png` crate yields
//! the expected dimensions, components, and decoded-pixel sha256. A lossy
//! (JPEG) payload cannot match a pixel sha by construction, so it pins
//! only magic bytes, dimensions, and components.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::compat::Document;
use sha2::{Digest, Sha256};

const KNOWN_PDF: &[u8] = include_bytes!("../../../fixtures/known-images.pdf");
const EXPECTED_JSON: &str = include_str!("../../../fixtures/known-images.expected.json");

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF];

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

fn for_size(want: &[Want], width: u32, height: u32) -> &Want {
    want.iter()
        .find(|w| w.width == width && w.height == height)
        .unwrap_or_else(|| panic!("unexpected size {width}x{height}"))
}

#[test]
fn extract_image_returns_the_expected_encoded_file() {
    let want = expected();
    let doc = Document::from_bytes(KNOWN_PDF).expect("fixture opens");
    let refs = doc.get_page_images(0, false).expect("list page 0");
    assert_eq!(refs.len(), 2, "fixture holds two images");
    for r in &refs {
        let w = for_size(&want, r.width as u32, r.height as u32);
        let image = doc.extract_image(r.xref).expect("extract_image");
        assert!(
            image.ext == "png" || image.ext == "jpeg",
            "ext is a file kind, got '{}'",
            image.ext
        );
        assert_eq!(image.width as u32, w.width);
        assert_eq!(image.height as u32, w.height);
        assert_eq!(image.colorspace as u8, w.components);
        assert_eq!(norm(&image.colorspace_name), norm(&w.colorspace));
        assert_eq!(image.bits as u8, w.bpc);
        if image.ext == "png" {
            assert!(
                image.data.starts_with(PNG_MAGIC),
                "png ext matches PNG magic"
            );
            let decoder = png::Decoder::new(std::io::Cursor::new(&image.data));
            let mut reader = decoder.read_info().expect("png header parses");
            let (width, height) = (reader.info().width, reader.info().height);
            let components = match reader.info().color_type {
                png::ColorType::Grayscale => 1,
                png::ColorType::Rgb => 3,
                other => panic!("unexpected PNG color type for this fixture: {other:?}"),
            };
            assert_eq!(
                reader.info().bit_depth,
                png::BitDepth::Eight,
                "8-bit samples, like the fixture"
            );
            let mut samples = vec![0; reader.output_buffer_size()];
            reader.next_frame(&mut samples).expect("png body parses");
            assert_eq!(width, w.width);
            assert_eq!(height, w.height);
            assert_eq!(components, w.components);
            assert_eq!(
                samples.len(),
                w.width as usize * w.height as usize * usize::from(w.components)
            );
            assert_eq!(hex_sha256(&samples), w.sha256);
        } else {
            assert!(
                image.data.starts_with(JPEG_MAGIC),
                "jpeg ext matches JPEG magic"
            );
        }
    }
}
