//! PDF table extraction and XLSX conversion.
//!
//! Detects tables in PDF documents via spatial analysis of text blocks
//! and writes them to Excel XLSX format using rust_xlsxwriter.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod error;
pub mod table;

pub use error::{Result, XlsxError};
pub use table::{CellValue, DetectedTable};

use lopdf::{Document, Object};
use pdf_extract::extract_text;
use rust_xlsxwriter::{DocProperties, ExcelDateTime, Workbook};
use table::detect_tables;

/// Maximum number of pages to process for XLSX conversion. Massive documents
/// (e.g. 500+ pages) cause timeouts and produce unwieldy spreadsheets.
const MAX_XLSX_PAGES: u32 = 500;

/// Pin the workbook's creation time to the document, never the wall clock.
///
/// `DocProperties::new()` stamps `ExcelDateTime::utc_now()` into the document
/// properties (rust_xlsxwriter `properties.rs`), so two exports of the same
/// PDF differ -- by one byte after deflate, which is enough to fail any
/// comparison and looks like anything but a clock. It cost wave 34410536849 a
/// red `pdf-capi` suite that read as a licence check coming back (#495), and
/// then a flaky landing gate comparing two export lengths at 4936 against
/// 4937 (#477).
///
/// The stamp comes from the source PDF's `/CreationDate` when there is one,
/// and from a fixed epoch otherwise. Same input, byte-identical output; the
/// environment cannot move it. `DocProperties::new()` still samples the clock
/// internally -- there is no clock-free constructor -- but the value is always
/// overwritten here before the workbook is written, so it never reaches the
/// file.
fn workbook_properties(doc: &Document) -> Option<DocProperties> {
    let stamp = creation_datetime(doc).or_else(fixed_epoch)?;
    Some(DocProperties::new().set_creation_datetime(&stamp))
}

/// The creation stamp for a PDF with no usable `/CreationDate`.
///
/// 1970-01-01T00:00:00Z: the Unix epoch, deliberately boring and visibly a
/// placeholder rather than a real export time.
fn fixed_epoch() -> Option<ExcelDateTime> {
    ExcelDateTime::from_timestamp(0).ok()
}

/// Read the workbook creation stamp from the PDF's `/CreationDate`.
///
/// Returns `None` when the trailer has no `/Info`, the info dict has no
/// `/CreationDate`, or the value does not parse -- any of those still exports,
/// on the fixed epoch.
fn creation_datetime(doc: &Document) -> Option<ExcelDateTime> {
    let info = info_dict(doc)?;
    let raw = match info.get(b"CreationDate").ok()? {
        Object::String(bytes, _) => std::str::from_utf8(bytes).ok()?,
        _ => return None,
    };
    let timestamp = parse_pdf_date_to_timestamp(raw.trim())?;
    ExcelDateTime::from_timestamp(timestamp).ok()
}

/// Resolve the trailer's `/Info` dictionary, direct or indirect.
fn info_dict(doc: &Document) -> Option<&lopdf::Dictionary> {
    match doc.trailer.get(b"Info").ok()? {
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Dictionary(dict) => Some(dict),
            _ => None,
        },
        Object::Dictionary(dict) => Some(dict),
        _ => None,
    }
}

/// Convert a PDF date string to a Unix timestamp in UTC.
///
/// Accepts the `D:YYYYMMDDHHmmSSOHH'mm'` form of PDF 32000-1 §7.9.4 and every
/// truncation of it down to the bare year, with or without the `D:` prefix;
/// a missing month or day defaults to 1, missing time fields to 0. A missing
/// offset means GMT per the spec, `Z` means UTC, and a signed offset is
/// applied so the stamp is the same instant the PDF names. Anything malformed
/// or out of range returns `None` and the caller falls back to the fixed
/// epoch rather than refusing the export.
fn parse_pdf_date_to_timestamp(raw: &str) -> Option<i64> {
    let s = raw.strip_prefix("D:").unwrap_or(raw);
    let tz_start = s.find(['Z', 'z', '+', '-']);
    let (date_part, tz_part) = match tz_start {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    };
    if !(4..=14).contains(&date_part.len())
        || date_part.len() % 2 != 0
        || !date_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }

    let year: u16 = date_part[0..4].parse().ok()?;
    let month: u8 = if date_part.len() >= 6 {
        date_part[4..6].parse().ok()?
    } else {
        1
    };
    let day: u8 = if date_part.len() >= 8 {
        date_part[6..8].parse().ok()?
    } else {
        1
    };
    let hour: u8 = if date_part.len() >= 10 {
        date_part[8..10].parse().ok()?
    } else {
        0
    };
    let minute: u8 = if date_part.len() >= 12 {
        date_part[10..12].parse().ok()?
    } else {
        0
    };
    let second: u8 = if date_part.len() >= 14 {
        date_part[12..14].parse().ok()?
    } else {
        0
    };

    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }

    let offset = parse_pdf_tz_offset(tz_part)?;
    let days = days_from_civil(i64::from(year), i64::from(month), i64::from(day));
    Some(
        days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second)
            - offset,
    )
}

