//! Does our own output still pass the standard after we have edited it?
//!
//! WHY THIS FILE EXISTS
//!
//! When text is replaced with a script the source font cannot represent, we
//! build a Type0/CIDFontType2 font by hand: the descriptor, the /W width array,
//! the CIDToGIDMap and the /ToUnicode CMap are all written by us. That is
//! exactly the class of structure where implementations go quietly wrong — the
//! file opens, renders, and only fails when something holds it to the spec.
//!
//! So the test converts a document to PDF/A, checks with veraPDF that it really
//! is conformant, then does a Unicode replacement on it and checks again. The
//! second check is the point: our edit must not take a conformant file out of
//! conformance.
//!
//! Using PDF/A as the yardstick is deliberate. Validating a plain PDF against a
//! PDF/A profile fails on things that have nothing to do with us (no XMP, no
//! OutputIntent), so the result would be noise. Starting from a file that
//! already passes makes every new violation attributable to the edit.
//!
//! Run: cargo test -p pdf-manip --test text_edit_verapdf -- --nocapture

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdf_manip::text_edit::{
    begin_text_edit, DocumentRevision, FontFallback, ReplaceOptions, TextQuery,
};
use pdf_manip::unicode_font::UnicodeFont;
use std::io::Write;
use std::process::Command;

const VERAPDF_CANDIDATES: [&str; 4] = [
    "/usr/local/bin/verapdf",
    "/opt/verapdf/verapdf",
    "/usr/bin/verapdf",
    "verapdf",
];

fn verapdf() -> Option<String> {
    for c in VERAPDF_CANDIDATES {
        if Command::new(c)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            return Some(c.to_string());
        }
    }
    None
}

fn skip(reason: &str) {
    eprintln!("SKIPPED (not a pass): {reason}");
}

fn host_font() -> Option<Vec<u8>> {
    for path in [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ] {
        if let Ok(data) = std::fs::read(path) {
            return Some(data);
        }
    }
    None
}

/// veraPDF's verdict plus the rule ids it objected to.
struct Verdict {
    compliant: bool,
    failed_rules: Vec<String>,
}

fn validate(bytes: &[u8], label: &str) -> Option<Verdict> {
    let bin = verapdf()?;
    let path =
        std::env::temp_dir().join(format!("pdfluent-vera-{}-{label}.pdf", std::process::id()));
    std::fs::File::create(&path).ok()?.write_all(bytes).ok()?;

    let out = Command::new(bin)
        .args(["--format", "text", "-f", "2b"])
        .arg(&path)
        .output()
        .ok()?;
    let _ = std::fs::remove_file(&path);

    let report = String::from_utf8_lossy(&out.stdout).to_string();
    // Text output reads "PASS <path> 2b" or "FAIL <path> 2b". Match on the
    // verdict token at the start of a line, not anywhere in the text: the path
    // is part of that line, and a temp directory containing "PASS" would
    // otherwise turn every result green.
    let verdict_line = report
        .lines()
        .find(|l| l.starts_with("PASS ") || l.starts_with("FAIL "));
    let compliant = match verdict_line {
        Some(l) => l.starts_with("PASS "),
        // No verdict at all means veraPDF did not judge the file. Treating that
        // as a pass is how a broken validator becomes a green suite.
        None => panic!("veraPDF produced no PASS/FAIL verdict for {label}:\n{report}"),
    };
    let failed_rules = report
        .lines()
        .filter(|l| l.trim_start().starts_with("FAIL") || l.contains("clause"))
        .map(|l| l.trim().to_string())
        .take(12)
        .collect();
    Some(Verdict {
        compliant,
        failed_rules,
    })
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

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

fn to_pdfa(bytes: &[u8]) -> Vec<u8> {
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        ..Default::default()
    };
    pdf_manip::pdfa::convert_bytes(bytes, &opts).expect("PDF/A conversion")
}

fn replace_unicode(pdf: &[u8], source: &str, replacement: &str, font: UnicodeFont) -> Vec<u8> {
    let mut doc = Document::load_mem(pdf).expect("reopen");
    let rev = DocumentRevision::from_source_bytes(pdf);
    let mut session = begin_text_edit(&mut doc, rev).expect("begin");
    let matches = session.find_text(TextQuery::exact(source)).expect("find");
    assert!(!matches.is_empty(), "fixture should contain {source:?}");
    session
        .stage_replace(
            &matches[0].id,
            replacement,
            ReplaceOptions::default().font_fallback(FontFallback::EmbedUnicode(font)),
        )
        .expect("stage");
    session.commit().expect("commit");
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The baseline. If this fails, the test below proves nothing, so it is checked
/// separately rather than folded into one assertion.
#[test]
fn our_pdfa_conversion_is_accepted_by_verapdf() {
    if verapdf().is_none() {
        skip("veraPDF not installed");
        return;
    }
    let mut doc = make_doc("Hello world");
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    let converted = to_pdfa(&buf);

    let v = validate(&converted, "baseline").expect("veraPDF ran");
    assert!(
        v.compliant,
        "our own PDF/A output must validate before we can judge an edit against it.\n{:#?}",
        v.failed_rules
    );
}

/// The one that matters: a Unicode replacement writes a Type0 font by hand into
/// a file that was conformant. It must still be conformant afterwards.
#[test]
fn a_unicode_replacement_keeps_the_file_pdfa_conformant() {
    if verapdf().is_none() {
        skip("veraPDF not installed");
        return;
    }
    let Some(data) = host_font() else {
        skip("no host font with Unicode coverage");
        return;
    };
    let font = UnicodeFont::from_bytes(data).expect("font parses");
    let replacement = "Γειά σου Κόσμε";
    if !font.covers(replacement) {
        skip("host font does not cover Greek");
        return;
    }

    let mut doc = make_doc("Hello world");
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    let converted = to_pdfa(&buf);

    let before = validate(&converted, "before").expect("veraPDF ran");
    assert!(
        before.compliant,
        "baseline must be conformant, otherwise this test measures nothing"
    );

    let edited = replace_unicode(&converted, "Hello world", replacement, font);
    let after = validate(&edited, "after").expect("veraPDF ran");

    assert!(
        after.compliant,
        "the embedded Type0 font took the file out of PDF/A conformance.\n\
         We hand-write the descriptor, /W array, CIDToGIDMap and /ToUnicode, so \
         a violation here points at one of those.\nveraPDF objected to:\n{:#?}",
        after.failed_rules
    );
}

/// Proves the check above can actually fail.
///
/// A validator that always says "conformant" is worse than no validator, and
/// nothing in the two tests above would notice. So: feed veraPDF a plain PDF
/// that was never converted, and require a rejection.
#[test]
fn the_validator_rejects_a_file_that_is_not_pdfa() {
    if verapdf().is_none() {
        skip("veraPDF not installed");
        return;
    }
    let mut doc = make_doc("Hello world");
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();

    let v = validate(&buf, "negative").expect("veraPDF ran");
    assert!(
        !v.compliant,
        "a plain PDF with no XMP and no OutputIntent must not validate as PDF/A-2b; \
         if it does, the verdict parsing is wrong and the other tests prove nothing"
    );
}
