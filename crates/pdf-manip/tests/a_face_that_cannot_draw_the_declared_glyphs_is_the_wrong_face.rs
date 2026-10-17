// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The glyph a document declares is the glyph it has to get.
//!
//! Two of the three documents #504 names lose their text to the same mistake
//! made in two places: something that is asked "can this program draw the glyph
//! this code names" answers from a source that cannot know, and the answer
//! "no" costs the byte.
//!
//! * `0083081` -- an unembedded `ArialMT` with an 88-entry Cyrillic
//!   `/Differences` (`afii10034`, `afii10070`, ...). The typeface name resolves
//!   to the Standard 14 Helvetica compiled in, a Latin Type 1C clone with none
//!   of those glyphs, while the Liberation face that is this name's own
//!   metric-compatible substitute carries Cyrillic and sat one line further
//!   down the same resolver. On top of that, `glyph_name_to_unicode` did not
//!   know a single `afiiNNNNN` name, so even with the right face embedded the
//!   whole `/Encoding` was judged unresolvable junk and replaced by the bare
//!   name `/WinAnsiEncoding` -- after which every Cyrillic code had no glyph
//!   name at all and became `/space`.
//!
//! * `829_829459` -- a Distiller 5 Helvetica subset whose `period` and `colon`
//!   charstrings carry the deprecated Type 1 `dotsection` operator. Both glyphs
//!   are in the charset with outlines, but the presence test asked for a width
//!   and our charstring interpreter refuses that operator, so every `:` and `.`
//!   on the page was overwritten with a space: `10:43` extracted as `10 43`.
//!
//! Measured after the fixes, on the documents themselves: both extract exactly
//! what the source extracts, and both stay PDF/A-2b (veraPDF 1.28.2).

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};

// ---------------------------------------------------------------------------
// A CFF program whose charstrings use an operator we do not interpret
// ---------------------------------------------------------------------------

/// The Standard Strings index of a glyph name (CFF specification, Appendix A).
///
/// Only the handful this fixture draws; a name outside it would need a String
/// INDEX entry, which is exactly the complication this list avoids.
fn standard_sid(name: &str) -> u16 {
    match name {
        ".notdef" => 0,
        "space" => 1,
        "period" => 15,
        "zero" => 17,
        "one" => 18,
        "three" => 20,
        "four" => 21,
        "colon" => 27,
        other => panic!("{other} is not one of the standard strings this fixture carries"),
    }
}

/// A CFF INDEX with one-byte offsets.
fn index(entries: &[Vec<u8>]) -> Vec<u8> {
    if entries.is_empty() {
        return vec![0, 0];
    }
    let mut out = Vec::new();
    out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    out.push(1); // offSize
    let mut offset = 1u8;
    out.push(offset);
    for entry in entries {
        offset += entry.len() as u8;
        out.push(offset);
    }
    for entry in entries {
        out.extend_from_slice(entry);
    }
    out
}

/// A DICT integer, always in the five-byte form so the DICT's size does not
/// depend on the value -- which is what lets the offsets be patched in after
/// the layout is known.
fn dict_int(value: i32) -> Vec<u8> {
    let mut out = vec![29u8];
    out.extend_from_slice(&value.to_be_bytes());
    out
}

