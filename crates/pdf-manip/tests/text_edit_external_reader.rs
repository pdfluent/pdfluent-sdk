//! Does the replaced text come back OUT of the PDF?
//!
//! WHY THIS FILE EXISTS
//!
//! Every structural test we had stayed green while `/ToUnicode` was written so
//! that a character occurring twice extracted as "aa". The document was
//! well-formed, the Type0 font was correct, the rendering was perfect — and the
//! text could not be copied. It surfaced only by running pdftotext by hand.
//!
//! Structure tests answer "did we write what we intended". They cannot answer
//! "can anyone read it back", because they ask our own code. So these tests ask
//! something that shares no code with us: poppler's pdftotext, and mutool as a
//! second opinion. For a translation substrate this is the only check that
//! matters — a translation you cannot select, copy or search is not a
//! translation.
//!
//! ON SKIPPING
//!
//! These need external binaries and a host font with the right coverage, so
//! they cannot always run. They print exactly why they skipped and mark it in
//! the name, because a test that skips silently looks identical to a test that
//! passed — which is how the gap above survived. CI installs the tools and
//! asserts their presence before running the suite.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdf_manip::text_edit::{
    begin_text_edit, DocumentRevision, FontFallback, ReplaceOptions, TextQuery,
};
use pdf_manip::unicode_font::UnicodeFont;
use std::io::Write;
use std::process::Command;

// ---------------------------------------------------------------------------
// Host prerequisites
// ---------------------------------------------------------------------------

fn tool(name: &str) -> Option<String> {
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name}"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!path.is_empty()).then_some(path)
}

/// Announce a skip loudly enough that nobody mistakes it for a pass.
fn skip(reason: &str) {
    eprintln!("SKIPPED (not a pass): {reason}");
}

fn host_font() -> Option<Vec<u8>> {
    for path in [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    ] {
        if let Ok(data) = std::fs::read(path) {
            return Some(data);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Fixture + replacement (same shape as text_edit_unicode.rs)
// ---------------------------------------------------------------------------

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
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        content.encode().expect("encode content"),
    ));
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => fonts },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

fn revision_for(doc: &mut Document) -> DocumentRevision {
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    DocumentRevision::from_source_bytes(&buf)
}

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

// ---------------------------------------------------------------------------
// Extraction through tools that share no code with us
// ---------------------------------------------------------------------------

fn write_temp(bytes: &[u8], name: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("pdfluent-extern-{}-{name}.pdf", std::process::id()));
    let mut f = std::fs::File::create(&path).expect("create temp pdf");
    f.write_all(bytes).expect("write temp pdf");
    path
}

