// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A /W array is keyed by CID, and under a CMap that is not the identity that
//! is not the number in the content stream.
//!
//! The defect (#512, a child of #503). `pdfjs__test_pdfs_issue3323` draws
//! `<0054>` for its `T` through an embedded CMap that turns that code into
//! CID 36, and its author wrote the widths under the codes: `84 [611]`. The
//! document was conformant out of this converter until `4580c2738` taught the
//! width pass to correct a declared /W in place instead of deriving a fresh
//! one -- the right rule for the seven CJK documents of #488, whose fonts are
//! `Identity-H` so that a key is a code and a CID at once, and the wrong one
//! here. Correcting in place restated the widths of CIDs 84, 72 and 65, which
//! the page never draws, and left CIDs 36, 26 and 19, which it does, on /DW
//! 1000 against a program saying 611, 722 and 667. veraPDF 6.2.11.5:1, three
//! failed checks, conformance gone on the axis nothing else offsets.
//!
//! So a declared array is corrected in place only while its keys are CIDs this
//! font has glyphs for. Where they are not, and the encoding is one whose codes
//! are not its CIDs, the array is not about this font and a fresh one is
//! derived from the program and the CIDToGIDMap -- which is what shipped before
//! `4580c2738` and what made this document conformant then.

use lopdf::{dictionary, Document, Object, Stream};

/// Glyphs in the fixture program.
const NUM_GLYPHS: u16 = 300;

/// CIDs the fixture's CIDToGIDMap defines. Every key above this is a key the
/// font cannot answer for, which is the whole signal.
const NUM_CIDS: u16 = 40;

/// The advance the fixture program carries for a glyph, in font units.
///
/// `units_per_em` is 1000, so these are also the widths a PDF dictionary must
/// state. The three that matter are the glyphs behind the CIDs the page draws.
fn program_advance(gid: u16) -> u16 {
    match gid {
        36 => 667, // A
        43 => 722, // H
        55 => 611, // T
        59 => 722, // N
        0 => 750,
        _ => 500,
    }
}

/// CID -> GID, as the fixture's CIDToGIDMap stream states it.
///
/// The document's CMap sends `<0041>`..`<0050>` to CIDs 19..34, so the letters
/// the page draws sit low in the CID space and high in the code space.
fn gid_for_cid(cid: u16) -> u16 {
    match cid {
        19 => 36, // A, from code 0x41
        26 => 43, // H, from code 0x48
        38 => 55, // T, from code 0x54
        32 => 59, // N, from code 0x4e
        0 => 0,
        _ => 100 + cid,
    }
}

/// A TrueType program with `NUM_GLYPHS` glyphs and nothing but metrics.
///
/// `head`, `hhea`, `maxp` and `hmtx` are all a width pass reads, and all
/// `ttf_parser` needs to answer `number_of_glyphs` and `glyph_hor_advance`.
/// Leaving `glyf` out keeps the fixture readable; no assertion here depends on
/// an outline.
fn metrics_only_truetype() -> Vec<u8> {
    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes()); // version 1.0
    head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magic
    head[18..20].copy_from_slice(&1000u16.to_be_bytes()); // unitsPerEm
    head[50..52].copy_from_slice(&0u16.to_be_bytes()); // indexToLocFormat: short

    let mut hhea = vec![0u8; 36];
    hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    hhea[4..6].copy_from_slice(&800u16.to_be_bytes()); // ascender
    hhea[34..36].copy_from_slice(&NUM_GLYPHS.to_be_bytes()); // numberOfHMetrics

    let mut maxp = vec![0u8; 32];
    maxp[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    maxp[4..6].copy_from_slice(&NUM_GLYPHS.to_be_bytes());

    let mut hmtx = Vec::with_capacity(NUM_GLYPHS as usize * 4);
    for gid in 0..NUM_GLYPHS {
        hmtx.extend_from_slice(&program_advance(gid).to_be_bytes());
        hmtx.extend_from_slice(&0u16.to_be_bytes()); // leftSideBearing
    }

    let tables: [(&[u8; 4], Vec<u8>); 4] = [
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"maxp", maxp),
    ];

    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // searchRange
    out.extend_from_slice(&0u16.to_be_bytes()); // entrySelector
    out.extend_from_slice(&0u16.to_be_bytes()); // rangeShift

    let mut offset = 12 + tables.len() * 16;
    let mut records = Vec::new();
    let mut body = Vec::new();
    for (tag, data) in &tables {
        records.push((*tag, offset as u32, data.len() as u32));
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + tables.len() * 16 + body.len();
    }
    for (tag, off, len) in records {
        out.extend_from_slice(tag);
        out.extend_from_slice(&0u32.to_be_bytes()); // checksum, unread here
        out.extend_from_slice(&off.to_be_bytes());
        out.extend_from_slice(&len.to_be_bytes());
    }
    out.extend_from_slice(&body);
    out
}

/// The CIDToGIDMap stream: two bytes per CID, big-endian, `NUM_CIDS` of them.
fn cid_to_gid_map() -> Vec<u8> {
    let mut out = Vec::with_capacity(NUM_CIDS as usize * 2);
    for cid in 0..NUM_CIDS {
        out.extend_from_slice(&gid_for_cid(cid).to_be_bytes());
    }
    out
}

