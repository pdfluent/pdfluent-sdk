//! Memory usage measurements for lopdf Document loading.
//!
//! Measures RSS before and after loading each PDF to establish
//! disk-size → in-memory-size ratios.
//!
//! Run with:
//!   cargo run -p pdf-bench --example measure_memory --release

use pdf_bench::mem_measure::physical_footprint;
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("corpus")
}

fn main() {
    // Force GC-like collection — drop any cached state before first measurement.
    let baseline = physical_footprint();

    let corpus = corpus_dir();
    if !corpus.exists() {
        eprintln!("corpus/ not found at {corpus:?}");
        std::process::exit(1);
    }

    // Collect PDFs, sort by size.
    let mut pdfs: Vec<(String, PathBuf, u64)> = Vec::new();
    for entry in std::fs::read_dir(&corpus).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "pdf") {
            if let Ok(meta) = path.metadata() {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                pdfs.push((name, path, meta.len()));
            }
        }
    }
    pdfs.sort_by_key(|x| x.2);

    println!(
        "{:<30} {:>10} {:>12} {:>12} {:>8}",
        "PDF", "Disk (KB)", "Mem before", "Mem after", "Ratio"
    );
    println!("{}", "-".repeat(80));

    let mut results: Vec<(String, u64, u64, f64)> = Vec::new();

    for (name, path, disk_bytes) in &pdfs {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("skip {name}: {e}");
                continue;
            }
        };

        // Sample RSS just before creating the Document.
        let before = physical_footprint();

        let doc = match lopdf::Document::load_mem(&data) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("load failed {name}: {e:?}");
                continue;
            }
        };

        // Sample RSS immediately after — Document is still live.
        let after = physical_footprint();

        // Keep doc live to prevent the optimizer from eliding the load.
        let obj_count = doc.objects.len();

        let mem_delta = after.saturating_sub(before);
        let ratio = if *disk_bytes > 0 {
            mem_delta as f64 / *disk_bytes as f64
        } else {
            0.0
        };

        println!(
            "{:<30} {:>10} {:>12} {:>12} {:>8.2}×   ({} objects)",
            name,
            disk_bytes / 1024,
            format_bytes(before.saturating_sub(baseline)),
            format_bytes(after.saturating_sub(baseline)),
            ratio,
            obj_count,
        );

        results.push((name.clone(), *disk_bytes, mem_delta, ratio));

        // Drop the document explicitly before next iteration.
        drop(doc);
        drop(data);
    }

    if !results.is_empty() {
        let avg_ratio: f64 = results.iter().map(|r| r.3).sum::<f64>() / results.len() as f64;
        println!("{}", "-".repeat(80));
        println!("Average ratio: {avg_ratio:.2}×");
        let max = results.iter().max_by(|a, b| a.3.partial_cmp(&b.3).unwrap()).unwrap();
        println!("Worst case:    {} ({:.2}×)", max.0, max.3);
    }
}

fn format_bytes(b: u64) -> String {
    if b >= 1024 * 1024 {
        format!("{:.1} MB", b as f64 / (1024.0 * 1024.0))
    } else if b >= 1024 {
        format!("{:.1} KB", b as f64 / 1024.0)
    } else {
        format!("{b} B")
    }
}