/// Convert a PDF date timezone suffix to seconds east of UTC.
///
/// Empty means GMT per the spec, `Z` means UTC; otherwise a sign, two hour
/// digits, and an optional two minute digits, tolerating the `'` and `:`
/// separators producers vary on (`+02'00'`, `-0800`, `+05:30`).
fn parse_pdf_tz_offset(tz: &str) -> Option<i64> {
    if tz.is_empty() || tz.eq_ignore_ascii_case("z") {
        return Some(0);
    }
    let (sign, rest) = match tz.as_bytes().first()? {
        b'+' => (1i64, &tz[1..]),
        b'-' => (-1i64, &tz[1..]),
        _ => return None,
    };
    let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 2 && digits.len() != 4 {
        return None;
    }
    if rest
        .chars()
        .any(|c| !(c.is_ascii_digit() || c == '\'' || c == ':'))
    {
        return None;
    }
    let hours: i64 = digits[0..2].parse().ok()?;
    let minutes: i64 = if digits.len() == 4 {
        digits[2..4].parse().ok()?
    } else {
        0
    };
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(sign * (hours * 3_600 + minutes * 60))
}

/// Days from 1970-01-01 to the given civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Length of a month, with leap years.
fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let y = i64::from(year);
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Convert tables from a PDF document to XLSX format.
///
/// Returns the XLSX file contents as bytes.
/// Each page's tables become a separate worksheet.
pub fn pdf_to_xlsx(doc: &Document) -> Result<Vec<u8>> {
    let pages = doc.get_pages();
    let total_pages = pages.len() as u32;
    let total_pages = total_pages.min(MAX_XLSX_PAGES);
    let text_blocks = extract_text(doc);

    let mut workbook = Workbook::new();
    if let Some(properties) = workbook_properties(doc) {
        workbook.set_properties(&properties);
    }
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
    let total_pages = total_pages.min(MAX_XLSX_PAGES);
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
            .map(|i| archive.name_for_index(i).unwrap().to_string())
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

    /// Helper: create a PDF with table content inside a Form XObject.
    fn make_form_xobject_pdf(xobj_content: &[u8], page_content: &[u8]) -> Document {
        let mut doc = Document::with_version("1.7");
        let xobj_stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            },
            xobj_content.to_vec(),
        );
        let xobj_id = doc.add_object(Object::Stream(xobj_stream));
        let xobj_dict = dictionary! { "Fm0" => Object::Reference(xobj_id) };
        let resources = dictionary! { "XObject" => Object::Dictionary(xobj_dict) };
        let content_stream = Stream::new(dictionary! {}, page_content.to_vec());
        let content_id = doc.add_object(Object::Stream(content_stream));
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
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
    fn form_xobject_table_extraction() {
        let xobj_content = b"BT /F1 12 Tf \
            1 0 0 1 72 700 Tm (Name) Tj \
            1 0 0 1 200 700 Tm (Age) Tj \
            1 0 0 1 72 684 Tm (Alice) Tj \
            1 0 0 1 200 684 Tm (30) Tj \
            1 0 0 1 72 668 Tm (Bob) Tj \
            1 0 0 1 200 668 Tm (25) Tj ET";
        let doc = make_form_xobject_pdf(xobj_content, b"q /Fm0 Do Q");
        let tables = extract_tables(&doc);
        assert!(!tables.is_empty(), "Should detect table in Form XObject");
        let all_text: String = tables
            .iter()
            .flat_map(|t| t.rows.iter())
            .flat_map(|r| r.iter())
            .map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(all_text.contains("Name"), "Missing 'Name': {all_text}");
        assert!(all_text.contains("Alice"), "Missing 'Alice': {all_text}");
    }

    #[test]
    fn form_xobject_inherited_resources() {
        let xobj_content = b"BT /F1 12 Tf \
            1 0 0 1 72 700 Tm (X) Tj \
            1 0 0 1 200 700 Tm (Y) Tj \
            1 0 0 1 72 684 Tm (1) Tj \
            1 0 0 1 200 684 Tm (2) Tj ET";
        let mut doc = Document::with_version("1.7");
        let xobj_stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            },
            xobj_content.to_vec(),
        );
        let xobj_id = doc.add_object(Object::Stream(xobj_stream));
        let resources = dictionary! {
            "XObject" => Object::Dictionary(dictionary! {
                "Fm0" => Object::Reference(xobj_id),
            }),
        };
        let content_stream = Stream::new(dictionary! {}, b"q /Fm0 Do Q".to_vec());
        let content_id = doc.add_object(Object::Stream(content_stream));
        // Page has NO Resources — inherited from parent Pages node.
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
            "Resources" => Object::Dictionary(resources),
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

        let tables = extract_tables(&doc);
        assert!(
            !tables.is_empty(),
            "Should detect table with inherited Resources"
        );
    }

    /// Attach an `/Info` dictionary carrying `/CreationDate` to a test PDF.
    fn set_creation_date(doc: &mut Document, value: &str) {
        let info_id = doc.add_object(Object::Dictionary(dictionary! {
            "CreationDate" => Object::string_literal(value),
        }));
        doc.trailer.set("Info", Object::Reference(info_id));
    }

    /// Two exports of the same document, taken across a second boundary, are
    /// byte-identical.
    ///
    /// The workbook used to stamp `ExcelDateTime::utc_now()` into
    /// `docProps/core.xml`, so two exports a moment apart differed -- by one
    /// byte after deflate, which failed the `pdf-capi` landing gate at 4936
    /// against 4937 (#477). The 1100 ms sleep always crosses a second boundary
    /// from any start instant, and the clock has one-second resolution, so with
    /// the pinning removed this fails every run, not flakily.
    #[test]
    fn xlsx_export_is_deterministic_across_a_second_boundary() {
        let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (Name) Tj 1 0 0 1 200 700 Tm (Age) Tj 1 0 0 1 72 684 Tm (Alice) Tj 1 0 0 1 200 684 Tm (30) Tj ET";
        let doc = make_test_pdf(content);
        let first = pdf_to_xlsx(&doc).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let second = pdf_to_xlsx(&doc).unwrap();
        assert_eq!(
            first, second,
            "two exports a second apart differ: the wall clock is back in the workbook"
        );
    }

    /// The workbook stamp is the PDF's `/CreationDate`, not the export time.
    #[test]
    fn xlsx_creation_time_comes_from_the_pdf_creation_date() {
        let mut doc = make_test_pdf(b"BT /F1 12 Tf (Hi) Tj ET");
        set_creation_date(&mut doc, "D:20200304151617Z");
        let xlsx = pdf_to_xlsx(&doc).unwrap();
        let core = read_zip_entry(&xlsx, "docProps/core.xml").expect("docProps/core.xml");
        assert!(
            core.contains("2020-03-04T15:16:17Z"),
            "core.xml should carry the PDF CreationDate, got: {core}"
        );
    }

    /// A signed offset in `/CreationDate` names the same instant in UTC.
    #[test]
    fn xlsx_creation_time_applies_the_creation_date_offset() {
        let mut doc = make_test_pdf(b"BT /F1 12 Tf (Hi) Tj ET");
        set_creation_date(&mut doc, "D:20200304151617+02'00'");
        let xlsx = pdf_to_xlsx(&doc).unwrap();
        let core = read_zip_entry(&xlsx, "docProps/core.xml").expect("docProps/core.xml");
        assert!(
            core.contains("2020-03-04T13:16:17Z"),
            "core.xml should carry the CreationDate as UTC, got: {core}"
        );
    }

    /// No date, or a date that does not parse, still exports -- on the epoch.
    #[test]
    fn xlsx_creation_time_falls_back_to_a_fixed_epoch() {
        let doc = make_test_pdf(b"BT /F1 12 Tf (Hi) Tj ET");
        let xlsx = pdf_to_xlsx(&doc).unwrap();
        let core = read_zip_entry(&xlsx, "docProps/core.xml").expect("docProps/core.xml");
        assert!(
            core.contains("1970-01-01T00:00:00Z"),
            "a PDF with no CreationDate should export on the fixed epoch, got: {core}"
        );

        let mut broken = make_test_pdf(b"BT /F1 12 Tf (Hi) Tj ET");
        set_creation_date(&mut broken, "not a date");
        let xlsx = pdf_to_xlsx(&broken).unwrap();
        let core = read_zip_entry(&xlsx, "docProps/core.xml").expect("docProps/core.xml");
        assert!(
            core.contains("1970-01-01T00:00:00Z"),
            "a malformed CreationDate should fall back to the fixed epoch, got: {core}"
        );
    }
}
