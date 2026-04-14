//! Extract text from PDF pages.

use anyhow::{Context, Result};
use std::path::Path;

use pdf_engine::PdfDocument;

pub fn run(input: &Path, pages: Option<&str>, json: bool) -> Result<()> {
    let data = std::fs::read(input).context("failed to read input PDF")?;
    let doc = PdfDocument::open(data).context("failed to open PDF")?;
    let total = doc.page_count();
    let explicit_pages = pages.is_some();

    let page_indices = match pages {
        Some(s) => crate::parse_page_list(s, total)?,
        None => (0..total).collect(),
    };

    if json {
        let mut pages_json = Vec::new();
        for &idx in &page_indices {
            let blocks = doc
                .extract_text_blocks(idx)
                .context(format!("failed to extract text from page {}", idx + 1))?;

            let block_arr: Vec<serde_json::Value> = blocks
                .iter()
                .map(|b| {
                    let spans: Vec<serde_json::Value> = b
                        .spans
                        .iter()
                        .map(|s| {
                            serde_json::json!({
                                "text": s.text,
                                "x": s.x,
                                "y": s.y,
                                "font_size": s.font_size,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "text": b.text(),
                        "spans": spans,
                    })
                })
                .collect();

            pages_json.push(serde_json::json!({
                "page": idx + 1,
                "blocks": block_arr,
            }));
        }
        println!("{}", serde_json::to_string_pretty(&pages_json)?);
    } else {
        if !explicit_pages {
            print!("{}", doc.extract_all_text());
            return Ok(());
        }

        let mut output = String::new();
        for &idx in &page_indices {
            let page_text = doc
                .extract_text(idx)
                .context(format!("failed to extract text from page {}", idx + 1))?;

            if !output.is_empty() {
                while !output.ends_with("\n\n") {
                    output.push('\n');
                }
                output.push('\u{000C}');
            }

            output.push_str(&page_text);
        }

        // Append AcroForm field values (pdftotext includes these).
        let acroform_text = doc.extract_acroform_text();
        if !acroform_text.is_empty() {
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(&acroform_text);
        }
        print!("{output}");
    }

    Ok(())
}
