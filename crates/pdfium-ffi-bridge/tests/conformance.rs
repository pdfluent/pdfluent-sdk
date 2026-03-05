//! Conformance tests — verify behavior against real-world XFA patterns.
//!
//! Tests complex XFA structures, edge cases, and error handling
//! that mirror patterns found in production XFA PDFs.

use pdfium_ffi_bridge::dataset_sync::sync_datasets;
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use pdfium_ffi_bridge::ur3::{detect_ur3, has_docmdp, remove_ur3};
use pdfium_ffi_bridge::xfa_extract::parse_xfa_xml;
use xfa_dom_resolver::data_dom::DataDom;

use lopdf::{dictionary, Document, Object, Stream, StringFormat};

// --- PDF builder helpers ---

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

fn build_xfa_array_pdf(packets: &[(&str, &str)]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");

    let mut xfa_array = Vec::new();
    for (name, content) in packets {
        let stream = Stream::new(dictionary! {}, content.as_bytes().to_vec());
        let stream_id = doc.add_object(Object::Stream(stream));
        xfa_array.push(Object::String(
            name.as_bytes().to_vec(),
            StringFormat::Literal,
        ));
        xfa_array.push(Object::Reference(stream_id));
    }

    let acroform = dictionary! { "XFA" => Object::Array(xfa_array) };
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

fn build_minimal_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.4");
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
        }),
    );
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

// =============================================================================
// XFA Extraction Conformance
// =============================================================================

#[test]
fn extract_xfa_with_all_standard_packets() {
    // Real XFA PDFs often include: preamble, config, template, localeSet,
    // datasets, connectionSet, xmpmeta, xfdf, postamble
    let xfa_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <config xmlns="http://www.xfa.org/schema/xci/3.1/">
    <present><pdf><version>1.7</version></pdf></present>
  </config>
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1">
      <field name="TextField1"/>
      <field name="NumericField1"/>
      <field name="DateField1"/>
    </subform>
  </template>
  <localeSet xmlns="http://www.xfa.org/schema/xfa-locale-set/2.7/">
    <locale name="en_US"><numberPattern><full>zzz,zzz,zz9.99</full></numberPattern></locale>
  </localeSet>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <form1>
        <TextField1>Hello World</TextField1>
        <NumericField1>12345.67</NumericField1>
        <DateField1>2024-01-15</DateField1>
      </form1>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf = build_xfa_pdf(xfa_xml);
    let reader = PdfReader::from_bytes(&pdf).unwrap();
    let packets = reader.extract_xfa().unwrap();

    assert!(packets.template().is_some());
    assert!(packets.datasets().is_some());
    assert!(packets.config().is_some());
    assert!(packets.get_packet("localeSet").is_some());
    assert_eq!(packets.packets.len(), 4);
}

#[test]
fn extract_xfa_array_with_multiple_packets() {
    let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="invoice">
      <field name="InvoiceNo"/>
      <field name="Date"/>
      <field name="Total"/>
    </subform>
</template>"#;

    let config = r#"<config xmlns="http://www.xfa.org/schema/xci/3.1/">
    <present><pdf><version>1.7</version></pdf></present>
</config>"#;

    let datasets = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <invoice>
        <InvoiceNo>INV-2024-001</InvoiceNo>
        <Date>2024-03-15</Date>
        <Total>1250.00</Total>
      </invoice>
    </xfa:data>
</xfa:datasets>"#;

    let pdf = build_xfa_array_pdf(&[
        ("template", template),
        ("config", config),
        ("datasets", datasets),
    ]);

    let reader = PdfReader::from_bytes(&pdf).unwrap();
    let packets = reader.extract_xfa().unwrap();

    assert_eq!(packets.packets.len(), 3);
    assert!(packets
        .get_packet("template")
        .unwrap()
        .contains("InvoiceNo"));
    assert!(packets
        .get_packet("datasets")
        .unwrap()
        .contains("INV-2024-001"));
    assert!(packets.get_packet("config").is_some());
}

#[test]
fn extract_xfa_with_namespaced_elements() {
    // Test namespace-heavy XFA (common in Adobe LiveCycle output)
    let xfa_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"
          xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1" layout="tb">
      <subform name="header">
        <field name="Title"/>
      </subform>
      <subform name="body">
        <field name="Content"/>
      </subform>
    </subform>
  </template>
  <xfa:datasets>
    <xfa:data>
      <form1>
        <header><Title>Report Title</Title></header>
        <body><Content>Report Body</Content></body>
      </form1>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf = build_xfa_pdf(xfa_xml);
    let reader = PdfReader::from_bytes(&pdf).unwrap();
    let packets = reader.extract_xfa().unwrap();

    assert!(packets.template().is_some());
    let template = packets.template().unwrap();
    assert!(template.contains("header"));
    assert!(template.contains("body"));
}

