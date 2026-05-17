//! Extract text from a PDF once.  Used for peak-RSS measurement of the
//! text-extraction benchmark via `/usr/bin/time -l`.
//!
//! Usage: `extract-once <pdf-path>`

use lopdf::Document;
use std::env;
use std::fs;
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: extract-once <pdf-path>");
            return ExitCode::from(2);
        }
    };

    let pdf_bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let doc = match Document::load_mem(&pdf_bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("load {path}: {e}");
            return ExitCode::from(1);
        }
    };

    let start = Instant::now();
    let blocks = pdf_extract::extract_text(&doc);
    let elapsed = start.elapsed();
    let total_chars: usize = blocks.iter().map(|b| b.text.len()).sum();
    println!(
        "extracted {} blocks ({} chars) in {:.3}s",
        blocks.len(),
        total_chars,
        elapsed.as_secs_f64()
    );
    ExitCode::SUCCESS
}
