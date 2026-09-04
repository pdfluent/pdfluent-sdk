//! Dump per-doc JSON for the chunking retrieval benchmark:
//! {doc, logical, chunks:[{path,title,body}]}. Run: chunk_dump <pdf>

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
use pdf_engine::PdfDocument;
fn js(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => {}
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => {}
            c => o.push(c),
        }
    }
    o.push('"');
    o
}
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let doc = PdfDocument::open(std::fs::read(&path).unwrap()).unwrap();
    let name = std::path::Path::new(&path)
        .file_stem()
        .unwrap()
        .to_string_lossy();
    let logical = doc.extract_text_logical();
    let chunks: Vec<String> = doc
        .extract_semantic_chunks()
        .iter()
        .map(|c| {
            format!(
                "{{\"path\":{},\"title\":{},\"body\":{}}}",
                js(&c.heading_path.join(" > ")),
                js(c.title.as_deref().unwrap_or("")),
                js(&c.text)
            )
        })
        .collect();
    println!(
        "{{\"doc\":{},\"logical\":{},\"chunks\":[{}]}}",
        js(&name),
        js(&logical),
        chunks.join(",")
    );
}
