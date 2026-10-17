// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! What a predefined CMap says about a code outlives the font it named.
//!
//! Adobe's predefined CMaps are public tables: a PDF names one, and the reader
//! supplies it. Two things then happen during PDF/A conversion that the
//! document did not ask for, and both destroyed text that was perfectly
//! readable in the source (#486, class B of the #381 tail).
//!
//! **The program is replaced and the CMap is not.** Acrobat's "searchable
//! image" writes its invisible OCR layer in an unembedded `CIDFontType0` under
//! `/Encoding /UniJIS-UCS2-H` with CIDSystemInfo Adobe-Japan1, and the text is
//! Latin. Embedding has to substitute a program, and no rule relates an
//! Adobe-Japan1 CID to a glyph in a Latin substitute -- so every code on the
//! page read as `.notdef` and was rewritten to the single value that could be
//! proved valid. 7974 codes of `0679931` became `<00CB>`; the file extracted
//! as 7741 replacement characters. Word retention over eight measured
//! documents: 0.9 % to 27.9 %.
//!
//! **A code we cannot resolve is read as if we could.** `90ms-RKSJ-H` writes
//! Shift-JIS, one *or two* bytes per character. Reading those bytes as CID
//! pairs invented a `/ToUnicode` that a reader believes over the CMap it has
//! itself: `7882852`'s "夏期講習会申込書" came out as "W OR U @", and the
//! `.notdef` repair rewrote or deleted 779 codes that were never `.notdef`.
//!
//! Both tests below assert the text that comes back out. Neither asserts
//! conformance on its own, because the pre-fix output of the first one was
//! non-conformant *and* empty, and of the second conformant *and* wrong;
//! veraPDF is not the instrument that sees this.

use lopdf::{dictionary, Document, Object, Stream};

/// A CID-keyed CFF with two glyphs, from `cff-parser`'s own fixture: GID 0 ->
/// CID 0 (width 500, the Private DICT default) and GID 1 -> CID 1 (width 650).
#[rustfmt::skip]
const TWO_GLYPH_CID_CFF: &[u8] = &[
    0x01, 0x00, 0x04, 0x01,
    0x00, 0x01, 0x01, 0x01, 0x02, 0x46,
    0x00, 0x01, 0x01, 0x01, 0x11,
    0xCD, 0xF7, 0x78, 0x8B, 0x0C, 0x1E,
    0xAE, 0x0F,
    0xB1, 0x0C, 0x25,
    0xBE, 0x0C, 0x24,
    0xB4, 0x11,
    0x00, 0x00,
    0x00, 0x00,
    0x00, 0x00, 0x01,
    0x00, 0x00, 0x00,
    0x00, 0x02, 0x01, 0x01, 0x02, 0x05,
    0x0E,
    0xF9, 0x1E, 0x0E,
    0x00, 0x01, 0x01, 0x01, 0x04,
    0x90, 0xC6, 0x12,
    0xF8, 0x88, 0x14,
    0x8B, 0x15,
];

/// Wrap a page's operators in the one-page document the converter expects.
fn document_with(content: Vec<u8>, font_id: ObjectIdSource) -> Vec<u8> {
    let ObjectIdSource { mut doc, font_id } = font_id;
    let pages_id = doc.new_object_id();
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
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save fixture");
    out
}

/// A part-built document plus the font resource `/F1` resolves to.
struct ObjectIdSource {
    doc: Document,
    font_id: lopdf::ObjectId,
}

/// The shape Acrobat's OCR layer has: a Type0 over `/UniJIS-UCS2-H` whose
/// Adobe-Japan1 descendant carries no font program at all.
fn unembedded_ucs2_ocr_font() -> ObjectIdSource {
    let mut doc = Document::with_version("1.7");
    let fd_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "KozGoPro-Medium-Acro",
        "Flags" => Object::Integer(4),
        "Ascent" => Object::Integer(880),
        "Descent" => Object::Integer(-120),
        "CapHeight" => Object::Integer(763),
        "ItalicAngle" => Object::Integer(0),
        "StemV" => Object::Integer(99),
        "FontBBox" => Object::Array(vec![
            Object::Integer(-149), Object::Integer(-374),
            Object::Integer(1254), Object::Integer(1008),
        ]),
    }));
    let descendant = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType0",
        "BaseFont" => "KozGoPro-Medium-Acro",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Japan1"),
            "Supplement" => Object::Integer(4),
        },
        "FontDescriptor" => Object::Reference(fd_id),
        "DW" => Object::Integer(1000),
    }));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "KozGoPro-Medium-Acro-UniJIS-UCS2-H",
        "Encoding" => Object::Name(b"UniJIS-UCS2-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant)]),
    }));
    ObjectIdSource { doc, font_id }
}

