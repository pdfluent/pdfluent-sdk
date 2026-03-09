//! PDF table extraction and XLSX conversion.
//!
//! Detects tables in PDF documents via spatial analysis of text blocks
//! and writes them to Excel XLSX format using rust_xlsxwriter.

pub mod error;
pub mod table;

pub use error::{Result, XlsxError};
pub use table::{CellValue, DetectedTable};

use lopdf::Document;
use pdf_extract::extract_text;
use rust_xlsxwriter::Workbook;
use table::detect_tables;

/// Convert tables from a PDF document to XLSX format.
///
/// Returns the XLSX file contents as bytes.
/// Each page's tables become a separate worksheet.
pub fn pdf_to_xlsx(doc: &Document) -> Result<Vec<u8>> {
    let pages = doc.get_pages();
    let total_pages = pages.len() as u32;
    let text_blocks = extract_text(doc);

    let mut workbook = Workbook::new();
    let mut sheet_count = 0;

    for page_num in 1..=total_pages {
        let page_blocks: Vec<_> = text_blocks
            .iter()
            .filter(|b| b.page == page_num)
            .cloned()
            .collect();

        let tables = detect_tables(&page_blocks, page_num);

        for (table_idx, table) in tables.iter().enumerate() {
            sheet_count += 1;
            let sheet_name = if tables.len() == 1 {
                format!("Page {page_num}")
            } else {
                format!("Page {} Table {}", page_num, table_idx + 1)
            };

            let worksheet = workbook.add_worksheet();
            worksheet.set_name(&sheet_name)?;

            write_table_to_sheet(worksheet, table)?;
        }
    }

    // If no tables found, create an empty sheet.
    if sheet_count == 0 {
        let worksheet = workbook.add_worksheet();
        worksheet.set_name("Sheet1")?;
    }

    let buf = workbook.save_to_buffer()?;
    Ok(buf)
}

/// Convert PDF bytes to XLSX format.
pub fn convert_pdf_bytes_to_xlsx(pdf_bytes: &[u8]) -> Result<Vec<u8>> {
    let doc = Document::load_mem(pdf_bytes)?;
    pdf_to_xlsx(&doc)
}

/// Extract all tables from a PDF document without writing XLSX.
pub fn extract_tables(doc: &Document) -> Vec<DetectedTable> {
    let pages = doc.get_pages();
    let total_pages = pages.len() as u32;
    let text_blocks = extract_text(doc);
    let mut all_tables = Vec::new();

    for page_num in 1..=total_pages {
        let page_blocks: Vec<_> = text_blocks
            .iter()
            .filter(|b| b.page == page_num)
            .cloned()
            .collect();

        all_tables.extend(detect_tables(&page_blocks, page_num));
    }

    all_tables
}

