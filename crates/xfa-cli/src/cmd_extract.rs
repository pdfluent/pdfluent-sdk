//! Extract text from PDF pages.

use anyhow::{Context, Result};
use std::path::Path;

use pdf_engine::PdfDocument;

pub fn run(input: &Path, pages: Option<&str>, json: bool) -> Result<()> {
    let data = std::fs::read(input).context("failed to read input PDF")?;
    let doc = PdfDocument::open(data).context("failed to open PDF")?;
    let total = doc.page_count();

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
        for &idx in &page_indices {
            let text = doc
                .extract_text(idx)
                .context(format!("failed to extract text from page {}", idx + 1))?;

            if page_indices.len() > 1 {
                println!("--- Page {} ---", idx + 1);
            }
            println!("{text}");
        }
    }

    Ok(())
}
