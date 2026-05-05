//! web_examples/merge_pdfs_rust
//!
//! Source: <https://pdfluent.com/how-to/merge-pdfs-rust> (fetched 2026-04-22)
//!
//! Auto-extracted by `tools/pdfluent-snippet-extract` (#1236).
//! Do not edit by hand — re-run the extractor instead.

use pdfluent::PdfMerger;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PdfMerger::new()
        .add_file("part1.pdf")?
        .add_file("part2.pdf")?
        .add_file("part3.pdf")?
        .merge("combined.pdf")?;

    println!("Merged {} pages into combined.pdf", output.page_count);
    Ok(())
}
