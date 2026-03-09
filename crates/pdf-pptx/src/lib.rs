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

    #[test]
    fn pptx_structure_has_required_files() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Structure test) Tj ET");
        let pptx = pdf_to_pptx(&doc).unwrap();
        let names = zip_file_names(&pptx);

        assert!(names.contains(&"[Content_Types].xml".to_string()));
        assert!(names.contains(&"_rels/.rels".to_string()));
        assert!(names.contains(&"ppt/presentation.xml".to_string()));
        assert!(names.contains(&"ppt/slides/slide1.xml".to_string()));
        assert!(names.contains(&"ppt/slideMasters/slideMaster1.xml".to_string()));
        assert!(names.contains(&"ppt/slideLayouts/slideLayout1.xml".to_string()));
    }

    #[test]
    fn pptx_presentation_xml_parseable() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Presentation test) Tj ET");
        let pptx = pdf_to_pptx(&doc).unwrap();
        let xml = read_zip_entry(&pptx, "ppt/presentation.xml").unwrap();

        let mut reader = quick_xml::Reader::from_str(&xml);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => panic!("Invalid XML in presentation.xml: {e}"),
                _ => {}
            }
            buf.clear();
        }
    }

    #[test]
    fn pptx_slide_xml_parseable() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Slide content) Tj ET");
        let pptx = pdf_to_pptx(&doc).unwrap();
        let xml = read_zip_entry(&pptx, "ppt/slides/slide1.xml").unwrap();

        let mut reader = quick_xml::Reader::from_str(&xml);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => panic!("Invalid XML in slide1.xml: {e}"),
                _ => {}
            }
            buf.clear();
        }
    }

    #[test]
    fn pptx_text_preserved_in_slide() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Hello World) Tj ET");
        let pptx = pdf_to_pptx(&doc).unwrap();
        let xml = read_zip_entry(&pptx, "ppt/slides/slide1.xml").unwrap();

        assert!(
            xml.contains("Hello World"),
            "Expected 'Hello World' in slide1.xml"
        );
    }

    #[test]
    fn pptx_content_types_valid() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Content types) Tj ET");
        let pptx = pdf_to_pptx(&doc).unwrap();
        let ct = read_zip_entry(&pptx, "[Content_Types].xml").unwrap();

        assert!(ct.contains("ContentType"));
        assert!(ct.contains("presentationml"));
    }

    #[test]
    fn pptx_multi_page_produces_multiple_slides() {
        // Create a 2-page PDF.
        let mut doc = Document::with_version("1.7");

        let c1 = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Page One) Tj ET".to_vec());
        let c1_id = doc.add_object(Object::Stream(c1));
        let c2 = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Page Two) Tj ET".to_vec());
        let c2_id = doc.add_object(Object::Stream(c2));

        let p1 = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(c1_id),
        };
        let p1_id = doc.add_object(Object::Dictionary(p1));

        let p2 = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(c2_id),
        };
        let p2_id = doc.add_object(Object::Dictionary(p2));

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(p1_id), Object::Reference(p2_id)],
            "Count" => 2_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));

        for pid in [p1_id, p2_id] {
            if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(pid) {
                d.set("Parent", Object::Reference(pages_id));
            }
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let pptx = pdf_to_pptx(&doc).unwrap();
        let names = zip_file_names(&pptx);

        assert!(names.contains(&"ppt/slides/slide1.xml".to_string()));
        assert!(names.contains(&"ppt/slides/slide2.xml".to_string()));
    }

    #[test]
    fn pptx_text_similarity_above_threshold() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Hello World) Tj ET");

        // Extract text from the source PDF.
        let blocks = pdf_extract::extract_text(&doc);
        let pdf_text: String = blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        // Convert to PPTX and extract text from slide XML.
        let pptx = pdf_to_pptx(&doc).unwrap();
        let xml = read_zip_entry(&pptx, "ppt/slides/slide1.xml").unwrap();

        // Extract text from a:t elements.
        let mut pptx_texts = Vec::new();
        let mut reader = quick_xml::Reader::from_str(&xml);
        let mut buf = Vec::new();
        let mut in_at = false;
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Start(e)) => {
                    in_at = e.name().as_ref() == b"a:t";
                }
                Ok(quick_xml::events::Event::Text(e)) if in_at => {
                    pptx_texts.push(e.unescape().unwrap().to_string());
                }
                Ok(quick_xml::events::Event::End(_)) => {
                    in_at = false;
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => panic!("XML parse error: {e}"),
                _ => {}
            }
            buf.clear();
        }
        let pptx_text = pptx_texts.join(" ");

        if pdf_text.len() >= 5 {
            let similarity = levenshtein_similarity(&pdf_text, &pptx_text);
            assert!(
                similarity >= 0.80,
                "Text similarity {similarity:.2} below 0.80.\n  PDF:  '{pdf_text}'\n  PPTX: '{pptx_text}'"
            );
        }
    }

    #[test]
    fn pptx_slide_master_xml_parseable() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Master test) Tj ET");
        let pptx = pdf_to_pptx(&doc).unwrap();
        let xml = read_zip_entry(&pptx, "ppt/slideMasters/slideMaster1.xml").unwrap();

        let mut reader = quick_xml::Reader::from_str(&xml);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => panic!("Invalid XML in slideMaster1.xml: {e}"),
                _ => {}
            }
            buf.clear();
        }
    }
}
