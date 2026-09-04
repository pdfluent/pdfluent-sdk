//! Convert a PDF to PDF/A and save the result to a file.
//! Usage: cargo run -p xfa-test-runner --example pdfa_dump -- input.pdf output.pdf

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = args.get(1).map(|s| s.as_str()).unwrap_or("/tmp/fail1.pdf");
    let output = args
        .get(2)
        .map(|s| s.as_str())
        .unwrap_or("/tmp/converted.pdf");

    let data = std::fs::read(input).expect("read");
    eprintln!("Converting {} ({} bytes)...", input, data.len());

    match xfa_test_runner::tests::pdfa_convert::convert_to_pdfa_bytes(&data, Path::new(input)) {
        Some(out) => {
            std::fs::write(output, &out).expect("write");
            eprintln!("Saved {} bytes -> {}", out.len(), output);
        }
        None => {
            eprintln!("Conversion returned None");
            std::process::exit(1);
        }
    }
}
