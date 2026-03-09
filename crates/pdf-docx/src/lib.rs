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
    use std::io::Read;

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

    fn read_zip_entry(data: &[u8], name: &str) -> Option<String> {
        let cursor = std::io::Cursor::new(data);
        let mut archive = zip::ZipArchive::new(cursor).ok()?;
        let mut file = archive.by_name(name).ok()?;
        let mut content = String::new();
        file.read_to_string(&mut content).ok()?;
        Some(content)
    }

    fn zip_file_names(data: &[u8]) -> Vec<String> {
        let cursor = std::io::Cursor::new(data);
        let archive = zip::ZipArchive::new(cursor).unwrap();
        (0..archive.len())
            .map(|i| {
                let cursor = std::io::Cursor::new(data);
                let archive = zip::ZipArchive::new(cursor).unwrap();
                archive.name_for_index(i).unwrap().to_string()
            })
            .collect()
    }

    fn levenshtein_similarity(a: &str, b: &str) -> f64 {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        let (m, n) = (a.len(), b.len());
        if m == 0 && n == 0 {
            return 1.0;
        }
        let mut prev: Vec<usize> = (0..=n).collect();
        let mut curr = vec![0; n + 1];
        for i in 1..=m {
            curr[0] = i;
            for j in 1..=n {
                let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
                curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
            }
            std::mem::swap(&mut prev, &mut curr);
        }
        1.0 - (prev[n] as f64 / m.max(n) as f64)
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

    #[test]
    fn docx_structure_has_required_files() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Structure test) Tj ET");
        let docx = pdf_to_docx(&doc).unwrap();
        let names = zip_file_names(&docx);

        assert!(names.contains(&"[Content_Types].xml".to_string()));
        assert!(names.contains(&"_rels/.rels".to_string()));
        assert!(names.contains(&"word/document.xml".to_string()));
        assert!(names.contains(&"word/styles.xml".to_string()));
        assert!(names.contains(&"word/_rels/document.xml.rels".to_string()));
    }

    #[test]
    fn docx_document_xml_parseable() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (XML parse test) Tj ET");
        let docx = pdf_to_docx(&doc).unwrap();
        let xml = read_zip_entry(&docx, "word/document.xml").unwrap();

        // Verify it parses as valid XML.
        let parsed = quick_xml::Reader::from_str(&xml);
        let mut buf = Vec::new();
        let mut reader = parsed;
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => panic!("Invalid XML in document.xml: {e}"),
                _ => {}
            }
            buf.clear();
        }
    }

    #[test]
    fn docx_styles_xml_parseable() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Styles test) Tj ET");
        let docx = pdf_to_docx(&doc).unwrap();
        let xml = read_zip_entry(&docx, "word/styles.xml").unwrap();

        let mut reader = quick_xml::Reader::from_str(&xml);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => panic!("Invalid XML in styles.xml: {e}"),
                _ => {}
            }
            buf.clear();
        }
    }

    #[test]
    fn docx_text_preserved() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Hello World) Tj ET");
        let docx = pdf_to_docx(&doc).unwrap();
        let xml = read_zip_entry(&docx, "word/document.xml").unwrap();

        assert!(
            xml.contains("Hello World"),
            "Expected 'Hello World' in document.xml, got: {xml}"
        );
    }

    #[test]
    fn docx_multiline_text_preserved() {
        let doc = make_test_pdf(b"BT /F1 12 Tf 12 TL (First line) Tj T* (Second line) Tj ET");
        let docx = pdf_to_docx(&doc).unwrap();
        let xml = read_zip_entry(&docx, "word/document.xml").unwrap();

        assert!(xml.contains("First line"));
        assert!(xml.contains("Second line"));
    }

    #[test]
    fn docx_table_content_in_xml() {
        let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (Name) Tj 1 0 0 1 200 700 Tm (Age) Tj 1 0 0 1 72 684 Tm (Alice) Tj 1 0 0 1 200 684 Tm (30) Tj ET";
        let doc = make_test_pdf(content);
        let docx = pdf_to_docx(&doc).unwrap();
        let xml = read_zip_entry(&docx, "word/document.xml").unwrap();

        // Table or paragraph content should contain the text.
        assert!(xml.contains("Name"));
        assert!(xml.contains("Alice"));
    }

    #[test]
    fn docx_text_similarity_above_threshold() {
        let input_text = "Hello World";
        let doc = make_test_pdf(b"BT /F1 12 Tf (Hello World) Tj ET");

        // Extract text from the source PDF via pdf-extract.
        let blocks = pdf_extract::extract_text(&doc);
        let pdf_text: String = blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        // Convert and extract text from DOCX XML.
        let docx = pdf_to_docx(&doc).unwrap();
        let xml = read_zip_entry(&docx, "word/document.xml").unwrap();

        // Extract text content from w:t elements.
        let mut docx_texts = Vec::new();
        let mut reader = quick_xml::Reader::from_str(&xml);
        let mut buf = Vec::new();
        let mut in_wt = false;
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Start(e)) => {
                    in_wt = e.name().as_ref() == b"w:t";
                }
                Ok(quick_xml::events::Event::Text(e)) if in_wt => {
                    docx_texts.push(e.unescape().unwrap().to_string());
                }
                Ok(quick_xml::events::Event::End(_)) => {
                    in_wt = false;
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => panic!("XML parse error: {e}"),
                _ => {}
            }
            buf.clear();
        }
        let docx_text = docx_texts.join(" ");

        if pdf_text.len() >= 5 {
            let similarity = levenshtein_similarity(&pdf_text, &docx_text);
            assert!(
                similarity >= 0.80,
                "Text similarity {similarity:.2} below 0.80 threshold.\n  PDF:  '{pdf_text}'\n  DOCX: '{docx_text}'"
            );
        }

        // Also check the known input text appears.
        assert!(
            docx_text.contains(input_text),
            "Expected '{input_text}' in DOCX text: '{docx_text}'"
        );
    }

    #[test]
    fn docx_content_types_valid() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Content types test) Tj ET");
        let docx = pdf_to_docx(&doc).unwrap();
        let xml = read_zip_entry(&docx, "[Content_Types].xml").unwrap();

        assert!(xml.contains("ContentType"));
        assert!(xml.contains("wordprocessingml"));
    }
}
