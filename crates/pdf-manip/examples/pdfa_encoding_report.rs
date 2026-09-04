// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! What fraction of each font's /Differences names actually resolve.
//!
//! The predicate that decides whether to keep or discard a font's /Differences is
//! a single boolean, and when it gets a document wrong there is no way to see why.
//! That cost real time twice: demanding every name resolve threw away a 69-name
//! Latin encoding over one `mu1` and dropped a document from 102.7% to 10.0% text
//! retention; relaxing the rule to three quarters then fixed two documents and
//! regressed three others.
//!
//! Both are the same question — what fraction resolves, and which names do not —
//! and neither was answerable without adding print statements to a release build.
//!
//! ```text
//! cargo run -p pdf-manip --release --example pdfa_encoding_report -- a.pdf b.pdf
//! ```

use lopdf::Document;
use pdf_manip::pdfa_fonts::encoding_diagnostics;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("usage: pdfa_encoding_report <pdf> [<pdf> ...]");
        std::process::exit(2);
    }

    for path in &files {
        let doc = match Document::load(path) {
            Ok(d) => d,
            Err(e) => {
                println!("{path}: could not open ({e})");
                continue;
            }
        };
        let report = encoding_diagnostics(&doc);
        if report.is_empty() {
            println!("{path}: no font carries /Differences");
            continue;
        }
        println!("{path}");
        for d in &report {
            let pct = d.resolved_fraction() * 100.0;
            // The two thresholds that have actually been used, so the number can
            // be read against both without doing arithmetic in your head.
            let unanimity = if d.resolved == d.differences {
                "keep"
            } else {
                "DISCARD"
            };
            let three_quarters = if d.resolved_fraction() >= 0.75 {
                "keep"
            } else {
                "DISCARD"
            };
            println!(
                "  {:24} {:>4}/{:<4} = {:5.1}%   unanimity: {:7}  ¾: {}",
                d.font, d.resolved, d.differences, pct, unanimity, three_quarters
            );
            if !d.unresolved.is_empty() {
                println!("      unresolved: {}", d.unresolved.join(", "));
            }
        }
        println!();
    }
    Ok(())
}
