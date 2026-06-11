//! Dump a specific table by index. Run: table_dump <pdf> <table_index>
use pdf_engine::PdfDocument;
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let idx: usize = std::env::args().nth(2).unwrap().parse().unwrap();
    let doc = PdfDocument::open(std::fs::read(&path).unwrap()).unwrap();
    for t in doc.extract_tables() {
        if t.table_index == idx {
            eprintln!(
                "rows={} cols={} rect={} header={} cov={:.2}",
                t.n_rows(),
                t.n_cols,
                t.rectangular,
                t.has_header,
                t.cell_coverage
            );
            for (ri, row) in t.rows.iter().enumerate().take(12) {
                let cells: Vec<String> = row
                    .iter()
                    .map(|c| {
                        let x = c.text.chars().take(22).collect::<String>();
                        format!("[{x}]")
                    })
                    .collect();
                println!("r{ri}({}): {}", row.len(), cells.join(" "));
            }
        }
    }
}
