//! Merge every PDF in a directory once.  Used for peak-RSS measurement of the
//! merge benchmark via `/usr/bin/time -l`.
//!
//! Usage: `merge-once <input-dir> [output-path]`
//!   input-dir : directory of *.pdf files
//!   output-path : optional; if omitted the merged document is dropped

use lopdf::Document;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    let input_dir = match env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("usage: merge-once <input-dir> [output-path]");
            return ExitCode::from(2);
        }
    };
    let output: Option<PathBuf> = env::args().nth(2).map(PathBuf::from);

    let mut paths: Vec<PathBuf> = match std::fs::read_dir(&input_dir) {
        Ok(it) => it
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
            .collect(),
        Err(e) => {
            eprintln!("read_dir {}: {e}", input_dir.display());
            return ExitCode::from(1);
        }
    };
    paths.sort();
    if paths.is_empty() {
        eprintln!("no PDFs in {}", input_dir.display());
        return ExitCode::from(1);
    }

    let docs: Vec<Document> = paths
        .iter()
        .map(|p| Document::load(p).expect("load corpus pdf"))
        .collect();

    let start = Instant::now();
    let mut merged = pdf_manip::pages::merge_documents(&docs).expect("merge");
    let elapsed = start.elapsed();

    if let Some(out) = output {
        merged.save(&out).expect("save merged");
        println!(
            "merged {} pdfs in {:.3}s → {}",
            docs.len(),
            elapsed.as_secs_f64(),
            out.display()
        );
    } else {
        println!(
            "merged {} pdfs in {:.3}s ({} pages, dropped)",
            docs.len(),
            elapsed.as_secs_f64(),
            merged.get_pages().len()
        );
    }
    ExitCode::SUCCESS
}
