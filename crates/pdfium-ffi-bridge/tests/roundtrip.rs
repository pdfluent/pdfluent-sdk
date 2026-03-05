//! Round-trip tests: open → edit → save → reopen → verify integrity.
//!
//! These tests exercise the complete persistence pipeline to ensure that
//! modifications to XFA data survive a full PDF save/reload cycle.

use pdfium_ffi_bridge::dataset_sync::sync_datasets;
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use pdfium_ffi_bridge::ur3::{detect_ur3, remove_ur3};
use xfa_dom_resolver::data_dom::DataDom;

use lopdf::{dictionary, Document, Object, Stream};

// --- Helpers ---

fn build_xfa_pdf(xfa_xml: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let xfa_stream = Stream::new(dictionary! {}, xfa_xml.as_bytes().to_vec());
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));

    let acroform = dictionary! { "XFA" => xfa_id };
    let acroform_id = doc.add_object(Object::Dictionary(acroform));

    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );

    let catalog_id = doc.new_object_id();
    doc.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform_id,
        }),
    );
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

fn build_xfa_pdf_with_ur3(xfa_xml: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let xfa_stream = Stream::new(dictionary! {}, xfa_xml.as_bytes().to_vec());
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));

    let acroform = dictionary! { "XFA" => xfa_id };
    let acroform_id = doc.add_object(Object::Dictionary(acroform));

    // UR3 signature
    let ur3_sig = dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
    };
    let ur3_id = doc.add_object(Object::Dictionary(ur3_sig));
    let perms = dictionary! { "UR3" => ur3_id };
    let perms_id = doc.add_object(Object::Dictionary(perms));

    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );

    let catalog_id = doc.new_object_id();
    doc.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform_id,
            "Perms" => perms_id,
        }),
    );
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

const SAMPLE_XFA: &str = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1">
      <field name="Name"/>
      <field name="Email"/>
      <field name="Amount"/>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <form1>
        <Name>John Doe</Name>
        <Email>john@example.com</Email>
        <Amount>100.00</Amount>
      </form1>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

// --- Tests ---

#[test]
fn roundtrip_open_save_reopen_preserves_xfa() {
    let pdf = build_xfa_pdf(SAMPLE_XFA);

    // Open
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();
    let packets1 = reader.extract_xfa().unwrap();
    assert!(packets1.template().is_some());
    assert!(packets1.datasets().is_some());

    // Save and reopen
    let saved = reader.save_to_bytes().unwrap();
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    let packets2 = reader2.extract_xfa().unwrap();

    // Verify structure preserved
    assert_eq!(packets1.packets.len(), packets2.packets.len());
    assert!(packets2.template().is_some());
    assert!(packets2.datasets().is_some());
}

#[test]
fn roundtrip_edit_data_and_save() {
    let pdf = build_xfa_pdf(SAMPLE_XFA);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    // Edit: update data
    let new_data = DataDom::from_xml(
        "<form1><Name>Jane Smith</Name><Email>jane@example.com</Email><Amount>250.00</Amount></form1>",
    )
    .unwrap();
    sync_datasets(&mut reader, &new_data).unwrap();

    // Save
    let saved = reader.save_to_bytes().unwrap();

    // Reopen and verify
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    let packets = reader2.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();

    assert!(full.contains("Jane Smith"), "new name should be in PDF");
    assert!(
        full.contains("jane@example.com"),
        "new email should be in PDF"
    );
    assert!(full.contains("250.00"), "new amount should be in PDF");
    assert!(!full.contains("John Doe"), "old name should be gone");
}

#[test]
fn roundtrip_multiple_edits() {
    let pdf = build_xfa_pdf(SAMPLE_XFA);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    // First edit
    let data1 = DataDom::from_xml("<form1><Name>Edit 1</Name></form1>").unwrap();
    sync_datasets(&mut reader, &data1).unwrap();
    let saved1 = reader.save_to_bytes().unwrap();

    // Second edit on saved PDF
    let mut reader2 = PdfReader::from_bytes(&saved1).unwrap();
    let data2 = DataDom::from_xml("<form1><Name>Edit 2</Name></form1>").unwrap();
    sync_datasets(&mut reader2, &data2).unwrap();
    let saved2 = reader2.save_to_bytes().unwrap();

    // Third edit
    let mut reader3 = PdfReader::from_bytes(&saved2).unwrap();
    let data3 = DataDom::from_xml("<form1><Name>Final</Name></form1>").unwrap();
    sync_datasets(&mut reader3, &data3).unwrap();
    let saved3 = reader3.save_to_bytes().unwrap();

    // Verify final state
    let reader4 = PdfReader::from_bytes(&saved3).unwrap();
    let packets = reader4.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();
    assert!(full.contains("Final"));
    assert!(!full.contains("Edit 1"));
    assert!(!full.contains("Edit 2"));
}

