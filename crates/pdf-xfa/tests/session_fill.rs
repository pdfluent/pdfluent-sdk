//! XfaSession Phase-1 integration tests: enumerate → fill → save → reopen.
//!
//! Covers both `/XFA` layouts (consolidated single stream and the
//! array-of-pairs form), value writes for every fillable kind, the readonly
//! guard, and the save/reopen roundtrip through the real PDF writeback.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_xfa::session::{XfaFieldType, XfaSession, XfaWriteValue};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const TEMPLATE: &str = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1" layout="tb" w="8.5in" h="11in">
      <pageSet>
        <pageArea name="Page1">
          <contentArea x="36pt" y="36pt" w="540pt" h="720pt"/>
          <medium stock="default" short="612pt" long="792pt"/>
        </pageArea>
      </pageSet>
      <subform name="applicant" layout="tb" w="540pt">
        <field name="name" w="200pt" h="18pt">
          <ui><textEdit/></ui>
        </field>
        <field name="notes" w="200pt" h="54pt">
          <ui><textEdit multiLine="1"/></ui>
        </field>
        <field name="locked" w="200pt" h="18pt" access="readOnly">
          <ui><textEdit/></ui>
          <value><text>fixed</text></value>
        </field>
        <field name="agree" w="12pt" h="12pt">
          <ui><checkButton/></ui>
          <items><integer>1</integer><integer>0</integer></items>
        </field>
        <exclGroup name="contact" layout="tb" w="200pt">
          <field name="optEmail" w="100pt" h="12pt">
            <ui><checkButton shape="round"/></ui>
            <items><text>Email</text></items>
          </field>
          <field name="optPhone" w="100pt" h="12pt">
            <ui><checkButton shape="round"/></ui>
            <items><text>Phone</text></items>
          </field>
        </exclGroup>
        <field name="country" w="200pt" h="18pt">
          <ui><choiceList/></ui>
          <items><text>Netherlands</text><text>Germany</text></items>
          <items save="1"><text>NL</text><text>DE</text></items>
          <validate nullTest="error"/>
        </field>
      </subform>
    </subform>
  </template>"#;

const DATASETS: &str = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <form1>
        <applicant>
          <name>Alice</name>
        </applicant>
      </form1>
    </xfa:data>
    <dd:dataDescription xmlns:dd="http://ns.adobe.com/data-description/"><form1/></dd:dataDescription>
  </xfa:datasets>"#;

fn consolidated_xdp() -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">\n{TEMPLATE}\n{DATASETS}\n</xdp:xdp>"
    )
}

/// Single consolidated XDP stream behind `/AcroForm /XFA`.
fn build_single_stream_pdf(xdp: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");
    let xdp_bytes = xdp.as_bytes().to_vec();
    let xfa_stream = Stream::new(
        dictionary! { "Length" => Object::Integer(xdp_bytes.len() as i64) },
        xdp_bytes,
    );
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));
    finish_pdf(doc, Object::Reference(xfa_id))
}

/// Array-of-pairs `/XFA [(template) (datasets)]` layout.
fn build_array_pdf(template: &str, datasets: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");
    let t_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        template.as_bytes().to_vec(),
    )));
    let d_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        datasets.as_bytes().to_vec(),
    )));
    let arr = Object::Array(vec![
        Object::String(b"template".to_vec(), lopdf::StringFormat::Literal),
        Object::Reference(t_id),
        Object::String(b"datasets".to_vec(), lopdf::StringFormat::Literal),
        Object::Reference(d_id),
    ]);
    finish_pdf(doc, arr)
}

fn finish_pdf(mut doc: Document, xfa_value: Object) -> Vec<u8> {
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
        "XFA"    => xfa_value,
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

fn field<'a>(s: &'a XfaSession, name: &str) -> &'a pdf_xfa::session::XfaFieldModel {
    s.field(name)
        .unwrap_or_else(|| panic!("field {name} not enumerated"))
}