/// A bare CFF (`/FontFile3` `/Subtype /Type1C`) drawing `glyphs`.
///
/// Every charstring in `dotsection` gets a leading `12 0` -- `dotsection`, an
/// operator Type 2 inherited from Type 1 and every real interpreter ignores.
/// Ours returns `UnsupportedOperator` for it, which is the point: the glyph is
/// there, and asking for its width is not how you find that out.
fn cff_program(glyphs: &[&str], dotsection: &[&str]) -> Vec<u8> {
    let mut charstrings: Vec<Vec<u8>> = vec![vec![14]]; // .notdef: endchar
    for name in glyphs {
        let mut cs = Vec::new();
        if dotsection.contains(name) {
            cs.extend_from_slice(&[12, 0]);
        }
        cs.push(14); // endchar
        charstrings.push(cs);
    }
    let charstrings = index(&charstrings);

    // charset format 0: one SID per glyph, .notdef excluded.
    let mut charset = vec![0u8];
    for name in glyphs {
        charset.extend_from_slice(&standard_sid(name).to_be_bytes());
    }

    // Private DICT: defaultWidthX and nominalWidthX, both stated.
    let mut private = dict_int(500);
    private.push(20);
    private.extend_from_slice(&dict_int(0));
    private.push(21);

    // Top DICT: charset (15), CharStrings (17), Private (18). Written twice --
    // once to learn its length, once with the offsets that length implies.
    let top_dict = |charset_at: i32, charstrings_at: i32, private_at: i32| {
        let mut d = dict_int(charset_at);
        d.push(15);
        d.extend_from_slice(&dict_int(charstrings_at));
        d.push(17);
        d.extend_from_slice(&dict_int(private.len() as i32));
        d.extend_from_slice(&dict_int(private_at));
        d.push(18);
        d
    };

    let header = vec![1u8, 0, 4, 1];
    let names = index(&[b"T8TEST+Helvetica".to_vec()]);
    let strings = index(&[]);
    let gsubrs = index(&[]);
    let prefix_len = header.len()
        + names.len()
        + index(&[top_dict(0, 0, 0)]).len()
        + strings.len()
        + gsubrs.len();

    let charset_at = prefix_len as i32;
    let charstrings_at = charset_at + charset.len() as i32;
    let private_at = charstrings_at + charstrings.len() as i32;

    let mut out = header;
    out.extend_from_slice(&names);
    out.extend_from_slice(&index(&[top_dict(charset_at, charstrings_at, private_at)]));
    out.extend_from_slice(&strings);
    out.extend_from_slice(&gsubrs);
    out.extend_from_slice(&charset);
    out.extend_from_slice(&charstrings);
    out.extend_from_slice(&private);
    out
}

/// One page drawing `text` through an embedded symbolic Type 1 font whose
/// `/Differences` names `glyphs` from code 0x30 up, with no `/BaseEncoding` --
/// the shape `829_829459`'s `/F6` has.
fn document_with_cff(text: &[u8]) -> Document {
    let glyphs = ["zero", "one", "three", "four", "colon", "period", "space"];
    let program = cff_program(&glyphs, &["colon", "period"]);

    let mut doc = Document::with_version("1.7");
    let program_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Subtype" => "Type1C" },
        program,
    )));
    let fd_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(b"T8TEST+Helvetica".to_vec()),
        // Symbolic, which is what puts this font in the pass's path at all.
        "Flags" => 4_i64,
        "ItalicAngle" => 0_i64,
        "Ascent" => 718_i64,
        "Descent" => -207_i64,
        "CapHeight" => 718_i64,
        "StemV" => 88_i64,
        "FontBBox" => vec![(-166).into(), (-225).into(), 1000.into(), 931.into()],
        "FontFile3" => Object::Reference(program_id),
    }));

    // 0x30 zero, 0x31 one, 0x33 three, 0x34 four, 0x3A colon, 0x2E period.
    let differences = vec![
        Object::Integer(0x2E),
        Object::Name(b"period".to_vec()),
        Object::Integer(0x30),
        Object::Name(b"zero".to_vec()),
        Object::Name(b"one".to_vec()),
        Object::Integer(0x33),
        Object::Name(b"three".to_vec()),
        Object::Name(b"four".to_vec()),
        Object::Integer(0x3A),
        Object::Name(b"colon".to_vec()),
    ];
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => Object::Name(b"T8TEST+Helvetica".to_vec()),
        "FirstChar" => 0x2E_i64,
        "LastChar" => 0x3A_i64,
        "Widths" => Object::Array(vec![Object::Integer(500); 0x3A - 0x2E + 1]),
        "FontDescriptor" => Object::Reference(fd_id),
        "Encoding" => dictionary! {
            "Type" => "Encoding",
            "Differences" => Object::Array(differences),
        },
    }));

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("/F1", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(text.to_vec(), StringFormat::Literal)],
            ),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        content.encode().expect("encode content"),
    )));

    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        },
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1_i64,
            "Kids" => vec![Object::Reference(page_id)],
        }),
    );
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc
}