#[test]
fn extract_deeply_nested_xfa_template() {
    // Test deeply nested subforms (common in government forms)
    let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1">
      <subform name="section1">
        <subform name="subsection1A">
          <subform name="group1">
            <field name="DeepField"/>
          </subform>
        </subform>
      </subform>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <form1>
        <section1>
          <subsection1A>
            <group1>
              <DeepField>Deep Value</DeepField>
            </group1>
          </subsection1A>
        </section1>
      </form1>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf = build_xfa_pdf(xfa_xml);
    let reader = PdfReader::from_bytes(&pdf).unwrap();
    let packets = reader.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();
    assert!(full.contains("DeepField"));
    assert!(full.contains("Deep Value"));
}

// =============================================================================
// Dataset Sync Conformance
// =============================================================================

#[test]
fn sync_preserves_other_packets() {
    let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <config xmlns="http://www.xfa.org/schema/xci/3.1/">
    <present><pdf><version>1.7</version></pdf></present>
  </config>
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="F1"/></subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><F1>Old</F1></form1></xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf = build_xfa_pdf(xfa_xml);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    let new_data = DataDom::from_xml("<form1><F1>New</F1></form1>").unwrap();
    sync_datasets(&mut reader, &new_data).unwrap();

    let saved = reader.save_to_bytes().unwrap();
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    let packets = reader2.extract_xfa().unwrap();

    // Config and template should still be there
    assert!(packets.config().is_some());
    assert!(packets.template().is_some());

    // Data should be updated
    let full = packets.full_xml.as_deref().unwrap();
    assert!(full.contains("New"));
    assert!(!full.contains(">Old<"));
}

#[test]
fn sync_with_unicode_data() {
    let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="Name"/></subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><Name>ASCII</Name></form1></xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf = build_xfa_pdf(xfa_xml);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    // Unicode: Chinese, Arabic, Emoji, accented chars
    let new_data =
        DataDom::from_xml("<form1><Name>Ünïcödé Tëst — André François Müller</Name></form1>")
            .unwrap();
    sync_datasets(&mut reader, &new_data).unwrap();

    let saved = reader.save_to_bytes().unwrap();
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    let packets = reader2.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();
    assert!(full.contains("Ünïcödé"));
    assert!(full.contains("Müller"));
}

#[test]
fn sync_array_form_preserves_template() {
    let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1">
      <field name="A"/>
      <field name="B"/>
    </subform>
</template>"#;

    let datasets = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><A>1</A><B>2</B></form1></xfa:data>
</xfa:datasets>"#;

    let pdf = build_xfa_array_pdf(&[("template", template), ("datasets", datasets)]);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    let new_data = DataDom::from_xml("<form1><A>X</A><B>Y</B></form1>").unwrap();
    sync_datasets(&mut reader, &new_data).unwrap();

    let saved = reader.save_to_bytes().unwrap();
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    let packets = reader2.extract_xfa().unwrap();

    // Template should be unchanged
    let tmpl = packets.get_packet("template").unwrap();
    assert!(tmpl.contains("field name=\"A\""));
    assert!(tmpl.contains("field name=\"B\""));

    // Data should be updated
    let ds = packets.get_packet("datasets").unwrap();
    assert!(ds.contains(">X<") || ds.contains("<A>X</A>"));
}

// =============================================================================
// UR3 Conformance
// =============================================================================

#[test]
fn detect_ur3_with_older_ur_key() {
    // Some older PDFs use "UR" instead of "UR3"
    let mut doc = Document::with_version("1.5");

    let ur_sig = dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
    };
    let ur_id = doc.add_object(Object::Dictionary(ur_sig));

    let perms = dictionary! { "UR" => ur_id };
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
        }),
    );

    let catalog_id = doc.new_object_id();
    doc.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "Perms" => perms_id,
        }),
    );
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();

    let reader = PdfReader::from_bytes(&buf).unwrap();
    let info = detect_ur3(&reader).unwrap();
    assert!(info.is_some(), "should detect older UR key");
}

