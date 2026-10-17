// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Substituting a program for an unembedded font may not restate the encoding.
//!
//! The bytes in the content stream are the author's. They were written against
//! whatever encoding the font dictionary declared -- or, when it declared none,
//! against the built-in encoding of the program a reader would substitute,
//! which for the Standard 14 is StandardEncoding. Writing `/WinAnsiEncoding`
//! over that keeps every byte and changes what each one draws: 0xAE and 0xAF
//! are `fi` and `fl` in StandardEncoding and `®`/`¯` in WinAnsi, and
//! MacRoman's 0xD2 and 0xD5 are `“` and `’` where WinAnsi has `Ò`/`Õ`.
//!
//! Measured on govdocs at `c226f89d8`: 001_001304 kept 95.3% of its words
//! (`flux` → `¯ux` 78 times), 001_001294 95.9%, 002_002168 96.9%
//! (`“The` → `ÒThe` 17 times, render difference 0.051).

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};

/// An unembedded Standard-14 Type 1 with no `/Encoding`, the shape of
/// govdocs 001_001304's `Times-Roman`. Codes 0xAE/0xAF are the `fi`/`fl`
/// ligatures of the built-in StandardEncoding.
fn times_roman_without_encoding() -> Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Times-Roman",
    }
}

/// An unembedded Type 1 that names MacRomanEncoding, the shape of govdocs
/// 002_002168's `AGaramond-Regular`. Codes 0xD2/0xD5 are `“` and `’` there.
fn garamond_with_macroman() -> Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "AGaramond-Regular",
        "Encoding" => "MacRomanEncoding",
    }
}

fn doc_with(fonts: Vec<(&str, Dictionary, &[u8])>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let mut resources = Dictionary::new();
    let mut operations = vec![Operation::new("BT", vec![])];
    for (name, font, text) in fonts {
        let font_id = doc.add_object(Object::Dictionary(font));
        resources.set(name, Object::Reference(font_id));
        operations.push(Operation::new("Td", vec![72.into(), 700.into()]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(name.as_bytes().to_vec()), 12.into()],
        ));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(text.to_vec(), StringFormat::Literal)],
        ));
    }
    operations.push(Operation::new("ET", vec![]));

    let content = Content { operations };
    let stream_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        content.encode().unwrap(),
    )));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(stream_id),
        "Resources" => dictionary! { "Font" => Object::Dictionary(resources) },
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
    }));
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save fixture");
    out
}

fn convert(source: &[u8]) -> Document {
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        ..Default::default()
    };
    let out = pdf_manip::pdfa::convert_bytes(source, &opts).expect("PDF/A conversion");
    Document::load_mem(&out).expect("load converted output")
}

fn name_of(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key)
        .and_then(|o| o.as_name())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .ok()
}

