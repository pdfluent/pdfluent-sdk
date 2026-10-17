//! WinAnsi code 0xAD is a hyphen, not a soft hyphen (#431, class R-D).
//!
//! PDF 32000-1 Annex D gives the hyphen two codes in WinAnsiEncoding: 0x2D and
//! 0xAD. Latin-1 calls the second one `sfthyphen`, and that is the name our
//! table carried, so every hyphen written at the second code came out of
//! `extract_text` as U+00AD -- a character a reader draws but a search does not
//! match and a RAG index quietly splits words on. `XiaoBiaoSong.pdf` writes all
//! 104 of its hyphens there; against `pdftotext` we were missing 104 hyphens
//! and carrying 104 soft hyphens nobody typed.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;

/// A one-page document drawing `content` with `font` as `/F1`.
///
/// Built by hand rather than through the writer: the point of the test is what
/// a byte in a content stream decodes to, and a builder that picks the encoding
/// for us would be answering the question under test.
fn one_page_with_font(font: &str, content: &[u8]) -> Vec<u8> {
    let mut stream = format!("<</Length {}>>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(content);
    stream.extend_from_slice(b"\nendstream");

    let objects: Vec<Vec<u8>> = vec![
        b"<</Type/Catalog/Pages 2 0 R>>".to_vec(),
        b"<</Type/Pages/Kids[3 0 R]/Count 1>>".to_vec(),
        b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 100]\
           /Resources<</Font<</F1 5 0 R>>>>/Contents 4 0 R>>"
            .to_vec(),
        stream,
        font.as_bytes().to_vec(),
    ];

    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }

    let xref_start = out.len();
    let size = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<</Size {size}/Root 1 0 R>>\nstartxref\n{xref_start}\n%%EOF").as_bytes(),
    );

    out
}

#[test]
fn winansi_code_ad_extracts_as_a_plain_hyphen() {
    // Both subtypes reach the encoding table by a different route -- the
    // standard-font one and the synthesised /Encoding map -- and both used to
    // hand back a soft hyphen.
    for subtype in ["Type1", "TrueType"] {
        let font =
            format!("<</Type/Font/Subtype/{subtype}/BaseFont/Helvetica/Encoding/WinAnsiEncoding>>");
        let bytes = one_page_with_font(&font, b"BT /F1 12 Tf 10 50 Td (A\xADB) Tj ET");

        let doc = PdfDocument::from_bytes(&bytes).expect("the document opens");
        let text = doc.extract_text().expect("text comes out");

        assert!(
            text.contains("A-B"),
            "{subtype}: code 0xAD must decode to U+002D, got {:?}",
            text
        );
        assert!(
            !text.contains('\u{00AD}'),
            "{subtype}: no soft hyphen may reach the text layer, got {:?}",
            text
        );
    }
}
