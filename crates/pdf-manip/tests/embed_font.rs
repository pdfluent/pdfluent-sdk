// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! `embed_font` has to leave a font a viewer can actually use.
//!
//! The failure this guards is not a crash. `lopdf::Document::add_font` on its
//! own returns an object id and writes no `/Widths` and no page resource entry,
//! so wiring the SDK straight to it produces an `Ok(())` and a document where
//! the font is unreachable and unmeasurable. Every assertion below is one half
//! of that.

use lopdf::{dictionary, Document, Object};
use pdf_manip::embed_font::{embed_font, EmbedFontError};

/// A TrueType font built byte by byte, with a `cmap` and known advances.
///
/// Generated rather than committed: this repository ships no font files, which
/// is exactly why the one upstream test for this feature carries `#[ignore]`.
/// `A` is 600 units wide and `B` is 1200 at 2048 units per em, so in glyph
/// space they must come out as 293 and 586 -- values that appear nowhere else,
/// so a width read from the wrong glyph is visible in the assertion.
mod synthetic {
    fn be16(o: &mut Vec<u8>, v: u16) {
        o.extend_from_slice(&v.to_be_bytes());
    }
    fn be32(o: &mut Vec<u8>, v: u32) {
        o.extend_from_slice(&v.to_be_bytes());
    }

    pub const UNITS_PER_EM: u16 = 2048;
    pub const ADVANCE_A: u16 = 600;
    pub const ADVANCE_B: u16 = 1200;
    /// Glyph 0 (.notdef) and the two mapped glyphs.
    const NUM_GLYPHS: u16 = 3;

    fn head() -> Vec<u8> {
        let mut t = Vec::new();
        be16(&mut t, 1);
        be16(&mut t, 0);
        be32(&mut t, 0x0001_0000);
        be32(&mut t, 0);
        be32(&mut t, 0x5F0F_3CF5);
        be16(&mut t, 0);
        be16(&mut t, UNITS_PER_EM);
        t.extend_from_slice(&0i64.to_be_bytes());
        t.extend_from_slice(&0i64.to_be_bytes());
        for v in [-100i16, -200, 1500, 1800, 0, 8, 2, 0, 0] {
            t.extend_from_slice(&v.to_be_bytes());
        }
        debug_assert_eq!(t.len(), 54);
        t
    }

    fn hhea() -> Vec<u8> {
        let mut t = Vec::new();
        be16(&mut t, 1);
        be16(&mut t, 0);
        for v in [1600i16, -400, 0] {
            t.extend_from_slice(&v.to_be_bytes());
        }
        be16(&mut t, ADVANCE_B);
        for v in [0i16, 0, 1500, 1, 0, 0, 0, 0, 0, 0, 0] {
            t.extend_from_slice(&v.to_be_bytes());
        }
        // numberOfHMetrics: one entry per glyph, so each has its own advance.
        be16(&mut t, NUM_GLYPHS);
        debug_assert_eq!(t.len(), 36);
        t
    }

    fn maxp() -> Vec<u8> {
        let mut t = Vec::new();
        be32(&mut t, 0x0000_5000);
        be16(&mut t, NUM_GLYPHS);
        t
    }

    fn hmtx() -> Vec<u8> {
        let mut t = Vec::new();
        for advance in [0, ADVANCE_A, ADVANCE_B] {
            be16(&mut t, advance);
            be16(&mut t, 0); // leftSideBearing
        }
        t
    }

    /// A format 4 cmap mapping 'A' to glyph 1 and 'B' to glyph 2.
    fn cmap() -> Vec<u8> {
        let mut sub = Vec::new();
        be16(&mut sub, 4);
        be16(&mut sub, 32); // length, filled below
        be16(&mut sub, 0); // language
        be16(&mut sub, 4); // segCountX2 -> 2 segments
        be16(&mut sub, 4); // searchRange
        be16(&mut sub, 1); // entrySelector
        be16(&mut sub, 0); // rangeShift
                           // endCode: 'B', then the required 0xFFFF terminator.
        be16(&mut sub, u16::from(b'B'));
        be16(&mut sub, 0xFFFF);
        be16(&mut sub, 0); // reservedPad
        be16(&mut sub, u16::from(b'A'));
        be16(&mut sub, 0xFFFF);
        // idDelta: 'A' is 0x41 and maps to glyph 1, so the delta is 1 - 0x41.
        be16(&mut sub, (1i32 - i32::from(b'A')) as u16);
        be16(&mut sub, 1);
        be16(&mut sub, 0); // idRangeOffset
        be16(&mut sub, 0);
        let len = sub.len() as u16;
        sub[2..4].copy_from_slice(&len.to_be_bytes());

        let mut t = Vec::new();
        be16(&mut t, 0); // version
        be16(&mut t, 1); // numTables
        be16(&mut t, 3); // platformID: Windows
        be16(&mut t, 1); // encodingID: Unicode BMP
        be32(&mut t, 12); // offset to the subtable
        t.extend_from_slice(&sub);
        t
    }

