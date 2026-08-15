//! Phase 2A acceptance tests: writing scripts the source document never
//! contained, via an embedded Type0/Identity-H font.
//!
//! These are the cases the [redacted] assessment (13 Aug 2026) named as the
//! blocker for using Text Replace as a translation substrate: Polish, Czech,
//! Russian, Greek and CJK all sit above U+00FF and were unreachable through
//! either the original-font route or the WinAnsi fallback.
//!
//! Every test that needs real glyph data resolves a font from the host and
//! *skips* when none is present, so the suite stays green on a machine
//! without one rather than failing for an environmental reason.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdf_manip::text_edit::{
    begin_text_edit, DocumentRevision, FontFallback, ReplaceOptions, TextQuery,
};
use pdf_manip::unicode_font::UnicodeFont;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn host_font() -> Option<Vec<u8>> {
    for path in [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ] {
        if let Ok(data) = std::fs::read(path) {
            return Some(data);
        }
    }
    None
}

fn make_doc(text: &str) -> Document {
    let mut doc = Document::with_version("1.7");

    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    }));
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_id));

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)]),
            Operation::new("Td", vec![Object::Real(100.0), Object::Real(700.0)]),
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
    let stream_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        content.encode().unwrap(),
    )));

    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(stream_id),
        "Resources" => Object::Dictionary(dictionary! { "Font" => Object::Dictionary(fonts) }),
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
    doc
}

fn revision_for(doc: &mut Document) -> DocumentRevision {
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    DocumentRevision::from_source_bytes(&buf)
}

