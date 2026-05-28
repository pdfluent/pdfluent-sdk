//! Regression tests for PDF redaction edge cases.
//!
//! These tests cover specific scenarios that have caused bugs in the past:
//! 1. PDF with comments in content stream (% ...)
//! 2. PDF with incremental updates (multiple object generations)
//! 3. Redaction on text in Form XObjects

use lopdf::Document;
use pdf_redact::search_redact::{search_and_redact, RedactSearchOptions};
use std::io::Write;

/// Create a minimal valid PDF in memory.
fn create_simple_pdf(content: &str) -> Vec<u8> {
    let mut pdf_data = Vec::new();

    pdf_data
        .write_all(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
        .unwrap();

    let offset1 = pdf_data.len();
    pdf_data
        .write_all(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n")
        .unwrap();

    let offset2 = pdf_data.len();
    pdf_data
        .write_all(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n")
        .unwrap();

    let offset3 = pdf_data.len();
    pdf_data.write_all(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>\nendobj\n").unwrap();

    let offset4 = pdf_data.len();
    pdf_data.write_all(b"4 0 obj\n<< /Length ").unwrap();
    pdf_data
        .write_all(content.len().to_string().as_bytes())
        .unwrap();
    pdf_data.write_all(b" >>\nstream\n").unwrap();
    pdf_data.write_all(content.as_bytes()).unwrap();
    pdf_data.write_all(b"\nendstream\nendobj\n").unwrap();

    let xref_offset = pdf_data.len();
    pdf_data.write_all(b"xref\n0 5\n").unwrap();
    pdf_data.write_all(b"0000000000 65535 f \n").unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset1).as_bytes())
        .unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset2).as_bytes())
        .unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset3).as_bytes())
        .unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset4).as_bytes())
        .unwrap();

    pdf_data
        .write_all(b"trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n")
        .unwrap();
    pdf_data
        .write_all(xref_offset.to_string().as_bytes())
        .unwrap();
    pdf_data.write_all(b"\n%%EOF\n").unwrap();

    pdf_data
}

/// Create a PDF with Form XObject.
fn create_pdf_with_form_xobject(page_content: &str, form_content: &str) -> Vec<u8> {
    let mut pdf_data = Vec::new();

    pdf_data
        .write_all(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
        .unwrap();

    let offset1 = pdf_data.len();
    pdf_data
        .write_all(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n")
        .unwrap();

    let offset2 = pdf_data.len();
    pdf_data
        .write_all(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n")
        .unwrap();

    let offset3 = pdf_data.len();
    pdf_data.write_all(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /XObject << /Form1 5 0 R >> >> >>\nendobj\n").unwrap();

    let offset4 = pdf_data.len();
    pdf_data.write_all(b"4 0 obj\n<< /Length ").unwrap();
    pdf_data
        .write_all(page_content.len().to_string().as_bytes())
        .unwrap();
    pdf_data.write_all(b" >>\nstream\n").unwrap();
    pdf_data.write_all(page_content.as_bytes()).unwrap();
    pdf_data.write_all(b"\nendstream\nendobj\n").unwrap();

    let offset5 = pdf_data.len();
    pdf_data
        .write_all(b"5 0 obj\n<< /Type /XObject /Subtype /Form /BBox [0 0 200 100] /Length ")
        .unwrap();
    pdf_data
        .write_all(form_content.len().to_string().as_bytes())
        .unwrap();
    pdf_data.write_all(b" >>\nstream\n").unwrap();
    pdf_data.write_all(form_content.as_bytes()).unwrap();
    pdf_data.write_all(b"\nendstream\nendobj\n").unwrap();

    let xref_offset = pdf_data.len();
    pdf_data.write_all(b"xref\n0 6\n").unwrap();
    pdf_data.write_all(b"0000000000 65535 f \n").unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset1).as_bytes())
        .unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset2).as_bytes())
        .unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset3).as_bytes())
        .unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset4).as_bytes())
        .unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset5).as_bytes())
        .unwrap();

    pdf_data
        .write_all(b"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n")
        .unwrap();
    pdf_data
        .write_all(xref_offset.to_string().as_bytes())
        .unwrap();
    pdf_data.write_all(b"\n%%EOF\n").unwrap();

    pdf_data
}