    pub fn font() -> Vec<u8> {
        let tables: [(&[u8; 4], Vec<u8>); 5] = [
            (b"cmap", cmap()),
            (b"head", head()),
            (b"hhea", hhea()),
            (b"hmtx", hmtx()),
            (b"maxp", maxp()),
        ];
        let n = tables.len() as u16;
        let mut out = Vec::new();
        be32(&mut out, 0x0001_0000);
        be16(&mut out, n);
        let entry_selector = (u16::BITS - 1 - n.leading_zeros()) as u16;
        let search_range = (1u16 << entry_selector) * 16;
        be16(&mut out, search_range);
        be16(&mut out, entry_selector);
        be16(&mut out, n * 16 - search_range);

        let mut offset = 12 + 16 * tables.len() as u32;
        let mut body = Vec::new();
        for (tag, data) in &tables {
            out.extend_from_slice(*tag);
            be32(&mut out, 0);
            be32(&mut out, offset);
            be32(&mut out, data.len() as u32);
            offset += data.len() as u32;
            body.extend_from_slice(data);
        }
        out.extend_from_slice(&body);
        out
    }
}

/// A two-page document, so "registered on every page" can fail by registering
/// on only the first.
fn two_page_doc() -> Document {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_ids: Vec<_> = (0..2)
        .map(|_| {
            doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
            })
        })
        .collect();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
            "Count" => page_ids.len() as i64,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

fn font_dict(doc: &Document, id: lopdf::ObjectId) -> lopdf::Dictionary {
    doc.get_object(id).unwrap().as_dict().unwrap().clone()
}

#[test]
fn the_synthetic_font_parses_at_all() {
    assert!(
        ttf_parser::Face::parse(&synthetic::font(), 0).is_ok(),
        "the hand-built font is not a font, so every test below tests the builder"
    );
}

#[test]
fn a_simple_font_carries_the_widths_a_viewer_needs() {
    let mut doc = two_page_doc();
    let embedded = embed_font(&mut doc, &synthetic::font(), "Synth").unwrap();
    let dict = font_dict(&doc, embedded.font_id);

    assert_eq!(dict.get(b"FirstChar").unwrap(), &Object::Integer(32));
    assert_eq!(dict.get(b"LastChar").unwrap(), &Object::Integer(255));

    let widths = dict.get(b"Widths").unwrap().as_array().unwrap();
    // One entry per code from FirstChar to LastChar inclusive. A count that is
    // off by one shifts every width onto the wrong character.
    assert_eq!(widths.len(), 255 - 32 + 1);

    // 600 and 1200 units at 2048 per em, scaled into PDF's 1000-unit glyph space.
    let expect = |c: u8, units: u16| {
        let want = (f64::from(units) * 1000.0 / f64::from(synthetic::UNITS_PER_EM)).round() as i64;
        assert_eq!(
            widths[usize::from(c) - 32],
            Object::Integer(want),
            "width of {:?}",
            char::from(c)
        );
    };
    expect(b'A', synthetic::ADVANCE_A);
    expect(b'B', synthetic::ADVANCE_B);
    // A code the cmap does not map gets zero rather than being skipped.
    assert_eq!(widths[usize::from(b'Z') - 32], Object::Integer(0));
}

#[test]
fn the_font_is_named_on_every_page_not_just_the_first() {
    let mut doc = two_page_doc();
    let embedded = embed_font(&mut doc, &synthetic::font(), "Synth").unwrap();
    assert_eq!(embedded.pages_registered, 2);

    for (_, page_id) in doc.get_pages() {
        let (resources, _) = doc.get_page_resources(page_id).unwrap();
        let fonts = resources
            .expect("resources")
            .get(b"Font")
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(
            fonts.get(embedded.resource_name.as_bytes()).unwrap(),
            &Object::Reference(embedded.font_id),
            "page {page_id:?} does not name the font"
        );
    }
}

#[test]
fn the_font_programme_itself_is_embedded() {
    let mut doc = two_page_doc();
    let bytes = synthetic::font();
    let embedded = embed_font(&mut doc, &bytes, "Synth").unwrap();

    let descriptor_id = font_dict(&doc, embedded.font_id)
        .get(b"FontDescriptor")
        .unwrap()
        .as_reference()
        .unwrap();
    let file_id = font_dict(&doc, descriptor_id)
        .get(b"FontFile2")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = doc.get_object(file_id).unwrap().as_stream().unwrap();
    assert_eq!(stream.content, bytes, "the embedded bytes are not the font");
}

#[test]
fn a_name_another_font_already_uses_is_not_overwritten() {
    let mut doc = two_page_doc();
    let first = embed_font(&mut doc, &synthetic::font(), "Synth").unwrap();
    let second = embed_font(&mut doc, &synthetic::font(), "Synth").unwrap();

    assert_eq!(first.resource_name, "Synth");
    assert_ne!(
        second.resource_name, first.resource_name,
        "the second font took the first one's name, so content using the first now renders the second"
    );

    let (resources, _) = doc.get_page_resources(doc.get_pages()[&1]).unwrap();
    let fonts = resources.unwrap().get(b"Font").unwrap().as_dict().unwrap();
    assert_eq!(
        fonts.get(b"Synth").unwrap(),
        &Object::Reference(first.font_id)
    );
}

#[test]
fn data_that_is_not_a_font_is_refused_rather_than_embedded() {
    let mut doc = two_page_doc();
    let err = embed_font(&mut doc, b"not a font at all", "Synth")
        .expect_err("bytes that are not a font must not embed");
    assert_eq!(err, EmbedFontError::NotAFont);
}
