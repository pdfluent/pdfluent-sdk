//! Demo + measurement of the production structured-extraction APIs.
//! Run: cargo run -q --release -p pdf-engine --example structured_demo -- <pdf> [tables|chunks|measure]

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::PdfDocument;

fn main() {
    let path = std::env::args().nth(1).expect("usage: <pdf> [mode]");
    let mode = std::env::args().nth(2).unwrap_or_else(|| "measure".into());
    let data = std::fs::read(&path).expect("read");
    let doc = PdfDocument::open(data).expect("open");
    let tables = doc.extract_tables();
    let chunks = doc.extract_semantic_chunks();
    let name = std::path::Path::new(&path)
        .file_stem()
        .unwrap()
        .to_string_lossy();

    match mode.as_str() {
        "tables" => {
            for t in tables.iter().take(3) {
                println!(
                    "\n-- table {} (page {:?}) {}x{} rect={} header={} cov={:.2} --",
                    t.table_index,
                    t.page_index,
                    t.n_rows(),
                    t.n_cols,
                    t.rectangular,
                    t.has_header,
                    t.cell_coverage
                );
                println!("{}", t.to_markdown());
            }
        }
        "chunks" => {
            for c in chunks.iter().take(6) {
                let path = if c.heading_path.is_empty() {
                    String::new()
                } else {
                    c.heading_path.join(" > ") + " > "
                };
                println!(
                    "\n## [{}{}] pages {:?}-{:?} types={:?} tables={:?}",
                    path,
                    c.title.as_deref().unwrap_or("(no heading)"),
                    c.page_start,
                    c.page_end,
                    c.element_types,
                    c.table_indices
                );
                println!("{}", c.text.chars().take(180).collect::<String>());
            }
        }
        _ => {
            // measure: machine-readable one-liner
            let rect = tables.iter().filter(|t| t.rectangular).count();
            let total_cells: usize = tables.iter().map(|t| t.n_rows() * t.n_cols).sum();
            let cov = if tables.is_empty() {
                0.0
            } else {
                tables.iter().map(|t| t.cell_coverage).sum::<f64>() / tables.len() as f64
            };
            let with_heading = chunks.iter().filter(|c| c.title.is_some()).count();
            let lens: Vec<usize> = chunks
                .iter()
                .map(|c| c.text.split_whitespace().count())
                .collect();
            let median = {
                let mut l = lens.clone();
                l.sort_unstable();
                l.get(l.len() / 2).copied().unwrap_or(0)
            };
            println!(
                "{{\"doc\":\"{name}\",\"tables\":{},\"rect_tables\":{},\"table_cells\":{},\"cell_cov\":{:.3},\"chunks\":{},\"chunks_with_heading\":{},\"median_chunk_words\":{}}}",
                tables.len(), rect, total_cells, cov, chunks.len(), with_heading, median
            );
        }
    }
}