// ---------------------------------------------------------------------------
// Session creation + enumeration
// ---------------------------------------------------------------------------

#[test]
fn session_opens_and_reports_layout_page_count() {
    let pdf = build_single_stream_pdf(&consolidated_xdp());
    let session = XfaSession::open(&pdf).expect("open session");
    assert!(session.page_count() >= 1, "layout must produce pages");
    let (w, h) = session.page_size(0).expect("page 0 size");
    assert!(w > 500.0 && h > 700.0, "US-Letter-ish page, got {w}x{h}");
}

#[test]
fn session_open_rejects_non_xfa_pdf() {
    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "Parent" => Object::Reference(pages_id),
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
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    assert!(XfaSession::open(&out).is_err());
}

#[test]
fn enumeration_exposes_model_fields() {
    let pdf = build_single_stream_pdf(&consolidated_xdp());
    let session = XfaSession::open(&pdf).expect("open session");

    let name = field(&session, "form1.applicant.name");
    assert_eq!(name.field_type, XfaFieldType::Text);
    assert_eq!(name.value, "Alice", "datasets prefill must bind");
    assert!(name.bound_to_data);
    assert!(!name.read_only);
    assert!(!name.multiline);
    assert_eq!(name.som_path, "form1[0].applicant[0].name[0]");
    assert_eq!(name.page, Some(0), "field must map to layout page 0");
    let rect = name.rect.expect("geometry");
    assert!(rect.width > 0.0 && rect.height > 0.0);

    let notes = field(&session, "form1.applicant.notes");
    assert!(notes.multiline, "textEdit multiLine=1 must surface");

    let locked = field(&session, "form1.applicant.locked");
    assert!(locked.read_only, "access=readOnly must surface");

    let agree = field(&session, "form1.applicant.agree");
    assert_eq!(agree.field_type, XfaFieldType::Checkbox);
    assert_eq!(agree.on_value.as_deref(), Some("1"));
    assert_eq!(agree.off_value.as_deref(), Some("0"));

    let contact = field(&session, "form1.applicant.contact");
    assert_eq!(contact.field_type, XfaFieldType::RadioGroup);
    let on_values: Vec<&str> = contact.options.iter().map(|o| o.save.as_str()).collect();
    assert_eq!(on_values, vec!["Email", "Phone"]);
    assert!(
        contact
            .widgets
            .iter()
            .any(|w| w.on_value.as_deref() == Some("Email")),
        "radio member widgets must carry on-values"
    );

    let country = field(&session, "form1.applicant.country");
    assert_eq!(country.field_type, XfaFieldType::Dropdown);
    assert!(country.required, "nullTest=error must surface as required");
    assert_eq!(country.options.len(), 2);
    assert_eq!(country.options[0].display, "Netherlands");
    assert_eq!(country.options[0].save, "NL");
}

// ---------------------------------------------------------------------------
// Value writes
// ---------------------------------------------------------------------------

#[test]
fn set_text_value_updates_model_and_data() {
    let pdf = build_single_stream_pdf(&consolidated_xdp());
    let mut session = XfaSession::open(&pdf).expect("open session");
    let out = session
        .set_value("form1.applicant.name", XfaWriteValue::Text("Bob"))
        .expect("set");
    assert_eq!(out.raw_value, "Bob");
    assert!(out.persisted_to_datasets);
    assert_eq!(field(&session, "form1.applicant.name").value, "Bob");
    assert!(session.is_dirty());
}

#[test]
fn set_multiline_value() {
    let pdf = build_single_stream_pdf(&consolidated_xdp());
    let mut session = XfaSession::open(&pdf).expect("open session");
    let text = "line one\nline two\nline three";
    let out = session
        .set_value("form1.applicant.notes", XfaWriteValue::Text(text))
        .expect("set");
    assert!(
        out.persisted_to_datasets,
        "created on demand under applicant"
    );
    assert_eq!(field(&session, "form1.applicant.notes").value, text);
}