/// The encoding a reader resolves a code through: the name, or the
/// `/BaseEncoding` of the dictionary, or nothing -- which means the font
/// program's own built-in encoding.
fn effective_base_encoding(doc: &Document, font: &Dictionary) -> String {
    match font.get(b"Encoding") {
        Ok(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
        Ok(Object::Dictionary(d)) => name_of(d, b"BaseEncoding").unwrap_or_default(),
        Ok(Object::Reference(r)) => match doc.objects.get(r) {
            Some(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
            Some(Object::Dictionary(d)) => name_of(d, b"BaseEncoding").unwrap_or_default(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

fn font_dict<'a>(doc: &'a Document, base_font: &str) -> &'a Dictionary {
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| {
            name_of(d, b"Type").as_deref() == Some("Font")
                && name_of(d, b"BaseFont")
                    .map(|n| n.contains(base_font))
                    .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("no font dictionary for {base_font} in the output"))
}

/// The `/ToUnicode` CMap of a font, as the text a reader parses.
fn to_unicode(doc: &Document, font: &Dictionary) -> String {
    let Ok(Object::Reference(r)) = font.get(b"ToUnicode") else {
        panic!("font has no /ToUnicode; extraction has nothing to read");
    };
    let Some(Object::Stream(s)) = doc.objects.get(r) else {
        panic!("/ToUnicode is not a stream");
    };
    let mut s = s.clone();
    let _ = s.decompress();
    String::from_utf8_lossy(&s.content).into_owned()
}

/// Does the CMap map `code` to `unicode`? Both `bfchar` and `bfrange`
/// spellings, since which one the pass emits is not the point of the test.
fn maps(cmap: &str, code: u8, unicode: u16) -> bool {
    let pair = format!("<{code:02X}> <{unicode:04X}>");
    let tight = format!("<{code:02X}><{unicode:04X}>");
    cmap.to_uppercase().contains(&pair.to_uppercase())
        || cmap.to_uppercase().contains(&tight.to_uppercase())
}

fn width_for(doc: &Document, font: &Dictionary, code: usize) -> i64 {
    let first = font
        .get(b"FirstChar")
        .and_then(|o| o.as_i64())
        .expect("no /FirstChar") as usize;
    let widths = match font.get(b"Widths") {
        Ok(Object::Array(a)) => a.clone(),
        Ok(Object::Reference(r)) => doc
            .get_object(*r)
            .and_then(|o| o.as_array())
            .expect("/Widths is not an array")
            .clone(),
        _ => panic!("font has no /Widths"),
    };
    widths
        .get(code - first)
        .and_then(|o| o.as_i64().ok())
        .unwrap_or_else(|| panic!("no width for code {code}"))
}

/// govdocs 001_001304: `fi`/`fl` written as StandardEncoding's 0xAE/0xAF.
#[test]
fn a_standard14_without_encoding_still_draws_its_ligatures() {
    let source = doc_with(vec![(
        "F1",
        times_roman_without_encoding(),
        b"\xAE\xAF".as_slice(),
    )]);
    let doc = convert(&source);
    let font = font_dict(&doc, "Times").clone();

    assert_ne!(
        effective_base_encoding(&doc, &font),
        "WinAnsiEncoding",
        "the source declared no /Encoding, so a reader draws the substitute's \
         built-in StandardEncoding: 0xAE is `fi`. Declaring WinAnsi over the \
         author's bytes turns it into `®` (govdocs 001_001304, `flux` -> `¯ux`)"
    );

    let cmap = to_unicode(&doc, &font);
    assert!(
        maps(&cmap, 0xAE, 0xFB01),
        "0xAE must extract as `fi` (U+FB01); the CMap says:\n{cmap}"
    );
    assert!(
        maps(&cmap, 0xAF, 0xFB02),
        "0xAF must extract as `fl` (U+FB02); the CMap says:\n{cmap}"
    );

    // §6.2.11.5 compares the /Widths entry against the program's advance for
    // the glyph the encoding names. `fi` is 556 in Times-Roman, `registered`
    // -- WinAnsi's 0xAE -- is 760, and the reader draws the width it is given.
    assert_eq!(
        width_for(&doc, &font, 0xAE),
        556,
        "code 0xAE must carry `fi`'s advance, not `registered`'s 760"
    );
}

/// The same document must still say what every code means: a substitution
/// that keeps the encoding but drops the mapping trades one loss for another.
#[test]
fn a_standard14_without_encoding_keeps_its_ordinary_letters() {
    let source = doc_with(vec![(
        "F1",
        times_roman_without_encoding(),
        b"Aa".as_slice(),
    )]);
    let doc = convert(&source);
    let font = font_dict(&doc, "Times").clone();
    let cmap = to_unicode(&doc, &font);
    assert!(
        maps(&cmap, b'A', 0x0041) && maps(&cmap, b'a', 0x0061),
        "the codes StandardEncoding and WinAnsi agree on must still extract; \
         the CMap says:\n{cmap}"
    );
}

/// The whole conversion, end to end: a dictionary that named MacRoman comes
/// out still naming it, whichever program the host had to offer.
#[test]
fn macroman_survives_the_conversion() {
    let source = doc_with(vec![(
        "F1",
        garamond_with_macroman(),
        b"\xD2Hi\xD5".as_slice(),
    )]);
    let doc = convert(&source);
    let font = font_dict(&doc, "AGaramond").clone();
    assert_eq!(
        effective_base_encoding(&doc, &font),
        "MacRomanEncoding",
        "0xD2 is `“` in MacRoman and `Ò` in WinAnsi, and the conversion does \
         not rewrite the bytes that were drawn through it"
    );
    let cmap = to_unicode(&doc, &font);
    assert!(
        maps(&cmap, 0xD2, 0x201C) && maps(&cmap, 0xD5, 0x2019),
        "0xD2/0xD5 must extract as `“`/`’`; the CMap says:\n{cmap}"
    );
}

/// govdocs 002_002168: `“`/`’` written as MacRoman's 0xD2/0xD5. The override
/// lives in `sync_subtypes_from_fontfile`, which fires for any dictionary
/// whose descriptor carries a `FontFile2` -- so this drives it directly,
/// rather than through a substitution whose program depends on the host's
/// font directories.
#[test]
fn a_truetype_substitute_keeps_macroman() {
    let mut doc = Document::with_version("1.7");
    let program = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length1" => 4_i64 },
        b"true".to_vec(),
    )));
    let fd = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "AGaramond-Regular",
        "Flags" => 32_i64,
        "FontFile2" => Object::Reference(program),
    }));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => "AGaramond-Regular",
        "Encoding" => "MacRomanEncoding",
        "FontDescriptor" => Object::Reference(fd),
    }));

    pdf_manip::pdfa_fonts::sync_subtypes_from_fontfile(&mut doc);

    let font = doc
        .get_object(font_id)
        .and_then(|o| o.as_dict())
        .expect("font dictionary");
    let encoding = name_of(font, b"Encoding").expect("the /Encoding must still be a name");
    assert_eq!(
        encoding, "MacRomanEncoding",
        "the author wrote MacRoman bytes: 0xD2 is `“` there and `Ò` in \
         WinAnsi. ISO 19005-2 6.2.11.3.1 allows either name, so there is \
         nothing to standardise away (govdocs 002_002168, `“The` -> `ÒThe`)"
    );
}

