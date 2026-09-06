//! CFF-flavoured OpenType fonts (kwaliteitsplan 3.3).
//!
//! We used to refuse these outright, which meant the *static* Noto CJK builds
//! were unusable and only the variable TTFs worked — a real limit on what a
//! caller could bring, for no reason other than that we had not written the
//! other descendant shape.
//!
//! CFF takes a different shape in the PDF: a `CIDFontType0` descendant with the
//! program under `/FontFile3`, and no `/CIDToGIDMap` (the CFF charset already
//! carries that relation). Getting any of those wrong yields a file that opens
//! in every reader and draws nothing, so structure alone is not enough evidence
//! here — the text has to come back out through a reader that is not ours.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdf_manip::text_edit::{
    begin_text_edit, DocumentRevision, FontFallback, ReplaceOptions, TextQuery,
};
use pdf_manip::unicode_font::UnicodeFont;
use std::io::Write;
use std::process::Command;
use test_skip::skip_test;

/// Above Latin-1, so the original font cannot represent it and the Unicode
/// route is actually taken.
const SAMPLE: &str = "Γειά σου";

/// A CFF-flavoured OpenType font (magic `OTTO`).
///
/// The sample text below is Greek on purpose. EmbedUnicode is a *fallback*: if
/// the original font can already represent the replacement, no Type0 font is
/// embedded at all and none of this code runs. An ASCII sample made two of
/// these tests pass while exercising nothing — the structure test caught it by
/// finding no descendant font to inspect.
fn cff_font() -> Option<(String, Vec<u8>)> {
    // Latin coverage first: the script-specific Noto OTFs are CFF too, but a
    // font that cannot draw "Hallo" makes the interesting tests skip, and a
    // skipped test proves nothing.
    let candidates = [
        "/System/Library/Fonts/Supplemental/STIXGeneral.otf",
        "/usr/share/fonts/opentype/urw-base35/NimbusSans-Regular.otf",
        "/usr/share/fonts/opentype/freefont/FreeSans.otf",
        "/usr/share/fonts/opentype/cantarell/Cantarell-VF.otf",
        "/System/Library/Fonts/Supplemental/NotoSansJavanese-Regular.otf",
    ];
    for path in candidates {
        let Ok(data) = std::fs::read(path) else {
            continue;
        };
        if data.len() >= 4 && &data[..4] == b"OTTO" {
            return Some((path.to_string(), data));
        }
    }
    None
}

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

fn replace(source: &str, replacement: &str, font: UnicodeFont) -> Vec<u8> {
    let mut doc = make_doc(source);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    let rev = DocumentRevision::from_source_bytes(&buf);
    let mut session = begin_text_edit(&mut doc, rev).expect("begin");
    let matches = session.find_text(TextQuery::exact(source)).expect("find");
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

fn descendant_of(doc: &Document) -> Option<Dictionary> {
    for obj in doc.objects.values() {
        let Object::Dictionary(d) = obj else { continue };
        if d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Type0") {
            let refs = d.get(b"DescendantFonts").ok()?.as_array().ok()?;
            let id = refs.first()?.as_reference().ok()?;
            return doc.get_object(id).ok()?.as_dict().ok().cloned();
        }
    }
    None
}

#[test]
fn a_cff_font_is_accepted_instead_of_refused() {
    let Some((path, data)) = cff_font() else {
        skip_test!("no CFF-flavoured OpenType font on this host")
    };
    UnicodeFont::from_bytes(data)
        .unwrap_or_else(|e| panic!("{path} should be embeddable now, got: {e}"));
}

#[test]
fn a_cff_font_gets_a_cidfonttype0_descendant_with_fontfile3() {
    let Some((_, data)) = cff_font() else {
        skip_test!("no CFF-flavoured OpenType font on this host")
    };
    let font = UnicodeFont::from_bytes(data).expect("font parses");
    if !font.covers(SAMPLE) {
        skip_test!("CFF font on this host lacks Latin coverage")
    }
    let saved = replace("Hello world", SAMPLE, font);
    let doc = Document::load_mem(&saved).expect("reopen");
    let cid = descendant_of(&doc).expect("a descendant font");

    assert_eq!(
        cid.get(b"Subtype").unwrap().as_name().unwrap(),
        b"CIDFontType0",
        "CFF outlines need a CIDFontType0 descendant; CIDFontType2 promises TrueType"
    );
    assert!(
        !cid.has(b"CIDToGIDMap"),
        "CIDToGIDMap is a CIDFontType2 key — the CFF charset already carries this"
    );

    let desc_id = cid.get(b"FontDescriptor").unwrap().as_reference().unwrap();
    let desc = doc.get_object(desc_id).unwrap().as_dict().unwrap();
    assert!(
        desc.has(b"FontFile3"),
        "a CFF program belongs under /FontFile3"
    );
    assert!(
        !desc.has(b"FontFile2"),
        "/FontFile2 promises TrueType outlines and would make readers draw nothing"
    );
}

/// The check that structure cannot give us.
///
/// Every assertion above can hold on a file that renders blank. Only a reader
/// that is not ours can say whether the glyphs are really reachable.
#[test]
fn text_written_with_a_cff_font_can_be_read_back_out() {
    let Some((_, data)) = cff_font() else {
        skip_test!("no CFF-flavoured OpenType font on this host")
    };
    let font = UnicodeFont::from_bytes(data).expect("font parses");
    let replacement = SAMPLE;
    if !font.covers(replacement) {
        skip_test!("CFF font on this host lacks Latin coverage")
    }
    let saved = replace("Hello world", replacement, font);

    let Ok(bin) = which("pdftotext") else {
        skip_test!("pdftotext not available")
    };
    let path = std::env::temp_dir().join(format!("pdfluent-cff-{}.pdf", std::process::id()));
    std::fs::File::create(&path)
        .unwrap()
        .write_all(&saved)
        .unwrap();
    let out = Command::new(bin)
        .args(["-enc", "UTF-8", "-nopgbrk"])
        .arg(&path)
        .arg("-")
        .output()
        .expect("pdftotext runs");
    let _ = std::fs::remove_file(&path);

    let text: String = String::from_utf8_lossy(&out.stdout)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    // Compare both sides with whitespace removed: readers are free to place
    // line and word breaks differently, and that is not what this test is
    // about. (Stripping only one side was this test's first bug — it reported
    // a failure over a single missing space.)
    let want: String = replacement.chars().filter(|c| !c.is_whitespace()).collect();
    assert_eq!(
        text, want,
        "the CFF-embedded text must be extractable, not just present"
    );
}

fn which(bin: &str) -> Result<String, ()> {
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {bin}"))
        .output()
        .map_err(|_| ())?;
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !p.is_empty() {
        Ok(p)
    } else {
        Err(())
    }
}
