//! Dump a document's paragraphs, for the paragraph-boundary measurement (#567).
//!
//! An example rather than a subcommand: `paragraph_axis.py` needs a reading of
//! our blocks in the same shape the foreign readers hand theirs over in, and
//! what the command line offers is a separate question (#573).
//!
//! Run: `paragraph_blocks <pdf>`. One record per line, tab separated, in the
//! reading order the blocks came back in. Coordinates are PDF user space, y up.
//!
//!     P<TAB>page<TAB>width<TAB>height<TAB>rotation
//!     B<TAB>page<TAB>index<TAB>x0<TAB>y0<TAB>x1<TAB>y1<TAB>text
//!
//! A tab, a newline or a backslash inside the text is escaped `\t`, `\n`, `\\`,
//! so one record is always one line.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{ExtractOptions, PdfDocument};

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' | '\r' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: paragraph_blocks <pdf>");
        std::process::exit(2);
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("read {path}: {err}");
            std::process::exit(2);
        }
    };
    let doc = match PdfDocument::open(bytes) {
        Ok(doc) => doc,
        Err(err) => {
            eprintln!("open {path}: {err}");
            std::process::exit(1);
        }
    };

    for page in 0..doc.page_count() {
        let Ok(geometry) = doc.page_geometry(page) else {
            continue;
        };
        let (width, height) = geometry.effective_dimensions();
        println!(
            "P\t{page}\t{width:.3}\t{height:.3}\t{}",
            geometry.rotation.degrees()
        );
    }
    let blocks = match doc.extract_blocks(&ExtractOptions::default()) {
        Ok(blocks) => blocks,
        Err(err) => {
            eprintln!("blocks {path}: {err}");
            std::process::exit(1);
        }
    };
    for block in blocks {
        println!(
            "B\t{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{}",
            block.page,
            block.index,
            block.bbox[0],
            block.bbox[1],
            block.bbox[2],
            block.bbox[3],
            escape(&block.text)
        );
    }
}