/// A Type0 over `90ms-RKSJ-H` whose descendant program *is* embedded, so
/// nothing about it needs repairing: the CMap the file names is the only thing
/// we cannot read.
fn embedded_shift_jis_font() -> ObjectIdSource {
    let mut doc = Document::with_version("1.7");
    let font_file = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Subtype" => "CIDFontType0C" },
        TWO_GLYPH_CID_CFF.to_vec(),
    )));
    let fd_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "AAAAAA+HGHeiseiMinchotaiW3",
        "Flags" => Object::Integer(6),
        "Ascent" => Object::Integer(859),
        "Descent" => Object::Integer(-137),
        "CapHeight" => Object::Integer(753),
        "ItalicAngle" => Object::Integer(0),
        "StemV" => Object::Integer(68),
        "FontBBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(-137),
            Object::Integer(1000), Object::Integer(859),
        ]),
        "FontFile3" => Object::Reference(font_file),
    }));
    let descendant = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType0",
        "BaseFont" => "AAAAAA+HGHeiseiMinchotaiW3",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Japan1"),
            "Supplement" => Object::Integer(2),
        },
        "FontDescriptor" => Object::Reference(fd_id),
        "DW" => Object::Integer(1000),
    }));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "AAAAAA+HGHeiseiMinchotaiW3",
        "Encoding" => Object::Name(b"90ms-RKSJ-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant)]),
    }));
    ObjectIdSource { doc, font_id }
}

/// `Tr 3` -- the invisible rendering mode an OCR layer uses -- and one hex
/// string, so what comes back out can only have come from these codes.
fn invisible_text(codes: &[u8]) -> Vec<u8> {
    let mut content = b"BT 3 Tr /F1 12 Tf 72 700 Td <".to_vec();
    for byte in codes {
        content.extend_from_slice(format!("{byte:02X}").as_bytes());
    }
    content.extend_from_slice(b"> Tj ET");
    content
}

fn convert(source: &[u8]) -> Vec<u8> {
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        ..Default::default()
    };
    pdf_manip::pdfa::convert_bytes(source, &opts).expect("PDF/A conversion")
}

/// The hex strings the one page draws, concatenated.
fn drawn_bytes(pdf: &[u8]) -> Vec<u8> {
    let doc = Document::load_mem(pdf).expect("load converted output");
    let page_id = *doc.get_pages().values().next().expect("one page");
    let mut out = Vec::new();
    for id in pdf_manip::content_editor::get_content_stream_ids(&doc, page_id) {
        let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
            continue;
        };
        let mut stream = stream.clone();
        let _ = stream.decompress();
        let ops = lopdf::content::Content::decode(&stream.content).expect("decode content");
        for op in ops.operations {
            if op.operator == "Tj" {
                if let Some(Object::String(bytes, _)) = op.operands.first() {
                    out.extend_from_slice(bytes);
                }
            }
        }
    }
    out
}

/// The character codes of the one page, as the two-byte values they are.
fn drawn_codes(pdf: &[u8]) -> Vec<u16> {
    drawn_bytes(pdf)
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect()
}

/// The UCS-2 codes of "Rotax", which is what the Latin OCR layer of
/// `0679931` actually draws through its Adobe-Japan1 font.
const ROTAX_UCS2: &[u8] = &[0x00, 0x52, 0x00, 0x6F, 0x00, 0x74, 0x00, 0x61, 0x00, 0x78];

#[test]
fn a_substituted_ucs2_ocr_layer_still_reads_as_its_own_words() {
    let source = document_with(invisible_text(ROTAX_UCS2), unembedded_ucs2_ocr_font());
    let converted = convert(&source);

    let doc = pdf_engine::PdfDocument::open(converted.clone()).expect("reopen converted output");
    let text = doc.extract_text(0).expect("extract text");
    assert!(
        text.contains("Rotax"),
        "the OCR layer's own word has to survive the substitution, got {text:?}"
    );

    // The defect's signature, stated separately from the reader's opinion: the
    // repair used to write one value over the whole font.
    let codes = drawn_codes(&converted);
    assert_eq!(codes.len(), 5, "five characters were drawn: {codes:?}");
    assert!(
        codes.iter().collect::<std::collections::HashSet<_>>().len() > 1,
        "every code was rewritten to one value: {codes:?}"
    );
}

#[test]
fn a_predefined_cmap_we_cannot_read_leaves_the_page_alone() {
    // Shift-JIS: 0x82 0xA0 is あ, 0x41 is a one-byte 'A'. Read as CID pairs
    // this is <82A0> followed by half of the next character -- which is how
    // the repair used to condemn codes that were never .notdef.
    let codes = &[0x82u8, 0xA0, 0x41, 0x82, 0xA2];
    let source = document_with(invisible_text(codes), embedded_shift_jis_font());
    let converted = convert(&source);

    assert_eq!(
        drawn_bytes(&converted),
        codes.to_vec(),
        "the bytes of a CMap we cannot resolve may not be rewritten"
    );

    let doc = Document::load_mem(&converted).expect("load converted output");
    let invented = doc.objects.values().any(|obj| {
        let Object::Dictionary(dict) = obj else {
            return false;
        };
        dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Type0")
            && dict.has(b"ToUnicode")
    });
    assert!(
        !invented,
        "a /ToUnicode read off the glyph indices contradicts the CMap the reader has"
    );
}
