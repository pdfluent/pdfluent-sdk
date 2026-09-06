#![warn(missing_docs)]
//! PDF to DOCX conversion with text, tables, and images.
//!
//! Extracts text blocks, images, and spatial layout from PDF documents
//! and produces valid OOXML (.docx) files.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod error;
pub mod layout;
pub mod writer;

pub use error::{DocxError, Result};
pub use layout::{DocxImage, PageElement, Paragraph, Run, Table};

use layout::analyze_page;
use lopdf::Document;
use pdf_extract::{encode_image_for_document, extract_page_images, extract_text};
use writer::write_docx;

/// Maximum number of pages to convert to DOCX. Massive documents (e.g. 1000+
/// pages) are rarely useful as documents and cause timeouts.
const MAX_DOCX_PAGES: u32 = 1000;

/// Convert a PDF document to DOCX format.
///
/// Returns the DOCX file contents as bytes.
pub fn pdf_to_docx(doc: &Document) -> Result<Vec<u8>> {
    pdf_to_docx_inner(doc, false)
}

/// Convert a PDF document to DOCX format, text only (no images).
///
/// Skips image extraction for faster conversion when only text content
/// is needed (e.g. text-similarity tests).
pub fn pdf_to_docx_text_only(doc: &Document) -> Result<Vec<u8>> {
    pdf_to_docx_sequential(doc)
}

/// Convert PDF to DOCX preserving text in extraction (content-stream) order.
///
/// Unlike `pdf_to_docx` which sorts text spatially for visual layout,
/// this version writes text blocks in the order they appear in the content
/// stream. This produces a DOCX whose text content matches `extract_text`
/// ordering, improving roundtrip similarity scores.
fn pdf_to_docx_sequential(doc: &Document) -> Result<Vec<u8>> {
    let pages = doc.get_pages();
    let total_pages = pages.len() as u32;
    let total_pages = total_pages.min(MAX_DOCX_PAGES);
    let text_blocks = extract_text(doc);

    let mut all_elements: Vec<Vec<PageElement>> = Vec::new();

    for page_num in 1..=total_pages {
        let page_blocks: Vec<_> = text_blocks
            .iter()
            .filter(|b| b.page == page_num)
            .cloned()
            .collect();

        // Write blocks in extraction order as individual paragraphs
        // (no spatial sorting, no table detection).
        let elements: Vec<PageElement> = page_blocks
            .iter()
            .map(|b| {
                PageElement::Para(layout::Paragraph {
                    runs: vec![layout::Run {
                        text: b.text.clone(),
                        font_name: String::new(),
                        font_size: b.font_size,
                        bold: false,
                        italic: false,
                    }],
                })
            })
            .collect();

        all_elements.push(elements);
    }

    let mut output = Vec::new();
    write_docx(&all_elements, &[], &mut output)?;
    Ok(output)
}

