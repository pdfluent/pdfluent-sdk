// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Measured run times on real documents, not on a mini fixture.
//!
//! The comparison pages carry 31 timings with no claim ID (#196). This produces
//! the figures that should be there: over documents from the govdocs corpus,
//! with the median and the spread, because one number across documents of 40 KB
//! to 10 MB is an impression rather than a measurement.
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if v.is_empty() {
        return 0.0;
    }
    v[v.len() / 2]
}

fn p95(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if v.is_empty() {
        return 0.0;
    }
    v[(v.len() as f64 * 0.95) as usize % v.len()]
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let (mut open, mut text, mut save, mut sizes) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());

    for path in &paths {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        sizes.push(bytes.len() as f64 / 1024.0);

        let t = Instant::now();
        let Ok(doc) = pdfluent::PdfDocument::from_bytes(&bytes) else {
            continue;
        };
        open.push(t.elapsed().as_secs_f64() * 1000.0);

        let t = Instant::now();
        let _ = doc.text();
        text.push(t.elapsed().as_secs_f64() * 1000.0);

        let t = Instant::now();
        let _ = doc.to_bytes();
        save.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    println!(
        "  documents: {}  (median {:.0} KB, p95 {:.0} KB)",
        open.len(),
        median(sizes.clone()),
        p95(sizes)
    );
    println!("  {:<22} {:>10} {:>10}", "operation", "median", "p95");
    for (name, v) in [("open", open), ("extract text", text), ("save", save)] {
        println!(
            "  {:<22} {:>8.1}ms {:>8.1}ms",
            name,
            median(v.clone()),
            p95(v)
        );
    }
}
