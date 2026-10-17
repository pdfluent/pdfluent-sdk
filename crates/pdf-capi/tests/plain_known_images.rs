//! C ABI plain known-images on the shared construction fixture (#643).
//!
//! `pdf_page_images_json` plus `pdf_image_decode_pixels` over
//! fixtures/known-images.pdf, checked against
//! fixtures/known-images.expected.json: count, dimensions, colour space,
//! components, and the decoded-pixel sha256. `pdf_image_extract_data`
//! rides along to pin the encoded-vs-pixels split (PNG bytes, not samples).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::ffi::CStr;
use std::mem::MaybeUninit;
use std::ptr;
use std::slice;

use pdf_capi::{
    pdf_decoded_image_free, pdf_document_free, pdf_document_open_from_bytes,
    pdf_extracted_image_free, pdf_image_decode_pixels, pdf_image_extract_data,
    pdf_page_images_json, pdf_string_free, PdfDecodedImage, PdfDocument, PdfExtractedImage,
    PdfStatus,
};
use sha2::{Digest, Sha256};

const KNOWN_PDF: &[u8] = include_bytes!("../../../fixtures/known-images.pdf");
const EXPECTED_JSON: &str = include_str!("../../../fixtures/known-images.expected.json");

struct Want {
    width: u32,
    height: u32,
    colorspace: String,
    components: u32,
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
            components: el["components"].as_u64().expect("components") as u32,
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

fn open_fixture() -> *mut PdfDocument {
    let mut doc: *mut PdfDocument = ptr::null_mut();
    let rc = unsafe { pdf_document_open_from_bytes(KNOWN_PDF.as_ptr(), KNOWN_PDF.len(), &mut doc) };
    assert_eq!(rc, PdfStatus::Ok, "open_from_bytes(fixture) failed");
    assert!(!doc.is_null());
    doc
}

fn read_str(s: *const std::os::raw::c_char) -> String {
    assert!(!s.is_null());
    unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
}

fn listing_ids(doc: *mut PdfDocument) -> Vec<(u32, u32, u32)> {
    let json = unsafe { pdf_page_images_json(doc, 0) };
    assert!(!json.is_null());
    let text = read_str(json);
    unsafe { pdf_string_free(json) };
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("listing parses as JSON");
    let images = parsed.as_array().expect("listing is an array");
    assert_eq!(images.len(), 2);
    let mut out: Vec<(u32, u32, u32)> = images
        .iter()
        .map(|el| {
            (
                el["id"].as_u64().expect("id") as u32,
                el["width"].as_u64().expect("width") as u32,
                el["height"].as_u64().expect("height") as u32,
            )
        })
        .collect();
    out.sort_by_key(|&(_, w, h)| (w, h));
    out
}

#[test]
fn page_images_json_lists_the_shared_fixture() {
    let want = expected();
    let doc = open_fixture();
    let json = unsafe { pdf_page_images_json(doc, 0) };
    assert!(!json.is_null());
    let text = read_str(json);
    unsafe { pdf_string_free(json) };
    unsafe { pdf_document_free(doc) };
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("listing parses as JSON");
    let images = parsed.as_array().expect("listing is an array");
    assert_eq!(images.len(), want.len());
    let mut got: Vec<&serde_json::Value> = images.iter().collect();
    got.sort_by_key(|el| {
        (
            el["width"].as_u64().unwrap_or(0),
            el["height"].as_u64().unwrap_or(0),
        )
    });
    for (el, w) in got.iter().zip(want.iter()) {
        assert_eq!(el["width"].as_u64().unwrap_or(0) as u32, w.width);
        assert_eq!(el["height"].as_u64().unwrap_or(0) as u32, w.height);
        assert_eq!(el["components"].as_u64().unwrap_or(0) as u32, w.components);
        let space = el["color_space"].as_str().expect("color_space");
        assert_eq!(norm(space), norm(&w.colorspace));
    }
}

#[test]
fn decode_pixels_matches_the_expected_sha() {
    let want = expected();
    let doc = open_fixture();
    let ids = listing_ids(doc);
    for (id, width, height) in ids {
        let w = want
            .iter()
            .find(|w| w.width == width && w.height == height)
            .unwrap_or_else(|| panic!("unexpected size {width}x{height}"));
        let mut out = MaybeUninit::<PdfDecodedImage>::uninit();
        let rc = unsafe { pdf_image_decode_pixels(doc, id, out.as_mut_ptr()) };
        assert_eq!(rc, PdfStatus::Ok);
        let image = unsafe { out.assume_init() };
        assert_eq!(image.width, w.width);
        assert_eq!(image.height, w.height);
        assert_eq!(image.components, w.components);
        assert_eq!(norm(&read_str(image.color_space)), norm(&w.colorspace));
        let pixels = unsafe { slice::from_raw_parts(image.pixels, image.len) };
        let expect_len = w.width as usize * w.height as usize * w.components as usize;
        assert_eq!(pixels.len(), expect_len);
        assert_eq!(hex_sha256(pixels), w.sha256);
        let mut image = image;
        unsafe { pdf_decoded_image_free(&mut image) };
    }
    unsafe { pdf_document_free(doc) };
}

#[test]
fn extract_data_is_png_not_pixels() {
    let doc = open_fixture();
    let ids = listing_ids(doc);
    for (id, _, _) in ids {
        let mut out = MaybeUninit::<PdfExtractedImage>::uninit();
        let rc = unsafe { pdf_image_extract_data(doc, id, out.as_mut_ptr()) };
        assert_eq!(rc, PdfStatus::Ok);
        let image = unsafe { out.assume_init() };
        assert_eq!(read_str(image.encoding), "png");
        let data = unsafe { slice::from_raw_parts(image.data, image.len) };
        assert_eq!(&data[..4], &[0x89, b'P', b'N', b'G']);
        let mut image = image;
        unsafe { pdf_extracted_image_free(&mut image) };
    }
    unsafe { pdf_document_free(doc) };
}
