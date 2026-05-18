//! M3-B Phase D κ — template page-drop regression tests.
//!
//! Verifies that pages whose only fields are non-data-bearing widgets
//! (signature, button, barcode) or purely static draw elements are never
//! dropped by the `page_has_fields` suppression heuristic in `flatten.rs`.
//!
//! XFA-01: adds `Draw` exclusion alongside the existing Signature/Button/Barcode
//! exclusions introduced in the Kappa sprint.

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

fn flatten_page_count(xdp: &str) -> usize {
    let pdf = build_xfa_pdf(xdp);
    let output = flatten_xfa_to_pdf(&pdf).expect("flatten_xfa_to_pdf failed");
    Document::load_mem(&output)
        .expect("reload flattened PDF")
        .get_pages()
        .len()
}

// ---------------------------------------------------------------------------
// XDP fixtures
// ---------------------------------------------------------------------------

/// Two-page form: page 1 has text fields with default values; page 2 has only
/// a signature field.  Models the 778a1138 (Finance Purchase Order) pattern.
const XDP_TWO_PAGE_SIGNATURE: &str = r#"<?xml version="1.0"?>
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
        <field name="Phone" w="200pt" h="18pt">
          <ui><textEdit/></ui>
          <value><text>0000000000</text></value>
        </field>
      </subform>
      <subform name="page2Section" layout="tb" w="540pt">
        <breakBefore targetType="pageArea"/>
        <field name="SignatureField1" w="300pt" h="72pt">
          <ui><signature/></ui>
        </field>
      </subform>
    </subform>
  </template>
</xdp:xdp>"#;

/// Two-page form: page 2 has only a button field.
const XDP_TWO_PAGE_BUTTON: &str = r#"<?xml version="1.0"?>
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
        <field name="OrderNumber" w="200pt" h="18pt">
          <ui><numericEdit/></ui>
          <value><integer>12345</integer></value>
        </field>
      </subform>
      <subform name="page2Section" layout="tb" w="540pt">
        <breakBefore targetType="pageArea"/>
        <field name="SubmitButton" w="100pt" h="24pt">
          <ui><button/></ui>
        </field>
      </subform>
    </subform>
  </template>
</xdp:xdp>"#;

/// Two-page form: page 2 has only static draw elements (text labels).
/// Models the separator-page pattern seen in corpus doc 13275420.
const XDP_TWO_PAGE_DRAW_ONLY: &str = r#"<?xml version="1.0"?>
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
        <field name="Name" w="300pt" h="18pt">
          <ui><textEdit/></ui>
          <value><text>John Smith</text></value>
        </field>
      </subform>
      <subform name="page2Section" layout="tb" w="540pt">
        <breakBefore targetType="pageArea"/>
        <draw name="SeparatorHeader" w="540pt" h="30pt">
          <value><text>--- Separator Page ---</text></value>
        </draw>
        <draw name="SeparatorNote" w="540pt" h="20pt">
          <value><text>This page contains static content only.</text></value>
        </draw>
      </subform>
    </subform>
  </template>
</xdp:xdp>"#;

/// Single-page form: one text field with default value.  Used to confirm
/// the fix does not introduce over-pagination.
const XDP_ONE_PAGE_TEXT: &str = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1" layout="paginate" w="8.5in" h="11in">
      <pageSet>
        <pageArea name="Page1">
          <contentArea x="36pt" y="36pt" w="540pt" h="720pt"/>
          <medium stock="default" short="612pt" long="792pt"/>
        </pageArea>
      </pageSet>
      <subform name="section" layout="tb" w="540pt">
        <field name="Name" w="300pt" h="18pt">
          <ui><textEdit/></ui>
          <value><text>John Smith</text></value>
        </field>
      </subform>
    </subform>
  </template>
</xdp:xdp>"#;

/// Two-page form where BOTH pages have data-bearing fields with values.
/// Both pages must be retained.
const XDP_TWO_PAGE_BOTH_DATA: &str = r#"<?xml version="1.0"?>
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
        <field name="Field1" w="300pt" h="18pt">
          <ui><textEdit/></ui>
          <value><text>Page one content</text></value>
        </field>
      </subform>
      <subform name="page2Section" layout="tb" w="540pt">
        <breakBefore targetType="pageArea"/>
        <field name="Field2" w="300pt" h="18pt">
          <ui><textEdit/></ui>
          <value><text>Page two content</text></value>
        </field>
      </subform>
    </subform>
  </template>
</xdp:xdp>"#;

// ---------------------------------------------------------------------------
// Tests — synthetic
// ---------------------------------------------------------------------------

#[test]
fn signature_only_page_is_not_dropped() {
    // Page 1 has data fields → any_keep=true → suppression activates.
    // Page 2 has ONLY a signature field → must be treated as static and kept.
    // Bug: before fix, page_has_fields(page2)=true (signature IS a Field) and
    // page_has_field_data(page2)=false → page 2 was dropped.
    assert_eq!(
        flatten_page_count(XDP_TWO_PAGE_SIGNATURE),
        2,
        "page with only a signature field must not be dropped"
    );
}