// ---------------------------------------------------------------------------
// An unembedded Latin typeface whose /Differences declares Cyrillic
// ---------------------------------------------------------------------------

/// `0083081`'s `/F_0`, small: `ArialMT`, not embedded, `/WinAnsiEncoding` with
/// a `/Differences` that names Cyrillic in the private code range, and a
/// `/ToUnicode` that says which characters those codes draw.
fn cyrillic_document() -> Vec<u8> {
    cyrillic_document_drawing(&[0x02, 0x03, 0x04, 0x05, 0x4C])
}

/// The same document, drawing only `drawn`.
fn cyrillic_document_drawing(drawn: &[u8]) -> Vec<u8> {
    cyrillic_document_drawing_in(drawn, false)
}

/// The same document again; with `in_form` the text is drawn inside a Form
/// XObject instead of in the page's own content stream.
fn cyrillic_document_drawing_in(drawn: &[u8], in_form: bool) -> Vec<u8> {
    cyrillic_document_shaped(drawn, in_form, false)
}

/// The same document once more; with `range_cmap` the /ToUnicode states the same
/// mapping as a `beginbfrange` whose destinations are an array, which is the
/// second shape ISO 32000-1 9.10.3 allows and the one the simple-font CMap
/// reader cannot see.
fn cyrillic_document_shaped(drawn: &[u8], in_form: bool, range_cmap: bool) -> Vec<u8> {
    cyrillic_document_full(drawn, in_form, range_cmap, false)
}

/// And once more; with `also_in_a_form` the page draws `drawn` itself AND a
/// second stream naming the same font draws the rest -- a Form XObject, or a
/// tiling pattern when `as_pattern` is set.
fn cyrillic_document_full(
    drawn: &[u8],
    in_form: bool,
    range_cmap: bool,
    also_in_a_form: bool,
) -> Vec<u8> {
    cyrillic_document_every_shape(drawn, in_form, range_cmap, also_in_a_form, false)
}

