//! Adversarial audit of extract_tables(): per-table failure-mode classification.
//! Emits one JSON line per table. Run: table_audit <pdf>

use pdf_engine::PdfDocument;

fn main() {
    let path = std::env::args().nth(1).expect("usage: <pdf>");
    let data = std::fs::read(&path).expect("read");
    let doc = PdfDocument::open(data).expect("open");
    let name = std::path::Path::new(&path)
        .file_stem()
        .unwrap()
        .to_string_lossy();
    for t in doc.extract_tables() {
        let n_rows = t.n_rows();
        let n_cols = t.n_cols;
        let total = n_rows * n_cols;
        let filled: usize = t
            .rows
            .iter()
            .flatten()
            .filter(|c| !c.text.is_empty())
            .count();
        let empty: usize = t
            .rows
            .iter()
            .flatten()
            .filter(|c| c.text.is_empty())
            .count();
        // spacing artifact: cells where >40% of "words" are single chars (e.g. "I R S")
        let mut artifact_cells = 0usize;
        let mut nonempty_cells = 0usize;
        let mut single_cell_rows = 0usize;
        for row in &t.rows {
            if row.len() <= 1 {
                single_cell_rows += 1;
            }
            for c in row {
                if c.text.is_empty() {
                    continue;
                }
                nonempty_cells += 1;
                let words: Vec<&str> = c.text.split(' ').collect();
                let singles = words.iter().filter(|w| w.chars().count() == 1).count();
                if words.len() >= 4 && singles as f64 / words.len() as f64 > 0.4 {
                    artifact_cells += 1;
                }
            }
        }
        // row-length irregularity => probable spans
        let row_lens: Vec<usize> = t.rows.iter().map(|r| r.len()).collect();
        let distinct_lens = {
            let mut v = row_lens.clone();
            v.sort_unstable();
            v.dedup();
            v.len()
        };
        let is_layout = n_cols <= 1;
        let is_data_candidate = t.rectangular && n_cols >= 2 && n_rows >= 2 && !is_layout;
        println!(
            "{{\"doc\":\"{name}\",\"ti\":{},\"rows\":{n_rows},\"cols\":{n_cols},\"rect\":{},\"layout\":{is_layout},\"data_candidate\":{is_data_candidate},\"distinct_row_lens\":{distinct_lens},\"single_cell_rows\":{single_cell_rows},\"empty_cells\":{empty},\"total_cells\":{total},\"cov\":{:.3},\"artifact_cells\":{artifact_cells},\"nonempty_cells\":{nonempty_cells}}}",
            t.table_index, t.rectangular, t.cell_coverage,
        );
        let _ = filled;
    }
}