/// Replace the whole source text with `replacement`, using an embedded
/// Unicode font. Returns the saved document bytes.
fn replace_with_unicode(source: &str, replacement: &str, font: UnicodeFont) -> Vec<u8> {
    let mut doc = make_doc(source);
    let rev = revision_for(&mut doc);
    let mut session = begin_text_edit(&mut doc, rev).expect("begin");
    let matches = session.find_text(TextQuery::exact(source)).expect("find");
    assert_eq!(matches.len(), 1, "fixture should contain the source once");

    session
        .stage_replace(
            &matches[0].id,
            replacement,
            ReplaceOptions::default().font_fallback(FontFallback::EmbedUnicode(font)),
        )
        .expect("stage");
    let report = session.commit().expect("commit");
    assert_eq!(report.replacements_applied, 1);

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// Every Type0 font object in the document, as (dict, descendant dict).
fn type0_fonts(doc: &Document) -> Vec<(Dictionary, Dictionary)> {
    let mut found = Vec::new();
    for obj in doc.objects.values() {
        let Object::Dictionary(d) = obj else { continue };
        if d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) != Some(b"Type0") {
            continue;
        }
        let descendant = d
            .get(b"DescendantFonts")
            .ok()
            .and_then(|o| o.as_array().ok())
            .and_then(|a| a.first())
            .and_then(|o| o.as_reference().ok())
            .and_then(|id| doc.get_object(id).ok())
            .and_then(|o| o.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        found.push((d.clone(), descendant));
    }
    found
}

// ---------------------------------------------------------------------------
// The blocker cases from the assessment
// ---------------------------------------------------------------------------

#[test]
fn writes_scripts_the_source_document_never_contained() {
    let Some(data) = host_font() else { return };

    // One per language band the assessment called out as broken.
    for (label, replacement) in [
        ("Pools", "zażółć gęślą jaźń"),
        ("Tsjechisch", "příliš žluťoučký kůň"),
        ("Russisch", "Съешь ещё этих мягких булок"),
        ("Grieks", "Γειά σου Κόσμε"),
        ("Turks", "Pijamalı hasta yağız şoföre"),
    ] {
        let font = UnicodeFont::from_bytes(data.clone()).expect("font parses");
        if !font.covers(replacement) {
            continue; // host font lacks this script — not a product failure
        }
        let saved = replace_with_unicode("Hello world", replacement, font);

        let reloaded = Document::load_mem(&saved)
            .unwrap_or_else(|e| panic!("{label}: output must reopen: {e}"));
        assert!(
            !type0_fonts(&reloaded).is_empty(),
            "{label}: a Type0 font should have been embedded"
        );
    }
}

#[test]
fn the_embedded_font_is_a_well_formed_identity_h_cid_font() {
    let Some(data) = host_font() else { return };
    let font = UnicodeFont::from_bytes(data).unwrap();
    if !font.covers("Γειά σου") {
        return;
    }
    let saved = replace_with_unicode("Hello world", "Γειά σου", font);
    let doc = Document::load_mem(&saved).expect("reopen");

    let fonts = type0_fonts(&doc);
    let (type0, cid) = fonts.first().expect("a Type0 font");

    assert_eq!(
        type0.get(b"Encoding").unwrap().as_name().unwrap(),
        b"Identity-H",
        "codes are glyph indices, so the encoding must say so"
    );
    assert!(
        type0.has(b"ToUnicode"),
        "without /ToUnicode the text renders but cannot be copied, searched \
         or read by a screen reader — for a translation that is a total loss"
    );
    assert_eq!(
        cid.get(b"Subtype").unwrap().as_name().unwrap(),
        b"CIDFontType2"
    );
    assert_eq!(
        cid.get(b"CIDToGIDMap").unwrap().as_name().unwrap(),
        b"Identity"
    );
    assert!(cid.has(b"W"), "per-glyph advances must be declared");
    assert!(cid.has(b"DW"), "a default width is required");

    let descriptor = cid
        .get(b"FontDescriptor")
        .and_then(|o| o.as_reference())
        .and_then(|id| doc.get_object(id))
        .and_then(|o| o.as_dict())
        .expect("descriptor");
    assert!(
        descriptor.has(b"FontFile2"),
        "the program itself must be embedded, not merely referenced"
    );
    for key in [
        b"Flags".as_slice(),
        b"FontBBox",
        b"ItalicAngle",
        b"Ascent",
        b"Descent",
        b"StemV",
    ] {
        assert!(
            descriptor.has(key),
            "descriptor is missing required key {}",
            String::from_utf8_lossy(key)
        );
    }
}

#[test]
fn the_subsetted_program_is_far_smaller_than_the_original() {
    let Some(data) = host_font() else { return };
    let original_size = data.len();
    let font = UnicodeFont::from_bytes(data).unwrap();
    // Must be genuinely outside Latin-1 (ř, š), or the original font encodes
    // it and no embedding happens at all — which is correct behaviour, but
    // measures nothing here.
    let saved = replace_with_unicode("Hello world", "příliš", font);
    let doc = Document::load_mem(&saved).expect("reopen");

    let embedded: usize = doc
        .objects
        .values()
        .filter_map(|o| match o {
            Object::Stream(s) if s.dict.has(b"Length1") => Some(s.content.len()),
            _ => None,
        })
        .sum();

    assert!(embedded > 0, "a font program should be embedded");
    assert!(
        embedded < original_size / 4,
        "subsetting should cut the program to a fraction of the full face \
         (embedded {embedded} vs original {original_size}); without this a \
         translated document carries megabytes per page"
    );
}

#[test]
fn the_same_glyph_used_twice_is_stored_once() {
    let Some(data) = host_font() else { return };
    let font = UnicodeFont::from_bytes(data).unwrap();

    // 'а' repeats; a per-occurrence subset would grow with the repetition.
    let saved = replace_with_unicode("Hello world", "аааааааааа", font);
    let doc = Document::load_mem(&saved).expect("reopen");
    let (_, cid) = type0_fonts(&doc).into_iter().next().expect("font");

    let w = cid.get(b"W").unwrap().as_array().unwrap();
    // One run: [start [width]] — a single distinct glyph plus .notdef.
    let widths: usize = w
        .iter()
        .filter_map(|o| o.as_array().ok())
        .map(|a| a.len())
        .sum();
    assert!(
        widths <= 2,
        "ten copies of one character should not produce {widths} width entries"
    );
}

#[test]
fn refuses_rather_than_writing_blank_boxes_for_missing_glyphs() {
    let Some(data) = host_font() else { return };
    let font = UnicodeFont::from_bytes(data).unwrap();

    // Private-use area: present in no real face.
    let mut doc = make_doc("Hello world");
    let rev = revision_for(&mut doc);
    let mut session = begin_text_edit(&mut doc, rev).unwrap();
    let matches = session.find_text(TextQuery::exact("Hello world")).unwrap();
    let staged = session.stage_replace(
        &matches[0].id,
        "\u{E000}\u{E001}",
        ReplaceOptions::default().font_fallback(FontFallback::EmbedUnicode(font)),
    );

    let failed = staged.is_err() || session.commit().is_err();
    assert!(
        failed,
        "a font without the requested glyphs must fail loudly, not emit .notdef boxes"
    );
}

#[test]
fn latin1_replacements_still_take_the_original_font_route() {
    let Some(data) = host_font() else { return };
    let font = UnicodeFont::from_bytes(data).unwrap();

    // "Bonjour" is encodable in the document's own Helvetica, so the Unicode
    // path should not engage at all — embedding a font for text that needs no
    // embedding would bloat every ordinary edit.
    let saved = replace_with_unicode("Hello world", "Bonjour", font);
    let doc = Document::load_mem(&saved).expect("reopen");
    assert!(
        type0_fonts(&doc).is_empty(),
        "no font should be embedded when the original can encode the text"
    );
}