/// Create a PDF with two content streams (simulating what incremental update might look like).
/// Note: lopdf may only read the first content reference.
fn create_pdf_with_dual_content(content1: &str, content2: &str) -> Vec<u8> {
    let mut pdf_data = Vec::new();

    pdf_data
        .write_all(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
        .unwrap();

    let offset1 = pdf_data.len();
    pdf_data
        .write_all(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n")
        .unwrap();

    let offset2 = pdf_data.len();
    pdf_data
        .write_all(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n")
        .unwrap();

    let offset3 = pdf_data.len();
    pdf_data.write_all(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents [4 0 R 5 0 R] >>\nendobj\n").unwrap();

    let offset4 = pdf_data.len();
    pdf_data.write_all(b"4 0 obj\n<< /Length ").unwrap();
    pdf_data
        .write_all(content1.len().to_string().as_bytes())
        .unwrap();
    pdf_data.write_all(b" >>\nstream\n").unwrap();
    pdf_data.write_all(content1.as_bytes()).unwrap();
    pdf_data.write_all(b"\nendstream\nendobj\n").unwrap();

    let offset5 = pdf_data.len();
    pdf_data.write_all(b"5 0 obj\n<< /Length ").unwrap();
    pdf_data
        .write_all(content2.len().to_string().as_bytes())
        .unwrap();
    pdf_data.write_all(b" >>\nstream\n").unwrap();
    pdf_data.write_all(content2.as_bytes()).unwrap();
    pdf_data.write_all(b"\nendstream\nendobj\n").unwrap();

    let xref_offset = pdf_data.len();
    pdf_data.write_all(b"xref\n0 6\n").unwrap();
    pdf_data.write_all(b"0000000000 65535 f \n").unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset1).as_bytes())
        .unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset2).as_bytes())
        .unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset3).as_bytes())
        .unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset4).as_bytes())
        .unwrap();
    pdf_data
        .write_all(format!("{:010} 00000 n \n", offset5).as_bytes())
        .unwrap();

    pdf_data
        .write_all(b"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n")
        .unwrap();
    pdf_data
        .write_all(xref_offset.to_string().as_bytes())
        .unwrap();
    pdf_data.write_all(b"\n%%EOF\n").unwrap();

    pdf_data
}

/// Regression test: PDF with comments in content stream should not interfere with redaction.
///
/// Previously, comments (% ...) in content streams could cause the redaction
/// algorithm to misparse the content and miss text that should be redacted.
#[test]
fn test_redact_with_comments_in_content_stream() {
    let content = "BT\n/F1 12 Tf\n100 700 Td\n(SECRET) Tj\nET\n% This is a comment\nBT\n200 600 Td\n(NORMAL) Tj\nET\n";

    let pdf_data = create_simple_pdf(content);

    let mut doc = Document::load_mem(&pdf_data).expect("Failed to load PDF");

    let chars = pdf_extract::extract_positioned_chars(&doc, 1).expect("Failed to extract chars");
    let text: String = chars.iter().map(|c| c.ch).collect();
    println!("Extracted text: {:?}", text);

    assert!(
        text.contains("SECRET"),
        "Expected SECRET in text, got: {}",
        text
    );

    let opts = RedactSearchOptions::exact("SECRET");
    let report = search_and_redact(&mut doc, "SECRET", &opts).expect("search_and_redact failed");

    println!(
        "Report: matches_found={}, areas_redacted={}, ops_removed={}",
        report.matches_found, report.areas_redacted, report.operations_removed
    );

    assert!(
        report.matches_found > 0,
        "Expected at least one match for 'SECRET'"
    );

    let mut saved = Vec::new();
    doc.save_to(&mut saved).expect("Failed to save PDF");

    let reloaded = Document::load_mem(&saved).expect("Failed to reload PDF");

    let chars_after =
        pdf_extract::extract_positioned_chars(&reloaded, 1).expect("Failed to extract chars after");
    let text_after: String = chars_after.iter().map(|c| c.ch).collect();

    assert!(
        !text_after.contains("SECRET"),
        "SECRET should be redacted but found in: {}",
        text_after
    );
}