/// The embedded CMap of `issue3323`, cut down to the codes this page shows,
/// plus one range whose CIDs run past the /CIDToGIDMap.
///
/// The name on the stream dictionary -- `Ming-Lt-HKSCS-ExtB`, not `Identity-H`
/// -- says a code is not a CID here; the body says which CIDs this encoding can
/// ever reach, and that is what tells a mis-keyed array from an over-declared
/// one. CIDs 200..204 are reachable and have no glyph, which is the ordinary
/// fallback to `.notdef` and not a signal at all.
fn embedded_cmap() -> Vec<u8> {
    b"/CIDInit /ProcSet findresource begin
  12 dict begin
    begincmap
      /CMapName /VASKKK+ArialMT def
      /CMapType 1 def
      /WMode 0 def
      1 begincodespacerange
        <0000> <ffff>
      endcodespacerange
      1 begincidrange <0041> <0050> 19 endcidrange
      1 begincidrange <0060> <0064> 200 endcidrange
    endcmap
  CMapName currentdict /CMap defineresource pop
  end
end"
    .to_vec()
}

/// One page drawing `A H T` through a CID font under a non-identity CMap.
///
/// `encoding_is_identity` swaps that CMap for `Identity-H`, which is the other
/// half of the rule: there a code *is* a CID, and a declared array stays.
fn document_with(author_w: Vec<Object>, encoding_is_identity: bool) -> (Document, lopdf::ObjectId) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let program = metrics_only_truetype();
    let program_len = program.len() as i64;
    let font_file = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length1" => Object::Integer(program_len) },
        program,
    )));
    let fd_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "VASKKK+ArialMT",
        "Flags" => Object::Integer(32),
        "FontFile2" => Object::Reference(font_file),
    }));
    let map_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        cid_to_gid_map(),
    )));
    let descendant_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => "VASKKK+ArialMT",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => Object::Integer(0),
        },
        "FontDescriptor" => Object::Reference(fd_id),
        "CIDToGIDMap" => Object::Reference(map_id),
        "DW" => Object::Integer(1000),
        "W" => Object::Array(author_w),
    }));

    let encoding = if encoding_is_identity {
        Object::Name(b"Identity-H".to_vec())
    } else {
        Object::Reference(doc.add_object(Object::Stream(Stream::new(
            dictionary! {
                "Type" => "CMap",
                "CMapName" => Object::Name(b"Ming-Lt-HKSCS-ExtB".to_vec()),
                "WMode" => Object::Integer(0),
            },
            embedded_cmap(),
        ))))
    };
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "VASKKK+ArialMT",
        "Encoding" => encoding,
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant_id)]),
    }));

    let content = b"BT /F1 20 Tf 10 20 Td <005400480041> Tj ET".to_vec();
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        },
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        }),
    );
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    (doc, descendant_id)
}

/// The author's /W as `issue3323` carries it: one entry per character code.
fn code_keyed_widths() -> Vec<Object> {
    let mut w = Vec::new();
    for (code, width) in [(65i64, 667i64), (72, 722), (78, 722), (84, 611)] {
        w.push(Object::Integer(code));
        w.push(Object::Array(vec![Object::Integer(width)]));
    }
    w
}

/// /DW and the per-CID widths, read back the way a reader reads them.
fn widths_of(
    doc: &Document,
    cid_id: lopdf::ObjectId,
) -> (i64, std::collections::HashMap<u16, i64>) {
    let Some(Object::Dictionary(cid)) = doc.objects.get(&cid_id) else {
        panic!("the CIDFont dictionary is gone");
    };
    let dw = match cid.get(b"DW") {
        Ok(Object::Integer(v)) => *v,
        _ => 1000,
    };
    let mut widths = std::collections::HashMap::new();
    if let Ok(Object::Array(w)) = cid.get(b"W") {
        let mut i = 0;
        while i < w.len() {
            let Ok(first) = w[i].as_i64() else { break };
            match w.get(i + 1) {
                Some(Object::Array(entries)) => {
                    for (j, entry) in entries.iter().enumerate() {
                        if let Ok(v) = entry.as_i64() {
                            widths.insert((first + j as i64) as u16, v);
                        }
                    }
                    i += 2;
                }
                Some(_) => {
                    let (Ok(last), Some(Ok(width))) =
                        (w[i + 1].as_i64(), w.get(i + 2).map(|o| o.as_i64()))
                    else {
                        break;
                    };
                    for cid in first..=last {
                        widths.insert(cid as u16, width);
                    }
                    i += 3;
                }
                None => break,
            }
        }
    }
    (dw, widths)
}

