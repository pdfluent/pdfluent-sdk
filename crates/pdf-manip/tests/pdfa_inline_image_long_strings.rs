// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A content stream that carries an inline image still has to lose its
//! over-long string literals.
//!
//! ISO 19005-2 6.1.13:3 caps a string literal at 32767 bytes, and
//! `cleanup_for_pdfa` truncates the ones it finds in page content. The plain
//! scanner that does that work reads `(` ... `)` at the byte level, which is
//! wrong the moment the page also draws an inline image: the image payload is
//! arbitrary binary, so its parentheses and backslashes are not string
//! syntax. So the pass hands any `BI`-bearing stream to
//! `inline_image::truncate_image_stream_strings`, which parses the stream
//! first and only edits real string operands.
//!
//! Both halves are load-bearing and both are checked here:
//!
//! * Skip the delegation (return the content unchanged) and the over-long
//!   string survives into the output -- a validation failure the conversion
//!   claims to have fixed, with `long_string_fixes` reporting zero.
//! * Drop the delegation and let the byte scanner run instead, and the
//!   unbalanced `(` inside the JPEG payload swallows the rest of the page:
//!   the scan never finds a closing `)`, copies the remainder verbatim, and
//!   again truncates nothing.

use lopdf::{dictionary, Document, Object, Stream};

/// Eight bytes of JPEG segment payload that are hostile to a byte-level
/// string scanner: two unbalanced `(` and two backslashes, one of which
/// escapes the only `)` present.
const HOSTILE_SEGMENT: &[u8] = b"(\\)\x00\xff(\\(";

/// A minimal DCT frame: SOI, one APP0 segment holding [`HOSTILE_SEGMENT`],
/// EOI. `/F /DCT` keeps the payload out of reach of the ASCIIHex rewrite that
/// protects *unfiltered* inline images, so the raw bytes above are still in
/// the stream when the long-string pass runs.
fn jpeg_payload() -> Vec<u8> {
    let mut data = vec![0xff, 0xd8, 0xff, 0xe0];
    // The APP0 length counts itself, so 2 + the segment body.
    data.extend_from_slice(
        &u16::try_from(2 + HOSTILE_SEGMENT.len())
            .unwrap()
            .to_be_bytes(),
    );
    data.extend_from_slice(HOSTILE_SEGMENT);
    data.extend_from_slice(&[0xff, 0xd9]);
    data
}

const OVERLONG: usize = 40000;
const MAX_STRING_LEN: usize = 32767;

/// One page: an inline JPEG, then a text run whose first operand is far over
/// the limit and whose second one must be left alone.
fn page_with_inline_image_and_long_string() -> (Document, (u32, u16)) {
    let mut content = b"q\nBI /W 8 /H 1 /BPC 8 /CS /G /F /DCT\nID ".to_vec();
    content.extend_from_slice(&jpeg_payload());
    content.extend_from_slice(b"\nEI\nQ\nBT\n(");
    content.extend(std::iter::repeat_n(b'A', OVERLONG));
    content.extend_from_slice(b") Tj\n(tail) Tj\nET\n");

    let mut doc = Document::with_version("1.7");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! {},
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
    }));
    if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
        page.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    (doc, content_id)
}

fn plain_content(doc: &Document, content_id: (u32, u16)) -> Vec<u8> {
    doc.get_object(content_id)
        .and_then(|o| o.as_stream())
        .expect("content stream survives the cleanup")
        .get_plain_content()
        .expect("content stream is still decodable")
}

/// Length of the longest run of `A`, i.e. what is left of the over-long
/// string operand.
fn longest_a_run(data: &[u8]) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for byte in data {
        run = if *byte == b'A' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    longest
}

#[test]
fn an_over_long_string_is_truncated_on_a_page_that_also_draws_an_inline_image() {
    let (mut doc, content_id) = page_with_inline_image_and_long_string();
    assert_eq!(
        longest_a_run(&plain_content(&doc, content_id)),
        OVERLONG,
        "fixture check: the string starts over the limit"
    );

    let report = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).expect("cleanup");
    let content = plain_content(&doc, content_id);

    assert_eq!(
        longest_a_run(&content),
        MAX_STRING_LEN,
        "ISO 19005-2 6.1.13:3 caps a literal at {MAX_STRING_LEN} bytes; the \
         inline image on this page must not buy the string an exemption"
    );
    assert_eq!(
        report.long_string_fixes, 1,
        "the conversion report has to admit the edit it made"
    );
    assert!(
        content.windows(b"(tail)".len()).any(|w| w == b"(tail)"),
        "the operand after the truncated one is untouched, not swallowed"
    );
}

#[test]
fn the_inline_image_payload_survives_byte_for_byte() {
    let (mut doc, content_id) = page_with_inline_image_and_long_string();
    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).expect("cleanup");
    let content = plain_content(&doc, content_id);

    let payload = jpeg_payload();
    assert!(
        content.windows(payload.len()).any(|w| w == payload),
        "truncating the string re-encodes the stream around the image; the \
         DCT frame has to come back out unchanged, parentheses and all"
    );
}

/// `pdfa_fixups::run_fixups` carries the same pass over the same hazard, on
/// every content stream rather than only the ones a page points at. Bypassing
/// that second call site has to be just as visible as bypassing the first.
#[test]
fn the_fixup_sweep_truncates_it_too() {
    let (mut doc, content_id) = page_with_inline_image_and_long_string();
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    let content = plain_content(&doc, content_id);

    assert_eq!(longest_a_run(&content), MAX_STRING_LEN);
    assert!(content.windows(b"(tail)".len()).any(|w| w == b"(tail)"));
}
