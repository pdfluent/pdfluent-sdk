//! `Quality::Balanced` through the facade: the typed model failure, kept (#641).
//!
//! The engine carries model failures as `Io` with a typed cause (neither enum
//! gains a variant in 1.x), and the facade keeps the chain as [`Error::Io`]
//! rather than reporting a valid document as unparseable. On the default
//! build that is `MODEL_NOT_AVAILABLE` naming the cargo feature and the
//! install command.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#[cfg(not(feature = "ort-layout"))]
use pdfluent::{Error, ExtractOptions, PdfDocument, Quality};

#[cfg(not(feature = "ort-layout"))]
const SAMPLE: &str = "tests/fixtures/sample.pdf";

#[cfg(not(feature = "ort-layout"))]
#[test]
fn balanced_without_the_feature_keeps_the_typed_model_failure() {
    let options = ExtractOptions::default().with_quality(Quality::Balanced);
    let error = PdfDocument::open(SAMPLE)
        .expect("sample opens")
        .extract_markdown(&options)
        .expect_err("balanced without the feature must fail");
    assert!(
        matches!(&error, Error::Io { .. }),
        "the failure rides as Io, not as an unparseable document: {error:?}"
    );
    let model = error
        .model_error()
        .expect("the chain carries the ModelError");
    assert_eq!(
        model.kind,
        pdfluent::model::ModelErrorKind::ModelNotAvailable
    );
    assert_eq!(model.kind.code(), "MODEL_NOT_AVAILABLE");
    let message = error.to_string();
    assert!(
        message.contains("`ort-layout`"),
        "the message names the cargo feature: {message}"
    );
    assert!(
        message.contains("pdfluent models install layout"),
        "the message names the install command: {message}"
    );
}
