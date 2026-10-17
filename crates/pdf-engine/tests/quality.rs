//! The quality tier through the public extraction path (#641).
//!
//! `Quality::Balanced` on `extract_markdown`: without the `ort-layout`
//! feature it fails with `MODEL_NOT_AVAILABLE` naming the feature and the
//! install command; with it but without the files, with
//! `MODEL_NOT_AVAILABLE` naming the path looked at and the install command.
//! Ungated is the markdown-only refusal: a call that returns blocks answers
//! `BalancedIsMarkdownOnly`, on every build.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::api_error::PdfError;
use pdf_engine::{ExtractOptions, OptionsError, PdfDocument, Quality};

/// One page with one line of Helvetica: enough document for the tier to
/// accept or refuse.
fn one_line_pdf() -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream, StringFormat};

    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
            Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    72.into(),
                    720.into(),
                ],
            ),
            Operation::new(
                "Tj",
                vec![Object::String(
                    b"Balanced quality reads this.".to_vec(),
                    StringFormat::Literal,
                )],
            ),
            Operation::new("ET", vec![]),
        ],
    };
    let content = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    let pages = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page)],
        "Count" => 1,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages,
    });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn balanced_is_rejected_for_blocks_on_every_build() {
    // The tier assembles markdown; a call that returns blocks has no balanced
    // answer and says so, rather than handing back the fast tier under a
    // balanced name. Ungated: validation runs on every build.
    let doc = PdfDocument::open(one_line_pdf()).unwrap();
    let options = ExtractOptions::default().with_quality(Quality::Balanced);
    let error = doc
        .extract_blocks(&options)
        .expect_err("balanced blocks must fail");
    assert_eq!(error.options(), Some(&OptionsError::BalancedIsMarkdownOnly));
    assert_eq!(error.code(), "INVALID_OPTIONS");
}

#[cfg(not(feature = "ort-layout"))]
#[test]
fn balanced_without_the_feature_names_the_feature_and_the_install() {
    // The default build: `MODEL_NOT_AVAILABLE` naming the cargo feature to
    // rebuild with and the install command. Only on this build -- with the
    // feature the same call reaches for the model files instead.
    let doc = PdfDocument::open(one_line_pdf()).unwrap();
    let options = ExtractOptions::default().with_quality(Quality::Balanced);
    let error = doc
        .extract_markdown(&options)
        .expect_err("balanced without the feature must fail");
    let model = error
        .model_error()
        .expect("the chain carries the ModelError");
    assert_eq!(
        model.kind,
        pdf_engine::model::ModelErrorKind::ModelNotAvailable
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

#[cfg(feature = "ort-layout")]
#[test]
fn balanced_with_a_missing_model_dir_names_the_path_and_the_install() {
    // The feature build without the files: `MODEL_NOT_AVAILABLE` naming the
    // path looked at and the install command. The model dir is a scratch path
    // that exists with nothing in it, so the failure is the missing file.
    let dir = std::env::temp_dir().join(format!("pdfluent-quality-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let doc = PdfDocument::open(one_line_pdf()).unwrap();
    let options = ExtractOptions::default()
        .with_quality(Quality::Balanced)
        .with_model_dir(&dir);
    let error = doc
        .extract_markdown(&options)
        .expect_err("balanced without the files must fail");
    let model = error
        .model_error()
        .expect("the chain carries the ModelError");
    assert_eq!(
        model.kind,
        pdf_engine::model::ModelErrorKind::ModelNotAvailable
    );
    assert_eq!(model.kind.code(), "MODEL_NOT_AVAILABLE");
    let path = model.path.as_ref().expect("the error names the path");
    assert!(
        path.starts_with(&dir),
        "the path is under the model dir: {}",
        path.display()
    );
    let message = error.to_string();
    assert!(
        message.contains("pdfluent models install layout"),
        "the message names the install command: {message}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
