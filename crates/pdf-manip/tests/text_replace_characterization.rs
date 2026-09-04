//! Characterization tests for the legacy `text_replace` API.
//!
//! Phase 1C migrated `replace_text{,_all_pages}` onto the `text_edit` engine
//! (design doc §9 step 3). Tests named `characterize_*` pin behaviour that
//! deliberately stayed the same (the count-only contract, fallback injection,
//! signature-breaking edits under the legacy API). Tests named `migrated_*`
//! are former defect pins that were DELIBERATELY flipped by the migration —
//! each one documents the old and the new behaviour.
//!
//! The Phase 1B/1C acceptance contract lives in `text_edit_contract.rs` and
//! runs against the `pdf_manip::text_edit` engine directly.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdf_manip::content_editor::editor_for_page;
use pdf_manip::text_replace::replace_text;
use pdf_manip::text_run::extract_page_text_runs;
use pdf_manip::FontMap;

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

fn helvetica_font() -> Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    }
}

/// Subset font (BaseFont "ABCDEF+…") with no ToUnicode and no Encoding:
/// decodable (Latin-1 fallback on decode) but NOT safely encodable, so the
/// legacy replacement path must take its fallback branch.
fn subset_font() -> Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "ABCDEF+MysterySans",
    }
}

/// Build a single-page document with the given fonts (name → dict) and one
/// content stream per element of `contents`. Multiple elements produce a
/// `/Contents [A B …]` array of separate stream objects.
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

/// Full decoded text of page 1, via the same run extractor the legacy
/// replacement uses.
fn page_text(doc: &Document) -> String {
    extract_page_text_runs(doc, 1)
        .unwrap_or_default()
        .iter()
        .map(|r| r.text.as_str())
        .collect()
}

fn fonts_for(doc: &Document) -> FontMap {
    FontMap::from_page(doc, 1).unwrap()
}

fn save_bytes(doc: &mut Document) -> Vec<u8> {
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

// ---------------------------------------------------------------------------
// Characterization: selection & counting
// ---------------------------------------------------------------------------

/// LEGACY CONTRACT: the wrapper replaces every occurrence (single-occurrence
/// selection is a text_edit feature).
#[test]
fn characterize_replaces_all_occurrences_no_selection() {
    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Acme"),
        td(0.0, -20.0),
        tj(b"Acme"),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", helvetica_font())], vec![content]);

    let fonts = fonts_for(&doc);
    let count = replace_text(&mut doc, 1, "Acme", "Bqme", &fonts).unwrap();

    assert_eq!(count, 2, "legacy API replaces every occurrence");
    assert_eq!(page_text(&doc), "BqmeBqme");
}

// ---------------------------------------------------------------------------
// Characterization: content stream structure
// ---------------------------------------------------------------------------

/// FLIPPED in Phase 1C (was defect C1): a page with /Contents [A B] used to
/// be collapsed to a single stream, rewriting untouched stream A. The engine
/// now preserves the array and rewrites only the touched stream.
#[test]
fn migrated_multiple_content_streams_preserved_on_write() {
    let stream_a = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"KeepMe"),
        Operation::new("ET", vec![]),
    ]);
    let stream_b = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 650.0),
        tj(b"Target"),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, stream_ids) = make_doc(
        vec![("F1", helvetica_font())],
        vec![stream_a.clone(), stream_b],
    );
    assert_eq!(stream_ids.len(), 2);

    let fonts = fonts_for(&doc);
    let count = replace_text(&mut doc, 1, "Target", "Edited", &fonts).unwrap();
    assert_eq!(count, 1);

    // /Contents remains a two-element array with the original object ids.
    let page_id = *doc.get_pages().get(&1).unwrap();
    let contents = doc
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();
    assert_eq!(
        contents,
        Object::Array(vec![
            Object::Reference(stream_ids[0]),
            Object::Reference(stream_ids[1]),
        ]),
        "the /Contents array structure is preserved"
    );

    // The untouched stream A keeps its exact original bytes.
    let a_bytes = match doc.get_object(stream_ids[0]).unwrap() {
        Object::Stream(s) => {
            let mut s = s.clone();
            let _ = s.decompress();
            s.content.clone()
        }
        _ => panic!("first content object is not a stream"),
    };
    assert_eq!(
        a_bytes, stream_a,
        "untouched first stream is byte-identical"
    );
    assert_eq!(page_text(&doc), "KeepMeEdited");
}

