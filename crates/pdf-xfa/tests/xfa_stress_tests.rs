//! XFA-F8-03 (#1119): Stress tests for large XFA documents.
//!
//! These tests verify that `flatten_xfa_to_pdf` handles large inputs without
//! panicking, running out of memory, or taking an unreasonable amount of time.
//! All tests use synthetic XFA XML wrapped in a minimal PDF structure.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_xfa::flatten_xfa_to_pdf;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Helper — build a minimal XFA PDF from an XDP string (same as edge_cases)
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
// 1. 100 repeating subform instances — must complete within 30 seconds
// ---------------------------------------------------------------------------

#[test]
fn stress_100_repeating_subform_instances() {
    // Build a form with 100 data records that drive occur expansion.
    let mut rows = String::new();
    for i in 0..100 {
        rows.push_str(&format!("<row><item>Item {i}</item></row>\n"));
    }

    let xdp = format!(
        r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    <subform name="root" layout="tb" w="612pt" h="792pt">
      <subform name="row" layout="tb" occur="1,-1">
        <field name="item" w="500pt" h="15pt">
          <ui><textEdit/></ui>
        </field>
      </subform>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <root>
        {rows}
      </root>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#
    );

    let pdf_bytes = build_xfa_pdf(&xdp);
    let start = Instant::now();
    let result = flatten_xfa_to_pdf(&pdf_bytes);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 30,
        "stress test took {elapsed:?} — must complete within 30 seconds"
    );

    // acceptable — the form may not be fully supported
    if let Ok(output) = result {
        assert!(!output.is_empty(), "flattened output must not be empty");
    }
}

// ---------------------------------------------------------------------------
// 2. 50 fields per page — must complete without OOM
// ---------------------------------------------------------------------------

#[test]
fn stress_50_fields_per_page() {
    let mut fields = String::new();
    for i in 0..50 {
        fields.push_str(&format!(
            r#"<field name="field{i}" w="500pt" h="14pt">
              <ui><textEdit/></ui>
              <caption><value><text>Label {i}</text></value></caption>
            </field>
            "#
        ));
    }

    let mut data_fields = String::new();
    for i in 0..50 {
        data_fields.push_str(&format!("<field{i}>value {i}</field{i}>\n"));
    }

    let xdp = format!(
        r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    <subform name="root" layout="tb" w="612pt" h="792pt">
      {fields}
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <root>
        {data_fields}
      </root>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#
    );

    let pdf_bytes = build_xfa_pdf(&xdp);

    // This test merely verifies that we do not OOM or panic. No explicit
    // memory check — Rust's allocator will abort on OOM before this assert.
    let result = flatten_xfa_to_pdf(&pdf_bytes);
    // acceptable — the form may not be fully supported
    if let Ok(output) = result {
        assert!(!output.is_empty(), "flattened output must not be empty");
    }
}

// ---------------------------------------------------------------------------
// 3. Deeply nested transparent subforms (20 levels) — no stack overflow
// ---------------------------------------------------------------------------

#[test]
fn stress_20_levels_deep_no_stack_overflow() {
    // Build 20 levels of nested subforms.
    const DEPTH: usize = 20;

    let mut template = String::new();
    for i in 0..DEPTH {
        template.push_str(&format!(r#"<subform name="l{i}" layout="tb">"#));
    }
    template.push_str(
        r#"<field name="deepField" w="200pt" h="10pt">
          <ui><textEdit/></ui>
        </field>"#,
    );
    for _ in 0..DEPTH {
        template.push_str("</subform>");
    }

    let xdp = format!(
        r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    {template}
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data/>
  </xfa:datasets>
</xdp:xdp>"#
    );

    let pdf_bytes = build_xfa_pdf(&xdp);

    // Must not panic with a stack overflow — use the default thread stack.
    let result = flatten_xfa_to_pdf(&pdf_bytes);
    // acceptable — the form may not be fully supported
    if let Ok(output) = result {
        assert!(!output.is_empty(), "flattened output must not be empty");
    }
}
