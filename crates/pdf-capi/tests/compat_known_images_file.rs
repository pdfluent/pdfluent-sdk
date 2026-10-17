//! C ABI compat known-images as encoded files, decoded independently (#636).
//!
//! `pdf_compat_document_extract_image` over fixtures/known-images.pdf
//! returns the image FILE (header facts plus encoded bytes), checked
//! against fixtures/known-images.expected.json: `ext` matches the
//! payload's magic bytes, and the payload decoded with the third-party
//! `png` crate yields the expected dimensions, components, and
//! decoded-pixel sha256. The compat entry points live in private modules,
//! so this test declares the C symbols itself, the way a C caller sees
//! them through `pdf_capi.h`. A lossy (JPEG) payload cannot match a pixel
//! sha by construction, so it pins only magic bytes, dimensions, and
//! components.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::ffi::{c_char, c_void, CStr};
use std::ptr;
use std::slice;

use pdf_capi::PdfStatus;
use sha2::{Digest, Sha256};

// The compat symbols as declared in pdf_capi.h; the crate keeps the
// implementing modules private, so a C caller (and this test) binds them
// by symbol name.
unsafe extern "C" {
    fn pdf_compat_document_open(
        filename: *const c_char,
        data: *const u8,
        data_len: usize,
        filetype: *const c_char,
        rect: *const c_void,
        width: f64,
        height: f64,
        fontsize: f64,
        archive: *const c_char,
        out: *mut *mut c_void,
    ) -> PdfStatus;
    fn pdf_compat_document_close(doc: *mut c_void) -> PdfStatus;
    fn pdf_compat_document_get_page_images(
        doc: *const c_void,
        page: i32,
        full: i32,
        out: *mut *mut c_void,
    ) -> PdfStatus;
    fn pdf_compat_document_extract_image(
        doc: *const c_void,
        xref: i32,
        out: *mut *mut c_void,
    ) -> PdfStatus;
    fn pdf_compat_image_ref_list_count(list: *const c_void, out: *mut i32) -> PdfStatus;
    fn pdf_compat_image_ref_list_xref(list: *const c_void, index: i32, out: *mut i32) -> PdfStatus;
    fn pdf_compat_image_ref_list_width(list: *const c_void, index: i32, out: *mut i32)
        -> PdfStatus;
    fn pdf_compat_image_ref_list_height(
        list: *const c_void,
        index: i32,
        out: *mut i32,
    ) -> PdfStatus;
    fn pdf_compat_image_ref_list_free(list: *mut c_void);
    fn pdf_compat_image_data_width(handle: *const c_void, out: *mut i32) -> PdfStatus;
    fn pdf_compat_image_data_height(handle: *const c_void, out: *mut i32) -> PdfStatus;
    fn pdf_compat_image_data_bits(handle: *const c_void, out: *mut i32) -> PdfStatus;
    fn pdf_compat_image_data_colorspace(handle: *const c_void, out: *mut i32) -> PdfStatus;
    fn pdf_compat_image_data_colorspace_name(
        handle: *const c_void,
        out: *mut *mut c_char,
    ) -> PdfStatus;
    fn pdf_compat_image_data_ext(handle: *const c_void, out: *mut *mut c_char) -> PdfStatus;
    fn pdf_compat_image_data_data(
        handle: *const c_void,
        out_data: *mut *mut u8,
        out_len: *mut usize,
    ) -> PdfStatus;
    fn pdf_compat_image_data_free(handle: *mut c_void);
    fn pdf_string_free(s: *mut c_char);
    fn pdf_bytes_free(data: *mut u8, len: usize);
}

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

fn open_fixture() -> *mut c_void {
    let mut doc: *mut c_void = ptr::null_mut();
    let rc = unsafe {
        pdf_compat_document_open(
            ptr::null(),
            KNOWN_PDF.as_ptr(),
            KNOWN_PDF.len(),
            ptr::null(),
            ptr::null(),
            0.0,
            0.0,
            0.0,
            ptr::null(),
            &mut doc,
        )
    };
    assert_eq!(rc, PdfStatus::Ok, "open_from_bytes(fixture) failed");
    assert!(!doc.is_null());
    doc
}

fn read_string(s: *mut c_char) -> String {
    assert!(!s.is_null());
    let text = unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned();
    unsafe { pdf_string_free(s) };
    text
}

fn get_i32(
    handle: *const c_void,
    get: unsafe extern "C" fn(*const c_void, *mut i32) -> PdfStatus,
) -> i32 {
    let mut out = 0;
    assert_eq!(unsafe { get(handle, &mut out) }, PdfStatus::Ok);
    out
}

