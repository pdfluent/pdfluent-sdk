//! A rebuilt cross-reference table may not define an object from stream data (#515).
//!
//! When the table is unusable the file is scanned for `N G obj` headers, and
//! the newest candidate that parses wins -- the rule an incremental update
//! asks for. Stream payloads are arbitrary bytes, so the same three letters
//! occur in them, and a payload that happens to contain a parseable
//! `1 0 obj ... endobj` was eligible. Being later in the file, it was the
//! newest candidate, and it replaced the real object 1. For a catalogue that
//! turns a repairable document into a corrupted one: the reader reports
//! success and hands back a page tree that came out of a content stream.

use pdfluent_lopdf::{Document, Object};

/// A PDF with a broken start offset whose content stream contains a fake
/// catalogue for the same object number as the real one.
///
/// Everything outside the stream is a correct one-page document. The `startxref`
/// value points nowhere, so the reader has to rebuild -- which is the only path
/// that reads the file linearly, and the only one that can see into a payload.
fn pdf_with_a_fake_catalogue_in_a_stream(real_catalogue: u32) -> Vec<u8> {
    let fake = format!(
        "BT\n(a payload that reads like a document) Tj\nET\n\
         {real_catalogue} 0 obj\n<< /Type /Catalog /Pages 99 0 R >>\nendobj\n"
    );

    let mut pdf = Vec::new();
    let mut offsets = Vec::new();

    pdf.extend_from_slice(b"%PDF-1.7\n");

    let object = |pdf: &mut Vec<u8>, offsets: &mut Vec<usize>, id: u32, body: &str| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    };

    object(
        &mut pdf,
        &mut offsets,
        real_catalogue,
        "<< /Type /Catalog /Pages 2 0 R >>",
    );
    object(&mut pdf, &mut offsets, 2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    object(
        &mut pdf,
        &mut offsets,
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>",
    );

    // The stream carrying the fake header, written by hand so the payload is
    // exactly these bytes.
    offsets.push(pdf.len());
    pdf.extend_from_slice(
        format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{fake}endstream\nendobj\n",
            fake.len()
        )
        .as_bytes(),
    );

    // A startxref that points at nothing, so the table has to be rebuilt.
    pdf.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n999999\n%%EOF\n");
    pdf
}

/// The real catalogue survives a rebuild, and the one in the stream does not.
#[test]
fn a_header_inside_a_payload_is_not_a_definition() {
    let pdf = pdf_with_a_fake_catalogue_in_a_stream(1);

    let doc = Document::load_mem(&pdf).expect("a file with a broken start offset is rebuilt");

    let catalogue = doc
        .get_object((1, 0))
        .expect("object 1 is defined by the file, not by a content stream");
    let dict = catalogue.as_dict().expect("the catalogue is a dictionary");
    let pages = dict.get(b"Pages").expect("a catalogue names its page tree");

    assert_eq!(
        pages,
        &Object::Reference((2, 0)),
        "the catalogue came out of the content stream: /Pages points at the object \
         the fake header names, so the rebuild preferred a payload to the file",
    );

    // And the document is still readable end to end, which is the point of
    // rebuilding at all.
    assert_eq!(doc.get_pages().len(), 1, "the page tree must still resolve");
}

/// Six letters that are not a stream opener do not open a payload.
///
/// A span is opened on the `stream` grammar -- after a dictionary's `>>`, and
/// followed by an end of line -- not on the word. Each half is needed, and
/// this file carries a trap for each: `(stream\n)` in a string satisfies the
/// end of line and not the dictionary, `>> streamish` satisfies the dictionary
/// and not the end of line. Either one opening a span would run to the real
/// `endstream` at the end of the file and drop every object in between --
/// a file that reads as complete and is not, which is worse than the fake
/// object this exclusion exists to refuse.
#[test]
fn the_word_alone_does_not_open_a_payload() {
    let content = "BT\n(the page) Tj\nET\n";

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.7\n");
    // Trap one: an end of line, no dictionary. Trap two: a dictionary, no end
    // of line. Both sit before the real objects, so a span opened on either
    // swallows all of them.
    pdf.extend_from_slice(b"5 0 obj\n<< /Label (stream\n) >>\nendobj\n");
    pdf.extend_from_slice(b"% << /Note 1 >> streamish\n");
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>\nendobj\n",
    );
    pdf.extend_from_slice(
        format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{content}endstream\nendobj\n",
            content.len(),
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n999999\n%%EOF\n");

    let doc = Document::load_mem(&pdf).expect("a file with a broken start offset is rebuilt");

    for id in [1u32, 2, 3, 4] {
        assert!(
            doc.get_object((id, 0)).is_ok(),
            "object {id} is missing: a span opened on six letters that are not a stream \
             opener swallowed everything up to the real endstream",
        );
    }
    assert_eq!(doc.get_pages().len(), 1, "the page tree must still resolve");
}

/// A wrongly detected payload costs nothing.
///
/// Deciding what is a payload means reading the file linearly, which is the
/// thing the table is being rebuilt to make possible, so the decision is a
/// guess. `% >> stream` in a comment satisfies the grammar this reader can see
/// from the outside and opens a span over the objects that follow. Being in a
/// span orders a candidate last; it does not remove it, so the objects are
/// still there and the document still reads as what it is.
#[test]
fn a_span_opened_by_mistake_does_not_lose_an_object() {
    let content = "BT\n(the page) Tj\nET\n";

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.7\n");
    // A comment that reads exactly like a stream opener from the outside.
    pdf.extend_from_slice(b"% << /Fake 1 >> stream\n");
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>\nendobj\n",
    );
    pdf.extend_from_slice(
        format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{content}endstream\nendobj\n",
            content.len(),
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n999999\n%%EOF\n");

    let doc = Document::load_mem(&pdf).expect("a file with a broken start offset is rebuilt");

    for id in [1u32, 2, 3, 4] {
        assert!(
            doc.get_object((id, 0)).is_ok(),
            "object {id} went missing because a span was opened on a comment. A guess \
             about what is payload may cost a fake object its precedence; it may not \
             cost a real one its existence",
        );
    }
    assert_eq!(doc.get_pages().len(), 1, "the page tree must still resolve");
}

/// The rebuild still finds objects that follow a stream.
///
/// The exclusion walks spans alongside the headers, so an off-by-one there
/// would drop every object after the first stream rather than one inside it --
/// a fix that trades a rare wrong object for a common missing one.
#[test]
fn objects_after_a_stream_are_still_found() {
    let pdf = pdf_with_a_fake_catalogue_in_a_stream(1);

    let doc = Document::load_mem(&pdf).expect("a file with a broken start offset is rebuilt");

    for id in [1u32, 2, 3, 4] {
        assert!(
            doc.get_object((id, 0)).is_ok(),
            "object {id} went missing from the rebuilt table; objects 2 and 3 sit before \
             the stream and 4 is the stream itself",
        );
    }
}
