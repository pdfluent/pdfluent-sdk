// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! What every conversion costs before it has converted anything.
//!
//! The size axis reads 102.63% of input over the 300-document tuning set
//! (07-09-2026, `benchmarks/pdfa/size/round1-300sample.json`), which sounds
//! close to done and is not: the MEDIAN document is at 119.4%, 148 of 300 are
//! above 120%, and the 108 documents under 100 kB have a median of 220%. A
//! total near 100% and a median at 119% is a corpus where a few large documents
//! shrink and most small ones roughly double.
//!
//! The reason small documents doubled was a fixed cost. `pdfa_colorspace.rs`
//! embedded three ICC profiles into EVERY output -- DefaultCMYK, DefaultRGB and
//! DefaultGray -- whether the document used those device colour spaces or not,
//! on the grounds that "device color spaces may appear in compressed streams or
//! inline images". 16,333 bytes per file; 4,899,900 across the 300, against a
//! total overshoot of 5,400,759.
//!
//! Round 2 of #379 makes them conditional on `census_device_colors`, which
//! reads the compressed streams and the inline images rather than assuming
//! about them (`crates/pdf-manip/tests/pdfa_device_colour_census.rs` holds one
//! fixture per route into it). What a conversion costs is therefore no longer
//! one number -- it depends on the document -- and the number below is the one
//! that matters for the small files: what a document that draws black text on
//! a white page pays.
//!
//! **16,333 -> 8,451 bytes, and the remaining 8,325 of it is the sRGB output
//! intent, not a Default\* entry.** The output intent is required to
//! characterise the device colours the document does use, `compact_storage`
//! shares its profile with DefaultRGB (same bytes, same dictionary), and so a
//! document that uses DeviceRGB pays for that profile once rather than twice.
//! What conditional Default\* actually removes from a gray document is the CMYK
//! profile: 7,882 bytes, 48% of the fixed cost.
//!
//! (Round 1's table has the two payloads' labels the wrong way round: the
//! 8,325-byte stored stream is sRGB and the 7,882-byte one is CGATS001Compat.
//! The total it reports, and every conclusion drawn from it, is unaffected.)
//!
//! The test still fixes the number, so that:
//!
//!   * an unconditional profile coming back, or a profile that grows, is red
//!     here rather than measured on a corpus two weeks later;
//!   * and the next round that moves it has to edit this test on purpose, with
//!     its new number written down.

use lopdf::{Document, Object};

/// Every ICC profile payload in a converted document, by size.
fn icc_payload_bytes(pdf: &[u8]) -> Vec<usize> {
    let doc = Document::load_mem(pdf).expect("the converted document must load");
    let mut sizes = Vec::new();
    for object in doc.objects.values() {
        let Object::Stream(stream) = object else {
            continue;
        };
        // An ICCBased profile stream is the one carrying /N: 1, 3 or 4
        // components. Matching on that rather than on a name, because the
        // stream itself has no /Type and the colour space array points at it.
        if stream
            .dict
            .get(b"N")
            .ok()
            .and_then(|n| n.as_i64().ok())
            .is_some()
        {
            sizes.push(stream.content.len());
        }
    }
    sizes.sort_unstable();
    sizes
}

/// What a document that reaches no device space but gray pays, smallest first,
/// in bytes as stored (Flate-encoded by `compact_storage`): the DefaultGray
/// profile and the sRGB output intent's. Measured 08-09-2026 on the round-2
/// branch; it was `[126, 7882, 8325]` on 07-09-2026, before the census.
const FIXED_ICC_BYTES: [usize; 2] = [126, 8325];

/// The CGATS001Compat profile a document that never draws in CMYK no longer
/// carries, as stored. Named here because it is the round-2 saving.
const CMYK_PROFILE_BYTES: usize = 7882;

#[test]
fn a_gray_document_embeds_two_icc_profiles_and_no_more() {
    // A document that draws almost nothing, so what is measured is the cost of
    // converting rather than the cost of its content.
    const SOURCE: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");

    let opts = pdf_manip::pdfa::PdfAConvertOptions::default();
    let converted = pdf_manip::pdfa::convert_bytes(SOURCE, &opts).expect("conversion failed");
    let sizes = icc_payload_bytes(&converted);

    assert_eq!(
        sizes.len(),
        FIXED_ICC_BYTES.len(),
        "this document draws black text on a white page. It embeds {} ICC \
         profile(s); it embeds {} since the census landed (DefaultGray, and \
         the sRGB output intent), and embedded three before it. If a third is \
         deliberate, say what it is for and what it costs a 20 kB document. \
         Sizes now: {sizes:?}",
        sizes.len(),
        FIXED_ICC_BYTES.len(),
    );
    assert!(
        !sizes.contains(&CMYK_PROFILE_BYTES),
        "the CMYK profile is back in a document with no CMYK in it, which is \
         {CMYK_PROFILE_BYTES} bytes on every small file in an archive. Sizes \
         now: {sizes:?}"
    );
}

#[test]
fn the_fixed_colour_cost_of_a_conversion_does_not_drift() {
    const SOURCE: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");

    let opts = pdf_manip::pdfa::PdfAConvertOptions::default();
    let converted = pdf_manip::pdfa::convert_bytes(SOURCE, &opts).expect("conversion failed");
    let total: usize = icc_payload_bytes(&converted).iter().sum();
    let expected: usize = FIXED_ICC_BYTES.iter().sum();

    // A window rather than an exact figure: the profiles are Flate-encoded and
    // a zlib version can move a stored size by a few bytes without anything
    // here having changed. Wide enough to survive that, far too narrow to hide
    // a profile appearing, growing or being dropped.
    let slack = expected / 20;
    assert!(
        total.abs_diff(expected) <= slack,
        "the colour cost of converting a gray document is {total} bytes; it is \
         {expected} since round 2 of #379 and was 16,333 before it. Every \
         converted document pays this before it has converted anything, and \
         the 108 documents under 100 kB in the tuning set had a median output \
         of 220% of their input largely because of it \
         (benchmarks/pdfa/size/ROUND1.md, ROUND2.md). Moving it is welcome; \
         moving it silently is not."
    );
}
