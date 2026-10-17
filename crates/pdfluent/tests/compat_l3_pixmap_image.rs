//! Compat L3: stored-image pixmaps (#647).
//!
//! `Pixmap(doc, xref)` decodes through the same image path as extraction,
//! so a stencil reads monochrome and an RGB image does not.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::compat::pixmap_from_image_in;

/// One page with a 2x2 raw image under `cs_name`.
fn image_pdf(cs_name: &str, comps: usize, samples: Vec<u8>) -> (Document, i32) {
    assert_eq!(samples.len(), comps * 4);
    let mut doc = Document::with_version("1.7");
    let image_id = doc.new_object_id();
    doc.objects.insert(
        image_id,
        Object::Stream(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 2,
                "Height" => 2,
                "ColorSpace" => cs_name,
                "BitsPerComponent" => 8,
            },
            samples,
        )),
    );
    (doc, image_id.0 as i32)
}

#[test]
fn pixmap_from_gray_image_is_monochrome() {
    let (doc, xref) = image_pdf("DeviceGray", 1, vec![0, 128, 192, 255]);
    let pixmap = pixmap_from_image_in(&doc, xref).expect("decode");
    assert_eq!((pixmap.width(), pixmap.height()), (2, 2));
    assert!(pixmap.is_monochrome());
    assert_eq!(pixmap.samples().to_vec(), vec![0, 128, 192, 255]);
}

#[test]
fn pixmap_from_rgb_image_is_not_monochrome() {
    let samples: Vec<u8> = (0..12).collect();
    let (doc, xref) = image_pdf("DeviceRGB", 3, samples.clone());
    let pixmap = pixmap_from_image_in(&doc, xref).expect("decode");
    assert!(!pixmap.is_monochrome());
    assert_eq!(pixmap.samples().to_vec(), samples);
}

#[test]
fn pixmap_from_missing_xref_fails() {
    let (doc, _) = image_pdf("DeviceGray", 1, vec![0, 0, 0, 0]);
    assert!(pixmap_from_image_in(&doc, 9999).is_err());
}