#[test]
fn button_only_page_is_not_dropped() {
    // Button fields carry no data value — a page with only a button field
    // must be treated as static (always kept), just like a signature page.
    assert_eq!(
        flatten_page_count(XDP_TWO_PAGE_BUTTON),
        2,
        "page with only a button field must not be dropped"
    );
}

#[test]
fn draw_only_page_is_not_dropped() {
    // XFA-01: draw elements are static content — they must never count as
    // data fields for page-drop suppression.  A page with only <draw>
    // elements must always be retained.
    assert_eq!(
        flatten_page_count(XDP_TWO_PAGE_DRAW_ONLY),
        2,
        "page with only draw elements must not be dropped"
    );
}

#[test]
fn single_page_text_form_stays_at_one_page() {
    // Regression guard: the fix must not cause over-pagination for a
    // standard 1-page text form.
    assert_eq!(
        flatten_page_count(XDP_ONE_PAGE_TEXT),
        1,
        "1-page text form must not gain extra pages after the fix"
    );
}

#[test]
fn two_data_pages_both_kept() {
    // Both pages have data-bearing fields with non-empty values.
    // Both must be retained regardless of the fix.
    assert_eq!(
        flatten_page_count(XDP_TWO_PAGE_BOTH_DATA),
        2,
        "both data pages must be retained"
    );
}

// ---------------------------------------------------------------------------
// Corpus doc gate tests (skipped when test-workspace is absent)
// ---------------------------------------------------------------------------

/// 13275420 — pageArea-expansion fidelity guard.
///
/// Oracle (pdfRest reference and Adobe Reader runtime) emits 10 pages
/// because the form-DOM packet records 10 `<pageArea name="Page1">`
/// instances.  The pageArea-expansion fix (XFA 3.3 §8.6 / §3.1) mirrors
/// those runtime-allocated instances into the FormTree and prevents the
/// data-empty page-drop heuristic from removing them.
///
/// This guard pins page-count fidelity at ≥ 8 (target 10) so any future
/// regression in form-DOM walking or runtime-instantiated semantics will
/// surface immediately.
#[test]
fn corpus_13275420_at_least_eight_pages() {
    let path = "/Users/jasperdewinter/xfa_analysis/input/13275420.pdf";
    if !std::path::Path::new(path).exists() {
        return;
    }
    let pdf = std::fs::read(path).expect("read 13275420 input.pdf");
    let output = flatten_xfa_to_pdf(&pdf).expect("flatten 13275420");
    let pages = Document::load_mem(&output)
        .expect("reload 13275420 output")
        .get_pages()
        .len();
    assert!(
        pages >= 8,
        "13275420 fidelity: pageArea expansion must produce ≥8 pages (oracle 10), got {pages}"
    );
}

#[test]
fn corpus_778a1138_two_pages() {
    let path = "test-workspace/issue-812/778a1138/input.pdf";
    if !std::path::Path::new(path).exists() {
        return;
    }
    let pdf = std::fs::read(path).expect("read 778a1138 input.pdf");
    let output = flatten_xfa_to_pdf(&pdf).expect("flatten 778a1138");
    let pages = Document::load_mem(&output)
        .expect("reload 778a1138 output")
        .get_pages()
        .len();
    assert_eq!(pages, 2, "778a1138: expected 2 pages (oracle), got {pages}");
}

#[test]
fn corpus_d9ec06f8_four_pages() {
    let path = "test-workspace/issue-812/d9ec06f8/input.pdf";
    if !std::path::Path::new(path).exists() {
        return;
    }
    let pdf = std::fs::read(path).expect("read d9ec06f8 input.pdf");
    let output = flatten_xfa_to_pdf(&pdf).expect("flatten d9ec06f8");
    let pages = Document::load_mem(&output)
        .expect("reload d9ec06f8 output")
        .get_pages()
        .len();
    assert_eq!(pages, 4, "d9ec06f8: expected 4 pages (oracle), got {pages}");
}

#[test]
fn corpus_03af199e_twentyone_pages() {
    let path = "test-workspace/issue-812/03af199e/input.pdf";
    if !std::path::Path::new(path).exists() {
        return;
    }
    let pdf = std::fs::read(path).expect("read 03af199e input.pdf");
    let output = flatten_xfa_to_pdf(&pdf).expect("flatten 03af199e");
    let pages = Document::load_mem(&output)
        .expect("reload 03af199e output")
        .get_pages()
        .len();
    assert_eq!(
        pages, 21,
        "03af199e: expected 21 pages (oracle), got {pages}"
    );
}

#[test]
fn corpus_3963b9b6_stays_at_three_pages() {
    let path = "test-workspace/issue-812/3963b9b6/input.pdf";
    if !std::path::Path::new(path).exists() {
        return;
    }
    let pdf = std::fs::read(path).expect("read 3963b9b6 input.pdf");
    let output = flatten_xfa_to_pdf(&pdf).expect("flatten 3963b9b6");
    let pages = Document::load_mem(&output)
        .expect("reload 3963b9b6 output")
        .get_pages()
        .len();
    assert_eq!(
        pages, 3,
        "3963b9b6 regression guard: must stay at 3 pages, got {pages}"
    );
}
