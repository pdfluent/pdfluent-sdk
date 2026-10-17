//! C ABI plain-image extraction contract test.
//!
//! Covers `pdf_page_images_json`, `pdf_image_extract_data` +
//! `pdf_extracted_image_free` and `pdf_image_decode_pixels` +
//! `pdf_decoded_image_free` from the Rust callable side. The companion C
//! smoke (`tests/test_plain_images.c`) exercises the same scenarios
//! through the C type system after `cargo build -p pdf-capi --release`.
//!
//! The fixture is built in the test: one page holding a raw 2x2 grey
//! image, so the known pixels below are the samples the builder wrote.

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

/// Grey samples of the fixture's image: 2x2, 8-bit, raw.
const GRAY_SAMPLES: [u8; 4] = [0, 128, 192, 255];

/// Object number the builder gives the image stream.
const IMAGE_ID: u32 = 6;

/// One page with a raw 2x2 DeviceGray image under `/G0`.
fn gray_image_pdf() -> Vec<u8> {
    let content = b"q\nQ\n";
    let image = [
        format!(
            "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n",
            GRAY_SAMPLES.len()
        )
        .into_bytes(),
        GRAY_SAMPLES.to_vec(),
        b"\nendstream".to_vec(),
    ]
    .concat();
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources 5 0 R >>"
            .to_vec(),
        [
            format!("<< /Length {} >>\nstream\n", content.len()).into_bytes(),
            content.to_vec(),
            b"\nendstream".to_vec(),
        ]
        .concat(),
        b"<< /XObject << /G0 6 0 R >> >>".to_vec(),
        image,
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (number, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", number + 1).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

/// Open the fixture or fail the test with a descriptive panic.
fn open_fixture(bytes: &[u8]) -> *mut PdfDocument {
    let mut doc: *mut PdfDocument = ptr::null_mut();
    let rc = unsafe { pdf_document_open_from_bytes(bytes.as_ptr(), bytes.len(), &mut doc) };
    assert_eq!(rc, PdfStatus::Ok, "open_from_bytes(fixture) failed");
    assert!(!doc.is_null());
    doc
}

/// Read a borrowed C string the ABI returned.
fn read_str(s: *const std::os::raw::c_char) -> String {
    assert!(!s.is_null());
    unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
}

#[test]
fn page_images_json_lists_the_stored_image() {
    let bytes = gray_image_pdf();
    let doc = open_fixture(&bytes);
    let json = unsafe { pdf_page_images_json(doc, 0) };
    assert!(!json.is_null());
    let text = read_str(json);
    unsafe { pdf_string_free(json) };
    unsafe { pdf_document_free(doc) };
    assert!(text.starts_with('['), "not an array: {text}");
    for field in [
        "\"width\":2",
        "\"height\":2",
        "\"bits_per_component\":8",
        "\"components\":1",
        "\"color_space\":\"DeviceGray\"",
        "\"name\":\"G0\"",
        "\"has_mask\":false",
    ] {
        assert!(text.contains(field), "missing {field} in {text}");
    }
}

#[test]
fn page_images_json_rejects_bad_arguments() {
    let bytes = gray_image_pdf();
    let doc = open_fixture(&bytes);
    assert!(unsafe { pdf_page_images_json(ptr::null(), 0) }.is_null());
    assert!(unsafe { pdf_page_images_json(doc, -1) }.is_null());
    assert!(unsafe { pdf_page_images_json(doc, 99) }.is_null());
    unsafe { pdf_document_free(doc) };
}

#[test]
fn extract_data_returns_png_bytes_with_header() {
    let bytes = gray_image_pdf();
    let doc = open_fixture(&bytes);
    let mut out = MaybeUninit::<PdfExtractedImage>::uninit();
    let rc = unsafe { pdf_image_extract_data(doc, IMAGE_ID, out.as_mut_ptr()) };
    assert_eq!(rc, PdfStatus::Ok);
    let image = unsafe { out.assume_init() };
    assert_eq!((image.width, image.height), (2, 2));
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.components, 1);
    assert_eq!(read_str(image.color_space), "DeviceGray");
    assert_eq!(read_str(image.encoding), "png");
    assert_eq!(image.masked, 0);
    let data = unsafe { slice::from_raw_parts(image.data, image.len) };
    assert_eq!(&data[..4], &[0x89, b'P', b'N', b'G']);
    let mut image = image;
    unsafe { pdf_extracted_image_free(&mut image) };
    assert!(image.color_space.is_null());
    assert!(image.data.is_null());
    assert_eq!(image.len, 0);
    unsafe { pdf_document_free(doc) };
}

#[test]
fn extract_data_rejects_bad_arguments() {
    let bytes = gray_image_pdf();
    let doc = open_fixture(&bytes);
    let mut out = MaybeUninit::<PdfExtractedImage>::uninit();
    assert_eq!(
        unsafe { pdf_image_extract_data(doc, 9999, out.as_mut_ptr()) },
        PdfStatus::ErrorExtract
    );
    assert_eq!(
        unsafe { pdf_image_extract_data(ptr::null(), IMAGE_ID, out.as_mut_ptr()) },
        PdfStatus::ErrorInvalidArgument
    );
    assert_eq!(
        unsafe { pdf_image_extract_data(doc, IMAGE_ID, ptr::null_mut()) },
        PdfStatus::ErrorInvalidArgument
    );
    unsafe { pdf_document_free(doc) };
}

#[test]
fn decode_pixels_returns_the_fixture_samples() {
    let bytes = gray_image_pdf();
    let doc = open_fixture(&bytes);
    let mut out = MaybeUninit::<PdfDecodedImage>::uninit();
    let rc = unsafe { pdf_image_decode_pixels(doc, IMAGE_ID, out.as_mut_ptr()) };
    assert_eq!(rc, PdfStatus::Ok);
    let image = unsafe { out.assume_init() };
    assert_eq!((image.width, image.height), (2, 2));
    assert_eq!(image.components, 1);
    assert_eq!(read_str(image.color_space), "DeviceGray");
    let pixels = unsafe { slice::from_raw_parts(image.pixels, image.len) };
    assert_eq!(pixels, GRAY_SAMPLES);
    let mut image = image;
    unsafe { pdf_decoded_image_free(&mut image) };
    assert!(image.color_space.is_null());
    assert!(image.pixels.is_null());
    assert_eq!(image.len, 0);
    unsafe { pdf_document_free(doc) };
}

#[test]
fn decode_pixels_rejects_bad_arguments() {
    let bytes = gray_image_pdf();
    let doc = open_fixture(&bytes);
    let mut out = MaybeUninit::<PdfDecodedImage>::uninit();
    assert_eq!(
        unsafe { pdf_image_decode_pixels(doc, 9999, out.as_mut_ptr()) },
        PdfStatus::ErrorExtract
    );
    assert_eq!(
        unsafe { pdf_image_decode_pixels(ptr::null(), IMAGE_ID, out.as_mut_ptr()) },
        PdfStatus::ErrorInvalidArgument
    );
    assert_eq!(
        unsafe { pdf_image_decode_pixels(doc, IMAGE_ID, ptr::null_mut()) },
        PdfStatus::ErrorInvalidArgument
    );
    unsafe { pdf_document_free(doc) };
}

#[test]
fn free_functions_accept_null() {
    unsafe {
        pdf_extracted_image_free(ptr::null_mut());
        pdf_decoded_image_free(ptr::null_mut());
        pdf_string_free(ptr::null_mut());
    }
    unsafe { pdf_document_free(ptr::null_mut()) };
}