/// The last shape: `as_pattern` makes that second stream a tiling pattern
/// rather than a Form XObject.
fn cyrillic_document_every_shape(
    drawn: &[u8],
    in_form: bool,
    range_cmap: bool,
    also_in_a_form: bool,
    as_pattern: bool,
) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");

    let fd_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(b"ArialMT".to_vec()),
        "Flags" => 32_i64,
        "ItalicAngle" => 0_i64,
        "Ascent" => 728_i64,
        "Descent" => -210_i64,
        "CapHeight" => 716_i64,
        "StemV" => 88_i64,
        "FontBBox" => vec![(-665).into(), (-325).into(), 2000.into(), 1006.into()],
    }));

    // Codes 2..5 are Р А С Ч; code 76 is `>`, a plain Latin name in the same
    // array. The Latin one is here on purpose: the AGL list that decides
    // whether a /Differences may survive is hand-written, and a single name
    // missing from it condemns the whole array.
    let differences = vec![
        Object::Integer(2),
        Object::Name(b"afii10034".to_vec()),
        Object::Name(b"afii10017".to_vec()),
        Object::Name(b"afii10035".to_vec()),
        Object::Name(b"afii10041".to_vec()),
        Object::Integer(76),
        Object::Name(b"greater".to_vec()),
    ];
    let tounicode = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
         5 beginbfchar\n<02> <0420>\n<03> <0410>\n<04> <0421>\n<05> <0427>\n<4C> <003E>\n\
         endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";
    let tounicode_range = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
         2 beginbfrange\n<02> <05> [<0420> <0410> <0421> <0427>]\n<4C> <4C> [<003E>]\n\
         endbfrange\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";
    let tounicode = if range_cmap {
        tounicode_range
    } else {
        tounicode
    };
    let tounicode_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        tounicode.as_bytes().to_vec(),
    )));

    let mut widths = vec![Object::Integer(556); 76 - 2 + 1];
    widths[76 - 2] = Object::Integer(584); // `>` is wider than the letters
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => Object::Name(b"ArialMT".to_vec()),
        "FirstChar" => 2_i64,
        "LastChar" => 76_i64,
        "Widths" => Object::Array(widths),
        "FontDescriptor" => Object::Reference(fd_id),
        "ToUnicode" => Object::Reference(tounicode_id),
        "Encoding" => dictionary! {
            "Type" => "Encoding",
            "BaseEncoding" => "WinAnsiEncoding",
            "Differences" => Object::Array(differences),
        },
    }));

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(drawn.to_vec(), StringFormat::Literal)],
            ),
            Operation::new("ET", vec![]),
        ],
    };
    let drawing = content.encode().expect("encode content");

    let pages_id = doc.new_object_id();
    let (content_id, resources) = if in_form {
        let form_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Resources" => dictionary! {
                    "Font" => dictionary! { "F1" => Object::Reference(font_id) },
                },
            },
            drawing,
        )));
        let page_content = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            b"/Fm0 Do".to_vec(),
        )));
        (
            page_content,
            dictionary! {
                "XObject" => dictionary! { "Fm0" => Object::Reference(form_id) },
            },
        )
    } else {
        (
            doc.add_object(Object::Stream(Stream::new(dictionary! {}, drawing))),
            dictionary! {
                "Font" => dictionary! { "F1" => Object::Reference(font_id) },
            },
        )
    };
    let mut resources = resources;
    if also_in_a_form {
        // A second stream, naming the same font dictionary, that nothing
        // reading page /Contents will ever open.
        let dict = if as_pattern {
            dictionary! {
                "Type" => "Pattern",
                "PatternType" => 1_i64,
                "PaintType" => 1_i64,
                "TilingType" => 1_i64,
                "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                "XStep" => 100_i64,
                "YStep" => 100_i64,
                "Resources" => dictionary! {
                    "Font" => dictionary! { "F1" => Object::Reference(font_id) },
                },
            }
        } else {
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Resources" => dictionary! {
                    "Font" => dictionary! { "F1" => Object::Reference(font_id) },
                },
            }
        };
        let second_id = doc.add_object(Object::Stream(Stream::new(
            dict,
            b"BT /F1 12 Tf 72 600 Td (\x4C) Tj ET".to_vec(),
        )));
        if as_pattern {
            resources.set(
                "Pattern",
                Object::Dictionary(dictionary! { "P0" => Object::Reference(second_id) }),
            );
        } else {
            resources.set(
                "XObject",
                Object::Dictionary(dictionary! { "Fm0" => Object::Reference(second_id) }),
            );
        }
    }
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(resources),
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1_i64,
            "Kids" => vec![Object::Reference(page_id)],
        }),
    );
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

fn the_arial_font(doc: &Document) -> Dictionary {
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| {
            d.get(b"Type").and_then(|o| o.as_name()).ok() == Some(b"Font")
                && d.get(b"BaseFont")
                    .and_then(|o| o.as_name())
                    .map(|n| n.ends_with(b"ArialMT"))
                    .unwrap_or(false)
        })
        .expect("no ArialMT font dictionary in the output")
        .clone()
}

