//! XFA Phase-1 fill API through the `pdfluent` facade: capability gating,
//! model enumeration, value writes, and the save → reopen roundtrip.

use lopdf::{dictionary, Object, Stream};
use pdfluent::prelude::*;

const TEMPLATE: &str = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1" layout="tb" w="8.5in" h="11in">
      <pageSet>
        <pageArea name="Page1">
          <contentArea x="36pt" y="36pt" w="540pt" h="720pt"/>
          <medium stock="default" short="612pt" long="792pt"/>
        </pageArea>
      </pageSet>
      <subform name="applicant" layout="tb" w="540pt">
        <field name="name" w="200pt" h="18pt"><ui><textEdit/></ui></field>
        <field name="agree" w="12pt" h="12pt">
          <ui><checkButton/></ui>
          <items><integer>1</integer><integer>0</integer></items>
        </field>
        <field name="locked" w="200pt" h="18pt" access="readOnly">
          <ui><textEdit/></ui>
        </field>
      </subform>
    </subform>
  </template>"#;

const DATASETS: &str = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><form1><applicant><name>Alice</name></applicant></form1></xfa:data></xfa:datasets>"#;

fn build_xfa_pdf() -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.4");
    let t_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        TEMPLATE.as_bytes().to_vec(),
    )));
    let d_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        DATASETS.as_bytes().to_vec(),
    )));
    let arr = Object::Array(vec![
        Object::String(b"template".to_vec(), lopdf::StringFormat::Literal),
        Object::Reference(t_id),
        Object::String(b"datasets".to_vec(), lopdf::StringFormat::Literal),
        Object::Reference(d_id),
    ]);
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
        "XFA"    => arr,
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

fn open_enterprise(bytes: &[u8]) -> PdfDocument {
    PdfDocument::from_bytes_with(
        bytes,
        OpenOptions::new().with_license_key("tier:enterprise"),
    )
    .expect("open")
}

#[test]
fn has_xfa_form_detects() {
    let doc = open_enterprise(&build_xfa_pdf());
    assert!(doc.has_xfa_form());
}

#[test]
fn xfa_form_model_enumerates_fields_and_pages() {
    let mut doc = open_enterprise(&build_xfa_pdf());
    let model = doc.xfa_form_model().expect("model");
    assert!(model.page_count >= 1);
    assert_eq!(model.fields.len(), 3);

    let name = model
        .fields
        .iter()
        .find(|f| f.name == "form1.applicant.name")
        .expect("name field");
    assert_eq!(name.field_type, XfaFieldType::Text);
    assert_eq!(name.value, "Alice");
    assert_eq!(name.page, Some(0));
    assert!(name.rect.is_some());

    let locked = model
        .fields
        .iter()
        .find(|f| f.name == "form1.applicant.locked")
        .expect("locked field");
    assert!(locked.read_only);
}

#[test]
fn set_value_roundtrips_through_save() {
    let mut doc = open_enterprise(&build_xfa_pdf());
    let outcome = doc
        .set_xfa_field_value("form1.applicant.name", XfaFieldValue::Text("Bob"))
        .expect("set");
    assert_eq!(outcome.raw_value, "Bob");
    assert!(outcome.persisted_to_datasets);
    doc.set_xfa_field_value("form1.applicant.agree", XfaFieldValue::Checkbox(true))
        .expect("set checkbox");

    let saved = doc.to_bytes().expect("to_bytes");

    // Fresh handle on the saved bytes: values must come back from datasets.
    let mut reopened = open_enterprise(&saved);
    let model = reopened.xfa_form_model().expect("model");
    let by_name = |n: &str| {
        model
            .fields
            .iter()
            .find(|f| f.name == n)
            .unwrap_or_else(|| panic!("{n} missing"))
    };
    assert_eq!(by_name("form1.applicant.name").value, "Bob");
    assert_eq!(by_name("form1.applicant.agree").value, "1");
}

#[test]
fn readonly_and_unknown_fields_rejected() {
    let mut doc = open_enterprise(&build_xfa_pdf());
    let ro = doc
        .set_xfa_field_value("form1.applicant.locked", XfaFieldValue::Text("x"))
        .expect_err("readonly");
    assert_eq!(ro.code(), "E-UNSUPPORTED");
    assert!(ro.to_string().contains("read-only"));

    let nf = doc
        .set_xfa_field_value("form1.nope", XfaFieldValue::Text("x"))
        .expect_err("not found");
    assert!(nf.to_string().contains("not found"));
}

#[test]
fn non_xfa_document_reports_unsupported() {
    // A plain PDF without /XFA.
    let mut doc = lopdf::Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1)
        }),
    );
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id)
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");

    let mut pdf = open_enterprise(&bytes);
    assert!(!pdf.has_xfa_form());
    let err = pdf.xfa_form_model().expect_err("non-XFA must error");
    assert_eq!(err.code(), "E-UNSUPPORTED");
}

#[test]
fn xfa_fill_requires_tier() {
    // Trial tier: XfaParse/XfaFill are not licensed.
    let bytes = build_xfa_pdf();
    let mut doc =
        PdfDocument::from_bytes_with(&bytes, OpenOptions::new().with_license_key("tier:trial"))
            .expect("open");
    let err = doc.xfa_form_model().expect_err("trial lacks XfaParse");
    assert_eq!(err.code(), "E-LICENSE-FEATURE-NOT-IN-TIER");
    let err = doc
        .set_xfa_field_value("form1.applicant.name", XfaFieldValue::Text("x"))
        .expect_err("trial lacks XfaFill");
    assert_eq!(err.code(), "E-LICENSE-FEATURE-NOT-IN-TIER");
}
