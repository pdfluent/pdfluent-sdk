//! PDF to DOCX conversion with text, tables, and images.
//!
//! Extracts text blocks, images, and spatial layout from PDF documents
//! and produces valid OOXML (.docx) files.

pub mod error;
pub mod layout;
pub mod writer;

pub use error::{DocxError, Result};
pub use layout::{DocxImage, PageElement, Paragraph, Run, Table};

use layout::analyze_page;
use lopdf::Document;
use pdf_extract::{extract_page_images, extract_text, ImageFilter};
use writer::write_docx;

/// Convert a PDF document to DOCX format.
///
/// Returns the DOCX file contents as bytes.
pub fn pdf_to_docx(doc: &Document) -> Result<Vec<u8>> {
    let pages = doc.get_pages();
    let total_pages = pages.len() as u32;

    let mut all_elements: Vec<Vec<PageElement>> = Vec::new();
    let mut all_images: Vec<DocxImage> = Vec::new();

    // Extract text blocks for all pages at once.
    let text_blocks = extract_text(doc);

    for page_num in 1..=total_pages {
        // Get text blocks for this page.
        let page_blocks: Vec<_> = text_blocks
            .iter()
            .filter(|b| b.page == page_num)
            .cloned()
            .collect();

        // Layout analysis.
        let mut elements = analyze_page(&page_blocks);

        // Extract images for this page.
        if let Ok(images) = extract_page_images(doc, page_num) {
            for img in images {
                let (content_type, ext) = match img.filter {
                    ImageFilter::Jpeg => ("image/jpeg", "jpeg"),
                    _ => ("image/png", "png"),
                };

                let id = format!("image{}_{}.{}", page_num, all_images.len(), ext);

                all_images.push(DocxImage {
                    data: img.data,
                    width: img.width,
                    height: img.height,
                    content_type: content_type.to_string(),
                    id: id.clone(),
                });

                elements.push(PageElement::Img(layout::DocxImage {
                    data: Vec::new(), // data stored in all_images
                    width: img.width,
                    height: img.height,
                    content_type: content_type.to_string(),
                    id,
                }));
            }
        }

        all_elements.push(elements);
    }

    let mut output = Vec::new();
    write_docx(&all_elements, &all_images, &mut output)?;
    Ok(output)
}

/// Convert a PDF file (bytes) to DOCX format.
pub fn convert_pdf_bytes_to_docx(pdf_bytes: &[u8]) -> Result<Vec<u8>> {
    let doc = Document::load_mem(pdf_bytes)?;
    pdf_to_docx(&doc)
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
    fn convert_simple_text_pdf() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Hello World) Tj ET");
        let docx = pdf_to_docx(&doc).unwrap();
        assert!(docx.len() > 100);
        assert_eq!(&docx[0..2], b"PK"); // ZIP magic bytes
    }

    #[test]
    fn convert_multiline_pdf() {
        let doc = make_test_pdf(b"BT /F1 12 Tf 12 TL (Line 1) Tj T* (Line 2) Tj ET");
        let docx = pdf_to_docx(&doc).unwrap();
        assert!(docx.len() > 100);
    }

    #[test]
    fn convert_empty_pdf() {
        let doc = make_test_pdf(b"");
        let docx = pdf_to_docx(&doc).unwrap();
        assert!(docx.len() > 100);
    }

    #[test]
    fn convert_from_bytes() {
        let mut doc = make_test_pdf(b"BT /F1 12 Tf (Test) Tj ET");
        let mut pdf_bytes = Vec::new();
        doc.save_to(&mut pdf_bytes).unwrap();

        let docx = convert_pdf_bytes_to_docx(&pdf_bytes).unwrap();
        assert!(docx.len() > 100);
    }
}