/// The program the output embeds for this font.
fn embedded_program(doc: &Document, font: &Dictionary) -> Vec<u8> {
    let Ok(Object::Reference(fd_id)) = font.get(b"FontDescriptor") else {
        panic!("the font has no /FontDescriptor");
    };
    let fd = doc
        .get_object(*fd_id)
        .and_then(|o| o.as_dict())
        .expect("/FontDescriptor is not a dictionary");
    let program_id = ["FontFile2", "FontFile3", "FontFile"]
        .iter()
        .find_map(|key| match fd.get(key.as_bytes()) {
            Ok(Object::Reference(r)) => Some(*r),
            _ => None,
        })
        .expect("the font embeds no program");
    let Some(Object::Stream(stream)) = doc.objects.get(&program_id) else {
        panic!("the embedded program is not a stream");
    };
    let mut stream = stream.clone();
    let _ = stream.decompress();
    stream.content
}

/// Code → glyph name, as the output's `/Encoding` states it.
fn encoded_names(doc: &Document, font: &Dictionary) -> std::collections::BTreeMap<u8, String> {
    let enc = match font.get(b"Encoding") {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(r)) => match doc.get_object(*r) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => return std::collections::BTreeMap::new(),
        },
        _ => return std::collections::BTreeMap::new(),
    };
    let diffs = match enc.get(b"Differences") {
        Ok(Object::Array(a)) => a.clone(),
        Ok(Object::Reference(r)) => doc
            .get_object(*r)
            .and_then(|o| o.as_array())
            .expect("/Differences is not an array")
            .clone(),
        _ => return std::collections::BTreeMap::new(),
    };
    let mut out = std::collections::BTreeMap::new();
    let mut code: u8 = 0;
    for item in &diffs {
        match item {
            Object::Integer(n) => code = (*n).clamp(0, 255) as u8,
            Object::Name(n) => {
                out.insert(code, String::from_utf8_lossy(n).into_owned());
                code = code.wrapping_add(1);
            }
            _ => {}
        }
    }
    out
}

