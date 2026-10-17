// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! An embedded Type 1 that declares no `/Encoding` is read through the array
//! in its own program, not through StandardEncoding.
//!
//! PDF says so (ISO 32000-1 9.6.6.2), and the two are not close. In
//! `MathematicalPi-One` and `Universal-GreekwithMathPi` the program puts
//! parentheses and Greek letters where ASCII has `~`, `!` and Latin letters,
//! so a `/ToUnicode` that maps every code to its own byte value turns
//! `(1997)` into `~1997!.` and `ε₂(ν)` into `e2(v)`. veraPDF passes such a
//! file at conformance level U: the characters are wrong, but they are there.
//!
//! Measured on govdocs: 002_002385 kept 94.6% of its text, 002_002231 96.3%,
//! and 002_002202 carries eighteen fonts of this shape.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};

// ---------------------------------------------------------------------------
// A Type 1 font program, built here so the test depends on no fixture file
// ---------------------------------------------------------------------------

/// Encrypt a Type 1 section (ISO 32000-1 Annex ..., Adobe Type 1 Font Format
/// §7): `r` is 55665 for eexec and 4330 for a charstring.
fn type1_encrypt(plain: &[u8], mut r: u16, leading: usize) -> Vec<u8> {
    const C1: u16 = 52845;
    const C2: u16 = 22719;
    let mut out = Vec::with_capacity(plain.len() + leading);
    // The leading bytes are discarded on decryption; their value is free.
    let padded: Vec<u8> = std::iter::repeat_n(0x55u8, leading)
        .chain(plain.iter().copied())
        .collect();
    for &p in &padded {
        let c = p ^ (r >> 8) as u8;
        r = (c as u16).wrapping_add(r).wrapping_mul(C1).wrapping_add(C2);
        out.push(c);
    }
    out
}

/// A charstring that only declares an advance: `0 <width> hsbw endchar`.
fn charstring(width: i32) -> Vec<u8> {
    let mut v = vec![139u8]; // 0
    if (108..=1131).contains(&width) {
        let d = width - 108;
        v.push((d / 256) as u8 + 247);
        v.push((d % 256) as u8);
    } else {
        v.push(width as u8 + 139);
    }
    v.push(13); // hsbw
    v.push(14); // endchar
    type1_encrypt(&v, 4330, 4)
}

/// A Type 1 program whose built-in encoding is an array, not a standard name.
///
/// `encoding` is written as `dup <code> /<name> put`, the shape every real
/// program uses. Every name listed gets a charstring, so the program is
/// internally consistent: a glyph the encoding reaches is a glyph it has.
fn type1_program(font_name: &str, encoding: &[(u8, &str)]) -> Vec<u8> {
    let mut cleartext = format!(
        "%!FontType1-1.0: {font_name} 001.001\n\
         /FontName /{font_name} def\n\
         /PaintType 0 def\n\
         /FontType 1 def\n\
         /FontMatrix [0.001 0 0 0.001 0 0] readonly def\n\
         /FontBBox {{0 0 1000 1000}} readonly def\n\
         /Encoding 256 array\n\
         0 1 255 {{1 index exch /.notdef put}} for\n"
    );
    for (code, name) in encoding {
        cleartext.push_str(&format!("dup {code} /{name} put\n"));
    }
    cleartext.push_str(
        "readonly def\n\
         currentdict end\n\
         currentfile eexec\n",
    );

    let mut private = String::from(
        "dup /Private 8 dict dup begin\n\
         /RD {string currentfile exch readstring pop} executeonly def\n\
         /ND {noaccess def} executeonly def\n\
         /NP {noaccess put} executeonly def\n\
         /lenIV 4 def\n\
         /BlueValues [] ND\n\
         /Subrs 0 array\n\
         ND\n\
         end\n",
    );
    private.push_str(&format!(
        "/CharStrings {} dict dup begin\n",
        encoding.len() + 1
    ));
    let mut private_bytes = private.into_bytes();
    for (name, width) in
        std::iter::once((".notdef", 0)).chain(encoding.iter().map(|(_, n)| (*n, 500)))
    {
        let cs = charstring(width);
        private_bytes.extend_from_slice(format!("/{name} {} RD ", cs.len()).as_bytes());
        private_bytes.extend_from_slice(&cs);
        private_bytes.extend_from_slice(b" ND\n");
    }
    private_bytes.extend_from_slice(b"end\nend\nmark currentfile closefile\n");

    let mut out = cleartext.into_bytes();
    out.extend_from_slice(&type1_encrypt(&private_bytes, 55665, 4));
    out.extend_from_slice(b"\n");
    out.extend_from_slice(&vec![b'0'; 512]);
    out.extend_from_slice(b"\ncleartomark\n");
    out
}

