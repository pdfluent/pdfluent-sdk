//! Throwaway: compare production `extract_text_logical` vs `extract_all_text`
//! to measure faithfulness/coverage of the logical path on real forms.
//! Run: cargo run -p pdf-engine --example logical_compare -- <pdf>...

use pdf_engine::PdfDocument;
use std::collections::HashMap;

fn toks(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

/// Bag recall of `cand` tokens against `reference` tokens.
fn recall(reference: &[String], cand: &[String]) -> f64 {
    let mut r: HashMap<&String, i32> = HashMap::new();
    for t in reference {
        *r.entry(t).or_default() += 1;
    }
    let mut c: HashMap<&String, i32> = HashMap::new();
    for t in cand {
        *c.entry(t).or_default() += 1;
    }
    let total: i32 = r.values().sum();
    if total == 0 {
        return 1.0;
    }
    let common: i32 = r
        .iter()
        .map(|(k, v)| (*v).min(*c.get(*k).unwrap_or(&0)))
        .sum();
    common as f64 / total as f64
}

fn main() {
    println!(
        "{:<14}{:>10}{:>10}{:>12}{:>12}",
        "doc", "plain_w", "logic_w", "recall_l/p", "recall_p/l"
    );
    for path in std::env::args().skip(1) {
        let data = std::fs::read(&path).expect("read");
        let doc = PdfDocument::open(data).expect("open");
        let plain = doc.extract_all_text();
        let logical = doc.extract_text_logical();
        let pt = toks(&plain);
        let lt = toks(&logical);
        let name = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        // recall of logical against plain (does logical keep plain's content?)
        // and plain against logical (does logical add/duplicate?).
        println!(
            "{:<14}{:>10}{:>10}{:>12.3}{:>12.3}",
            name,
            pt.len(),
            lt.len(),
            recall(&pt, &lt),
            recall(&lt, &pt),
        );
    }
}
