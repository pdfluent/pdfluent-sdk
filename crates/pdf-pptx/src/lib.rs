//! PDF to PowerPoint PPTX conversion.
//!
//! Each PDF page becomes a PPTX slide with text shapes at their original
//! positions and images as picture shapes.

pub mod error;
pub mod writer;

pub use error::{PptxError, Result};
pub use writer::{PptxImage, SlideData};

use lopdf::Document;
use pdf_extract::{extract_page_images, extract_text};
use writer::{extracted_to_pptx_image, write_pptx};

/// Convert a PDF document to PPTX format.
///
/// Returns the PPTX file contents as bytes.
pub fn pdf_to_pptx(doc: &Document) -> Result<Vec<u8>> {
    let pages = doc.get_pages();
    let total_pages = pages.len() as u32;
    let text_blocks = extract_text(doc);

    let mut slides = Vec::new();
    let mut img_counter = 0;

    for page_num in 1..=total_pages {
        let page_blocks: Vec<_> = text_blocks
            .iter()
            .filter(|b| b.page == page_num)
            .cloned()
            .collect();

        // Get page dimensions.
        let page_id = pages[&page_num];
        let (page_width, page_height) = get_page_dimensions(doc, page_id);

        // Extract images.
        let mut pptx_images = Vec::new();
        if let Ok(images) = extract_page_images(doc, page_num) {
            for img in &images {
                pptx_images.push(extracted_to_pptx_image(img, img_counter));
                img_counter += 1;
            }
        }

        slides.push(SlideData {
            text_blocks: page_blocks,
            images: pptx_images,
            page_width,
            page_height,
        });
    }

    let mut output = Vec::new();
    write_pptx(&slides, &mut output)?;
    Ok(output)
}

/// Convert PDF bytes to PPTX format.
pub fn convert_pdf_bytes_to_pptx(pdf_bytes: &[u8]) -> Result<Vec<u8>> {
    let doc = Document::load_mem(pdf_bytes)?;
    pdf_to_pptx(&doc)
}

/// Get page dimensions from page dictionary, falling back to letter size.
fn get_page_dimensions(doc: &Document, page_id: lopdf::ObjectId) -> (f64, f64) {
    let page_obj = match doc.get_object(page_id) {
        Ok(obj) => obj,
        Err(_) => return (612.0, 792.0),
    };

    let dict = match page_obj {
        lopdf::Object::Dictionary(ref d) => d,
        _ => return (612.0, 792.0),
    };

    if let Ok(lopdf::Object::Array(ref arr)) = dict.get(b"MediaBox") {
        if arr.len() >= 4 {
            let w = obj_to_f64(&arr[2]).unwrap_or(612.0);
            let h = obj_to_f64(&arr[3]).unwrap_or(792.0);
            return (w, h);
        }
    }

    (612.0, 792.0)
}

fn obj_to_f64(obj: &lopdf::Object) -> Option<f64> {
    match obj {
        lopdf::Object::Integer(i) => Some(*i as f64),
        lopdf::Object::Real(f) => Some(*f as f64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};

    fn make_test_pdf(content: &[u8]) -> Document {
        let mut doc = Document::with_version("1.7");

        let content_stream = Stream::new(dictionary! {}, content.to_vec());
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn convert_simple_pdf_to_pptx() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Hello World) Tj ET");
        let pptx = pdf_to_pptx(&doc).unwrap();
        assert!(pptx.len() > 100);
        assert_eq!(&pptx[0..2], b"PK");
    }

    #[test]
    fn convert_empty_pdf_to_pptx() {
        let doc = make_test_pdf(b"");
        let pptx = pdf_to_pptx(&doc).unwrap();
        assert!(pptx.len() > 100);
    }

    #[test]
    fn convert_from_bytes() {
        let mut doc = make_test_pdf(b"BT /F1 12 Tf (Test) Tj ET");
        let mut pdf_bytes = Vec::new();
        doc.save_to(&mut pdf_bytes).unwrap();

        let pptx = convert_pdf_bytes_to_pptx(&pdf_bytes).unwrap();
        assert!(pptx.len() > 100);
    }
}