/// The bytes of every `Tj` string on the page, concatenated.
fn drawn_bytes(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    for page_id in doc.get_pages().values() {
        for cs_id in pdf_manip::content_editor::get_content_stream_ids(doc, *page_id) {
            let Some(Object::Stream(s)) = doc.objects.get(&cs_id) else {
                continue;
            };
            let mut s = s.clone();
            let _ = s.decompress();
            let Ok(editor) = pdf_manip::content_editor::ContentEditor::from_stream(&s.content)
            else {
                continue;
            };
            // Both operators: `pdfa_text_geometry` compensates a substitute's
            // width change by splitting a run into per-character `Tj`s inside a
            // `TJ` array, so a reader that only knows `Tj` reports the split
            // characters as lost.
            for op in editor.operations() {
                match op.operator.as_str() {
                    "Tj" => {
                        if let Some(Object::String(bytes, _)) = op.operands.first() {
                            out.extend_from_slice(bytes);
                        }
                    }
                    "TJ" => {
                        if let Some(Object::Array(items)) = op.operands.first() {
                            for item in items {
                                if let Object::String(bytes, _) = item {
                                    out.extend_from_slice(bytes);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The Cyrillic document
// ---------------------------------------------------------------------------

/// The face that gets embedded has to be able to draw the page.
///
/// Red without the repertoire check in `simple_font_source_for`: the name
/// `ArialMT` answers with the Standard 14 Helvetica, a Latin CFF in which
/// U+0420 does not exist.
#[test]
fn a_cyrillic_document_gets_a_face_that_can_draw_cyrillic() {
    let doc = convert(&cyrillic_document());
    let font = the_arial_font(&doc);
    let program = embedded_program(&doc, &font);

    let face = ttf_parser::Face::parse(&program, 0)
        .expect("a face that can draw Cyrillic is an SFNT, not a Latin CFF clone");
    for ch in ['\u{0420}', '\u{0410}', '\u{0421}', '\u{0427}'] {
        assert!(
            face.glyph_index(ch).is_some_and(|gid| gid.0 != 0),
            "the embedded face has no glyph for {ch:?}, so the page draws nothing there",
        );
    }
}

/// The names the document declared are the names the output declares.
///
/// Red without the Adobe Glyph List fallback in `glyph_name_to_unicode`: no
/// `afiiNNNNN` name resolved, the whole `/Encoding` was judged unresolvable and
/// flattened to the bare name `/WinAnsiEncoding`, and the codes then became
/// `/space`.
#[test]
fn the_cyrillic_glyph_names_survive_the_conversion() {
    let doc = convert(&cyrillic_document());
    let font = the_arial_font(&doc);
    let names = encoded_names(&doc, &font);

    for (code, expected) in [
        (2u8, "afii10034"),
        (3, "afii10017"),
        (4, "afii10035"),
        (5, "afii10041"),
    ] {
        assert_eq!(
            names.get(&code).map(String::as_str),
            Some(expected),
            "code {code} no longer names {expected}; the encoding is {names:?}",
        );
    }
    assert_eq!(
        drawn_bytes(&doc),
        vec![0x02, 0x03, 0x04, 0x05, 0x4C],
        "the page no longer draws the bytes it drew",
    );
}

/// A plain Latin name in the same array is not sanitised away either.
///
/// Red when `greater` is missing from `AGL_NAMES`: the sanitiser rewrites every
/// name it does not recognise to `space` and leaves `/Widths` alone, so code 76
/// declares 584 while the program draws a 278-wide space -- ISO 19005-2
/// 6.2.11.5, on a document that was conformant before the repair.
#[test]
fn a_latin_name_in_a_cyrillic_encoding_keeps_its_own_glyph() {
    let doc = convert(&cyrillic_document());
    let font = the_arial_font(&doc);
    let names = encoded_names(&doc, &font);

    assert_eq!(
        names.get(&76u8).map(String::as_str),
        Some("greater"),
        "code 76 no longer names `greater`, so its declared width of 584 now \
         belongs to whatever it does name",
    );
}

// ---------------------------------------------------------------------------
// The colon whose charstring we cannot run
// ---------------------------------------------------------------------------

/// A glyph the font carries keeps its byte, whether or not we can run it.
///
/// Red without `cff_names_a_charstring`: the presence test asked
/// `glyph_width_f64`, which returns `None` for a charstring containing
/// `dotsection`, and `10:43` came out as `10 43`.
#[test]
fn a_colon_our_interpreter_cannot_run_keeps_its_byte() {
    let mut doc = document_with_cff(b"10:43.");

    let condemned = pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);

    assert_eq!(
        drawn_bytes(&doc),
        b"10:43.".to_vec(),
        "the page lost a character the embedded program carries \
         ({condemned} font(s) were condemned)",
    );
}

/// The same pass still removes a code the program really has nothing for.
///
/// The counterweight: widening "does this glyph exist" must not turn the pass
/// off. `0x39` is `nine`, which this fixture's `/Differences` does not name and
/// its charset does not carry.
#[test]
fn a_code_the_program_really_has_no_glyph_for_is_still_removed() {
    let mut doc = document_with_cff(b"1\x3943");

    pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);

    assert_eq!(
        drawn_bytes(&doc),
        b"1 43".to_vec(),
        "a code with no glyph at all survived, which is 6.2.11.8:1",
    );
}

/// A face is judged on what the page draws, not on what the dictionary declares.
///
/// The counterweight to the choice above. A fallback font dictionary declares
/// every code and means none of them: `issue6605` carries seven Type 3 fallbacks
/// declaring 224 codes each, most of them Latin Extended-A that no page shows.
/// Scored against that list the Standard 14 falls under the floor and a 410 KB
/// Liberation is embedded to draw glyphs nobody asked for -- 51 % of the source
/// became 164 % with no change on any other axis.
///
/// Here: the same Cyrillic dictionary, drawing only `>`. The metric-compatible
/// Standard 14 keeps the page, so it keeps the font.
#[test]
fn a_face_is_judged_on_what_the_page_draws() {
    let doc = convert(&cyrillic_document_drawing(&[0x4C]));
    let font = the_arial_font(&doc);
    let program = embedded_program(&doc, &font);

    assert!(
        cff_parser::Table::parse(&program).is_some(),
        "a page that draws only `>` was given a {} byte SFNT substitute; the \
         compiled-in Standard 14 draws it in 15 KB",
        program.len(),
    );
}

/// A Type 3 font's substitute is bookkeeping, and is not chosen by repertoire.
///
/// Its glyphs are its own /CharProcs; the program a substitute supplies gives
/// the width and .notdef passes something to point at and never reaches the
/// page. `issue6605` is where that matters: preferring a face that can draw the
/// Latin Extended-A its /ToUnicode declares cost 200 KB of output and changed
/// nothing a reader can see.
#[test]
fn a_type3_fallback_is_not_chosen_by_repertoire() {
    let mut doc = Document::with_version("1.7");

    // One glyph procedure, drawing nothing but an advance.
    let proc_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"500 0 d0".to_vec(),
    )));
    // Latin Extended-A: the Standard 14 has none of it and Liberation has all
    // of it, which is what made this document swap faces.
    let tounicode = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
         4 beginbfchar\n<41> <0100>\n<42> <0102>\n<43> <0104>\n<44> <0106>\n\
         endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";
    let tounicode_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        tounicode.as_bytes().to_vec(),
    )));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type3",
        "FontBBox" => vec![0.into(), 0.into(), 1000.into(), 1000.into()],
        "FontMatrix" => vec![
            Object::Real(0.001), 0.into(), 0.into(), Object::Real(0.001),
            0.into(), 0.into(),
        ],
        "FirstChar" => 0x41_i64,
        "LastChar" => 0x44_i64,
        "Widths" => Object::Array(vec![Object::Integer(500); 4]),
        "CharProcs" => dictionary! { "a" => Object::Reference(proc_id) },
        "Encoding" => dictionary! {
            "Type" => "Encoding",
            "Differences" => Object::Array(vec![
                Object::Integer(0x41),
                Object::Name(b"a".to_vec()),
            ]),
        },
        "ToUnicode" => Object::Reference(tounicode_id),
    }));

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(b"ABCD".to_vec(), StringFormat::Literal)],
            ),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        content.encode().expect("encode content"),
    )));
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        },
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1_i64,
            "Kids" => vec![Object::Reference(page_id)],
        }),
    );
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    pdf_manip::pdfa_fonts::embed_fonts(&mut doc).expect("embedding failed");

    let embedded: Vec<usize> = doc
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.get(b"Type").and_then(|o| o.as_name()).ok() == Some(b"FontDescriptor"))
        .filter_map(|fd| {
            ["FontFile2", "FontFile3", "FontFile"]
                .iter()
                .find_map(|k| match fd.get(k.as_bytes()) {
                    Ok(Object::Reference(r)) => Some(*r),
                    _ => None,
                })
        })
        .filter_map(|id| match doc.objects.get(&id) {
            Some(Object::Stream(s)) => {
                let mut s = s.clone();
                let _ = s.decompress();
                Some(s.content.len())
            }
            _ => None,
        })
        .collect();

    assert!(
        embedded.iter().all(|n| *n < 100_000),
        "a Type 3 fallback was given a face chosen by repertoire: embedded \
         program size(s) {embedded:?}",
    );
}

