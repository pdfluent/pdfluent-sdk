// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! Cookbook G1 — HTML to PDF, and everything after it.
//!
//! PDFluent does not convert HTML. Rendering modern HTML and CSS correctly means
//! shipping a browser engine, and a renderer that is nearly right is worse than
//! none: the output looks plausible and is wrong.
//!
//! So the recipe is a division of labour. Headless Chrome renders; PDFluent does
//! the document work that follows, which is the part a browser cannot do at all —
//! merging, page operations, compression, watermarks, encryption, signing, PDF/A,
//! redaction.
//!
//! Run it with a Chrome or Chromium on PATH:
//!
//! ```text
//! cargo run --example html_to_pdf_via_chrome -- invoice.html out.pdf
//! ```
//!
//! This file is compiled by CI (`cargo build --examples`), so it cannot rot into
//! something that no longer builds while the cookbook page still shows it.

// Import exactly what is used, not the prelude glob: the prelude exports its own
// `Result<T>` alias, which shadows std's two-parameter one and turns every
// ordinary signature in this file into a compile error. Explicit imports also
// tell the reader where each name comes from, which is the point of an example.
use pdfluent::prelude::PdfDocument;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Chrome's own binary name differs per platform and per install. Try the usual
/// ones rather than making the reader edit the example first.
const CHROME_CANDIDATES: &[&str] = &[
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
];

fn find_chrome() -> Option<String> {
    CHROME_CANDIDATES
        .iter()
        .find(|c| {
            Path::new(c).exists()
                || Command::new(c)
                    .arg("--version")
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
        })
        .map(|c| (*c).to_string())
}

/// Render `html` to a PDF at `out` with headless Chrome.
///
/// `--print-to-pdf` is the supported entry point; `--no-pdf-header-footer` keeps
/// Chrome from stamping the URL and a date into the margins, which is almost
/// never what you want in a document you are about to sign or archive.
fn render_with_chrome(chrome: &str, html: &Path, out: &Path) -> Result<(), String> {
    let status = Command::new(chrome)
        .arg("--headless")
        .arg("--disable-gpu")
        .arg("--no-pdf-header-footer")
        .arg(format!("--print-to-pdf={}", out.display()))
        .arg(html.canonicalize().map_err(|e| e.to_string())?)
        .status()
        .map_err(|e| format!("could not start {chrome}: {e}"))?;

    if !status.success() {
        return Err(format!("{chrome} exited with {status}"));
    }
    if !out.exists() {
        return Err(format!("{chrome} reported success but wrote no file"));
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let html = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!("usage: html_to_pdf_via_chrome <input.html> <output.pdf>");
        std::process::exit(2);
    }));
    let out = PathBuf::from(args.next().unwrap_or_else(|| "out.pdf".to_string()));

    let chrome = find_chrome().ok_or(
        "no Chrome or Chromium found. Install one, or point CHROME_CANDIDATES at yours. \
         PDFluent deliberately does not bundle a browser engine.",
    )?;
    println!("rendering with {chrome}");
    render_with_chrome(&chrome, &html, &out)?;

    // From here on it is ours. Open what Chrome produced and do the document work.
    let doc = PdfDocument::open(&out)?;
    println!("Chrome produced {} page(s)", doc.page_count());

    // Everything below is what the browser cannot do:
    //
    //   doc.add_watermark(...)      // stamp it
    //   doc.encrypt(...)            // restrict it
    //   doc.convert_to_pdfa(...)    // archive it
    //   doc.sign(...)               // sign it
    //
    // Left commented so the example stays runnable without credentials or a
    // certificate; see the cookbook page for each of those as its own recipe.

    println!("wrote {}", out.display());
    Ok(())
}
