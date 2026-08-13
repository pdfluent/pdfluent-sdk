//! Facade tests for the Layout-Aware Text Replacement API on `PdfDocument`
//! (find_text / replace_text / replace_text_matches) including capability
//! gating and revision continuity across commits.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdfluent::text_edit::{ReplaceOptions, TextQuery};
use pdfluent::{Error, OpenOptions, PdfDocument};

/// Build a one-page PDF with Helvetica text lines, returned as bytes.
fn pdf_with_lines(lines: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    }));
    let mut operations = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)]),
        Operation::new("Td", vec![Object::Real(100.0), Object::Real(700.0)]),
    ];
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            operations.push(Operation::new(
                "Td",
                vec![Object::Real(0.0), Object::Real(-20.0)],
            ));
        }
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(
                line.as_bytes().to_vec(),
                StringFormat::Literal,
            )],
        ));
    }
    operations.push(Operation::new("ET", vec![]));
    let content = Content { operations }.encode().unwrap();

    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => Object::Reference(font_id),
            }),
        }),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
    }));
    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

fn open_developer(bytes: &[u8]) -> PdfDocument {
    PdfDocument::from_bytes_with(bytes, OpenOptions::new().with_license_key("tier:developer"))
        .unwrap()
}

#[test]
fn facade_find_and_replace_by_match_id() {
    let bytes = pdf_with_lines(&["Acme", "Acme"]);
    let mut doc = open_developer(&bytes);

    let matches = doc.find_text(TextQuery::exact("Acme")).unwrap();
    assert_eq!(matches.len(), 2);

    // Replace only the second occurrence (the async/[redacted] workflow shape).
    let edits = vec![(matches[1].id.clone(), "Bqme".to_string())];
    let report = doc
        .replace_text_matches(&edits, ReplaceOptions::default())
        .unwrap();
    assert_eq!(report.replacements_applied, 1);

    // The engine view is re-synced: extraction sees the change immediately.
    let text = doc.text().unwrap();
    assert!(
        text.contains("Bqme"),
        "engine synced after commit: {text:?}"
    );
    assert!(
        text.contains("Acme"),
        "first occurrence untouched: {text:?}"
    );
}

#[test]
fn facade_convenience_replace_text() {
    let bytes = pdf_with_lines(&["Hello World"]);
    let mut doc = open_developer(&bytes);

    let report = doc
        .replace_text(
            TextQuery::exact("Hello"),
            "Howdy",
            ReplaceOptions::default(),
        )
        .unwrap();
    assert_eq!(report.matches_found, 1);
    assert_eq!(report.replacements_applied, 1);
    assert_eq!(report.replacements_failed, 0);

    let text = doc.text().unwrap();
    assert!(text.contains("Howdy World"), "{text:?}");
}

#[test]
fn facade_revision_advances_and_stale_ids_are_refused() {
    let bytes = pdf_with_lines(&["Alpha", "Beta"]);
    let mut doc = open_developer(&bytes);

    let matches = doc.find_text(TextQuery::exact("Alpha")).unwrap();
    let old_id = matches[0].id.clone();

    // First commit succeeds and advances the internal revision.
    let report = doc
        .replace_text_matches(
            &[(old_id.clone(), "Delta".to_string())],
            ReplaceOptions::default(),
        )
        .unwrap();
    assert_eq!(report.replacements_applied, 1);

    // The old id is now stale: staging it again is a typed refusal, not a
    // silent no-op and not a fuzzy re-match.
    let err = doc
        .replace_text_matches(&[(old_id, "Gamma".to_string())], ReplaceOptions::default())
        .unwrap_err();
    match err {
        Error::TextEditFailed { reason } => {
            assert!(reason.contains("stale"), "stale refusal: {reason}");
        }
        other => panic!("expected TextEditFailed, got {other:?}"),
    }
    let text = doc.text().unwrap();
    assert!(
        text.contains("Delta") && !text.contains("Gamma"),
        "{text:?}"
    );
}

/// TextEdit is available in Trial, but trial edits stamp a visible notice
/// on every modified page. Licensed tiers edit without the notice.
#[test]
fn facade_trial_edits_work_but_stamp_a_notice() {
    let bytes = pdf_with_lines(&["Hello World"]);

    // Trial tier (no license key): the edit succeeds…
    let mut doc = PdfDocument::from_bytes(&bytes).unwrap();
    let report = doc
        .replace_text(
            TextQuery::exact("Hello"),
            "Howdy",
            ReplaceOptions::default(),
        )
        .unwrap();
    assert_eq!(report.replacements_applied, 1);

    // …and the modified page carries the trial notice.
    let text = doc.text().unwrap();
    assert!(text.contains("Howdy World"), "{text:?}");
    assert!(
        text.contains("PDFluent trial"),
        "trial notice stamped on the modified page: {text:?}"
    );

    // A licensed document edits WITHOUT the notice.
    let mut doc = open_developer(&bytes);
    doc.replace_text(
        TextQuery::exact("Hello"),
        "Howdy",
        ReplaceOptions::default(),
    )
    .unwrap();
    let text = doc.text().unwrap();
    assert!(text.contains("Howdy World"), "{text:?}");
    assert!(
        !text.contains("PDFluent trial"),
        "licensed edit is notice-free: {text:?}"
    );
}

/// A trial find_text (read-only) never stamps anything.
#[test]
fn facade_trial_find_is_read_only() {
    let bytes = pdf_with_lines(&["Hello"]);
    let mut doc = PdfDocument::from_bytes(&bytes).unwrap();
    let matches = doc.find_text(TextQuery::exact("Hello")).unwrap();
    assert_eq!(matches.len(), 1);
    let text = doc.text().unwrap();
    assert!(!text.contains("PDFluent trial"), "{text:?}");
}