// ---------------------------------------------------------------------------
// Characterization: silent skips & silent fallback
// ---------------------------------------------------------------------------

/// LEGACY CONTRACT: a replacement that cannot be encoded in any font
/// (Ω is outside Latin-1) returns Ok(0) under the count-only wrapper.
/// The failure IS reported per edit through `text_edit::replace_text`;
/// only this wrapper reduces it to a count.
#[test]
fn characterize_unencodable_replacement_silently_skipped() {
    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Hello"),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", helvetica_font())], vec![content]);

    let fonts = fonts_for(&doc);
    let result = replace_text(&mut doc, 1, "Hello", "\u{03A9}mega", &fonts);

    assert_eq!(
        result.unwrap(),
        0,
        "encode failure is reported as count 0, not as an error"
    );
    assert_eq!(page_text(&doc), "Hello", "document is untouched");
}

/// LEGACY CONTRACT: the wrapper opts into FontFallback::InjectStandard, so a
/// Helvetica/WinAnsi fallback is injected when the original (subset) font
/// cannot encode the replacement. The substitution is reported through
/// `text_edit::replace_text`; this wrapper only returns the count.
#[test]
fn characterize_font_fallback_injected_silently() {
    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Hello"),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", subset_font())], vec![content]);

    let fonts = fonts_for(&doc);
    let count = replace_text(&mut doc, 1, "Hello", "Howdy", &fonts).unwrap();
    assert_eq!(count, 1, "replacement succeeds via fallback");

    // The page's font resources silently gained the injected fallback.
    let page_id = *doc.get_pages().get(&1).unwrap();
    let resources = doc
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .clone();
    let res_dict = match resources {
        Object::Dictionary(d) => d,
        Object::Reference(id) => doc.get_dictionary(id).unwrap().clone(),
        other => panic!("unexpected Resources object: {other:?}"),
    };
    let font_dict = match res_dict.get(b"Font").unwrap() {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(id) => doc.get_dictionary(*id).unwrap().clone(),
        other => panic!("unexpected Font object: {other:?}"),
    };
    assert!(
        font_dict.has(b"F__Helv"),
        "fallback font was injected into page resources without any report"
    );
}

/// FLIPPED in Phase 1C (was defect C3): a cross-run match whose replacement
/// cannot be encoded in the subset font used to be silently dropped. The
/// engine applies the same fallback-font path as single-run matches, so the
/// replacement now succeeds (and the substitution is reported through the
/// text_edit API).
#[test]
fn migrated_cross_run_fallback_now_succeeds() {
    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Hel"),
        tj(b"lo World"),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", subset_font())], vec![content]);

    let fonts = fonts_for(&doc);
    let count = replace_text(&mut doc, 1, "Hello", "Howdy", &fonts).unwrap();

    assert_eq!(count, 1, "cross-run fallback replacement succeeds");
    assert_eq!(page_text(&doc), "Howdy World");
}

// ---------------------------------------------------------------------------
// Characterization: Form XObjects
// ---------------------------------------------------------------------------