#[test]
fn set_checkbox_normalizes_bool_to_on_off_values() {
    let pdf = build_single_stream_pdf(&consolidated_xdp());
    let mut session = XfaSession::open(&pdf).expect("open session");
    let on = session
        .set_value("form1.applicant.agree", XfaWriteValue::Checkbox(true))
        .expect("set on");
    assert_eq!(on.raw_value, "1");
    let off = session
        .set_value("form1.applicant.agree", XfaWriteValue::Checkbox(false))
        .expect("set off");
    assert_eq!(off.raw_value, "0");
}

#[test]
fn set_radio_selects_member_and_rejects_unknown() {
    let pdf = build_single_stream_pdf(&consolidated_xdp());
    let mut session = XfaSession::open(&pdf).expect("open session");
    session
        .set_value("form1.applicant.contact", XfaWriteValue::Radio("Phone"))
        .expect("set radio");
    assert_eq!(field(&session, "form1.applicant.contact").value, "Phone");

    let err = session
        .set_value("form1.applicant.contact", XfaWriteValue::Radio("Fax"))
        .expect_err("unknown on-value must fail");
    assert!(err.to_string().contains("Fax"));
}

#[test]
fn set_dropdown_maps_display_to_save_value() {
    let pdf = build_single_stream_pdf(&consolidated_xdp());
    let mut session = XfaSession::open(&pdf).expect("open session");
    let out = session
        .set_value(
            "form1.applicant.country",
            XfaWriteValue::Text("Netherlands"),
        )
        .expect("set");
    assert_eq!(out.raw_value, "NL", "display value must map to save value");
}

#[test]
fn readonly_field_rejects_writes() {
    let pdf = build_single_stream_pdf(&consolidated_xdp());
    let mut session = XfaSession::open(&pdf).expect("open session");
    let err = session
        .set_value("form1.applicant.locked", XfaWriteValue::Text("nope"))
        .expect_err("readonly must reject");
    assert!(matches!(err, pdf_xfa::error::XfaError::FieldReadOnly(_)));
    assert_eq!(field(&session, "form1.applicant.locked").value, "fixed");
}

#[test]
fn unknown_field_name_errors() {
    let pdf = build_single_stream_pdf(&consolidated_xdp());
    let mut session = XfaSession::open(&pdf).expect("open session");
    let err = session
        .set_value("form1.nope", XfaWriteValue::Text("x"))
        .expect_err("unknown field");
    assert!(matches!(err, pdf_xfa::error::XfaError::FieldNotFound(_)));
}

// ---------------------------------------------------------------------------
// Save / reopen roundtrips (the Acrobat-compatibility proxy: values must
// live in the datasets packet of the saved PDF)
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_single_stream_pdf() {
    let pdf = build_single_stream_pdf(&consolidated_xdp());
    let mut session = XfaSession::open(&pdf).expect("open session");
    session
        .set_value("form1.applicant.name", XfaWriteValue::Text("Käthe & Søn"))
        .expect("set name");
    session
        .set_value("form1.applicant.notes", XfaWriteValue::Text("multi\nline"))
        .expect("set notes");
    session
        .set_value("form1.applicant.agree", XfaWriteValue::Checkbox(true))
        .expect("set agree");
    session
        .set_value("form1.applicant.contact", XfaWriteValue::Radio("Email"))
        .expect("set contact");

    let saved = session.save_to_bytes().expect("save");

    // Reopen with a fresh session: values must come back from datasets.
    let reopened = XfaSession::open(&saved).expect("reopen");
    assert_eq!(
        field(&reopened, "form1.applicant.name").value,
        "Käthe & Søn"
    );
    assert_eq!(
        field(&reopened, "form1.applicant.notes").value,
        "multi\nline"
    );
    assert_eq!(field(&reopened, "form1.applicant.agree").value, "1");
    assert_eq!(field(&reopened, "form1.applicant.contact").value, "Email");
}

