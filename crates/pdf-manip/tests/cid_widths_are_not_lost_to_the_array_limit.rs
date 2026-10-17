// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! An embedded CID font keeps the widths its own program carries.
//!
//! The defect (#488, class G of #381). Japanese Word and PowerPoint output
//! converted with the content stream byte-identical, the fonts still embedded,
//! and the text broken anyway: `ネット予約の場合、予約を確認して` came out of
//! mutool as `・`, `ださ`, `い`, and veraPDF failed 6.2.11.5 -- width in the
//! dictionary against width in the program -- 94 times on `3768063`, 649 on
//! `4225004`, 266 on `5067962`. Seven documents, word retention 42-92 % with
//! character retention 1.000: every glyph still on the page, every word cut in
//! a different place.
//!
//! Two of our own steps did it, in this order.
//!
//!   1. The width pass threw the author's /W away and derived a fresh one from
//!      every glyph in the program -- 25,059 of them on `3768063`, most with a
//!      zero advance because the subsetter kept their `hmtx` entries. /DW
//!      became the most common of those widths, which is 0, where the author
//!      had written 1000.
//!   2. `fix_long_containers` then enforced ISO 19005-2 6.1.13's limit of 8191
//!      array entries by truncating that array. Every CID above 8190 -- which
//!      on a CJK subset is most of the text -- silently fell back on /DW.
//!
//! So the dictionary said 0 where the program said 1000, and a reader that
//! believes the dictionary lays the glyphs on top of each other.
//!
//! What this test pins down is the rule that replaced it: a dictionary that
//! already spells out its widths is corrected in place and never replaced, and
//! an array we do write is short enough to survive 6.1.13.

use lopdf::{dictionary, Document, Object, Stream};

/// Glyphs in the fixture program -- past 8191, which is the whole point.
const NUM_GLYPHS: u16 = 9000;

/// The advance the fixture program carries for a glyph, in font units.
///
/// `units_per_em` is 1000, so these are also the widths a PDF dictionary must
/// state. Most glyphs are zero-advance, exactly as a subsetter leaves them.
fn program_advance(gid: u16) -> u16 {
    match gid {
        0 => 1000,
        40 => 500,
        8500 => 1000,
        8501 => 750,
        _ => 0,
    }
}

/// A TrueType program with `NUM_GLYPHS` glyphs and nothing but metrics.
///
/// `head`, `hhea`, `maxp` and `hmtx` are all a width pass reads, and all
/// `ttf_parser` needs to answer `number_of_glyphs` and `glyph_hor_advance`.
/// Leaving `glyf` out keeps the fixture readable; no assertion here depends on
/// an outline.
fn metrics_only_truetype(advance: impl Fn(u16) -> u16) -> Vec<u8> {
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
        hmtx.extend_from_slice(&advance(gid).to_be_bytes());
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

/// One page drawing three CIDs through an embedded Identity-H CID font.
///
/// `w` is the /W array as the author wrote it; `None` leaves the key out
/// altogether, which is the other half of the rule.
fn document_with(w: Option<Vec<Object>>, program: Vec<u8>) -> (Document, lopdf::ObjectId) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let program_len = program.len() as i64;
    let font_file = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length1" => Object::Integer(program_len) },
        program,
    )));
    let fd_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "BCDEEE+Meiryo-Bold",
        "Flags" => Object::Integer(4),
        "FontFile2" => Object::Reference(font_file),
    }));
    let mut descendant = dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => "BCDEEE+Meiryo-Bold",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => Object::Integer(0),
        },
        "FontDescriptor" => Object::Reference(fd_id),
        "CIDToGIDMap" => Object::Name(b"Identity".to_vec()),
        "DW" => Object::Integer(1000),
    };
    if let Some(w) = w {
        descendant.set("W", Object::Array(w));
    }
    let descendant_id = doc.add_object(Object::Dictionary(descendant));

    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "BCDEEE+Meiryo-Bold",
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant_id)]),
    }));

    let content = b"BT /F1 12 Tf 72 700 Td <002821342135> Tj ET".to_vec();
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

/// The longest inner array in /W. ISO 19005-2 6.1.13 allows 8191.
fn longest_array(doc: &Document, cid_id: lopdf::ObjectId) -> usize {
    let Some(Object::Dictionary(cid)) = doc.objects.get(&cid_id) else {
        return 0;
    };
    match cid.get(b"W") {
        Ok(Object::Array(w)) => w
            .iter()
            .filter_map(|o| match o {
                Object::Array(inner) => Some(inner.len()),
                _ => None,
            })
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

/// The author wrote the widths of the CIDs the page draws. They stay.
///
/// Before the fix: /DW went 1000 -> 0 (the most common advance in a program
/// whose unused glyphs carry none), /W became one entry per glyph, and 6.1.13
/// cut it at CID 8190 -- so CID 8500 was 1000 in the program and 0 in the
/// dictionary. That is 6.2.11.5, 94 times over on `3768063`.
#[test]
fn an_agreeing_dictionary_keeps_its_widths_and_its_default() {
    let author = vec![
        Object::Integer(0),
        Object::Array(vec![Object::Integer(1000)]),
        Object::Integer(40),
        Object::Array(vec![Object::Integer(500)]),
        Object::Integer(8500),
        Object::Array(vec![Object::Integer(1000), Object::Integer(750)]),
    ];
    let (mut doc, cid_id) = document_with(Some(author), metrics_only_truetype(program_advance));

    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);

    let (dw, widths) = widths_of(&doc, cid_id);
    assert_eq!(dw, 1000, "the author's /DW was replaced by a derived one");
    for cid in [0u16, 40, 8500, 8501] {
        assert_eq!(
            widths.get(&cid).copied(),
            Some(i64::from(program_advance(cid))),
            "CID {cid} lost the width its own program carries; /W = {widths:?}",
        );
    }
    assert!(
        longest_array(&doc, cid_id) <= 8191,
        "/W carries an array 6.1.13 will truncate, which is how the widths went missing",
    );
}

/// With no /W to correct, one is derived -- in entries short enough to keep.
///
/// This is the second half: the array limit is not a reason to leave a font
/// without widths, it is a reason to write more than one entry.
#[test]
fn a_derived_width_array_survives_the_array_limit() {
    // Every third glyph is wider, so the run of non-default widths spans the
    // whole program and has to be cut into entries somewhere.
    let advance = |gid: u16| if gid.is_multiple_of(3) { 1000 } else { 500 };
    let (mut doc, cid_id) = document_with(None, metrics_only_truetype(advance));

    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);

    assert!(
        longest_array(&doc, cid_id) <= 8191,
        "the derived /W carries an array 6.1.13 will truncate",
    );
    let (dw, widths) = widths_of(&doc, cid_id);
    for cid in [8190u16, 8191, 8192, 8997, 8999] {
        let stated = widths.get(&cid).copied().unwrap_or(dw);
        assert_eq!(
            stated,
            i64::from(advance(cid)),
            "CID {cid} states {stated} where its program says {}",
            advance(cid),
        );
    }
}
