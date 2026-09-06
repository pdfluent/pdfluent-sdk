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
//!
//! `--tsv` as the first argument prints one row per document instead of the
//! summary -- name, status, bytes and the three timings. That is what
//! `scripts/benchmarks/v7_benchmark.py` reads for its performance axis: a
//! programme that has to parse a printed table is a programme that breaks when
//! somebody improves the table (#169).
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
    let mut paths: Vec<String> = std::env::args().skip(1).collect();

    // `--tsv` prints one row per document instead of the summary, so a
    // programme can aggregate the sample itself rather than parse prose. The
    // summary below stays the default: it is what a person runs this for.
    let tsv = paths.first().is_some_and(|a| a == "--tsv");
    if tsv {
        paths.remove(0);
        println!("name\tstatus\tbytes\topen_ms\ttext_ms\tsave_ms");
    }

    let (mut open, mut text, mut save, mut sizes) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());

    for path in &paths {
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());

        let Ok(bytes) = std::fs::read(path) else {
            // A document that cannot be read is a fact about the run, not an
            // absence. Dropping it silently is how a sample gets better than
            // the corpus it was drawn from.
            if tsv {
                println!("{name}\tunreadable\t0\t-1\t-1\t-1");
            }
            continue;
        };
        sizes.push(bytes.len() as f64 / 1024.0);

        let t = Instant::now();
        let Ok(doc) = pdfluent::PdfDocument::from_bytes(&bytes) else {
            if tsv {
                println!("{name}\tunreadable\t{}\t-1\t-1\t-1", bytes.len());
            }
            continue;
        };
        let ms_open = t.elapsed().as_secs_f64() * 1000.0;
        open.push(ms_open);

        let t = Instant::now();
        let _ = doc.text();
        let ms_text = t.elapsed().as_secs_f64() * 1000.0;
        text.push(ms_text);

        let t = Instant::now();
        let _ = doc.to_bytes();
        let ms_save = t.elapsed().as_secs_f64() * 1000.0;
        save.push(ms_save);

        if tsv {
            println!(
                "{name}\tok\t{}\t{ms_open:.3}\t{ms_text:.3}\t{ms_save:.3}",
                bytes.len()
            );
        }
    }

    if tsv {
        return;
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
