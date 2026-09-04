//! Flatten an XFA PDF to a static PDF content stream.
//!
//! # Usage
//!
//! ```text
//! cargo run --example flatten_xfa -- <input.pdf> <output.pdf>
//! ```
//!
//! The example:
//! 1. Reads the input PDF bytes.
//! 2. Detects whether the PDF is encrypted; exits with code 2 if so.
//! 3. Flattens all XFA and AcroForm content to static content streams.
//! 4. Writes the output PDF.
//! 5. Prints the page count of the output.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: flatten_xfa <input.pdf> <output.pdf>");
        std::process::exit(1);
    }

    let input = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);

    let pdf_bytes =
        std::fs::read(&input).map_err(|e| format!("failed to read {}: {e}", input.display()))?;

    // Early check: encrypted PDFs cannot be flattened without a password.
    if pdf_xfa::is_pdf_encrypted(&pdf_bytes) {
        eprintln!(
            "SKIP: {} is encrypted — supply a decrypted copy",
            input.display()
        );
        std::process::exit(2);
    }

    let (flattened, metadata) = pdf_xfa::flatten_xfa_to_pdf_with_metadata(&pdf_bytes)
        .map_err(|e| format!("flatten failed: {e}"))?;

    std::fs::write(&output, &flattened)
        .map_err(|e| format!("failed to write {}: {e}", output.display()))?;

    // Count pages in the output using lopdf.
    let page_count = count_pages(&flattened).unwrap_or(0);

    println!(
        "Flattened {} -> {} ({page_count} page{}, quality: {})",
        input.display(),
        output.display(),
        if page_count == 1 { "" } else { "s" },
        metadata.output_quality.as_str(),
    );

    Ok(())
}

/// Return the page count of a PDF byte slice.
fn count_pages(pdf_bytes: &[u8]) -> Option<usize> {
    let doc = lopdf::Document::load_mem(pdf_bytes).ok()?;
    Some(doc.page_iter().count())
}