#[test]
fn roundtrip_with_ur3_removal() {
    let pdf = build_xfa_pdf_with_ur3(SAMPLE_XFA);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    // Verify UR3 exists
    assert!(detect_ur3(&reader).unwrap().is_some());

    // Remove UR3 before editing
    remove_ur3(&mut reader).unwrap();

    // Edit data
    let new_data = DataDom::from_xml("<form1><Name>Updated</Name></form1>").unwrap();
    sync_datasets(&mut reader, &new_data).unwrap();

    // Save
    let saved = reader.save_to_bytes().unwrap();

    // Verify round-trip
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    assert!(
        detect_ur3(&reader2).unwrap().is_none(),
        "UR3 should be gone"
    );
    let packets = reader2.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();
    assert!(full.contains("Updated"));
}

#[test]
fn roundtrip_page_count_preserved() {
    let pdf = build_xfa_pdf(SAMPLE_XFA);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();
    let page_count = reader.page_count();

    // Edit and save
    let data = DataDom::from_xml("<form1><Name>Test</Name></form1>").unwrap();
    sync_datasets(&mut reader, &data).unwrap();
    let saved = reader.save_to_bytes().unwrap();

    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    assert_eq!(
        reader2.page_count(),
        page_count,
        "page count should not change"
    );
}

#[test]
fn roundtrip_template_unchanged_after_data_edit() {
    let pdf = build_xfa_pdf(SAMPLE_XFA);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    // Get original template
    let original = reader.extract_xfa().unwrap();
    let original_template = original.template().unwrap().to_string();

    // Edit data
    let data = DataDom::from_xml("<form1><Name>Changed</Name></form1>").unwrap();
    sync_datasets(&mut reader, &data).unwrap();
    let saved = reader.save_to_bytes().unwrap();

    // Verify template is unchanged
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    let after = reader2.extract_xfa().unwrap();
    let after_template = after.template().unwrap().to_string();

    assert_eq!(
        original_template, after_template,
        "template should not change"
    );
}

#[test]
fn roundtrip_special_characters_in_data() {
    let pdf = build_xfa_pdf(SAMPLE_XFA);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    // Data with special XML characters
    let data = DataDom::from_xml(
        "<form1><Name>O'Brien &amp; Co</Name><Email>test@test.com</Email></form1>",
    )
    .unwrap();
    sync_datasets(&mut reader, &data).unwrap();
    let saved = reader.save_to_bytes().unwrap();

    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    let packets = reader2.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();

    // Should contain properly escaped data
    assert!(
        full.contains("O'Brien") || full.contains("O&apos;Brien"),
        "name with apostrophe should round-trip"
    );
}

#[test]
fn roundtrip_empty_data_values() {
    let pdf = build_xfa_pdf(SAMPLE_XFA);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    let data = DataDom::from_xml("<form1><Name/><Email></Email></form1>").unwrap();
    sync_datasets(&mut reader, &data).unwrap();
    let saved = reader.save_to_bytes().unwrap();

    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    let packets = reader2.extract_xfa().unwrap();
    // Should not crash, XFA should still be extractable
    assert!(packets.full_xml.is_some());
}

#[test]
fn roundtrip_save_to_file_and_reload() {
    let pdf = build_xfa_pdf(SAMPLE_XFA);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    let data = DataDom::from_xml("<form1><Name>File Test</Name></form1>").unwrap();
    sync_datasets(&mut reader, &data).unwrap();

    // Save to file
    let dir = std::env::temp_dir().join("xfa_roundtrip_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.pdf");
    reader.save_to_file(&path).unwrap();

    // Reload from file
    let reader2 = PdfReader::from_file(&path).unwrap();
    let packets = reader2.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();
    assert!(full.contains("File Test"));

    let _ = std::fs::remove_dir_all(&dir);
}