/// No usage, no opinion.
///
/// `collect_simple_font_used_codes` reads page `/Contents` and not the streams
/// of Form XObjects, so a font drawn only inside a form arrives at the choice
/// with nothing recorded. Reading that as "every declared code is drawn" is the
/// narrowing above inverted, on exactly the documents that made it necessary,
/// so the name's answer stands until the page says otherwise. What that costs
/// is stated here rather than left to be discovered: a `0083081` whose text
/// lives in a form does not get the better face.
#[test]
fn a_font_drawn_only_in_a_form_keeps_the_name_s_answer() {
    let doc = convert(&cyrillic_document_drawing_in(
        &[0x02, 0x03, 0x04, 0x05],
        true,
    ));
    let font = the_arial_font(&doc);
    let program = embedded_program(&doc, &font);

    assert!(
        cff_parser::Table::parse(&program).is_some(),
        "a font whose usage is unknown was given a {} byte substitute chosen \
         from characters nothing was seen to draw",
        program.len(),
    );
}

/// The second shape a /ToUnicode may take is read too.
///
/// ISO 32000-1 9.10.3 allows `<start> <end> [<dst> ...]`, and the simple-font
/// CMap reader accepts only a pure-hex third token: it answers nothing at all
/// for the array form. Read through that reader this document declares no
/// character, which is indistinguishable from "the name's face is fine" — so a
/// page of Cyrillic keeps a Latin face and draws boxes (Codex P2 on #1882).
#[test]
fn a_bfrange_with_an_array_of_destinations_is_read() {
    let doc = convert(&cyrillic_document_shaped(
        &[0x02, 0x03, 0x04, 0x05, 0x4C],
        false,
        true,
    ));
    let font = the_arial_font(&doc);
    let program = embedded_program(&doc, &font);

    let face = ttf_parser::Face::parse(&program, 0)
        .expect("a face that can draw Cyrillic is an SFNT, not a Latin CFF clone");
    for ch in ['\u{0420}', '\u{0410}', '\u{0421}', '\u{0427}'] {
        assert!(
            face.glyph_index(ch).is_some_and(|gid| gid.0 != 0),
            "the embedded face has no glyph for {ch:?}: the range form of the \
             /ToUnicode was not read, so the page's script was never seen",
        );
    }
}

