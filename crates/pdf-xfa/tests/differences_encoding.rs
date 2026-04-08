use lopdf::{dictionary, Document, Object, Stream};
use pdf_xfa::flatten::extract_embedded_fonts;
use pdf_xfa::font_bridge::{PdfBaseEncoding, PdfSimpleEncoding, ResolvedFont};

fn make_simple_font_doc_with_differences() -> Document {
    let mut doc = Document::new();

    let font_file_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        vec![0_u8, 1, 2, 3],
    )));

    let descriptor_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(b"Arial".to_vec()),
        "FontFile2" => Object::Reference(font_file_id),
    }));

    let encoding = Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Encoding".to_vec()),
        "BaseEncoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        "Differences" => Object::Array(vec![
            Object::Integer(24),
            Object::Name(b"breve".to_vec()),
            Object::Name(b"caron".to_vec()),
            Object::Name(b"circumflex".to_vec()),
        ]),
    });

    doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"TrueType".to_vec()),
        "BaseFont" => Object::Name(b"Arial".to_vec()),
        "FirstChar" => Object::Integer(24),
        "LastChar" => Object::Integer(65),
        "Widths" => Object::Array((0..42).map(|_| Object::Integer(500)).collect()),
        "Encoding" => encoding,
        "FontDescriptor" => Object::Reference(descriptor_id),
    }));

    doc
}

#[test]
fn extract_embedded_fonts_parses_differences_encoding() {
    let doc = make_simple_font_doc_with_differences();
    let fonts = extract_embedded_fonts(&doc);
    let font = fonts
        .iter()
        .find(|font| font.name == "Arial")
        .expect("expected extracted Arial font");

    assert_eq!(
        font.pdf_encoding,
        Some(PdfSimpleEncoding {
            base_encoding: PdfBaseEncoding::WinAnsi,
            differences: vec![(24, 0x02D8), (25, 0x02C7), (26, 0x02C6)],
        })
    );
}

#[test]
fn pdf_glyph_widths_projects_differences_to_unicode_slots() {
    let mut pdf_widths = vec![0_u16; 42];
    pdf_widths[0] = 400; // code 24 -> breve
    pdf_widths[1] = 410; // code 25 -> caron
    pdf_widths[2] = 420; // code 26 -> circumflex
    pdf_widths[41] = 700; // code 65 -> 'A' via WinAnsi base encoding

    let font = ResolvedFont {
        name: "TestFont".to_string(),
        data: Vec::new(),
        face_index: 0,
        units_per_em: 1000,
        ascender: 800,
        descender: -200,
        pdf_widths: Some((24, pdf_widths)),
        pdf_encoding: Some(PdfSimpleEncoding {
            base_encoding: PdfBaseEncoding::WinAnsi,
            differences: vec![(24, 0x02D8), (25, 0x02C7), (26, 0x02C6)],
        }),
    };

    let (_first_char, widths) = font.pdf_glyph_widths();

    assert_eq!(widths[0x02D8], 400);
    assert_eq!(widths[0x02C7], 410);
    assert_eq!(widths[0x02C6], 420);
    assert_eq!(widths['A' as usize], 700);
    assert_eq!(widths[24], 0);
}