/// LEGACY CONTRACT: the count-only wrapper cannot edit Form XObject text, so
/// it does not count (Ok(0)). The new API FINDS the match and reports it as
/// UnsupportedContainer::FormXObject (see text_edit_contract.rs).
#[test]
fn characterize_form_xobject_text_reported_not_found() {
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

    // Attach the Form XObject (with its own F1 resource) to the page.
    let font_id = doc.add_object(Object::Dictionary(helvetica_font()));
    let mut xobj_stream = Stream::new(
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
    xobj_stream
        .dict
        .set("Length", Object::Integer(xobj_stream.content.len() as i64));
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

    let fonts = fonts_for(&doc);
    let count = replace_text(&mut doc, 1, "Hidden", "Shown!", &fonts).unwrap();

    assert_eq!(
        count, 0,
        "XObject text is reported as not-found instead of unsupported-container"
    );
}

// ---------------------------------------------------------------------------
// Characterization: tagged text (/ActualText)
// ---------------------------------------------------------------------------

/// FLIPPED in Phase 1C (was defect C7): glyph text inside a /Span with
/// /ActualText used to be replaced while /ActualText kept the old string,
/// desyncing extraction. The engine now refuses such matches, so the legacy
/// wrapper leaves the span untouched (count 0) instead of desyncing it.
#[test]
fn migrated_actual_text_covered_span_left_untouched() {
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

    let fonts = fonts_for(&doc);
    let count = replace_text(&mut doc, 1, "OldText", "NewText", &fonts).unwrap();
    assert_eq!(count, 0, "ActualText-covered span is not edited");
    assert_eq!(page_text(&doc), "OldText", "glyph text untouched");

    // The BDC property list is untouched and stays consistent with glyphs.
    let editor = editor_for_page(&doc, 1).unwrap();
    let bdc = editor
        .operations()
        .iter()
        .find(|op| op.operator == "BDC")
        .expect("BDC untouched");
    let props = match &bdc.operands[1] {
        Object::Dictionary(d) => d,
        other => panic!("unexpected BDC operand: {other:?}"),
    };
    let actual = match props.get(b"ActualText").unwrap() {
        Object::String(s, _) => s.clone(),
        other => panic!("unexpected ActualText: {other:?}"),
    };
    assert_eq!(actual, b"OldText", "ActualText remains in sync with glyphs");
}

// ---------------------------------------------------------------------------
// Characterization: TJ kerning
// ---------------------------------------------------------------------------

/// FLIPPED in Phase 1C (was defect C5): a length-changing replacement inside
/// a TJ array used to flatten the whole operator to a plain Tj. The engine
/// keeps the TJ operator and preserves elements (and kerning) outside the
/// edited region; kerning inside the edited region is dropped and reported
/// through the text_edit API.
#[test]
fn migrated_tj_operator_preserved_on_length_change() {
    let tj_array = Operation::new(
        "TJ",
        vec![Object::Array(vec![
            Object::String(b"Hel".to_vec(), StringFormat::Literal),
            Object::Integer(-120),
            Object::String(b"lo World".to_vec(), StringFormat::Literal),
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

    let fonts = fonts_for(&doc);
    let count = replace_text(&mut doc, 1, "Hello", "Hi", &fonts).unwrap();
    assert_eq!(count, 1);
    assert_eq!(page_text(&doc), "Hi World");

    let editor = editor_for_page(&doc, 1).unwrap();
    assert!(
        editor.operations().iter().any(|op| op.operator == "TJ"),
        "the TJ operator survives the rewrite"
    );
}

// ---------------------------------------------------------------------------
// Characterization: signatures & DocMDP
// ---------------------------------------------------------------------------

fn test_signer() -> Option<pdf_sign::Pkcs12Signer> {
    let p12_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdf-sign/tests/fixtures/test-rsa.p12"
    );
    let data = std::fs::read(p12_path).ok()?;
    pdf_sign::Pkcs12Signer::from_pkcs12(&data, "test123").ok()
}

fn validate(bytes: &[u8]) -> Vec<pdf_sign::ValidationResult> {
    let pdf = pdf_syntax::Pdf::new(bytes.to_vec()).expect("parse signed pdf");
    pdf_sign::validate_signatures(&pdf)
}

/// LEGACY CONTRACT: the wrapper opts into AllowPostSignatureChange, so a
/// cryptographically signed document is still edited (and the signature
/// invalidated). Signature protection is the text_edit default, not the
/// legacy wrapper's.
#[test]
fn characterize_signed_document_modified_without_protection() {
    let Some(signer) = test_signer() else {
        panic!("test PKCS#12 fixture missing — expected in pdf-sign/tests/fixtures");
    };

    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Hello World"),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", helvetica_font())], vec![content]);
    let unsigned = save_bytes(&mut doc);

    let signed = pdf_sign::sign_pdf(&unsigned, &signer, &pdf_sign::SignOptions::default())
        .expect("sign fixture");

    // Sanity: the signature we just created validates.
    let before = validate(&signed);
    assert_eq!(before.len(), 1);
    assert_eq!(
        before[0].status,
        pdf_sign::ValidationStatus::Valid,
        "signed fixture must start out valid"
    );

    // The legacy API neither detects nor rejects the signature.
    let mut signed_doc = Document::load_mem(&signed).unwrap();
    let fonts = FontMap::from_page(&signed_doc, 1).unwrap();
    let count = replace_text(&mut signed_doc, 1, "Hello", "Howdy", &fonts).unwrap();
    assert_eq!(count, 1, "signed document edited without any protection");

    // And the signature is now broken.
    let modified = save_bytes(&mut signed_doc);
    let after = validate(&modified);
    assert!(
        after.is_empty() || after[0].status != pdf_sign::ValidationStatus::Valid,
        "signature silently invalidated (or dropped) by the edit: {after:?}"
    );
}

/// LEGACY CONTRACT: the wrapper's AllowPostSignatureChange also applies to
/// DocMDP P=1 certification signatures.
#[test]
fn characterize_docmdp_certified_document_modified_without_protection() {
    let Some(signer) = test_signer() else {
        panic!("test PKCS#12 fixture missing — expected in pdf-sign/tests/fixtures");
    };

    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Certified Content"),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", helvetica_font())], vec![content]);
    let unsigned = save_bytes(&mut doc);

    let options = pdf_sign::SignOptions {
        certification: Some(pdf_sign::DocMdpPermission::NoChanges),
        ..Default::default()
    };
    let signed = pdf_sign::sign_pdf(&unsigned, &signer, &options).expect("certify fixture");

    let mut signed_doc = Document::load_mem(&signed).unwrap();
    let fonts = FontMap::from_page(&signed_doc, 1).unwrap();
    let count = replace_text(&mut signed_doc, 1, "Certified", "Tampered!", &fonts).unwrap();

    assert_eq!(
        count, 1,
        "DocMDP NoChanges certification is ignored by the legacy API"
    );
}