fn ref_i32(
    list: *const c_void,
    index: i32,
    get: unsafe extern "C" fn(*const c_void, i32, *mut i32) -> PdfStatus,
) -> i32 {
    let mut out = 0;
    assert_eq!(unsafe { get(list, index, &mut out) }, PdfStatus::Ok);
    out
}

#[test]
fn extract_image_returns_the_expected_encoded_file() {
    let want = expected();
    let doc = open_fixture();
    let mut list: *mut c_void = ptr::null_mut();
    assert_eq!(
        unsafe { pdf_compat_document_get_page_images(doc, 0, 0, &mut list) },
        PdfStatus::Ok
    );
    assert!(!list.is_null());
    let mut count = 0;
    assert_eq!(
        unsafe { pdf_compat_image_ref_list_count(list, &mut count) },
        PdfStatus::Ok
    );
    assert_eq!(count, 2, "fixture holds two images");
    let mut refs: Vec<(i32, u32, u32)> = Vec::new();
    for index in 0..count {
        let xref = ref_i32(list, index, pdf_compat_image_ref_list_xref);
        let width = ref_i32(list, index, pdf_compat_image_ref_list_width) as u32;
        let height = ref_i32(list, index, pdf_compat_image_ref_list_height) as u32;
        refs.push((xref, width, height));
    }
    unsafe { pdf_compat_image_ref_list_free(list) };

    for (xref, width, height) in refs {
        let w = for_size(&want, width, height);
        let mut handle: *mut c_void = ptr::null_mut();
        assert_eq!(
            unsafe { pdf_compat_document_extract_image(doc, xref, &mut handle) },
            PdfStatus::Ok
        );
        assert!(!handle.is_null());
        let header_w = get_i32(handle, pdf_compat_image_data_width) as u32;
        let header_h = get_i32(handle, pdf_compat_image_data_height) as u32;
        let bits = get_i32(handle, pdf_compat_image_data_bits) as u8;
        let colorspace = get_i32(handle, pdf_compat_image_data_colorspace) as u8;
        let mut name: *mut c_char = ptr::null_mut();
        assert_eq!(
            unsafe { pdf_compat_image_data_colorspace_name(handle, &mut name) },
            PdfStatus::Ok
        );
        let colorspace_name = read_string(name);
        let mut ext_ptr: *mut c_char = ptr::null_mut();
        assert_eq!(
            unsafe { pdf_compat_image_data_ext(handle, &mut ext_ptr) },
            PdfStatus::Ok
        );
        let ext = read_string(ext_ptr);
        let mut data_ptr: *mut u8 = ptr::null_mut();
        let mut data_len: usize = 0;
        assert_eq!(
            unsafe { pdf_compat_image_data_data(handle, &mut data_ptr, &mut data_len) },
            PdfStatus::Ok
        );
        let data = unsafe { slice::from_raw_parts(data_ptr, data_len) }.to_vec();
        unsafe { pdf_bytes_free(data_ptr, data_len) };
        unsafe { pdf_compat_image_data_free(handle) };

        assert!(
            ext == "png" || ext == "jpeg",
            "ext is a file kind, got '{ext}'"
        );
        assert_eq!(header_w, w.width);
        assert_eq!(header_h, w.height);
        assert_eq!(colorspace, w.components);
        assert_eq!(norm(&colorspace_name), norm(&w.colorspace));
        assert_eq!(bits, w.bpc);
        if ext == "png" {
            assert!(data.starts_with(PNG_MAGIC), "png ext matches PNG magic");
            let decoder = png::Decoder::new(std::io::Cursor::new(&data));
            let mut reader = decoder.read_info().expect("png header parses");
            let (decoded_w, decoded_h) = (reader.info().width, reader.info().height);
            let decoded_comps = match reader.info().color_type {
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
            assert_eq!(decoded_w, w.width);
            assert_eq!(decoded_h, w.height);
            assert_eq!(decoded_comps, w.components);
            assert_eq!(
                samples.len(),
                w.width as usize * w.height as usize * usize::from(w.components)
            );
            assert_eq!(hex_sha256(&samples), w.sha256);
        } else {
            assert!(data.starts_with(JPEG_MAGIC), "jpeg ext matches JPEG magic");
        }
    }
    assert_eq!(unsafe { pdf_compat_document_close(doc) }, PdfStatus::Ok);
}
