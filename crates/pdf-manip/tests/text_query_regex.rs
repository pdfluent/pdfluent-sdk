//! Regex search in `TextQuery` (PDF.co parity).
//!
//! The interesting cases here are not "does a pattern match" — the `regex`
//! crate answers that. They are the ways a regex can be wrong in a way that
//! only shows up once it is pointed at a document: a pattern that matches
//! nothing anchors differently than expected, one that matches everywhere
//! rewrites the whole page, and one that does not compile should say so before
//! any document is touched rather than halfway through a search.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdf_manip::text_edit::{begin_text_edit, DocumentRevision, TextEditError, TextQuery};

fn make_doc(text: &str) -> Document {
    let mut doc = Document::with_version("1.7");
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    }));
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_id));
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)]),
            Operation::new("Td", vec![Object::Real(72.0), Object::Real(700.0)]),
            Operation::new(
                "Tj",
                vec![Object::String(
                    text.as_bytes().to_vec(),
                    StringFormat::Literal,
                )],
            ),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => fonts },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc
}

fn find(text: &str, query: TextQuery) -> usize {
    let mut doc = make_doc(text);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    let rev = DocumentRevision::from_source_bytes(&buf);
    let mut session = begin_text_edit(&mut doc, rev).expect("begin");
    session.find_text(query).expect("find").len()
}

#[test]
fn finds_every_occurrence_of_a_pattern() {
    let q = TextQuery::regex(r"\d{4}").expect("compiles");
    assert_eq!(find("Invoice 2024 and 2025 and 2026", q), 3);
}

#[test]
fn anchors_and_classes_behave_as_the_engine_defines_them() {
    // Not a tautology: it pins that we hand the raw pattern to the engine
    // rather than escaping or rewriting it on the way through.
    let q = TextQuery::regex(r"[A-Z]{2,}").expect("compiles");
    assert_eq!(find("VAT is BTW in NL", q), 3);
}

#[test]
fn a_pattern_that_matches_nothing_is_not_an_error() {
    let q = TextQuery::regex(r"zzz\d+").expect("compiles");
    assert_eq!(
        find("Invoice 2024", q),
        0,
        "no matches is an empty result, not a failure"
    );
}

#[test]
fn case_insensitivity_goes_through_the_regex_engine() {
    let q = TextQuery::regex(r"invoice")
        .expect("compiles")
        .case_insensitive(true);
    assert_eq!(find("Invoice INVOICE invoice", q), 3);

    let q = TextQuery::regex(r"invoice").expect("compiles");
    assert_eq!(
        find("Invoice INVOICE invoice", q),
        1,
        "without the flag only the exact-case occurrence matches"
    );
}

#[test]
fn a_malformed_pattern_is_rejected_when_the_query_is_built() {
    let err = TextQuery::regex(r"(unclosed").expect_err("must not compile");
    match err {
        TextEditError::InvalidQuery { reason } => {
            assert!(
                reason.contains("does not compile"),
                "the reason should name the problem, got: {reason}"
            );
        }
        other => panic!("expected InvalidQuery, got {other:?}"),
    }
}

/// The case that would quietly ruin a document.
///
/// `a*` matches the empty string, so it matches at every position. Used in a
/// replace that means inserting the replacement between every pair of
/// characters on the page. Rejecting it at build time costs one comparison;
/// discovering it in the output costs the document.
#[test]
fn a_pattern_matching_the_empty_string_is_refused() {
    for pattern in [r"a*", r"\d?", r"(?:)", r"x|"] {
        let err = TextQuery::regex(pattern)
            .expect_err(&format!("{pattern:?} matches empty and must be refused"));
        match err {
            TextEditError::InvalidQuery { reason } => assert!(
                reason.contains("empty string"),
                "the reason should explain the empty-match problem, got: {reason}"
            ),
            other => panic!("expected InvalidQuery for {pattern:?}, got {other:?}"),
        }
    }
}

#[test]
fn a_plus_quantifier_is_fine_because_it_requires_one_character() {
    let q = TextQuery::regex(r"a+").expect("a+ requires at least one 'a'");
    assert_eq!(find("aa b aaa", q), 2);
}

/// Literal queries must be untouched by any of this.
#[test]
fn literal_queries_still_treat_metacharacters_literally() {
    assert_eq!(
        find("price is 2+2 today", TextQuery::exact("2+2")),
        1,
        "exact() must not interpret + as a quantifier"
    );
    assert_eq!(
        find("price is 2+2 today", TextQuery::exact("2.2")),
        0,
        "exact() must not interpret . as any-character"
    );
}
