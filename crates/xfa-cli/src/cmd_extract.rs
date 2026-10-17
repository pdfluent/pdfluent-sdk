//! Extract text from PDF pages.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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

        let mut page_texts = Vec::with_capacity(page_indices.len());
        for &idx in &page_indices {
            page_texts.push(
                doc.extract_text(idx)
                    .context(format!("failed to extract text from page {}", idx + 1))?,
            );
        }

        // Append AcroForm field values (pdftotext includes these). On the last
        // selected page rather than behind its separator: one convention with
        // `extract_all_text`, and no phantom page for a consumer counting them.
        let acroform_text = doc.extract_acroform_text();
        if !acroform_text.is_empty() {
            match page_texts.last_mut() {
                Some(last) => {
                    if !last.is_empty() && !last.ends_with('\n') {
                        last.push('\n');
                    }
                    last.push_str(&acroform_text);
                }
                None => page_texts.push(acroform_text),
            }
        }

        print!(
            "{}",
            pdf_engine::join_page_texts(page_texts.iter().map(String::as_str))
        );
    }

    Ok(())
}
