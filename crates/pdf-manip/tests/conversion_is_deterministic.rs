// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The same input must produce the same bytes.
//!
//! Measured on govdocs 002_002456 (24-08-2026): five conversions produced two
//! different files. Same content, same extracted text, different object
//! numbers -- `/FontDescriptor 2415 0 R` against `2416 0 R`. The cause was
//! `isolate_font_descriptors` walking a HashMap while creating objects, so the
//! numbers it handed out depended on an iteration order that is not stable
//! between runs.
//!
//! `text_retention_baseline.json` had already recorded the symptom on two
//! documents as non-deterministic conversion and worked around it by
//! taking the measured minimum over four runs. The cause was not written down,
//! so the workaround stayed.
//!
//! For an archive this is not cosmetic. An archive that verifies its files by
//! hash, or signs them, cannot use output that differs per run -- and a
//! benchmark an outsider reruns (AR3) cannot either.

/// Convert the same bytes twice and compare.
fn convert_twice(source: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let opts = pdf_manip::pdfa::PdfAConvertOptions::default();
    let first = pdf_manip::pdfa::convert_bytes(source, &opts).expect("first conversion failed");
    let second = pdf_manip::pdfa::convert_bytes(source, &opts).expect("second conversion failed");
    (first, second)
}

#[test]
fn converting_the_same_document_twice_gives_the_same_bytes() {
    const SOURCE: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");

    let (first, second) = convert_twice(SOURCE);
    assert_eq!(
        first.len(),
        second.len(),
        "two conversions of one document produced different sizes"
    );
    assert!(
        first == second,
        "two conversions of one document produced different bytes; the first \
         difference is at offset {}",
        first
            .iter()
            .zip(second.iter())
            .position(|(a, b)| a != b)
            .map_or_else(|| "end".to_string(), |i| i.to_string())
    );
}

#[test]
fn a_document_with_shared_font_descriptors_stays_stable() {
    // The failing path was descriptor isolation, which only runs when several
    // fonts point at one descriptor. A fixture without that shape would pass
    // the test above on code that is still unstable.
    const SOURCE: &[u8] = include_bytes!("../../../tests/corpus-mini/acroform.pdf");

    let (first, second) = convert_twice(SOURCE);
    assert!(
        first == second,
        "conversion is not stable on a document with shared font descriptors"
    );
}
