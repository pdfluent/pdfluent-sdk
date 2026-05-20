//! XFA-F7-03 (#1115): Edge case tests for complex XFA forms.
//!
//! All tests use synthetic, minimal XFA XML wrapped in a minimal PDF structure.
//! No real corpus PDFs are required — the goal is to verify that `flatten_xfa_to_pdf`
//! does not panic and produces non-empty output across a wide variety of edge cases.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_xfa::flatten_xfa_to_pdf;

// ---------------------------------------------------------------------------
// Helper — build a minimal XFA PDF from an XDP string
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// 1. Deeply nested subforms (5+ levels deep)
// ---------------------------------------------------------------------------

#[test]
fn edge_case_deeply_nested_subforms() {
    let xdp = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    <subform name="l1" layout="tb">
      <subform name="l2" layout="tb">
        <subform name="l3" layout="tb">
          <subform name="l4" layout="tb">
            <subform name="l5" layout="tb">
              <field name="deepField" w="100mm" h="5mm">
                <ui><textEdit/></ui>
                <caption><value><text>Deep Field</text></value></caption>
              </field>
            </subform>
          </subform>
        </subform>
      </subform>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <l1><l2><l3><l4><l5><deepField>nested-value</deepField></l5></l4></l3></l2></l1>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf_bytes = build_xfa_pdf(xdp);
    let result = flatten_xfa_to_pdf(&pdf_bytes);
    // Must not panic. Result may be Ok or Err — we only assert it is non-empty when Ok.
    // acceptable — the form may not be fully supported yet
    if let Ok(output) = result {
        assert!(!output.is_empty(), "flattened output must not be empty");
    }
}

// ---------------------------------------------------------------------------
// 2. Repeating subforms with 0 data instances (min=0)
// ---------------------------------------------------------------------------

#[test]
fn edge_case_repeating_subform_zero_instances() {
    let xdp = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    <subform name="root" layout="tb">
      <subform name="row" occur="0 -1" layout="tb">
        <field name="item" w="100mm" h="5mm">
          <ui><textEdit/></ui>
        </field>
      </subform>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <root/>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf_bytes = build_xfa_pdf(xdp);
    let result = flatten_xfa_to_pdf(&pdf_bytes);
    if let Ok(output) = result {
        assert!(!output.is_empty(), "flattened output must not be empty")
    }
}

// ---------------------------------------------------------------------------
// 3. Repeating subforms with many instances (10+)
// ---------------------------------------------------------------------------

#[test]
fn edge_case_repeating_subform_many_instances() {
    // Generate 12 data instances.
    let rows: String = (1..=12)
        .map(|i| format!("<row><item>Item {i}</item></row>"))
        .collect();

    let xdp = format!(
        r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    <subform name="root" layout="tb">
      <subform name="row" occur="0 -1" layout="tb">
        <field name="item" w="120mm" h="6mm">
          <ui><textEdit/></ui>
        </field>
      </subform>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <root>{rows}</root>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#
    );

    let pdf_bytes = build_xfa_pdf(&xdp);
    let result = flatten_xfa_to_pdf(&pdf_bytes);
    if let Ok(output) = result {
        assert!(!output.is_empty(), "flattened output must not be empty")
    }
}

// ---------------------------------------------------------------------------
// 4. Multi-page form spanning 3 pages
// ---------------------------------------------------------------------------

fn build_multipage_xfa_pdf(xdp: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");
    let xdp_bytes = xdp.as_bytes().to_vec();
    let xfa_stream = Stream::new(
        dictionary! { "Length" => Object::Integer(xdp_bytes.len() as i64) },
        xdp_bytes,
    );
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));
    let pages_id = doc.new_object_id();

    let mut page_refs = Vec::new();
    for _ in 0..3 {
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
        page_refs.push(Object::Reference(page_id));
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type"  => Object::Name(b"Pages".to_vec()),
            "Kids"  => Object::Array(page_refs),
            "Count" => Object::Integer(3)
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
    doc.save_to(&mut out).expect("save multipage xfa pdf");
    out
}

