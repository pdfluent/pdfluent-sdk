//! A document whose `%%EOF` is not the last thing in the file (#450).
//!
//! `get_xref_start` read the last 512 bytes. PDFBox's `data-000001.pdf` carries
//! 530 zero bytes behind its `%%EOF`, which puts the marker just outside that
//! window: we refused the document for an "invalid start value" while its
//! cross-reference table was intact and correctly pointed at. Six other readers
//! -- mutool, pdftotext, PyMuPDF, pypdf, pdfminer.six and Docling -- read it,
//! and in the #390 comparison we were the only one of seven that did not.

use pdfluent_lopdf::{Document, Object, Stream, dictionary};

/// A one-page PDF with `padding` bytes of trailing rubbish behind `%%EOF`.
fn pdf_with_trailing_padding(padding: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        b"BT\n/F1 12 Tf\n72 720 Td\n(Behind the padding) Tj\nET\n".to_vec(),
    ));
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("serialise fixture");
    buf.extend(std::iter::repeat_n(0u8, padding));
    buf
}

#[test]
fn a_document_is_read_through_its_trailing_padding() {
    // Well beyond the 512-byte tail the reader used to look at, and beyond it
    // by roughly what the corpus document carries.
    let bytes = pdf_with_trailing_padding(530);
    let doc = Document::load_mem(&bytes).expect("a padded document is still a document");
    assert_eq!(doc.get_pages().len(), 1);
}

#[test]
fn the_tail_is_still_read_without_padding() {
    let bytes = pdf_with_trailing_padding(0);
    let doc = Document::load_mem(&bytes).expect("an unpadded document reads as before");
    assert_eq!(doc.get_pages().len(), 1);
}

#[test]
fn a_document_without_a_marker_is_rebuilt_rather_than_emptied() {
    // The fallback widens where `%%EOF` is looked for; it does not invent one.
    // A file with no marker at all was a refusal until #433 taught the reader
    // to rebuild the table by scanning for objects. What this pins either way
    // is the thing that would be worse than a refusal: a document that opens
    // and is silently empty.
    let mut bytes = pdf_with_trailing_padding(0);
    let marker = bytes
        .windows(5)
        .rposition(|w| w == b"%%EOF")
        .expect("fixture carries a marker");
    bytes.truncate(marker);

    let doc = Document::load_mem(&bytes).expect("the objects are all still in the file");
    assert_eq!(doc.get_pages().len(), 1, "the page must come back with it");
}
