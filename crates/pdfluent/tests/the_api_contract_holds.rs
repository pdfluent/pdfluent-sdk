//! `docs/API_CONTRACT.md` in code (#246).
//!
//! The contract says two things: `PdfDocument` is the entry point, and the
//! modules the site names exist. Neither was true of the site on 25-08-2026, and
//! there was nothing that would have noticed.
//!
//! A document in `docs/` changes nothing if nobody reads it. This test reads it:
//! it compiles the paths the contract promises, so a re-export that is dropped
//! by accident turns this red instead of the website.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// The three modules the contract adds exist and carry what it says.
#[test]
fn the_promised_modules_exist() {
    // `pdfa` is an alias for `compliance`; that the site calls it that is the
    // reason it is there.
    let _: pdfluent::pdfa::PdfAProfile = pdfluent::pdfa::PdfAProfile::A2b;
    let _: pdfluent::compliance::PdfAProfile = pdfluent::compliance::PdfAProfile::A2b;

    // `annotation` gives a name to what `PdfDocument::annotations()` returns.
    fn takes_annotation(_: &pdfluent::annotation::AnnotationInfo) {}
    let _ = takes_annotation;

    // `ocr` carries the trait, not a wired-up backend. That distinction is the
    // decision of 19-08-2026: a plain `pdfluent` dependency must not drag in
    // system libraries or download models.
    fn takes_backend<B: pdfluent::ocr::OcrBackend>(_: &B) {}
    let _ = takes_backend::<DummyBackend>;
}

struct DummyBackend;
impl pdfluent::ocr::OcrBackend for DummyBackend {
    fn name(&self) -> &str {
        "dummy"
    }
    fn recognize(
        &self,
        _rgb: &[u8],
        _w: u32,
        _h: u32,
    ) -> Result<pdfluent::ocr::OcrResult, pdfluent::ocr::OcrError> {
        unreachable!("never called; the type is the point")
    }
}

/// There is one entry point and not two, and that is meant to stay so.
///
/// Not a compilable assertion -- you cannot test that something does not exist
/// -- but a readable one: the contract says why, and `#[test]` puts the reason
/// in the test output where somebody looking for it will find it.
#[test]
fn there_is_one_entry_point_and_not_two() {
    // What the site described: `Sdk::new()?` and then `sdk.open(path)`. Two
    // steps of which the first carries nothing: no configuration, no lifetime,
    // no shared cache that PdfDocument does not already own.
    //
    // What exists:
    fn opens(path: &str) -> pdfluent::Result<pdfluent::PdfDocument> {
        pdfluent::PdfDocument::open(path)
    }
    let _ = opens;

    // And per-call configuration goes through options, not through a session:
    fn opens_with(path: &str) -> pdfluent::Result<pdfluent::PdfDocument> {
        pdfluent::PdfDocument::open_with(path, pdfluent::OpenOptions::new())
    }
    let _ = opens_with;
}

/// The verbs sit on the document, not in a module.
///
/// That is the contract's test: if the answer needs a document, it is a method.
/// The site invented `pdfluent::color`, `::content`, `::digest`, `::nup`,
/// `::stamp` and `::text` -- all of them things you do *to a document*.
#[test]
fn the_verbs_sit_on_the_document() {
    use pdfluent::PdfDocument;
    // Only the types are touched here; that they exist is the assertion.
    let _ = PdfDocument::extract_text;
    let _ = PdfDocument::find_text;
    let _ = PdfDocument::validate_pdfa;
    let _ = PdfDocument::convert_to_pdfa;
    let _ = PdfDocument::annotations;
    let _ = PdfDocument::add_watermark;
    let _ = PdfDocument::redact;
}
