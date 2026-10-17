// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! "The metadata stream shall not be filtered" is an ISO 19005-1 rule, and only
//! that.
//!
//! The converter compresses the catalog's XMP stream for part >= 2 on purpose
//! (`pdfa_xmp.rs`: `allows_compression = conformance.part() != 1`). Our own
//! validator used to reject exactly that for every part, with a `_ =>` arm that
//! cited §6.6.2.1 for parts 2/3 and §6.5.2 for part 4. The result was that every
//! document this library converted to PDF/A-2b was non-conforming by our own
//! measurement -- 0 of 17 on the desktop editor's golden set -- while veraPDF
//! 1.28.2 passed the same bytes.
//!
//! veraPDF is the oracle, and it is unambiguous. Counting the rule
//! `isCatalogMetadata == false || Filter == null` over
//! `profiles/veraPDF-validation-profiles-rel-1.28/PDF_A/`: present in PDFA-1A
//! and PDFA-1B (ISO 19005-1 §6.7.2 test 2), absent from all of 2A/2B/2U,
//! 3A/3B/3U and 4/4E/4F.
//!
//! These two tests are a pair, and they fail in opposite directions:
//!
//!   * drop the `level.part() == 1` guard and `a_compressed_metadata_stream_is_
//!     fine_for_part_2` goes red -- the false alarm is back;
//!   * drop the check itself and `a_compressed_metadata_stream_is_a_part_1_
//!     violation` goes red -- a real ISO 19005-1 violation goes unreported.
//!
//! Refs #458.

use lopdf::{Document, Object};

const SOURCE: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");

/// The message `check_xmp_metadata` raises for a filtered metadata stream.
const FILTER_MESSAGE: &str = "XMP metadata stream dictionary contains /Filter key";

fn convert(conformance: pdf_manip::pdfa_xmp::PdfAConformance) -> Vec<u8> {
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance,
        ..Default::default()
    };
    pdf_manip::pdfa::convert_bytes(SOURCE, &opts).expect("conversion failed")
}

/// Whether the catalog's /Metadata stream carries a /Filter key.
fn metadata_stream_is_filtered(pdf: &[u8]) -> bool {
    let doc = Document::load_mem(pdf).expect("the converted document must load");
    let catalog = doc.catalog().expect("converted output has a catalog");
    let Ok(Object::Reference(meta_id)) = catalog.get(b"Metadata") else {
        panic!("converted output has no /Metadata reference in its catalog");
    };
    let stream = doc
        .get_object(*meta_id)
        .and_then(|o| o.as_stream())
        .expect("/Metadata must point at a stream");
    stream.dict.has(b"Filter")
}

/// Every error the validator reports about a filtered metadata stream.
fn filter_errors(pdf: &[u8], level: pdf_compliance::PdfALevel) -> Vec<String> {
    let parsed = pdf_syntax::Pdf::new(pdf.to_vec()).expect("the converted document must parse");
    pdf_compliance::validate_pdfa(&parsed, level)
        .issues
        .iter()
        .filter(|issue| {
            issue.severity == pdf_compliance::Severity::Error
                && issue.message.contains(FILTER_MESSAGE)
        })
        .map(|issue| format!("{}: {}", issue.rule, issue.message))
        .collect()
}

#[test]
fn a_compressed_metadata_stream_is_fine_for_part_2() {
    let converted = convert(pdf_manip::pdfa_xmp::PdfAConformance::A2b);

    // Guard the premise: if the converter ever stops compressing XMP for part
    // 2, this test would pass for the wrong reason and quietly stop covering
    // anything.
    assert!(
        metadata_stream_is_filtered(&converted),
        "the converter no longer compresses the XMP stream for PDF/A-2b, so \
         this test no longer exercises the rule it was written for. Either \
         restore the compression or retire this pair of tests on purpose."
    );

    let errors = filter_errors(&converted, pdf_compliance::PdfALevel::A2b);
    assert!(
        errors.is_empty(),
        "PDF/A-2 carries no rule against a filtered metadata stream -- \
         ISO 19005-2 §6.6.2.3.1 asks for well-formed, unencrypted XMP, and \
         veraPDF 1.28.2's PDFA-2B profile has no `Filter == null` rule at all. \
         Reported anyway: {errors:?}"
    );
}

#[test]
fn a_compressed_metadata_stream_is_a_part_1_violation() {
    // PDF/A-1 output is written with an uncompressed metadata stream, which is
    // the correct behaviour; compress it after the fact to produce the file
    // ISO 19005-1 §6.7.2 forbids.
    let converted = convert(pdf_manip::pdfa_xmp::PdfAConformance::A1b);
    assert!(
        !metadata_stream_is_filtered(&converted),
        "the converter must leave the XMP stream unfiltered for PDF/A-1b"
    );

    let mut doc = Document::load_mem(&converted).expect("the converted document must load");
    let meta_id = match doc.catalog().unwrap().get(b"Metadata") {
        Ok(Object::Reference(id)) => *id,
        _ => panic!("converted output has no /Metadata reference in its catalog"),
    };
    match doc.objects.get_mut(&meta_id) {
        Some(Object::Stream(stream)) => {
            stream
                .compress()
                .expect("compressing the XMP stream failed");
        }
        _ => panic!("/Metadata must point at a stream"),
    }
    let mut filtered = Vec::new();
    doc.save_to(&mut filtered).expect("saving failed");

    assert!(
        metadata_stream_is_filtered(&filtered),
        "the fixture for this test must carry a filtered metadata stream"
    );

    let errors = filter_errors(&filtered, pdf_compliance::PdfALevel::A1b);
    assert_eq!(
        errors.len(),
        1,
        "ISO 19005-1 §6.7.2 test 2 forbids a filtered metadata stream and \
         veraPDF's PDFA-1B profile enforces it, so exactly one error is \
         expected here. Got: {errors:?}"
    );
    assert!(
        errors[0].starts_with("6.7.2:"),
        "the part-1 metadata-filter violation is §6.7.2, not {}",
        errors[0]
    );
}
