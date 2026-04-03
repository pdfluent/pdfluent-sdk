use lopdf::{dictionary, Document, Object, Stream};
use xfa_wasm::PdfDoc;

const SIMPLE_XDP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="section" layout="tb" w="7.5in">
      <field name="firstName" w="3.5in" h="0.3in">
        <caption><value><text>First Name</text></value></caption>
        <ui><textEdit/></ui>
        <value><text>John</text></value>
      </field>
    </subform>
  </subform>
</template>
</xdp:xdp>"#;

const PLACEHOLDER_STREAM: &str = r#"BT
/Helv 24 Tf
72 720 Td
(Please wait...) Tj
0 -32 Td
(If this message is not eventually replaced by the proper contents of the document,) Tj
0 -32 Td
(your PDF viewer may not be able to display this type of document.) Tj
0 -32 Td
(You can upgrade to the latest version of Adobe Reader by visiting reader_download.) Tj
ET
"#;

fn build_xfa_pdf_with_content(xdp: &str, page_content: Vec<u8>) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");
    let xdp_bytes = xdp.as_bytes().to_vec();
    let xfa_stream = Stream::new(
        dictionary! { "Length" => Object::Integer(xdp_bytes.len() as i64) },
        xdp_bytes,
    );
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));
    let pages_id = doc.new_object_id();
    let content_stream = Stream::new(
        dictionary! { "Length" => Object::Integer(page_content.len() as i64) },
        page_content,
    );
    let content_id = doc.add_object(Object::Stream(content_stream));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type"     => Object::Name(b"Page".to_vec()),
        "Parent"   => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(content_id)
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type"  => Object::Name(b"Pages".to_vec()),
            "Kids"  => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1)
        }),
    );
    let acroform_id = doc.add_object(Object::Dictionary(dictionary! {
        "XFA"    => Object::Reference(xfa_id),
        "Fields" => Object::Array(vec![])
    }));
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type"     => Object::Name(b"Catalog".to_vec()),
        "Pages"    => Object::Reference(pages_id),
        "AcroForm" => Object::Reference(acroform_id)
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save XFA render fixture");
    out
}

#[test]
fn render_page_matches_explicitly_flattened_xfa_output() {
    let xfa_pdf = build_xfa_pdf_with_content(SIMPLE_XDP, PLACEHOLDER_STREAM.as_bytes().to_vec());
    let doc = PdfDoc::open(&xfa_pdf).expect("open XFA PDF");

    let rendered = doc.render_page(0, 1.0).expect("render XFA page");
    let flattened = doc.flatten_xfa().expect("flatten XFA PDF");
    let flattened_doc = PdfDoc::open(&flattened).expect("open flattened PDF");
    let expected = flattened_doc
        .render_page(0, 1.0)
        .expect("render flattened page");

    assert_eq!(
        rendered, expected,
        "renderPage should rasterize the flattened XFA content rather than the AcroForm placeholder"
    );
}