/// Write a detected table to an xlsxwriter worksheet.
fn write_table_to_sheet(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    table: &DetectedTable,
) -> Result<()> {
    for (row_idx, row) in table.rows.iter().enumerate() {
        for (col_idx, cell) in row.iter().enumerate() {
            let r = row_idx as u32;
            let c = col_idx as u16;

            match cell {
                CellValue::Number(n) => {
                    worksheet.write_number(r, c, *n)?;
                }
                CellValue::Text(s) => {
                    worksheet.write_string(r, c, s)?;
                }
                CellValue::Empty => {}
            }
        }
    }

    // Auto-fit columns for readability.
    for col in 0..table.col_count as u16 {
        worksheet.set_column_width(col, 15)?;
    }

    Ok(())
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

    #[test]
    fn convert_table_to_xlsx() {
        // Two columns of data: simulated via text positioning.
        let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (Name) Tj 1 0 0 1 200 700 Tm (Age) Tj 1 0 0 1 72 684 Tm (Alice) Tj 1 0 0 1 200 684 Tm (30) Tj 1 0 0 1 72 668 Tm (Bob) Tj 1 0 0 1 200 668 Tm (25) Tj ET";
        let doc = make_test_pdf(content);
        let xlsx = pdf_to_xlsx(&doc).unwrap();
        assert!(xlsx.len() > 100);
    }

    #[test]
    fn extract_tables_api() {
        let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (A) Tj 1 0 0 1 200 700 Tm (B) Tj 1 0 0 1 72 684 Tm (1) Tj 1 0 0 1 200 684 Tm (2) Tj ET";
        let doc = make_test_pdf(content);
        let tables = extract_tables(&doc);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].col_count, 2);
    }

    #[test]
    fn no_tables_still_produces_xlsx() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Just text) Tj ET");
        let xlsx = pdf_to_xlsx(&doc).unwrap();
        assert!(xlsx.len() > 100);
    }

    #[test]
    fn empty_pdf_produces_xlsx() {
        let doc = make_test_pdf(b"");
        let xlsx = pdf_to_xlsx(&doc).unwrap();
        assert!(xlsx.len() > 100);
    }

    #[test]
    fn xlsx_is_valid_zip() {
        let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (Name) Tj 1 0 0 1 200 700 Tm (Age) Tj 1 0 0 1 72 684 Tm (Alice) Tj 1 0 0 1 200 684 Tm (30) Tj ET";
        let doc = make_test_pdf(content);
        let xlsx = pdf_to_xlsx(&doc).unwrap();

        assert_eq!(&xlsx[0..2], b"PK");
        let cursor = std::io::Cursor::new(&xlsx);
        assert!(zip::ZipArchive::new(cursor).is_ok());
    }

    #[test]
    fn xlsx_structure_has_required_files() {
        let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (A) Tj 1 0 0 1 200 700 Tm (B) Tj 1 0 0 1 72 684 Tm (1) Tj 1 0 0 1 200 684 Tm (2) Tj ET";
        let doc = make_test_pdf(content);
        let xlsx = pdf_to_xlsx(&doc).unwrap();
        let names = zip_file_names(&xlsx);

        assert!(
            names.contains(&"[Content_Types].xml".to_string()),
            "Missing [Content_Types].xml, found: {names:?}"
        );
        // XLSX must have at least one worksheet.
        assert!(
            names.iter().any(|n| n.starts_with("xl/worksheets/")),
            "No worksheet found, entries: {names:?}"
        );
    }

    #[test]
    fn xlsx_content_types_valid() {
        let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (X) Tj 1 0 0 1 200 700 Tm (Y) Tj ET";
        let doc = make_test_pdf(content);
        let xlsx = pdf_to_xlsx(&doc).unwrap();
        let ct = read_zip_entry(&xlsx, "[Content_Types].xml").unwrap();

        assert!(ct.contains("ContentType"));
        assert!(ct.contains("spreadsheetml"));
    }

    #[test]
    fn xlsx_worksheet_xml_parseable() {
        let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (Data) Tj 1 0 0 1 200 700 Tm (123) Tj ET";
        let doc = make_test_pdf(content);
        let xlsx = pdf_to_xlsx(&doc).unwrap();

        // Find the first worksheet.
        let names = zip_file_names(&xlsx);
        let sheet = names
            .iter()
            .find(|n| n.starts_with("xl/worksheets/sheet"))
            .expect("No worksheet found");

        let xml = read_zip_entry(&xlsx, sheet).unwrap();
        let mut reader = quick_xml::Reader::from_str(&xml);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => panic!("Invalid XML in {sheet}: {e}"),
                _ => {}
            }
            buf.clear();
        }
    }

    #[test]
    fn xlsx_table_data_text_preserved() {
        let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (Name) Tj 1 0 0 1 200 700 Tm (Age) Tj 1 0 0 1 72 684 Tm (Alice) Tj 1 0 0 1 200 684 Tm (30) Tj 1 0 0 1 72 668 Tm (Bob) Tj 1 0 0 1 200 668 Tm (25) Tj ET";
        let doc = make_test_pdf(content);
        let tables = extract_tables(&doc);

        // Verify the extracted table has the expected content.
        assert!(!tables.is_empty(), "No tables detected");
        let all_text: String = tables
            .iter()
            .flat_map(|t| t.rows.iter())
            .flat_map(|r| r.iter())
            .map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join(" ");

        assert!(
            all_text.contains("Name"),
            "Missing 'Name' in table: {all_text}"
        );
        assert!(
            all_text.contains("Alice"),
            "Missing 'Alice' in table: {all_text}"
        );
    }

    #[test]
    fn xlsx_empty_produces_valid_structure() {
        let doc = make_test_pdf(b"");
        let xlsx = pdf_to_xlsx(&doc).unwrap();

        assert_eq!(&xlsx[0..2], b"PK");
        let names = zip_file_names(&xlsx);
        assert!(names.contains(&"[Content_Types].xml".to_string()));
        assert!(
            names.iter().any(|n| n.starts_with("xl/worksheets/")),
            "Empty XLSX should still have a worksheet"
        );
    }
}