/// Without an encoding of its own a non-symbolic TrueType has none at all,
/// which 6.2.11.3.1 does forbid -- keeping MacRoman must not turn into
/// keeping nothing.
#[test]
fn a_truetype_substitute_without_an_encoding_still_gets_one() {
    let mut doc = Document::with_version("1.7");
    let program = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length1" => 4_i64 },
        b"true".to_vec(),
    )));
    let fd = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "AGaramond-Regular",
        "Flags" => 32_i64,
        "FontFile2" => Object::Reference(program),
    }));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => "AGaramond-Regular",
        "FontDescriptor" => Object::Reference(fd),
    }));

    pdf_manip::pdfa_fonts::sync_subtypes_from_fontfile(&mut doc);

    assert_eq!(
        name_of(
            doc.get_object(font_id)
                .and_then(|o| o.as_dict())
                .expect("font dictionary"),
            b"Encoding",
        )
        .unwrap_or_default(),
        "WinAnsiEncoding",
        "a non-symbolic TrueType with no /Encoding names none of the two \
         encodings 6.2.11.3.1 permits"
    );
}

/// Writes the two fixtures the pass-level traces in `crates/pdf-manip/examples`
/// are pointed at, so those traces read the same bytes the assertions above run
/// on.
///
/// Until 10-09-2026 this returned on the first line when `PDFA_FIXTURE_DIR` was
/// unset, which is every run but the one where somebody sets it by hand. libtest
/// has no skipped state, so that return was reported as a pass -- the shape
/// `scripts/ci/test_skip_lint.py` exists to refuse (#510). There is nothing here
/// that needs the variable to be set: the destination does, and a temporary
/// directory is a destination. So the fixtures are built and written on every
/// run, read back, and only *also* copied to `PDFA_FIXTURE_DIR` when one is
/// named. What the test measures no longer depends on the environment.
#[test]
fn fixtures_can_be_written_out() {
    let named = std::env::var("PDFA_FIXTURE_DIR").ok();
    let scratch =
        std::env::temp_dir().join(format!("pdfa-substitution-fixtures-{}", std::process::id()));
    let fixtures: [(&str, Vec<u8>); 2] = [
        (
            "standard14_no_encoding.pdf",
            doc_with(vec![(
                "F1",
                times_roman_without_encoding(),
                b"\xAE\xAF".as_slice(),
            )]),
        ),
        (
            "macroman.pdf",
            doc_with(vec![(
                "F1",
                garamond_with_macroman(),
                b"\xD2Hi\xD5".as_slice(),
            )]),
        ),
    ];

    for dir in std::iter::once(scratch.clone()).chain(named.map(std::path::PathBuf::from)) {
        std::fs::create_dir_all(&dir).expect("create fixture directory");
        for (name, bytes) in &fixtures {
            let path = dir.join(name);
            std::fs::write(&path, bytes).expect("write fixture");
            let written = std::fs::read(&path).expect("read fixture back");
            assert_eq!(&written, bytes, "{name} did not survive the round trip");
            assert!(
                written.starts_with(b"%PDF-"),
                "{name} is not a PDF: {:?}",
                &written[..written.len().min(8)]
            );
            Document::load_mem(&written)
                .unwrap_or_else(|e| panic!("{name} does not parse back: {e}"));
        }
    }

    std::fs::remove_dir_all(&scratch).expect("remove the scratch directory");
}