// ---------------------------------------------------------------------------
// Fixture assembly
// ---------------------------------------------------------------------------

struct Embedded {
    font_name: &'static str,
    encoding: &'static [(u8, &'static str)],
    text: &'static [u8],
}

fn doc_with(font: &Embedded) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let program = type1_program(font.font_name, font.encoding);
    let cleartext_len = program
        .windows(5)
        .position(|w| w == b"eexec")
        .expect("the program has an eexec section")
        + 6;
    let program_len = program.len() as i64;
    let file_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Length" => program_len,
            "Length1" => cleartext_len as i64,
            "Length2" => program_len - cleartext_len as i64,
            "Length3" => 0_i64,
        },
        program,
    )));
    let fd_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(font.font_name.as_bytes().to_vec()),
        // Not symbolic: the flag is about the face, and these are text faces
        // with an unusual code layout. Nothing here depends on it.
        "Flags" => 34_i64,
        "FontBBox" => vec![0.into(), 0.into(), 1000.into(), 1000.into()],
        "ItalicAngle" => 0_i64,
        "Ascent" => 800_i64,
        "Descent" => (-200_i64),
        "CapHeight" => 700_i64,
        "StemV" => 80_i64,
        "FontFile" => Object::Reference(file_id),
    }));
    // No /Encoding: the program's own array is what applies.
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => Object::Name(font.font_name.as_bytes().to_vec()),
        "FirstChar" => 32_i64,
        "LastChar" => 126_i64,
        "Widths" => (32..=126).map(|_| Object::Integer(500)).collect::<Vec<_>>(),
        "FontDescriptor" => Object::Reference(fd_id),
    }));

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(font.text.to_vec(), StringFormat::Literal)],
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
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        },
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

fn the_font(doc: &Document) -> Dictionary {
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| d.get(b"Type").and_then(|o| o.as_name()).ok() == Some(b"Font"))
        .expect("no font dictionary in the output")
        .clone()
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
    let cmap = cmap.to_uppercase();
    cmap.contains(&format!("<{code:02X}> <{unicode:04X}>"))
        || cmap.contains(&format!("<{code:02X}><{unicode:04X}>"))
}