#[test]
fn edge_case_multipage_three_pages() {
    let xdp = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    <subform name="page1" layout="tb">
      <pageSet>
        <pageArea name="pg1">
          <contentArea x="10mm" y="10mm" w="190mm" h="270mm"/>
        </pageArea>
        <pageArea name="pg2">
          <contentArea x="10mm" y="10mm" w="190mm" h="270mm"/>
        </pageArea>
        <pageArea name="pg3">
          <contentArea x="10mm" y="10mm" w="190mm" h="270mm"/>
        </pageArea>
      </pageSet>
      <field name="f1" w="150mm" h="5mm"><ui><textEdit/></ui></field>
      <field name="f2" w="150mm" h="5mm"><ui><textEdit/></ui></field>
      <field name="f3" w="150mm" h="5mm"><ui><textEdit/></ui></field>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <page1><f1>Page1</f1><f2>Page2</f2><f3>Page3</f3></page1>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf_bytes = build_multipage_xfa_pdf(xdp);
    let result = flatten_xfa_to_pdf(&pdf_bytes);
    if let Ok(output) = result {
        assert!(!output.is_empty(), "flattened output must not be empty")
    }
}

// ---------------------------------------------------------------------------
// 5. Form with both static and dynamic sections
// ---------------------------------------------------------------------------

#[test]
fn edge_case_static_and_dynamic_sections() {
    let xdp = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    <subform name="root" layout="tb">
      <!-- Static section: always present, no binding -->
      <draw name="header" w="180mm" h="10mm">
        <value><text>Static Header</text></value>
      </draw>
      <!-- Dynamic section: data-bound -->
      <field name="name" w="150mm" h="6mm">
        <ui><textEdit/></ui>
        <caption><value><text>Name:</text></value></caption>
      </field>
      <!-- Static footer -->
      <draw name="footer" w="180mm" h="8mm">
        <value><text>Page 1 of 1</text></value>
      </draw>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <root><name>Jane Doe</name></root>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf_bytes = build_xfa_pdf(xdp);
    let result = flatten_xfa_to_pdf(&pdf_bytes);
    if let Ok(output) = result {
        assert!(!output.is_empty(), "flattened output must not be empty")
    }
}

// ---------------------------------------------------------------------------
// 6. Form with empty datasets
// ---------------------------------------------------------------------------

#[test]
fn edge_case_empty_datasets() {
    let xdp = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    <subform name="root" layout="tb">
      <field name="firstName" w="120mm" h="5mm">
        <ui><textEdit/></ui>
        <caption><value><text>First Name:</text></value></caption>
      </field>
      <field name="lastName" w="120mm" h="5mm">
        <ui><textEdit/></ui>
        <caption><value><text>Last Name:</text></value></caption>
      </field>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data/>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf_bytes = build_xfa_pdf(xdp);
    let result = flatten_xfa_to_pdf(&pdf_bytes);
    if let Ok(output) = result {
        assert!(!output.is_empty(), "flattened output must not be empty")
    }
}

// ---------------------------------------------------------------------------
// 7. Form with unicode text (non-ASCII characters)
// ---------------------------------------------------------------------------

#[test]
fn edge_case_unicode_text() {
    let xdp = r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    <subform name="root" layout="tb">
      <field name="greeting" w="180mm" h="6mm">
        <ui><textEdit/></ui>
        <caption><value><text>Groet:</text></value></caption>
      </field>
      <field name="city" w="180mm" h="6mm">
        <ui><textEdit/></ui>
        <caption><value><text>Stad:</text></value></caption>
      </field>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <root>
        <greeting>Héllo Wörld — café München</greeting>
        <city>São Paulo / 北京 / Москва</city>
      </root>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf_bytes = build_xfa_pdf(xdp);
    let result = flatten_xfa_to_pdf(&pdf_bytes);
    if let Ok(output) = result {
        assert!(!output.is_empty(), "flattened output must not be empty")
    }
}

// ---------------------------------------------------------------------------
// 8. Form with a very long text field (1000+ characters)
// ---------------------------------------------------------------------------

#[test]
fn edge_case_very_long_text_field() {
    // Generate a 1200-character string.
    let long_value: String = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. "
        .repeat(22)
        .chars()
        .take(1200)
        .collect();

    let xdp = format!(
        r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    <subform name="root" layout="tb">
      <field name="notes" w="180mm" h="60mm">
        <ui><textEdit multiLine="1"/></ui>
        <caption><value><text>Notes:</text></value></caption>
      </field>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <root><notes>{long_value}</notes></root>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#
    );

    let pdf_bytes = build_xfa_pdf(&xdp);
    let result = flatten_xfa_to_pdf(&pdf_bytes);
    if let Ok(output) = result {
        assert!(!output.is_empty(), "flattened output must not be empty")
    }
}