#[test]
fn remove_ur3_with_older_ur_key() {
    let mut doc = Document::with_version("1.5");

    let ur_sig = dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
    };
    let ur_id = doc.add_object(Object::Dictionary(ur_sig));

    let perms = dictionary! { "UR" => ur_id };
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
        }),
    );

    let catalog_id = doc.new_object_id();
    doc.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "Perms" => perms_id,
        }),
    );
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();

    let mut reader = PdfReader::from_bytes(&buf).unwrap();
    let removed = remove_ur3(&mut reader).unwrap();
    assert!(removed);

    let saved = reader.save_to_bytes().unwrap();
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    assert!(detect_ur3(&reader2).unwrap().is_none());
}

#[test]
fn ur3_removal_then_sync_roundtrip() {
    // Full workflow: detect UR3 → remove → sync data → save → verify
    let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="Name"/></subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><Name>Original</Name></form1></xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    // Build PDF with UR3
    let mut doc = Document::with_version("1.7");
    let xfa_stream = Stream::new(dictionary! {}, xfa_xml.as_bytes().to_vec());
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));
    let acroform = dictionary! { "XFA" => xfa_id };
    let acroform_id = doc.add_object(Object::Dictionary(acroform));

    let ur3_sig = dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
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

    let mut reader = PdfReader::from_bytes(&buf).unwrap();

    // Step 1: Detect and remove UR3
    assert!(detect_ur3(&reader).unwrap().is_some());
    assert!(remove_ur3(&mut reader).unwrap());

    // Step 2: Sync new data
    let new_data = DataDom::from_xml("<form1><Name>Updated</Name></form1>").unwrap();
    sync_datasets(&mut reader, &new_data).unwrap();

    // Step 3: Save and verify
    let saved = reader.save_to_bytes().unwrap();
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    assert!(detect_ur3(&reader2).unwrap().is_none());
    let packets = reader2.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();
    assert!(full.contains("Updated"));
    assert!(!full.contains("Original"));
}

// =============================================================================
// Edge Cases — Error Handling
// =============================================================================

#[test]
fn invalid_pdf_bytes_returns_error() {
    let result = PdfReader::from_bytes(b"not a pdf at all");
    assert!(result.is_err());
}

#[test]
fn pdf_without_xfa_returns_error() {
    let pdf = build_minimal_pdf();
    let reader = PdfReader::from_bytes(&pdf).unwrap();
    assert!(reader.extract_xfa().is_err());
}

#[test]
fn pdf_without_acroform_returns_error() {
    let pdf = build_minimal_pdf();
    let reader = PdfReader::from_bytes(&pdf).unwrap();
    let result = reader.extract_xfa();
    assert!(result.is_err());
}

#[test]
fn ur3_on_pdf_without_perms_returns_none() {
    let pdf = build_minimal_pdf();
    let reader = PdfReader::from_bytes(&pdf).unwrap();
    assert!(detect_ur3(&reader).unwrap().is_none());
    assert!(!has_docmdp(&reader).unwrap());
}

#[test]
fn remove_ur3_on_pdf_without_perms_returns_false() {
    let pdf = build_minimal_pdf();
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();
    assert!(!remove_ur3(&mut reader).unwrap());
}

// =============================================================================
// XFA XML Parser Edge Cases
// =============================================================================

#[test]
fn parse_xfa_xml_with_xml_comments() {
    let xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <!-- This is a comment -->
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="F1"/></subform>
  </template>
  <!-- Another comment -->
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><F1>Data</F1></form1></xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let packets = parse_xfa_xml(xml).unwrap();
    assert_eq!(packets.packets.len(), 2);
    assert!(packets.template().is_some());
    assert!(packets.datasets().is_some());
}

#[test]
fn parse_xfa_xml_with_processing_instructions() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<?xfa generator="AdobeDesigner_2.3" APIVersion="2.4.7068.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="F1"/></subform>
  </template>
</xdp:xdp>"#;

    let packets = parse_xfa_xml(xml).unwrap();
    assert!(packets.template().is_some());
}

#[test]
fn parse_xfa_xml_empty_xdp() {
    let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"></xdp:xdp>"#;
    let packets = parse_xfa_xml(xml).unwrap();
    assert_eq!(packets.packets.len(), 0);
}

#[test]
fn parse_xfa_xml_with_whitespace_only_content() {
    let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">


</xdp:xdp>"#;

    let packets = parse_xfa_xml(xml).unwrap();
    assert_eq!(packets.packets.len(), 0);
}

// =============================================================================
// Multi-page PDF Conformance
// =============================================================================

