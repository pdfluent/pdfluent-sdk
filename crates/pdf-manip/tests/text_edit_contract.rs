//! Phase 1B contract tests for the `text_edit` engine
//! (docs/TEXT_REPLACE_ENGINE_DESIGN.md). Each test is one acceptance
//! criterion; together they are the executable spec that the legacy
//! characterization pins get migrated onto in Phase 1C.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdf_manip::text_edit::{
    begin_text_edit, ContainerKind, DocumentRevision, FontFallback, ReplaceOptions,
    ReplacementStatus, SignaturePolicy, StaleReason, TextEditError, TextQuery,
    UnsupportedContainer,
};
use pdf_manip::text_run::extract_page_text_runs;

// ---------------------------------------------------------------------------
// Fixture builders (mirrors text_replace_characterization.rs)
// ---------------------------------------------------------------------------

fn helvetica_font() -> Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    }
}

fn subset_font() -> Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "ABCDEF+MysterySans",
    }
}

fn make_doc(fonts: Vec<(&str, Dictionary)>, contents: Vec<Vec<u8>>) -> (Document, Vec<(u32, u16)>) {
    let mut doc = Document::with_version("1.7");

    let mut font_resources = Dictionary::new();
    for (name, dict) in fonts {
        let id = doc.add_object(Object::Dictionary(dict));
        font_resources.set(name, Object::Reference(id));
    }
    let resources = dictionary! {
        "Font" => Object::Dictionary(font_resources),
    };

    let mut stream_ids = Vec::new();
    for bytes in contents {
        let id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, bytes)));
        stream_ids.push(id);
    }
    let contents_obj = if stream_ids.len() == 1 {
        Object::Reference(stream_ids[0])
    } else {
        Object::Array(stream_ids.iter().map(|&id| Object::Reference(id)).collect())
    };

    let page_dict = dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => contents_obj,
        "Resources" => Object::Dictionary(resources),
    };
    let page_id = doc.add_object(Object::Dictionary(page_dict));

    let pages_dict = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
    };
    let pages_id = doc.add_object(Object::Dictionary(pages_dict));
    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }

    let catalog = dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    };
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    (doc, stream_ids)
}

fn ops(operations: Vec<Operation>) -> Vec<u8> {
    Content { operations }.encode().unwrap()
}

fn tj(text: &[u8]) -> Operation {
    Operation::new(
        "Tj",
        vec![Object::String(text.to_vec(), StringFormat::Literal)],
    )
}

fn tf(font: &str, size: f64) -> Operation {
    Operation::new(
        "Tf",
        vec![
            Object::Name(font.as_bytes().to_vec()),
            Object::Real(size as f32),
        ],
    )
}

fn td(x: f64, y: f64) -> Operation {
    Operation::new("Td", vec![Object::Real(x as f32), Object::Real(y as f32)])
}

fn simple_text_content(lines: &[&str]) -> Vec<u8> {
    let mut operations = vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
    ];
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            operations.push(td(0.0, -20.0));
        }
        operations.push(tj(line.as_bytes()));
    }
    operations.push(Operation::new("ET", vec![]));
    ops(operations)
}

fn page_text(doc: &Document) -> String {
    extract_page_text_runs(doc, 1)
        .unwrap_or_default()
        .iter()
        .map(|r| r.text.as_str())
        .collect()
}

fn revision_for(doc: &mut Document) -> DocumentRevision {
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    DocumentRevision::from_source_bytes(&buf)
}

fn stream_content(doc: &Document, id: (u32, u16)) -> Vec<u8> {
    match doc.get_object(id).unwrap() {
        Object::Stream(s) => {
            let mut s = s.clone();
            let _ = s.decompress();
            s.content.clone()
        }
        _ => panic!("not a stream"),
    }
}

// ---------------------------------------------------------------------------
// §4: selection
// ---------------------------------------------------------------------------