fn extract_with_pdftotext(bytes: &[u8], name: &str) -> Option<String> {
    let bin = tool("pdftotext")?;
    let path = write_temp(bytes, name);
    let out = Command::new(bin)
        .args(["-enc", "UTF-8", "-nopgbrk"])
        .arg(&path)
        .arg("-")
        .output()
        .ok()?;
    let _ = std::fs::remove_file(&path);
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

fn extract_with_mutool(bytes: &[u8], name: &str) -> Option<String> {
    let bin = tool("mutool")?;
    let path = write_temp(bytes, name);
    let out = Command::new(bin)
        .args(["draw", "-F", "txt", "-o", "-"])
        .arg(&path)
        .output()
        .ok()?;
    let _ = std::fs::remove_file(&path);
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

fn normalise(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The regression that started this: a character occurring twice must extract
/// twice, not four times.
///
/// The original defect built /ToUnicode by appending to the entry for a glyph
/// id. Because the same character maps to the same glyph every time it occurs,
/// the second occurrence appended to the first and "aa" came out for "a". Every
/// structural assertion still held.
#[test]
fn repeated_characters_do_not_multiply_on_extraction() {
    let Some(data) = host_font() else {
        skip("no host font with Unicode coverage found");
        return;
    };
    let font = UnicodeFont::from_bytes(data).expect("font parses");

    // The repeats must sit ABOVE Latin-1. EmbedUnicode is a fallback: text the
    // original font can already represent never reaches the Type0 route, so an
    // all-ASCII sample like "banana" exercises none of this and passes happily
    // with the bug present. (Checked, not assumed — that was this test's first
    // version, and it stayed green while the defect was reinstated.)
    let replacement = "żółć żółć źdźbło";
    if !font.covers(replacement) {
        skip("host font does not cover the sample");
        return;
    }
    let saved = replace_with_unicode("Hello world", replacement, font);

    let Some(text) = extract_with_pdftotext(&saved, "repeats") else {
        skip("pdftotext not available");
        return;
    };

    let got = normalise(&text);
    let want = normalise(replacement);
    assert_eq!(
        got, want,
        "pdftotext must return exactly what we wrote.\n  wrote:     {replacement:?}\n  extracted: {text:?}\n\
         A doubled character here means /ToUnicode is appending per occurrence \
         instead of assigning once per glyph."
    );
}

/// Scripts the source document never contained must survive the round trip.
///
/// Writing them is not the hard part — Phase 2A already proves the Type0 font
/// is well formed. Getting them back out is, and that is what a translation
/// customer actually does with the file.
#[test]
fn non_latin_scripts_survive_a_round_trip_through_pdftotext() {
    let Some(data) = host_font() else {
        skip("no host font with Unicode coverage found");
        return;
    };
    if tool("pdftotext").is_none() {
        skip("pdftotext not available");
        return;
    }

    let mut checked = 0usize;
    for (label, replacement) in [
        ("Polish", "zażółć gęślą jaźń"),
        ("Czech", "příliš žluťoučký kůň"),
        ("Russian", "Съешь ещё этих мягких булок"),
        ("Greek", "Γειά σου Κόσμε"),
        ("Turkish", "Pijamalı hasta yağız şoföre"),
    ] {
        let font = UnicodeFont::from_bytes(data.clone()).expect("font parses");
        if !font.covers(replacement) {
            continue; // host font lacks this script — not a product failure
        }
        let saved = replace_with_unicode("Hello world", replacement, font);
        let text = extract_with_pdftotext(&saved, label).expect("pdftotext ran");

        assert_eq!(
            normalise(&text),
            normalise(replacement),
            "{label}: text must come back out unchanged.\n  wrote:     {replacement:?}\n  extracted: {text:?}"
        );
        checked += 1;
    }

    assert!(
        checked > 0,
        "no script could be checked — the host font covered none of them, so this \
         test proved nothing and must not report success"
    );
}

/// A second reader, because agreement between two independent implementations
/// is worth more than either one alone. poppler and mupdf have separate
/// parsers; a file that only one of them can read is a file some customer
/// cannot read.
#[test]
fn a_second_independent_reader_agrees() {
    let Some(data) = host_font() else {
        skip("no host font with Unicode coverage found");
        return;
    };
    let font = UnicodeFont::from_bytes(data).expect("font parses");
    let replacement = "Γειά σου Κόσμε";
    if !font.covers(replacement) {
        skip("host font does not cover Greek");
        return;
    }
    let saved = replace_with_unicode("Hello world", replacement, font);

    let (Some(poppler), Some(mupdf)) = (
        extract_with_pdftotext(&saved, "second-a"),
        extract_with_mutool(&saved, "second-b"),
    ) else {
        skip("need both pdftotext and mutool for this comparison");
        return;
    };

    let want = normalise(replacement);
    assert_eq!(normalise(&poppler), want, "poppler disagrees: {poppler:?}");
    assert_eq!(normalise(&mupdf), want, "mupdf disagrees: {mupdf:?}");
}

/// Replacing ASCII with ASCII must not leave the old text findable. A reader
/// that still sees the original would mean the replacement only covered it
/// visually — which for redaction-adjacent uses is a correctness problem, not
/// a cosmetic one.
#[test]
fn the_replaced_text_is_really_gone() {
    let Some(data) = host_font() else {
        skip("no host font with Unicode coverage found");
        return;
    };
    let font = UnicodeFont::from_bytes(data).expect("font parses");
    let source = "Hello world";
    let replacement = "Goodbye moon";
    if !font.covers(replacement) {
        skip("host font does not cover the sample");
        return;
    }
    let saved = replace_with_unicode(source, replacement, font);

    let Some(text) = extract_with_pdftotext(&saved, "gone") else {
        skip("pdftotext not available");
        return;
    };

    assert!(
        text.contains("Goodbye"),
        "the new text must be extractable, got: {text:?}"
    );
    assert!(
        !text.contains("Hello"),
        "the replaced text must be gone from the content stream, got: {text:?}"
    );
}