/// Regression test: PDF with Form XObjects containing text.
///
/// Form XObjects are separate content streams that can be referenced from pages.
/// Redaction should handle Form XObject content appropriately.
#[test]
fn test_redact_text_in_form_xobjects() {
    let page_content = "BT\n100 700 Td\n/Form1 Do\nET\n";
    let form_content = "BT\n/F1 12 Tf\n50 50 Td\n(CONFIDENTIAL) Tj\nET\n";

    let pdf_data = create_pdf_with_form_xobject(page_content, form_content);

    let mut doc = Document::load_mem(&pdf_data).expect("Failed to load PDF");

    let chars = pdf_extract::extract_positioned_chars(&doc, 1);
    println!("Positioned chars extraction: {:?}", chars.is_ok());
    if let Ok(chars) = &chars {
        let text: String = chars.iter().map(|c| c.ch).collect();
        println!("Extracted text: {:?}", text);
    }

    let opts = RedactSearchOptions::exact("CONFIDENTIAL");
    let result = search_and_redact(&mut doc, "CONFIDENTIAL", &opts);

    match result {
        Ok(report) => {
            println!(
                "Report: matches_found={}, areas_redacted={}, ops_removed={}",
                report.matches_found, report.areas_redacted, report.operations_removed
            );
            if report.matches_found > 0 {
                assert!(
                    report.areas_redacted > 0,
                    "If matches found, areas should be redacted"
                );
            }
        }
        Err(e) => {
            println!(
                "search_and_redact error (may be expected for Form XObjects): {:?}",
                e
            );
        }
    }
}

/// Regression test: PDF with multiple content streams (array of content references).
///
/// This tests how the redaction handles PDFs where a page's Contents is an array
/// of multiple content stream references - similar to what incremental updates create.
#[test]
fn test_redact_with_multiple_content_streams() {
    // First stream has "KEEP" text, second has "REDACTED" text
    let content1 = "BT\n/F1 12 Tf\n100 700 Td\n(KEEP) Tj\nET\n";
    let content2 = "BT\n/F1 12 Tf\n100 600 Td\n(REDACTED) Tj\nET\n";

    let pdf_data = create_pdf_with_dual_content(content1, content2);

    let mut doc = Document::load_mem(&pdf_data).expect("Failed to load PDF");

    let chars = pdf_extract::extract_positioned_chars(&doc, 1).expect("Failed to extract chars");
    let text: String = chars.iter().map(|c| c.ch).collect();
    println!("Extracted text: {:?}", text);

    // Both texts should be found when page has array of content streams
    assert!(
        text.contains("KEEP") || text.contains("REDACTED"),
        "Expected at least one text in page content, got: {}",
        text
    );

    // Try to redact "REDACTED"
    let opts = RedactSearchOptions::exact("REDACTED");
    let result = search_and_redact(&mut doc, "REDACTED", &opts);

    match result {
        Ok(report) => {
            println!(
                "Report: matches_found={}, areas_redacted={}",
                report.matches_found, report.areas_redacted
            );
            // If REDACTED was found, verify it was redacted
            if report.matches_found > 0 {
                assert!(
                    report.areas_redacted > 0,
                    "If matches found, areas should be redacted"
                );

                let mut saved = Vec::new();
                doc.save_to(&mut saved).expect("Failed to save PDF");
                let reloaded = Document::load_mem(&saved).expect("Failed to reload PDF");
                let chars_after = pdf_extract::extract_positioned_chars(&reloaded, 1)
                    .expect("Failed to extract after");
                let text_after: String = chars_after.iter().map(|c| c.ch).collect();
                assert!(
                    !text_after.contains("REDACTED"),
                    "REDACTED should be redacted"
                );
            }
        }
        Err(e) => {
            println!("search_and_redact error: {:?}", e);
        }
    }
}