#[test]
fn roundtrip_array_form_pdf() {
    let pdf = build_array_pdf(TEMPLATE, DATASETS);
    let mut session = XfaSession::open(&pdf).expect("open session");
    session
        .set_value("form1.applicant.name", XfaWriteValue::Text("ArrayForm"))
        .expect("set");
    let saved = session.save_to_bytes().expect("save");
    let reopened = XfaSession::open(&saved).expect("reopen");
    assert_eq!(field(&reopened, "form1.applicant.name").value, "ArrayForm");
}

#[test]
fn writeback_splices_and_preserves_datadescription() {
    let pdf = build_array_pdf(TEMPLATE, DATASETS);
    let mut session = XfaSession::open(&pdf).expect("open session");
    session
        .set_value("form1.applicant.name", XfaWriteValue::Text("Spliced"))
        .expect("set");

    let mut doc = Document::load_mem(&pdf).expect("load");
    let report = session.write_into_document(&mut doc).expect("writeback");
    assert!(
        report.datasets_spliced,
        "existing node must take the splice path"
    );

    let mut saved = Vec::new();
    doc.save_to(&mut saved).expect("serialize");
    let packets =
        pdf_xfa::extract::extract_xfa_from_bytes(saved.clone()).expect("extract from saved");
    let ds = packets.datasets().expect("datasets packet");
    assert!(
        ds.contains("<name>Spliced</name>"),
        "value in datasets: {ds}"
    );
    assert!(
        ds.contains("dataDescription"),
        "splice must preserve the dataDescription sub-packet"
    );
    // The untouched prefill structure survives byte-for-byte.
    assert!(ds.contains("<xfa:data>"));
}

#[test]
fn save_without_changes_is_noop_clean() {
    let pdf = build_single_stream_pdf(&consolidated_xdp());
    let session = XfaSession::open(&pdf).expect("open session");
    assert!(!session.is_dirty());
    let saved = session.save_to_bytes().expect("save");
    // Still a valid XFA PDF with the same field set.
    let reopened = XfaSession::open(&saved).expect("reopen");
    assert_eq!(reopened.fields().len(), session.fields().len());
}

// ---------------------------------------------------------------------------
// Saved form packet: access locks + value sync
// ---------------------------------------------------------------------------

#[test]
fn form_packet_access_lock_enforced_and_values_synced() {
    let form_packet = r#"<form xmlns="http://www.xfa.org/schema/xfa-form/2.8/">
    <subform name="form1">
      <subform name="applicant">
        <field name="name"><value><text>Alice</text></value></field>
        <field name="notes" access="readOnly"/>
      </subform>
    </subform>
  </form>"#;
    let xdp = format!(
        "<?xml version=\"1.0\"?>\n<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">\n{TEMPLATE}\n{DATASETS}\n{form_packet}\n</xdp:xdp>"
    );
    let pdf = build_single_stream_pdf(&xdp);
    let mut session = XfaSession::open(&pdf).expect("open session");

    // The form packet locks `notes` even though the template leaves it open.
    let notes = field(&session, "form1.applicant.notes");
    assert!(
        notes.read_only,
        "form-packet access=readOnly must be enforced"
    );
    assert!(session
        .set_value("form1.applicant.notes", XfaWriteValue::Text("x"))
        .is_err());

    // Setting `name` must sync the saved form packet so Adobe's
    // restoreState does not resurrect "Alice".
    session
        .set_value("form1.applicant.name", XfaWriteValue::Text("Updated"))
        .expect("set");
    let saved = session.save_to_bytes().expect("save");
    let packets = pdf_xfa::extract::extract_xfa_from_bytes(saved.clone()).expect("extract");
    let form = packets.get_packet("form").expect("form packet survives");
    assert!(
        form.contains("<value><text>Updated</text></value>"),
        "form packet value must be synced: {form}"
    );
    let ds = packets.datasets().expect("datasets");
    assert!(ds.contains("<name>Updated</name>"));
}