/// find_text returns every occurrence with a distinct MatchId; staging a
/// replacement on ONLY the second occurrence leaves the first untouched.
#[test]
fn contract_second_occurrence_individually_selectable() {
    let (mut doc, _) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Acme", "Acme"])],
    );
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let matches = session.find_text(TextQuery::exact("Acme")).unwrap();
    assert_eq!(matches.len(), 2);
    assert_ne!(matches[0].id, matches[1].id, "distinct locators");

    session
        .stage_replace(&matches[1].id, "Bqme", ReplaceOptions::default())
        .unwrap();
    let report = session.commit().unwrap();

    assert_eq!(report.replacements_applied, 1);
    assert_eq!(page_text(&doc), "AcmeBqme", "first occurrence untouched");
}

// ---------------------------------------------------------------------------
// §3: MatchId lifecycle
// ---------------------------------------------------------------------------

/// A MatchId from revision N is rejected with StaleMatch after any commit,
/// and source-byte changes under a (mistakenly reused) old revision are also
/// detected. No fuzzy relocation.
#[test]
fn contract_stale_match_id_rejected_typed() {
    let (mut doc, _) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Hello World"])],
    );
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let matches = session.find_text(TextQuery::exact("Hello")).unwrap();
    let old_id = matches[0].id.clone();
    session
        .stage_replace(&old_id, "Howdy", ReplaceOptions::default())
        .unwrap();
    let report = session.commit().unwrap();

    // Correct workflow: next session uses next_revision → RevisionChanged.
    let mut session2 = begin_text_edit(&mut doc, report.next_revision).unwrap();
    match session2.resolve(&old_id) {
        Err(TextEditError::StaleMatch { reason, .. }) => {
            assert_eq!(reason, StaleReason::RevisionChanged);
        }
        other => panic!("expected StaleMatch(RevisionChanged), got {other:?}"),
    }
    drop(session2);

    // Caller error: reusing the OLD revision. The revision check passes, but
    // the source bytes under the locator changed → SourceBytesChanged.
    let mut session3 = begin_text_edit(&mut doc, rev).unwrap();
    match session3.resolve(&old_id) {
        Err(TextEditError::StaleMatch { reason, .. }) => {
            assert_eq!(reason, StaleReason::SourceBytesChanged);
        }
        other => panic!("expected StaleMatch(SourceBytesChanged), got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// §5: conflicts & atomicity
// ---------------------------------------------------------------------------

/// Two staged edits whose ranges intersect fail commit with
/// OverlappingEdits naming both ids.
#[test]
fn contract_overlapping_edits_detected() {
    let (mut doc, _) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Hello World"])],
    );
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let a = &session.find_text(TextQuery::exact("Hello Wor")).unwrap()[0]
        .id
        .clone();
    let b = &session.find_text(TextQuery::exact("World")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(a, "XXXXX XXX", ReplaceOptions::default())
        .unwrap();
    session
        .stage_replace(b, "YYYYY", ReplaceOptions::default())
        .unwrap();

    let err = session.commit().unwrap_err();
    match &err.error {
        TextEditError::OverlappingEdits { a: ia, b: ib } => {
            assert_ne!(ia, ib);
        }
        other => panic!("expected OverlappingEdits, got {other:?}"),
    }
    assert_eq!(err.results.len(), 2, "every staged edit reported");
    assert_eq!(page_text(&doc), "Hello World", "document untouched");
}

/// AllOrNothing: if one of three staged edits fails validation, commit
/// returns CommitError with all three per-edit results and the document is
/// untouched.
#[test]
fn contract_all_or_nothing_rollback() {
    let (mut doc, stream_ids) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Alpha", "Beta", "Gamma"])],
    );
    let before = stream_content(&doc, stream_ids[0]);
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let a = session.find_text(TextQuery::exact("Alpha")).unwrap()[0]
        .id
        .clone();
    let b = session.find_text(TextQuery::exact("Beta")).unwrap()[0]
        .id
        .clone();
    let c = session.find_text(TextQuery::exact("Gamma")).unwrap()[0]
        .id
        .clone();

    session
        .stage_replace(&a, "Delta", ReplaceOptions::default())
        .unwrap();
    // Ω is not encodable in Helvetica (Latin-1) and fallback is denied.
    session
        .stage_replace(&b, "\u{03A9}mega", ReplaceOptions::default())
        .unwrap();
    session
        .stage_replace(&c, "Kappa", ReplaceOptions::default())
        .unwrap();

    let err = session.commit().unwrap_err();
    assert_eq!(err.results.len(), 3, "all three edits reported");
    let failed = err
        .results
        .iter()
        .filter(|r| matches!(r.status, ReplacementStatus::Failed { .. }))
        .count();
    assert_eq!(failed, 3, "AllOrNothing: every edit reports failure/abort");

    assert_eq!(page_text(&doc), "AlphaBetaGamma", "text untouched");
    assert_eq!(
        stream_content(&doc, stream_ids[0]),
        before,
        "stream bytes bit-identical after rollback"
    );
}

// ---------------------------------------------------------------------------
// §2.1: stream structure preservation
// ---------------------------------------------------------------------------

fn three_stream_doc() -> (Document, Vec<(u32, u16)>) {
    let a = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Alpha"),
        Operation::new("ET", vec![]),
    ]);
    let b = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 650.0),
        tj(b"Target"),
        Operation::new("ET", vec![]),
    ]);
    let c = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 600.0),
        tj(b"Gamma"),
        Operation::new("ET", vec![]),
    ]);
    make_doc(vec![("F1", helvetica_font())], vec![a, b, c])
}