/// The CIDs the page draws state the widths their own program carries.
///
/// Before the fix: /W came back with the author's keys 65, 72, 78 and 84 --
/// codes, not CIDs -- so CIDs 19, 26, 32 and 38 fell back on /DW 1000 while
/// the program said 667, 722, 722 and 611. That is veraPDF 6.2.11.5:1, and it
/// is what `pdfjs__test_pdfs_issue3323` failed on master.
#[test]
fn a_code_keyed_array_under_a_real_cmap_is_derived_afresh() {
    let (mut doc, cid_id) = document_with(code_keyed_widths(), false);

    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);

    let (dw, widths) = widths_of(&doc, cid_id);
    for cid in [19u16, 26, 32, 38] {
        let stated = widths.get(&cid).copied().unwrap_or(dw);
        let program = i64::from(program_advance(gid_for_cid(cid)));
        assert_eq!(
            stated, program,
            "CID {cid} states {stated} where its own program says {program}; \
             /W is still keyed by the character code",
        );
    }
}

/// Under `Identity-H` a key is a code and a CID at once, so it is corrected.
///
/// This is the boundary the fix must not cross: the seven CJK documents of
/// #488 declare widths for CIDs their subset program has no glyph for, and
/// deriving a fresh array for them is what wrote /DW 0 over the author's 1000.
/// Nothing about them changed here -- the array keeps its keys, and its widths
/// are restated from the program.
#[test]
fn an_identity_encoding_keeps_the_keys_the_author_wrote() {
    // 84 is above `NUM_CIDS`, so the font cannot answer for it: under a real
    // CMap that is the signal, and under Identity-H it must not be.
    let (mut doc, cid_id) = document_with(code_keyed_widths(), true);

    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);

    let (dw, widths) = widths_of(&doc, cid_id);
    assert_eq!(dw, 1000, "the author's /DW was replaced by a derived one");
    let mut keys: Vec<u16> = widths.keys().copied().collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![65u16, 72, 78, 84],
        "the author's keys were replaced instead of corrected",
    );
    for cid in [19u16, 26, 32, 38] {
        let program = i64::from(program_advance(gid_for_cid(cid)));
        assert_eq!(
            widths.get(&cid).copied(),
            None,
            "CID {cid} gained an entry the author did not write (program says {program})",
        );
    }
}

/// The parent is read before the descendant, whatever their object numbers say.
///
/// `sync_widths_from_embedded_fonts` walks font dictionaries in object order
/// and reaches a CIDFont twice: once as the descendant of its Type0, once as a
/// font in its own right. The descendant here has the lower object number, as
/// it does in `issue3323`, so the second route ran first -- with no parent to
/// read, therefore no CMap, therefore the assumption that a code is a CID --
/// and marked the font done before the parent could say otherwise.
#[test]
fn the_encoding_is_read_from_the_parent_before_the_descendant_is_touched() {
    let (mut doc, cid_id) = document_with(code_keyed_widths(), false);
    let type0_id = doc
        .objects
        .iter()
        .find(|(_, obj)| match obj {
            Object::Dictionary(d) => {
                d.get(b"Subtype").ok() == Some(&Object::Name(b"Type0".to_vec()))
            }
            _ => false,
        })
        .map(|(id, _)| *id)
        .expect("the fixture has a Type0 font");
    assert!(
        cid_id.0 < type0_id.0,
        "the fixture stopped being the shape this test is about: \
         descendant {cid_id:?} no longer precedes its parent {type0_id:?}",
    );

    pdf_manip::pdfa_fonts::sync_widths_from_embedded_fonts(&mut doc);

    let (dw, widths) = widths_of(&doc, cid_id);
    for cid in [19u16, 26, 32, 38] {
        let stated = widths.get(&cid).copied().unwrap_or(dw);
        let program = i64::from(program_advance(gid_for_cid(cid)));
        assert_eq!(
            stated, program,
            "CID {cid} states {stated} where its own program says {program}; \
             the descendant was settled before its parent was read",
        );
    }
}

/// A reachable CID the program cannot draw is a fallback, not a wrong key.
///
/// The `.notdef` case Codex raised on the review of this branch: an author who
/// keys /W by CID may name a CID the encoding reaches and the subset program
/// has no glyph for. Nothing about that array is wrong, and deriving a fresh
/// one over it would throw away a width the author stated and /DW with it.
/// Only a key that is *neither* reachable through the encoding nor drawable by
/// the program says the array is keyed by something else.
#[test]
fn a_reachable_cid_without_a_glyph_keeps_the_width_its_author_wrote() {
    let author = vec![
        Object::Integer(19),
        Object::Array(vec![Object::Integer(667)]),
        Object::Integer(26),
        Object::Array(vec![Object::Integer(500)]),
        Object::Integer(200),
        Object::Array(vec![Object::Integer(480)]),
    ];
    let (mut doc, cid_id) = document_with(author, false);

    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);

    let (_, widths) = widths_of(&doc, cid_id);
    let mut keys: Vec<u16> = widths.keys().copied().collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![19u16, 26, 200],
        "a CID-keyed array was replaced instead of corrected",
    );
    assert_eq!(
        widths.get(&200).copied(),
        Some(480),
        "CID 200 is reachable through the CMap and has no glyph: its author's width stays",
    );
    assert_eq!(
        widths.get(&26).copied(),
        Some(i64::from(program_advance(gid_for_cid(26)))),
        "CID 26 was not corrected from the program it does have",
    );
}
