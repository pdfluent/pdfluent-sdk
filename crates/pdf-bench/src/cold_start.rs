//! Cold-start benchmark binary.
//!
//! Opens a single PDF, prints the page count, exits.  Wall-clock time and peak
//! RSS are measured by the surrounding shell (`/usr/bin/time -l`).
//!
//! Usage: `cold-start <pdf-path>`

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: cold-start <pdf-path>");
            return ExitCode::from(2);
        }
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {path}: {e}");
            return ExitCode::from(1);
        }
    };

    let doc = match pdf_engine::PdfDocument::open(bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("open {path}: {e}");
            return ExitCode::from(1);
        }
    };

    println!("{}", doc.page_count());
    ExitCode::SUCCESS
}