/// A page with /Contents [A B C] keeps a three-element array after an edit
/// that only touches B; A and C remain byte-identical with their original
/// object ids.
#[test]
fn contract_multiple_content_streams_preserved() {
    let (mut doc, stream_ids) = three_stream_doc();
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let m = session.find_text(TextQuery::exact("Target")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(&m, "Edited", ReplaceOptions::default())
        .unwrap();
    let report = session.commit().unwrap();
    assert_eq!(report.replacements_applied, 1);

    let page_id = *doc.get_pages().get(&1).unwrap();
    let contents = doc
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();
    match contents {
        Object::Array(arr) => {
            assert_eq!(arr.len(), 3, "three-element /Contents array preserved");
            for (i, obj) in arr.iter().enumerate() {
                assert_eq!(
                    obj,
                    &Object::Reference(stream_ids[i]),
                    "stream {i} keeps its object id"
                );
            }
        }
        other => panic!("/Contents no longer an array: {other:?}"),
    }
    assert_eq!(page_text(&doc), "AlphaEditedGamma");
}

/// Only the container holding the match is re-encoded; every other stream
/// keeps its exact original bytes.
#[test]
fn contract_only_touched_stream_rewritten() {
    let (mut doc, stream_ids) = three_stream_doc();
    let before_a = stream_content(&doc, stream_ids[0]);
    let before_b = stream_content(&doc, stream_ids[1]);
    let before_c = stream_content(&doc, stream_ids[2]);
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let m = session.find_text(TextQuery::exact("Target")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(&m, "Edited", ReplaceOptions::default())
        .unwrap();
    let report = session.commit().unwrap();

    assert_eq!(stream_content(&doc, stream_ids[0]), before_a, "A untouched");
    assert_eq!(stream_content(&doc, stream_ids[2]), before_c, "C untouched");
    assert_ne!(stream_content(&doc, stream_ids[1]), before_b, "B rewritten");
    assert_eq!(
        report.containers_modified.len(),
        1,
        "exactly one container reported modified"
    );
    assert_eq!(report.containers_modified[0].stream_obj, stream_ids[1]);
}

// ---------------------------------------------------------------------------
// §2.1/§8: Form XObjects
// ---------------------------------------------------------------------------

/// Text inside a Form XObject is FOUND (editable = false,
/// UnsupportedContainer::FormXObject); staging fails with that typed error —
/// never reported as "not found".
#[test]
fn contract_xobject_match_detected_reported_unsupported() {
    let xobj_content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(50.0, 50.0),
        tj(b"Hidden"),
        Operation::new("ET", vec![]),
    ]);
    let page_content = ops(vec![Operation::new(
        "Do",
        vec![Object::Name(b"Fm0".to_vec())],
    )]);
    let (mut doc, _) = make_doc(vec![("F1", helvetica_font())], vec![page_content]);

    let font_id = doc.add_object(Object::Dictionary(helvetica_font()));
    let xobj_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 200.into(), 100.into()],
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "F1" => Object::Reference(font_id),
                }),
            }),
        },
        xobj_content,
    );
    let xobj_id = doc.add_object(Object::Stream(xobj_stream));
    let page_id = *doc.get_pages().get(&1).unwrap();
    let resources = doc
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .clone();
    if let Object::Dictionary(mut res) = resources {
        res.set(
            "XObject",
            Object::Dictionary(dictionary! { "Fm0" => Object::Reference(xobj_id) }),
        );
        if let Ok(Object::Dictionary(ref mut pd)) = doc.get_object_mut(page_id) {
            pd.set("Resources", Object::Dictionary(res));
        }
    }
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let matches = session.find_text(TextQuery::exact("Hidden")).unwrap();
    assert_eq!(
        matches.len(),
        1,
        "XObject text is FOUND, not silently absent"
    );
    let m = &matches[0];
    assert!(!m.editable);
    assert!(matches!(
        m.container.kind,
        ContainerKind::FormXObject { .. }
    ));

    let err = session
        .stage_replace(&m.id, "Shown!", ReplaceOptions::default())
        .unwrap_err();
    assert!(
        matches!(
            err,
            TextEditError::UnsupportedContainer {
                kind: UnsupportedContainer::FormXObject,
                ..
            }
        ),
        "typed unsupported-container error, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// §7: tagged text
// ---------------------------------------------------------------------------

/// A match covered by /ActualText fails replacement with TaggedTextConflict
/// carrying both strings.
#[test]
fn contract_actual_text_conflict_typed_diagnostic() {
    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        Operation::new(
            "BDC",
            vec![
                Object::Name(b"Span".to_vec()),
                Object::Dictionary(dictionary! {
                    "ActualText" => Object::String(b"OldText".to_vec(), StringFormat::Literal),
                }),
            ],
        ),
        tj(b"OldText"),
        Operation::new("EMC", vec![]),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", helvetica_font())], vec![content]);
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let matches = session.find_text(TextQuery::exact("OldText")).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].actual_text.as_deref(), Some("OldText"));
    assert!(!matches[0].editable);

    let err = session
        .stage_replace(&matches[0].id, "NewText", ReplaceOptions::default())
        .unwrap_err();
    match err {
        TextEditError::TaggedTextConflict {
            visual_text,
            actual_text,
            ..
        } => {
            assert_eq!(visual_text, "OldText");
            assert_eq!(actual_text, "OldText");
        }
        other => panic!("expected TaggedTextConflict, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// §4.3/§4.5: fonts & encoding
// ---------------------------------------------------------------------------

/// Font fallback only occurs under an explicit policy and is always
/// reported; FontFallback::Deny (default) fails typed instead.
#[test]
fn contract_font_fallback_never_silent() {
    // Subset font: original font cannot encode any replacement.
    let (mut doc, _) = make_doc(
        vec![("F1", subset_font())],
        vec![simple_text_content(&["Hello"])],
    );
    let rev = revision_for(&mut doc);

    // Default (Deny): typed failure, nothing applied.
    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let m = session.find_text(TextQuery::exact("Hello")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(&m, "Howdy", ReplaceOptions::default())
        .unwrap();
    let err = session.commit().unwrap_err();
    assert!(
        matches!(err.error, TextEditError::FontFallbackDenied { .. }),
        "expected FontFallbackDenied, got {:?}",
        err.error
    );
    assert_eq!(page_text(&doc), "Hello", "document untouched under Deny");

    // InjectStandard: succeeds AND reports the substitution.
    let rev = revision_for(&mut doc);
    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let m = session.find_text(TextQuery::exact("Hello")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(
            &m,
            "Howdy",
            ReplaceOptions::default().font_fallback(FontFallback::InjectStandard),
        )
        .unwrap();
    let report = session.commit().unwrap();
    assert_eq!(report.replacements_applied, 1);
    assert!(report.results[0].font_substituted, "substitution reported");
    assert_eq!(report.results[0].font_used, "F__Helv");
    assert_eq!(page_text(&doc), "Howdy");
}

/// A cross-run match whose replacement cannot be encoded produces a per-edit
/// failure — never a silent count-0 success.
#[test]
fn contract_cross_run_encoding_failure_reported() {
    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Hel"),
        tj(b"lo World"),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", subset_font())], vec![content]);
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let matches = session.find_text(TextQuery::exact("Hello")).unwrap();
    assert_eq!(matches.len(), 1, "cross-run match is found");
    session
        .stage_replace(&matches[0].id, "Howdy", ReplaceOptions::default())
        .unwrap();

    let err = session.commit().unwrap_err();
    assert!(
        matches!(
            err.error,
            TextEditError::FontFallbackDenied { .. } | TextEditError::EncodingFailed { .. }
        ),
        "typed encoding failure, got {:?}",
        err.error
    );
    assert_eq!(err.results.len(), 1);
    assert!(matches!(
        err.results[0].status,
        ReplacementStatus::Failed { .. }
    ));
    assert_eq!(page_text(&doc), "Hello World", "document untouched");
}

// ---------------------------------------------------------------------------
// §6: signatures
// ---------------------------------------------------------------------------

fn signed_fixture() -> Vec<u8> {
    let p12_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdf-sign/tests/fixtures/test-rsa.p12"
    );
    let data = std::fs::read(p12_path).expect("test PKCS#12 fixture");
    let signer = pdf_sign::Pkcs12Signer::from_pkcs12(&data, "test123").expect("signer");

    let (mut doc, _) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Hello World"])],
    );
    let mut unsigned = Vec::new();
    doc.save_to(&mut unsigned).unwrap();
    pdf_sign::sign_pdf(&unsigned, &signer, &pdf_sign::SignOptions::default()).expect("sign")
}

/// SignaturePolicy::RejectSignedDocuments (default) refuses to commit into a
/// signed document; AllowPostSignatureChange proceeds and reports
/// signatures_invalidated.
#[test]
fn contract_signed_document_rejected_by_default() {
    let signed = signed_fixture();

    // Default policy: rejected, document untouched.
    let mut doc = Document::load_mem(&signed).unwrap();
    let rev = DocumentRevision::from_source_bytes(&signed);
    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let m = session.find_text(TextQuery::exact("Hello")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(&m, "Howdy", ReplaceOptions::default())
        .unwrap();
    let err = session.commit().unwrap_err();
    match &err.error {
        TextEditError::SignedDocumentRejected { signatures } => {
            assert!(!signatures.is_empty(), "signatures listed in the error");
        }
        other => panic!("expected SignedDocumentRejected, got {other:?}"),
    }
    assert_eq!(page_text(&doc), "Hello World", "document untouched");

    // Explicit opt-in: proceeds and reports invalidation.
    let mut doc = Document::load_mem(&signed).unwrap();
    let rev = DocumentRevision::from_source_bytes(&signed);
    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let m = session.find_text(TextQuery::exact("Hello")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(
            &m,
            "Howdy",
            ReplaceOptions::default().signature_policy(SignaturePolicy::AllowPostSignatureChange),
        )
        .unwrap();
    let report = session.commit().unwrap();
    assert_eq!(report.replacements_applied, 1);
    assert!(report.signatures_present);
    assert!(report.signatures_invalidated);
    assert_eq!(page_text(&doc), "Howdy World");
}

// ---------------------------------------------------------------------------
// §4.4: accounting law
// ---------------------------------------------------------------------------

/// matches_found == replacements_applied + replacements_failed holds for
/// every commit (the no-silent-skips law).
#[test]
fn contract_no_silent_skips_accounting() {
    let (mut doc, _) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Alpha", "Beta"])],
    );
    let rev = revision_for(&mut doc);

    // Successful commit: 2 staged → 2 applied + 0 failed.
    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let a = session.find_text(TextQuery::exact("Alpha")).unwrap()[0]
        .id
        .clone();
    let b = session.find_text(TextQuery::exact("Beta")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(&a, "Delta", ReplaceOptions::default())
        .unwrap();
    session
        .stage_replace(&b, "Zeta", ReplaceOptions::default())
        .unwrap();
    let report = session.commit().unwrap();
    assert_eq!(
        report.matches_found,
        report.replacements_applied + report.replacements_failed
    );
    assert_eq!(report.results.len(), report.matches_found);

    // Failing commit: every staged edit accounted for in the error.
    let rev = report.next_revision;
    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let d = session.find_text(TextQuery::exact("Delta")).unwrap()[0]
        .id
        .clone();
    let z = session.find_text(TextQuery::exact("Zeta")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(&d, "Alpha", ReplaceOptions::default())
        .unwrap();
    session
        .stage_replace(&z, "\u{03A9}mega", ReplaceOptions::default())
        .unwrap();
    let err = session.commit().unwrap_err();
    assert_eq!(err.results.len(), 2, "all staged edits in the error report");
}

// ---------------------------------------------------------------------------
// Engine behaviour beyond the 12 core contracts
// ---------------------------------------------------------------------------

/// TJ kerning adjustments BEFORE the edited region are preserved verbatim
/// (the legacy engine flattened the whole array — defect C5).
#[test]
fn engine_tj_kerning_preserved_outside_match() {
    let tj_array = Operation::new(
        "TJ",
        vec![Object::Array(vec![
            Object::String(b"AB".to_vec(), StringFormat::Literal),
            Object::Integer(-120),
            Object::String(b"CD".to_vec(), StringFormat::Literal),
            Object::Integer(-80),
            Object::String(b"EF".to_vec(), StringFormat::Literal),
        ])],
    );
    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj_array,
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", helvetica_font())], vec![content]);
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let m = session.find_text(TextQuery::exact("EF")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(&m, "GH", ReplaceOptions::default())
        .unwrap();
    let report = session.commit().unwrap();

    assert_eq!(page_text(&doc), "ABCDGH");
    // The rebuilt TJ keeps the untouched prefix verbatim, incl. the -120.
    let editor = pdf_manip::content_editor::editor_for_page(&doc, 1).unwrap();
    let tj_op = editor
        .operations()
        .iter()
        .find(|op| op.operator == "TJ")
        .expect("TJ preserved");
    let arr = match &tj_op.operands[0] {
        Object::Array(a) => a,
        other => panic!("unexpected TJ operand {other:?}"),
    };
    assert_eq!(
        arr[0],
        Object::String(b"AB".to_vec(), StringFormat::Literal),
        "leading element verbatim"
    );
    assert_eq!(arr[1], Object::Integer(-120), "leading kerning preserved");
    assert_eq!(
        arr[2],
        Object::String(b"CD".to_vec(), StringFormat::Literal),
        "second element verbatim"
    );
    // Spacing adjacent to the edit (-80) is dropped and reported.
    assert!(report.results[0]
        .diagnostics
        .iter()
        .any(|d| d.code == "kerning-dropped-in-match-region"));
}

/// A cross-run match keeps every line's text in the operator that draws it
/// (offset-aware distribution, parity with the legacy multiline fix).
#[test]
fn engine_cross_run_replacement_distributes_by_offset() {
    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Hel"),
        tj(b"lo World"),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", helvetica_font())], vec![content]);
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let m = session.find_text(TextQuery::exact("Hello")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(&m, "Howdy", ReplaceOptions::default())
        .unwrap();
    session.commit().unwrap();

    let runs = extract_page_text_runs(&doc, 1).unwrap();
    let texts: Vec<&str> = runs.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(texts, vec!["Howdy", " World"]);
}

/// Engine output stays openable and extractable by the real reader, and the
/// old text is gone (roundtrip acceptance criterion).
#[test]
fn engine_roundtrip_open_and_extract() {
    let (mut doc, _) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Hello World"])],
    );
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let m = session.find_text(TextQuery::exact("Hello")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(&m, "Howdy", ReplaceOptions::default())
        .unwrap();
    session.commit().unwrap();

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let engine_doc = pdf_engine::PdfDocument::open(bytes).expect("reopen");
    let text = engine_doc.extract_text(0).expect("extract");
    assert!(text.contains("Howdy"), "{text:?}");
    assert!(!text.contains("Hello"), "old text gone: {text:?}");
}

/// Region queries select by positive-area bbox intersection (design §10.3);
/// case-insensitive matching folds per char.
#[test]
fn engine_region_and_case_insensitive_queries() {
    let (mut doc, _) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Acme", "Acme"])],
    );
    let rev = revision_for(&mut doc);
    let mut session = begin_text_edit(&mut doc, rev).unwrap();

    // Both occurrences, case-insensitively.
    let all = session
        .find_text(TextQuery::exact("ACME").case_insensitive(true))
        .unwrap();
    assert_eq!(all.len(), 2);

    // Region around the second line (y = 680) only.
    let region = session
        .find_text(TextQuery::exact("Acme").region(1, [50.0, 670.0, 500.0, 695.0]))
        .unwrap();
    assert_eq!(region.len(), 1);
    assert_eq!(region[0].id, all[1].id, "the second occurrence");
}

// ---------------------------------------------------------------------------
// §10.4 Phase 1C: BestEffort + convenience API
// ---------------------------------------------------------------------------

fn best_effort() -> ReplaceOptions {
    ReplaceOptions::default().commit_policy(pdf_manip::text_edit::CommitPolicy::BestEffort)
}

/// BestEffort applies the valid subset and reports every failure per edit —
/// the accounting law holds and nothing is silent.
#[test]
fn contract_best_effort_partial_apply() {
    let (mut doc, _) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Alpha", "Beta", "Gamma"])],
    );
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let a = session.find_text(TextQuery::exact("Alpha")).unwrap()[0]
        .id
        .clone();
    let b = session.find_text(TextQuery::exact("Beta")).unwrap()[0]
        .id
        .clone();
    let c = session.find_text(TextQuery::exact("Gamma")).unwrap()[0]
        .id
        .clone();
    session.stage_replace(&a, "Delta", best_effort()).unwrap();
    session
        .stage_replace(&b, "\u{03A9}mega", best_effort()) // unencodable
        .unwrap();
    session.stage_replace(&c, "Kappa", best_effort()).unwrap();

    let report = session.commit().expect("BestEffort commit returns Ok");
    assert_eq!(report.matches_found, 3);
    assert_eq!(report.replacements_applied, 2);
    assert_eq!(report.replacements_failed, 1);
    assert_eq!(
        report.matches_found,
        report.replacements_applied + report.replacements_failed
    );
    assert!(matches!(
        report.results[1].status,
        ReplacementStatus::Failed { .. }
    ));
    assert_eq!(page_text(&doc), "DeltaBetaKappa");
}

/// BestEffort resolves overlaps deterministically: the earlier-staged edit
/// wins; the later one fails with the overlap reason.
#[test]
fn contract_best_effort_overlap_earlier_staged_wins() {
    let (mut doc, _) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Hello World"])],
    );
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let a = session.find_text(TextQuery::exact("Hello")).unwrap()[0]
        .id
        .clone();
    let b = session.find_text(TextQuery::exact("lo World")).unwrap()[0]
        .id
        .clone();
    session.stage_replace(&a, "Howdy", best_effort()).unwrap();
    session
        .stage_replace(&b, "XXXXXXXX", best_effort())
        .unwrap();

    let report = session.commit().unwrap();
    assert_eq!(report.replacements_applied, 1);
    assert_eq!(report.replacements_failed, 1);
    assert!(matches!(
        report.results[0].status,
        ReplacementStatus::Applied
    ));
    match &report.results[1].status {
        ReplacementStatus::Failed { reason } => {
            assert!(
                reason.contains("overlap"),
                "reason names the conflict: {reason}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert_eq!(page_text(&doc), "Howdy World", "earlier-staged edit won");
}

/// A BestEffort commit where every edit fails still returns Ok, touches
/// nothing, and does not advance the revision.
#[test]
fn contract_best_effort_all_failed_still_ok() {
    let (mut doc, _) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Hello"])],
    );
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let m = session.find_text(TextQuery::exact("Hello")).unwrap()[0]
        .id
        .clone();
    session
        .stage_replace(&m, "\u{03A9}mega", best_effort())
        .unwrap();

    let report = session.commit().unwrap();
    assert_eq!(report.replacements_applied, 0);
    assert_eq!(report.replacements_failed, 1);
    assert!(report.pages_modified.is_empty());
    assert_eq!(report.next_revision, rev, "no mutation → same revision");
    assert_eq!(page_text(&doc), "Hello");
}

/// Mixed policies: AllOrNothing is strictest and wins, so one failing edit
/// aborts the whole transaction even though the other edit asked BestEffort.
#[test]
fn contract_mixed_policy_strictest_wins() {
    let (mut doc, _) = make_doc(
        vec![("F1", helvetica_font())],
        vec![simple_text_content(&["Alpha", "Beta"])],
    );
    let rev = revision_for(&mut doc);

    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let a = session.find_text(TextQuery::exact("Alpha")).unwrap()[0]
        .id
        .clone();
    let b = session.find_text(TextQuery::exact("Beta")).unwrap()[0]
        .id
        .clone();
    session.stage_replace(&a, "Delta", best_effort()).unwrap();
    session
        .stage_replace(&b, "\u{03A9}mega", ReplaceOptions::default()) // AllOrNothing
        .unwrap();

    let err = session.commit().unwrap_err();
    assert_eq!(err.results.len(), 2);
    assert_eq!(page_text(&doc), "AlphaBeta", "nothing applied");
}

/// The convenience API accounts for every found occurrence: editable ones
/// commit, non-editable ones (here: Form XObject text) appear as Failed
/// results — never silently skipped.
#[test]
fn contract_convenience_replace_text_accounts_everything() {
    // Page text "Acme" plus an XObject that also draws "Acme".
    let xobj_content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(50.0, 50.0),
        tj(b"Acme"),
        Operation::new("ET", vec![]),
    ]);
    let page_content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Acme"),
        Operation::new("ET", vec![]),
        Operation::new("Do", vec![Object::Name(b"Fm0".to_vec())]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", helvetica_font())], vec![page_content]);

    let font_id = doc.add_object(Object::Dictionary(helvetica_font()));
    let xobj_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 200.into(), 100.into()],
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "F1" => Object::Reference(font_id),
                }),
            }),
        },
        xobj_content,
    );
    let xobj_id = doc.add_object(Object::Stream(xobj_stream));
    let page_id = *doc.get_pages().get(&1).unwrap();
    let resources = doc
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .clone();
    if let Object::Dictionary(mut res) = resources {
        res.set(
            "XObject",
            Object::Dictionary(dictionary! { "Fm0" => Object::Reference(xobj_id) }),
        );
        if let Ok(Object::Dictionary(ref mut pd)) = doc.get_object_mut(page_id) {
            pd.set("Resources", Object::Dictionary(res));
        }
    }
    let rev = revision_for(&mut doc);

    let report = pdf_manip::text_edit::replace_text(
        &mut doc,
        rev,
        TextQuery::exact("Acme"),
        "Bqme",
        ReplaceOptions::default(),
    )
    .unwrap();

    assert_eq!(report.matches_found, 2, "page + XObject occurrence found");
    assert_eq!(report.replacements_applied, 1);
    assert_eq!(report.replacements_failed, 1);
    assert_eq!(report.results.len(), 2);
    assert_eq!(page_text(&doc), "Bqme", "page occurrence replaced");
}