#[test]
fn multipage_pdf_preserves_pages_after_sync() {
    // Build a 3-page PDF with XFA
    let mut doc = Document::with_version("1.7");

    let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="F1"/></subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><F1>Test</F1></form1></xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let xfa_stream = Stream::new(dictionary! {}, xfa_xml.as_bytes().to_vec());
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));
    let acroform = dictionary! { "XFA" => xfa_id };
    let acroform_id = doc.add_object(Object::Dictionary(acroform));

    let pages_id = doc.new_object_id();
    let mut page_ids = Vec::new();
    for _ in 0..3 {
        let page_id = doc.new_object_id();
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );
        page_ids.push(page_id);
    }

    let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => 3,
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

    let mut reader = PdfReader::from_bytes(&buf).unwrap();
    assert_eq!(reader.page_count(), 3);

    // Sync and save
    let data = DataDom::from_xml("<form1><F1>Updated</F1></form1>").unwrap();
    sync_datasets(&mut reader, &data).unwrap();
    let saved = reader.save_to_bytes().unwrap();

    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    assert_eq!(reader2.page_count(), 3, "page count should be preserved");

    let packets = reader2.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();
    assert!(full.contains("Updated"));
}

// =============================================================================
// Data DOM Conformance
// =============================================================================

#[test]
fn data_dom_with_repeated_elements() {
    // Repeating data groups (common in table-driven forms)
    let xml = r#"<form1>
  <item><Description>Widget A</Description><Qty>5</Qty><Price>10.00</Price></item>
  <item><Description>Widget B</Description><Qty>3</Qty><Price>25.00</Price></item>
  <item><Description>Widget C</Description><Qty>1</Qty><Price>100.00</Price></item>
</form1>"#;

    let dom = DataDom::from_xml(xml).unwrap();
    let roundtrip = dom.to_xml();

    // All items should survive roundtrip
    assert!(roundtrip.contains("Widget A"));
    assert!(roundtrip.contains("Widget B"));
    assert!(roundtrip.contains("Widget C"));
    assert!(roundtrip.contains("10.00"));
    assert!(roundtrip.contains("100.00"));
}

#[test]
fn data_dom_with_empty_and_null_values() {
    let xml = r#"<form1>
  <EmptyTag></EmptyTag>
  <SelfClosed/>
  <WithValue>Data</WithValue>
</form1>"#;

    let dom = DataDom::from_xml(xml).unwrap();
    let roundtrip = dom.to_xml();
    assert!(roundtrip.contains("WithValue"));
    assert!(roundtrip.contains("Data"));
}

#[test]
fn data_dom_with_special_characters() {
    let xml = r#"<form1>
  <Company>O'Brien &amp; Associates</Company>
  <Note>Line 1
Line 2</Note>
</form1>"#;

    let dom = DataDom::from_xml(xml).unwrap();
    let roundtrip = dom.to_xml();
    assert!(
        roundtrip.contains("O'Brien") || roundtrip.contains("O&apos;Brien"),
        "apostrophe should survive roundtrip"
    );
}

// =============================================================================
// Large XFA Structure Stress Test
// =============================================================================

#[test]
fn large_xfa_with_many_fields() {
    // Simulate a form with 100 fields (e.g., a government tax form)
    let mut fields = String::new();
    let mut data = String::from("<form1>");
    for i in 0..100 {
        fields.push_str(&format!("      <field name=\"Field{i}\"/>\n"));
        data.push_str(&format!("<Field{i}>Value {i}</Field{i}>"));
    }
    data.push_str("</form1>");

    let xfa_xml = format!(
        r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1">
{fields}    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>{data}</xfa:data>
  </xfa:datasets>
</xdp:xdp>"#
    );

    let pdf = build_xfa_pdf(&xfa_xml);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();
    let packets = reader.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();

    // All fields should be present
    assert!(full.contains("Field0"));
    assert!(full.contains("Field99"));
    assert!(full.contains("Value 50"));

    // Edit and verify roundtrip
    let new_data = DataDom::from_xml(&data.replace("Value 50", "CHANGED")).unwrap();
    sync_datasets(&mut reader, &new_data).unwrap();
    let saved = reader.save_to_bytes().unwrap();
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    let packets2 = reader2.extract_xfa().unwrap();
    let full2 = packets2.full_xml.as_deref().unwrap();
    assert!(full2.contains("CHANGED"));
    assert!(!full2.contains("Value 50"));
}