// ---------------------------------------------------------------------------
// Regression guards (desired behaviour that must NOT change)
// ---------------------------------------------------------------------------

/// The offset-aware cross-run replacement (multiline fix, c827f3682) keeps
/// every line's text in the operator that draws that line.
#[test]
fn regression_cross_run_replacement_distributes_by_offset() {
    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Hel"),
        tj(b"lo World"),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", helvetica_font())], vec![content]);

    let fonts = fonts_for(&doc);
    let count = replace_text(&mut doc, 1, "Hello", "Howdy", &fonts).unwrap();
    assert_eq!(count, 1);

    let runs = extract_page_text_runs(&doc, 1).unwrap();
    let texts: Vec<&str> = runs.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["Howdy", " World"],
        "replacement lands in the run where the match starts; suffix stays in its run"
    );
}

/// Output stays openable and extractable by the real reader (pdf-engine)
/// after a replacement — the roundtrip acceptance criterion.
#[test]
fn regression_roundtrip_open_and_extract_after_replace() {
    let content = ops(vec![
        Operation::new("BT", vec![]),
        tf("F1", 12.0),
        td(100.0, 700.0),
        tj(b"Hello World"),
        Operation::new("ET", vec![]),
    ]);
    let (mut doc, _) = make_doc(vec![("F1", helvetica_font())], vec![content]);

    let fonts = fonts_for(&doc);
    let count = replace_text(&mut doc, 1, "Hello", "Howdy", &fonts).unwrap();
    assert_eq!(count, 1);

    let bytes = save_bytes(&mut doc);
    let engine_doc = pdf_engine::PdfDocument::open(bytes).expect("reopen after replace");
    let text = engine_doc.extract_text(0).expect("extract after replace");
    assert!(text.contains("Howdy"), "new text extractable: {text:?}");
    assert!(
        !text.contains("Hello"),
        "old text no longer extractable: {text:?}"
    );
}
