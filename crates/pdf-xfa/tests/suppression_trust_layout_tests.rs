//! `XFA_SUPPRESSION_TRUST_LAYOUT` (default-off) regression tests.
//!
//! The flag relaxes the §4.3 data-empty page-drop heuristic so that suppression
//! trusts the layout page count: a laid-out page that still renders visible
//! content (field box, static draw/text, image) is kept even when its fields
//! carry no bound value, rather than being dropped.
//!
//! Verifies: (1) default OFF still drops a data-empty continuation page
//! (byte-identical legacy behavior), (2) `XFA_SUPPRESSION_TRUST_LAYOUT=1` keeps
//! that page. All env manipulation lives in a SINGLE test function so the
//! parallel test runner cannot race on the process-global env var.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_xfa::flatten_xfa_to_pdf;

fn build_xfa_pdf(xdp: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");
    let xdp_bytes = xdp.as_bytes().to_vec();
    let xfa_stream = Stream::new(
        dictionary! { "Length" => Object::Integer(xdp_bytes.len() as i64) },
        xdp_bytes,
    );
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length" => Object::Integer(0_i64) },
        vec![],
    )));
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
    doc.save_to(&mut out).expect("save xfa pdf");
    out
}

fn flatten_page_count(pdf: &[u8]) -> usize {
    let output = flatten_xfa_to_pdf(pdf).expect("flatten_xfa_to_pdf failed");
    Document::load_mem(&output)
        .expect("reload flattened PDF")
        .get_pages()
        .len()
}

/// Two-page form: page 1 has a text field with a value; page 2 is forced onto a
/// new pageArea and carries a single data-bearing text field with NO value plus
/// a static draw label. Page 2 is "data-empty" (a field with no bound value) but
/// renders visible content — the case the trust-layout flag is designed to keep.
const XDP_DATA_EMPTY_CONTINUATION: &str = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1" layout="paginate" w="8.5in" h="11in">
      <pageSet>
        <pageArea name="Page1">
          <contentArea x="36pt" y="36pt" w="540pt" h="720pt"/>
          <medium stock="default" short="612pt" long="792pt"/>
        </pageArea>
        <pageArea name="Page2">
          <contentArea x="36pt" y="36pt" w="540pt" h="720pt"/>
          <medium stock="default" short="612pt" long="792pt"/>
        </pageArea>
      </pageSet>
      <subform name="page1Section" layout="tb" w="540pt">
        <field name="CompanyName" w="300pt" h="18pt">
          <ui><textEdit/></ui>
          <value><text>ACME Corporation</text></value>
        </field>
      </subform>
      <subform name="page2Section" layout="tb" w="540pt">
        <breakBefore targetType="pageArea"/>
        <draw name="Page2Header" w="540pt" h="20pt">
          <value><text>Continuation</text></value>
        </draw>
        <field name="EmptyDetail" w="300pt" h="18pt">
          <ui><textEdit/></ui>
        </field>
      </subform>
    </subform>
  </template>
</xdp:xdp>"#;

#[test]
fn trust_layout_flag_keeps_data_empty_visible_page_only_when_enabled() {
    let pdf = build_xfa_pdf(XDP_DATA_EMPTY_CONTINUATION);

    let prev = std::env::var("XFA_SUPPRESSION_TRUST_LAYOUT").ok();

    // Flag OFF (default): the data-empty continuation page is dropped.
    std::env::remove_var("XFA_SUPPRESSION_TRUST_LAYOUT");
    let off_pages = flatten_page_count(&pdf);
    assert_eq!(
        off_pages, 1,
        "default-off: data-empty continuation page must be dropped (got {off_pages})"
    );

    // Flag ON: suppression trusts the layout — the visible-but-data-empty
    // page is kept.
    std::env::set_var("XFA_SUPPRESSION_TRUST_LAYOUT", "1");
    let on_pages = flatten_page_count(&pdf);
    assert_eq!(
        on_pages, 2,
        "trust-layout: visible data-empty page must be kept (got {on_pages})"
    );

    // Restore prior env so we never leak state to other test binaries.
    match prev {
        Some(v) => std::env::set_var("XFA_SUPPRESSION_TRUST_LAYOUT", v),
        None => std::env::remove_var("XFA_SUPPRESSION_TRUST_LAYOUT"),
    }
}
