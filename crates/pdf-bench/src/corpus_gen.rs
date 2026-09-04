//! Generate a reproducible 100-PDF, 50-page-each test corpus.
//!
//! Each PDF contains 50 pages; each page has one text line and one rectangle
//! drawn with the standard PDF graphics operators.  The output goes to
//! `/tmp/bench-corpus/doc_{000..099}.pdf` and is the input for the
//! comparison benchmarks (see `benches/comparison.rs`).
//!
//! Run:
//!     cargo run -p pdf-bench --bin gen-corpus --release

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use std::path::Path;

const NUM_DOCS: usize = 100;
const PAGES_PER_DOC: usize = 50;
const PAGE_WIDTH: i64 = 612; // US Letter, points
const PAGE_HEIGHT: i64 = 792;

fn build_doc(doc_index: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");

    // Standard 14 Helvetica — no font program needed.
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type"     => Object::Name(b"Font".to_vec()),
        "Subtype"  => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    }));

    let resources_id = doc.add_object(Object::Dictionary(dictionary! {
        "Font" => Object::Dictionary(dictionary! {
            "F1" => Object::Reference(font_id),
        }),
    }));

    let pages_id = doc.new_object_id();
    let mut page_refs: Vec<Object> = Vec::with_capacity(PAGES_PER_DOC);

    for page_index in 0..PAGES_PER_DOC {
        // One text line + one rectangle.  Vary the y-coordinate of the
        // rectangle slightly per page so streams aren't byte-identical
        // (otherwise dedup-style optimizations could skew the merge bench).
        let rect_y = 100 + (page_index as i64 % 50) * 4;
        let text = format!(
            "PDFluent benchmark corpus — document {doc_index:03}, page {page_num:02} of {PAGES_PER_DOC}",
            page_num = page_index + 1,
        );
        let escaped = escape_pdf_string(&text);
        let content = format!(
            "q\n\
             1 1 0.9 rg\n\
             50 {rect_y} 500 60 re f\n\
             0 0 0 rg\n\
             BT /F1 12 Tf 72 720 Td ({escaped}) Tj ET\n\
             Q\n"
        );
        let content_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! { "Length" => Object::Integer(content.len() as i64) },
            content.into_bytes(),
        )));

        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"      => Object::Name(b"Page".to_vec()),
            "Parent"    => Object::Reference(pages_id),
            "MediaBox"  => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(PAGE_WIDTH), Object::Integer(PAGE_HEIGHT),
            ]),
            "Resources" => Object::Reference(resources_id),
            "Contents"  => Object::Reference(content_id),
        }));
        page_refs.push(Object::Reference(page_id));
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type"  => Object::Name(b"Pages".to_vec()),
            "Kids"  => Object::Array(page_refs),
            "Count" => Object::Integer(PAGES_PER_DOC as i64),
        }),
    );

    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type"  => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("lopdf save");
    out
}

fn escape_pdf_string(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '(' => "\\(".to_string(),
            ')' => "\\)".to_string(),
            '\\' => "\\\\".to_string(),
            // Replace anything outside printable ASCII with '?'.
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => "?".to_string(),
            c => c.to_string(),
        })
        .collect()
}

fn main() {
    let out_dir = Path::new("/tmp/bench-corpus");
    std::fs::create_dir_all(out_dir).expect("create /tmp/bench-corpus");

    let mut total_bytes = 0usize;
    for i in 0..NUM_DOCS {
        let bytes = build_doc(i);
        total_bytes += bytes.len();
        let path = out_dir.join(format!("doc_{i:03}.pdf"));
        std::fs::write(&path, &bytes).expect("write pdf");
    }

    println!(
        "Wrote {NUM_DOCS} PDFs ({PAGES_PER_DOC} pages each) → {} ({:.1} MB total)",
        out_dir.display(),
        total_bytes as f64 / 1024.0 / 1024.0
    );
}