/// Half a record is not a record.
///
/// The harder half of the same gap: when one font dictionary is used both on
/// the page and inside a Form XObject, `used_codes` is not empty — it holds the
/// page's codes and none of the form's. Scoring a face on that sample judges it
/// on characters it was not asked about, and the answer is confidently wrong
/// rather than absent. So the name's answer stands until the page has said all
/// of it (Codex P2 on #1882).
///
/// Here the page draws the Cyrillic and a form draws `>` through the same
/// dictionary. On the page's half alone the Standard 14 scores near zero and
/// the face is swapped on evidence that is missing the half it was not shown.
/// The swap may even be the right one; being right by accident is not the rule
/// this pass is allowed to run on, and the counterweight has to be a case where
/// the guard changes the answer, or it guards nothing.
#[test]
fn a_font_used_both_on_the_page_and_in_a_form_is_not_judged_on_half_of_it() {
    let doc = convert(&cyrillic_document_full(
        &[0x02, 0x03, 0x04, 0x05],
        false,
        false,
        true,
    ));
    let font = the_arial_font(&doc);
    let program = embedded_program(&doc, &font);

    assert!(
        cff_parser::Table::parse(&program).is_some(),
        "a font whose usage is only half recorded was given a {} byte \
         substitute chosen on that half",
        program.len(),
    );
}

/// And a tiling pattern is the other stream that carries its own resources.
///
/// ISO 32000-1 gives exactly two content streams a /Resources of their own: a
/// Form XObject (8.10) and a tiling pattern (8.7.3). The code collector opens
/// neither, so both leave the same incomplete sample behind, and a guard that
/// knew only about forms would let the pattern through (Codex P2 on #1882).
#[test]
fn a_font_used_on_the_page_and_in_a_tiling_pattern_is_not_judged_on_half_of_it() {
    let doc = convert(&cyrillic_document_every_shape(
        &[0x02, 0x03, 0x04, 0x05],
        false,
        false,
        true,
        true,
    ));
    let font = the_arial_font(&doc);
    let program = embedded_program(&doc, &font);

    assert!(
        cff_parser::Table::parse(&program).is_some(),
        "a font a tiling pattern also draws with was given a {} byte substitute \
         chosen from the page's codes alone",
        program.len(),
    );
}