fn pdf_to_docx_inner(doc: &Document, skip_images: bool) -> Result<Vec<u8>> {
    let pages = doc.get_pages();
    let total_pages = pages.len() as u32;
    let total_pages = total_pages.min(MAX_DOCX_PAGES);

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

        // Extract images for this page (unless skip_images is set).
        if !skip_images {
            if let Ok(images) = extract_page_images(doc, page_num) {
                for img in images {
                    // Re-encode raw PDF samples into a real PNG/JPEG. Skipping an
                    // unsupported image is correct: embedding raw bytes labelled
                    // `image/png` makes Word/PowerPoint reject the whole package.
                    let Some(encoded) = encode_image_for_document(&img) else {
                        log::warn!(
                            "pdf-docx: skipping page {page_num} image ({}x{}, cs={}, {:?}) — unsupported for embedding",
                            img.width,
                            img.height,
                            img.color_space,
                            img.filter
                        );
                        continue;
                    };

                    let id = format!("image{}_{}.{}", page_num, all_images.len(), encoded.ext);

                    all_images.push(DocxImage {
                        data: encoded.data,
                        width: img.width,
                        height: img.height,
                        content_type: encoded.mime.to_string(),
                        id: id.clone(),
                    });

                    elements.push(PageElement::Img(layout::DocxImage {
                        data: Vec::new(), // data stored in all_images
                        width: img.width,
                        height: img.height,
                        content_type: encoded.mime.to_string(),
                        id,
                    }));
                }
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

    /// A content stream for a grid of `rows` x `columns` cells, in which every
    /// cell holds several separate words.
    ///
    /// The separate words are the whole point. A fixture whose cells are a
    /// single block at exactly one x per column passes even when the detection
    /// counts every x as a column -- there are then precisely as many x-values
    /// as columns, and the test measures nothing. That is what the first
    /// version of this test looked like, and the mutation "every x is a column"
    /// simply survived it.
    ///
    /// Real tables do not look like that. A cell carries words, and each word
    /// starts at its own x. What separates a column from a word is that the
    /// column x recurs on several lines and the word x does not, so the word
    /// spacing here shifts a little per row: the second and third word x-values
    /// then do not coincide between rows.
    fn grid_content(rows: usize, columns: usize) -> Vec<u8> {
        let mut ops = String::from("BT\n/F1 11 Tf\n");
        for r in 0..rows {
            for c in 0..columns {
                let column_x = 60.0 + (c as f64) * 130.0;
                let y = 700.0 - (r as f64) * 24.0;
                // Three words per cell. The first sits on the column x -- that
                // is what makes a column a column. The other two shift per row,
                // so they can never become a second column.
                let shift = 4.0 + (r as f64) * 3.0;
                for w in 0..3usize {
                    let x = column_x + (w as f64) * (18.0 + shift);
                    let text = match w {
                        0 => format!("R{r}C{c}"),
                        1 => format!("w{r}{c}b"),
                        _ => format!("w{r}{c}c"),
                    };
                    ops.push_str(&format!("1 0 0 1 {x:.1} {y:.1} Tm\n({text}) Tj\n"));
                }
            }
        }
        ops.push_str("ET\n");
        ops.into_bytes()
    }

    /// How many columns the DOCX declares for its first table.
    fn columns_in_docx(docx: &[u8]) -> usize {
        let xml = read_zip_entry(docx, "word/document.xml").expect("word/document.xml");
        xml.matches("<w:gridCol").count()
    }

    /// A four-column table must come back with four columns, not twenty-two.
    ///
    /// The old column detection took every x-position it met, so a table got
    /// roughly one column per word. The file was valid and opened cleanly; the
    /// layout was unusable. See #161.
    ///
    /// The grid is the comparison: four columns in, four out. Without putting
    /// those two numbers side by side, "the table looks better" is an opinion.
    #[test]
    fn a_table_keeps_the_column_count_of_the_original() {
        for columns in [2usize, 3, 4, 5] {
            let doc = make_test_pdf(&grid_content(4, columns));
            let docx = pdf_to_docx(&doc).expect("conversion succeeds");

            let found = columns_in_docx(&docx);
            assert_eq!(
                found, columns,
                "a grid of 4 rows x {columns} columns came back with {found} columns"
            );
        }
    }

    /// And the content must sit in the right cell, not merely in the right count.
    ///
    /// A table with the right number of columns in which everything landed in
    /// the first cell counts just as well as a table -- and is just as unusable.
    #[test]
    fn the_cells_sit_in_the_column_they_belong_to() {
        let doc = make_test_pdf(&grid_content(3, 4));
        let docx = pdf_to_docx(&doc).expect("conversion succeeds");
        let xml = read_zip_entry(&docx, "word/document.xml").expect("word/document.xml");

        assert_eq!(columns_in_docx(&docx), 4);

        // Every cell appears exactly once.
        for r in 0..3 {
            for c in 0..4 {
                let text = format!("R{r}C{c}");
                assert_eq!(
                    xml.matches(&text).count(),
                    1,
                    "cell {text} appears {} time(s)",
                    xml.matches(&text).count()
                );
            }
        }

        // And in the right order: R0C0 before R0C1, R0C3 before R1C0.
        let at = |needle: &str| xml.find(needle).unwrap_or(usize::MAX);
        assert!(at("R0C0") < at("R0C1"), "the columns are out of order");
        assert!(at("R0C3") < at("R1C0"), "the rows are out of order");
    }

    fn zip_file_names(data: &[u8]) -> Vec<String> {
        let cursor = std::io::Cursor::new(data);
        let archive = zip::ZipArchive::new(cursor).unwrap();
        (0..archive.len())
            .map(|i| archive.name_for_index(i).unwrap().to_string())
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

    // ── embedded-image validity (the DOCX corruption regression) ───────

    /// Build a one-page PDF whose single content stream draws image `Im0`,
    /// using the given image XObject dict + (possibly compressed) stream data.
    fn make_pdf_with_image(img_dict: lopdf::Dictionary, stream_data: Vec<u8>) -> Document {
        let mut doc = Document::with_version("1.7");
        let img_id = doc.add_object(Object::Stream(Stream::new(img_dict, stream_data)));
        let resources = dictionary! {
            "XObject" => Object::Dictionary(dictionary! { "Im0" => Object::Reference(img_id) }),
        };
        let content_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            b"q 100 0 0 100 0 0 cm /Im0 Do Q".to_vec(),
        )));
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => Object::Dictionary(resources),
            "Contents" => Object::Reference(content_id),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        let pages_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc
    }

    fn flate(raw: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut e, raw).unwrap();
        e.finish().unwrap()
    }

    fn read_zip_bytes(data: &[u8], name: &str) -> Option<Vec<u8>> {
        let cursor = std::io::Cursor::new(data);
        let mut archive = zip::ZipArchive::new(cursor).ok()?;
        let mut file = archive.by_name(name).ok()?;
        let mut content = Vec::new();
        file.read_to_end(&mut content).ok()?;
        Some(content)
    }

    fn media_names(data: &[u8]) -> Vec<String> {
        zip_file_names(data)
            .into_iter()
            .filter(|n| n.starts_with("word/media/"))
            .collect()
    }

    #[test]
    fn flate_rgb_image_is_embedded_as_valid_decodable_png() {
        // 4x4 DeviceRGB raw samples, flate-compressed — the Canva case that
        // previously produced an unreadable .png and made Word reject the file.
        let img_dict = dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => 4_i64, "Height" => 4_i64,
            "BitsPerComponent" => 8_i64, "ColorSpace" => "DeviceRGB",
            "Filter" => "FlateDecode",
        };
        let docx = pdf_to_docx(&make_pdf_with_image(img_dict, flate(&[123u8; 4 * 4 * 3]))).unwrap();

        let media = media_names(&docx);
        assert_eq!(media.len(), 1, "expected one embedded image, got {media:?}");
        assert!(media[0].ends_with(".png"));
        let bytes = read_zip_bytes(&docx, &media[0]).unwrap();
        assert_eq!(
            &bytes[..8],
            b"\x89PNG\r\n\x1a\n",
            "embedded media must be a real PNG, not raw samples"
        );
        let decoded = image::load_from_memory(&bytes).expect("embedded PNG must decode");
        assert_eq!((decoded.width(), decoded.height()), (4, 4));

        let ct = read_zip_entry(&docx, "[Content_Types].xml").unwrap();
        assert!(ct.contains("Extension=\"png\""));
    }

    #[test]
    fn jpeg_image_is_embedded_as_jpeg_passthrough() {
        let jpeg = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00,
        ];
        let img_dict = dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => 8_i64, "Height" => 8_i64,
            "BitsPerComponent" => 8_i64, "ColorSpace" => "DeviceRGB",
            "Filter" => "DCTDecode",
        };
        let docx = pdf_to_docx(&make_pdf_with_image(img_dict, jpeg.clone())).unwrap();

        let media = media_names(&docx);
        assert_eq!(media.len(), 1);
        assert!(media[0].ends_with(".jpeg"));
        let bytes = read_zip_bytes(&docx, &media[0]).unwrap();
        assert_eq!(
            &bytes[..2],
            &[0xFF, 0xD8],
            "jpeg media must keep its signature"
        );
        assert_eq!(bytes, jpeg, "DCTDecode stream must be embedded verbatim");
        assert!(read_zip_entry(&docx, "[Content_Types].xml")
            .unwrap()
            .contains("image/jpeg"));
    }

    #[test]
    fn every_image_embed_resolves_to_a_relationship() {
        let img_dict = dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => 4_i64, "Height" => 4_i64,
            "BitsPerComponent" => 8_i64, "ColorSpace" => "DeviceRGB",
            "Filter" => "FlateDecode",
        };
        let docx = pdf_to_docx(&make_pdf_with_image(img_dict, flate(&[7u8; 4 * 4 * 3]))).unwrap();
        let document = read_zip_entry(&docx, "word/document.xml").unwrap();
        let rels = read_zip_entry(&docx, "word/_rels/document.xml.rels").unwrap();

        let needle = "r:embed=\"";
        let mut idx = 0;
        let mut embeds = 0;
        while let Some(pos) = document[idx..].find(needle) {
            let start = idx + pos + needle.len();
            let end = start + document[start..].find('"').unwrap();
            let id = &document[start..end];
            assert!(
                rels.contains(&format!("Id=\"{id}\"")),
                "embed {id} has no matching relationship"
            );
            embeds += 1;
            idx = end;
        }
        assert_eq!(embeds, 1, "expected exactly one image embed");
    }

    #[test]
    fn text_only_pdf_has_no_media_and_stays_valid() {
        let docx = pdf_to_docx(&make_test_pdf(b"BT /F1 12 Tf (No images here) Tj ET")).unwrap();
        assert!(
            media_names(&docx).is_empty(),
            "text-only docx must embed no media"
        );
        assert_eq!(&docx[0..2], b"PK");
    }
}
