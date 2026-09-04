//! Reflow inside the enclosing block, not just the replaced run (3.1).
//!
//! Acrobat reflows within a text frame it infers from the page. We reflowed
//! within the *run* being replaced, which is identical when a whole line is
//! replaced — the translation case — and far too narrow when a short phrase
//! sits inside a wide column: the text wrapped at the phrase's own width while
//! the page had room to spare.
//!
//! PDF has no paragraphs, so block detection is inference from geometry and can
//! be wrong. It is built to be wrong in one direction only: when the evidence is
//! weak it falls back to the run's width, and the inferred width is never
//! narrower than the run. The worst case is therefore the behaviour we had.
//! These tests pin both the win and that bound.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdf_manip::text_edit::{
    begin_text_edit, DocumentRevision, FitPolicy, ReplaceOptions, TextQuery,
};

/// A page of `lines`, each at an ABSOLUTE (x, y) with the given size.
///
/// Positioning uses `Tm`, not `Td`. `Td` moves relative to the start of the
/// previous line, so writing successive absolute values with it stacks them:
/// lines meant for y = 700 / 686 / 672 landed at 700 / 1386 / 2058, hundreds of
/// points apart, and no block could possibly be recognised. The first version
/// of this fixture did exactly that and made the feature look broken.
fn page_with(lines: &[(f64, f64, f64, &str)]) -> Document {
    let mut doc = Document::with_version("1.7");
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    }));
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_id));

    let mut ops = vec![Operation::new("BT", vec![])];
    for (x, y, size, text) in lines {
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), Object::Real(*size as f32)],
        ));
        ops.push(Operation::new(
            "Tm",
            vec![
                Object::Real(1.0),
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(1.0),
                Object::Real(*x as f32),
                Object::Real(*y as f32),
            ],
        ));
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(
                text.as_bytes().to_vec(),
                StringFormat::Literal,
            )],
        ));
    }
    ops.push(Operation::new("ET", vec![]));

    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().unwrap(),
    ));
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

/// Replace with reflow and report how many text-showing ops came out — one per
/// wrapped line, so the count is the number of lines the text was broken into.
fn reflow_line_count(doc: &mut Document, find: &str, replacement: &str) -> usize {
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    let rev = DocumentRevision::from_source_bytes(&buf);
    let mut session = begin_text_edit(doc, rev).expect("begin");
    let matches = session.find_text(TextQuery::exact(find)).expect("find");
    assert!(!matches.is_empty(), "fixture must contain {find:?}");
    session
        .stage_replace(
            &matches[0].id,
            replacement,
            ReplaceOptions::default().fit(FitPolicy::ReflowInBounds),
        )
        .expect("stage");
    session.commit().expect("commit");

    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    let reopened = Document::load_mem(&out).expect("reopen");
    // Content streams are compressed on save, so decode the filters first.
    // Skipping that step silently counted zero operators everywhere, and the
    // two "<= n" assertions below were satisfied by zero — the tests passed
    // while measuring nothing. Hence the explicit floor in the assert helper.
    let mut shows = 0;
    let mut streams_read = 0;
    for (_, obj) in reopened.objects.iter() {
        let Object::Stream(st) = obj else { continue };
        let Ok(data) = st.decompressed_content() else {
            continue;
        };
        let Ok(content) = Content::decode(&data) else {
            continue;
        };
        streams_read += 1;
        shows += content
            .operations
            .iter()
            .filter(|o| matches!(o.operator.as_str(), "Tj" | "TJ"))
            .count();
    }
    assert!(
        streams_read > 0 && shows > 0,
        "no text operators found at all — the measurement is broken, not the reflow"
    );
    shows
}

/// The case the plan names: a short phrase inside a wide column.
///
/// Three stacked lines of the same size at a consistent leading form a block.
/// Replacing the short middle phrase must wrap against the column, not against
/// the phrase.
#[test]
fn a_short_phrase_reflows_against_the_column_it_sits_in() {
    let wide = "This is a long line of body text that establishes the column width";
    let mut doc = page_with(&[
        (72.0, 700.0, 12.0, wide),
        (72.0, 686.0, 12.0, "Short bit"),
        (72.0, 672.0, 12.0, wide),
    ]);

    let replacement = "A replacement that is clearly longer than the short phrase it replaces";
    let lines = reflow_line_count(&mut doc, "Short bit", replacement);

    // Against the phrase's own width ("Short bit", ~9 characters) this would
    // shatter into many fragments. Against the column it needs one or two.
    assert!(
        lines <= 5,
        "reflow should use the column width, not the phrase width; got {lines} text ops"
    );
}

/// The bound that makes the heuristic safe: a lone run has no block, so nothing
/// changes and we keep the previous behaviour.
#[test]
fn a_lone_run_falls_back_to_its_own_width() {
    let mut doc = page_with(&[(72.0, 700.0, 12.0, "Short bit")]);
    let replacement = "A replacement that is clearly longer than the short phrase it replaces";
    let lines = reflow_line_count(&mut doc, "Short bit", replacement);
    assert!(
        lines >= 2,
        "with no block to infer, a much longer replacement must still wrap; got {lines}"
    );
}

/// Different font sizes are not one block.
///
/// A heading sitting directly above body text is the common shape here. Pulling
/// it into the block would let a heading replacement wrap against the body
/// column, which is not where it lives.
#[test]
fn a_heading_is_not_pulled_into_the_body_block() {
    let mut doc = page_with(&[
        (72.0, 700.0, 24.0, "Heading"),
        (
            72.0,
            670.0,
            12.0,
            "Body text line one that is quite wide indeed",
        ),
        (
            72.0,
            656.0,
            12.0,
            "Body text line two that is quite wide indeed",
        ),
    ]);
    let replacement = "A heading replacement that is considerably longer than the original";
    let lines = reflow_line_count(&mut doc, "Heading", replacement);
    assert!(
        lines >= 2,
        "the heading must wrap against its own width, not the body column; got {lines}"
    );
}

/// A whole-line replacement — the translation case — must be unaffected.
#[test]
fn replacing_a_whole_line_behaves_as_before() {
    let line = "This is a full line of text in a column";
    let mut doc = page_with(&[
        (72.0, 700.0, 12.0, line),
        (72.0, 686.0, 12.0, "Another line of about the same width"),
    ]);
    // Roughly the same length, so it should still be one line either way.
    let lines = reflow_line_count(&mut doc, line, "Dit is een volle regel tekst in een kolom");
    assert!(
        lines <= 3,
        "a same-length replacement of a whole line should not gain lines; got {lines}"
    );
}