/// Is `code` mentioned on the left of any mapping at all?
fn mentions(cmap: &str, code: u8) -> bool {
    let cmap = cmap.to_uppercase();
    let needle = format!("<{code:02X}>");
    cmap.match_indices(&needle)
        .any(|(i, _)| cmap[i + needle.len()..].trim_start().starts_with('<'))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// govdocs 002_002385: `MathematicalPi-One` puts parentheses on `~` and `!`.
#[test]
fn a_builtin_encoding_decides_what_a_code_means() {
    let source = doc_with(&Embedded {
        font_name: "MathematicalPi-One",
        encoding: &[
            (126, "parenleft"),
            (33, "parenright"),
            (49, "one"),
            (57, "nine"),
            (55, "seven"),
        ],
        text: b"~1997!",
    });
    let doc = convert(&source);
    let font = the_font(&doc);
    let cmap = to_unicode(&doc, &font);

    assert!(
        maps(&cmap, 0x7E, 0x0028),
        "code 0x7E draws `parenleft`, so it must extract as `(`, not as `~`. \
         `(1997)` extracted as `~1997!` on govdocs 002_002385. The CMap says:\n{cmap}"
    );
    assert!(
        maps(&cmap, 0x21, 0x0029),
        "code 0x21 draws `parenright`, so it must extract as `)`, not `!`:\n{cmap}"
    );
    // The digits are where ASCII has them, and must survive the correction.
    assert!(
        maps(&cmap, 0x31, 0x0031) && maps(&cmap, 0x39, 0x0039),
        "the codes the program does put where ASCII has them must still map:\n{cmap}"
    );
}

/// A Greek face: the same defect, and the reason a U-level pass on these
/// files is worth nothing.
#[test]
fn greek_letters_do_not_extract_as_latin() {
    let source = doc_with(&Embedded {
        font_name: "Universal-GreekwithMathPi",
        encoding: &[(101, "epsilon"), (118, "nu"), (116, "tau")],
        text: b"env",
    });
    let doc = convert(&source);
    let font = the_font(&doc);
    let cmap = to_unicode(&doc, &font);

    for (code, want, latin) in [
        (0x65u8, 0x03B5u16, 'e'),
        (0x76, 0x03BD, 'v'),
        (0x74, 0x03C4, 't'),
    ] {
        assert!(
            maps(&cmap, code, want),
            "code {code:#04X} draws a Greek letter; extracting it as `{latin}` \
             is how `ε₂(ν)` became `e2(v)` on govdocs 002_002385:\n{cmap}"
        );
    }
}

/// The rule that makes the correction honest: a name nobody can resolve gets
/// no entry. An identity entry would state a character the font does not draw,
/// and a reader cannot tell the two apart.
#[test]
fn an_unresolvable_glyph_name_gets_no_mapping() {
    let source = doc_with(&Embedded {
        font_name: "Universal-GreekwithMathPi",
        encoding: &[(65, "foo123"), (66, "beta")],
        text: b"AB",
    });
    let doc = convert(&source);
    let font = the_font(&doc);
    let cmap = to_unicode(&doc, &font);

    assert!(
        maps(&cmap, 0x42, 0x03B2),
        "the resolvable neighbour must still map:\n{cmap}"
    );
    assert!(
        !mentions(&cmap, 0x41),
        "`foo123` is in no glyph list, so code 0x41 has no established meaning \
         and must carry no mapping -- least of all U+0041, which says the font \
         draws a letter A:\n{cmap}"
    );
}

/// The guard against fixing this by reading the program for every font: a
/// dictionary that names its encoding is still read through that name.
#[test]
fn a_declared_encoding_still_wins() {
    let mut source = doc_with(&Embedded {
        font_name: "Universal-GreekwithMathPi",
        encoding: &[(101, "epsilon")],
        text: b"e",
    });
    // Re-open and declare WinAnsi over the program's own array.
    let mut doc = Document::load_mem(&source).unwrap();
    let font_id = *doc
        .objects
        .iter()
        .find(|(_, o)| {
            o.as_dict()
                .map(|d| d.get(b"Type").and_then(|o| o.as_name()).ok() == Some(b"Font"))
                .unwrap_or(false)
        })
        .unwrap()
        .0;
    if let Some(Object::Dictionary(d)) = doc.objects.get_mut(&font_id) {
        d.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    }
    source.clear();
    doc.save_to(&mut source).unwrap();

    let doc = convert(&source);
    let font = the_font(&doc);
    let cmap = to_unicode(&doc, &font);
    assert!(
        maps(&cmap, 0x65, 0x0065),
        "the dictionary declares WinAnsiEncoding, so code 0x65 is `e` and the \
         program's array does not get a say:\n{cmap}"
    );
}
