//! Integration tests for Epic 2 #1245 — Metadata & document info wiring.
//!
//! Exercises `PdfDocument::metadata`, `metadata_mut`, `form_fields`.

use pdfluent::prelude::*;

const SAMPLE: &str = "tests/fixtures/sample.pdf";
const FORM: &str = "tests/fixtures/form.pdf";

// ---------------------------------------------------------------------------
// metadata() — read
// ---------------------------------------------------------------------------

#[test]
fn metadata_reads_info_dict() {
    let doc = PdfDocument::open(SAMPLE).expect("open sample");
    let meta = doc.metadata();
    assert_eq!(meta.title.as_deref(), Some("Fixture Sample"));
    assert_eq!(meta.author.as_deref(), Some("PDFluent Tests"));
    assert_eq!(meta.subject.as_deref(), Some("Integration fixture"));
    assert_eq!(meta.creator.as_deref(), Some("gen_fixture"));
    assert_eq!(meta.producer.as_deref(), Some("lopdf"));
}

#[test]
fn metadata_parses_comma_separated_keywords() {
    let doc = PdfDocument::open(SAMPLE).expect("open sample");
    let meta = doc.metadata();
    assert_eq!(meta.keywords, vec!["test", "fixture", "pdf"]);
}

#[test]
fn metadata_reads_creation_date() {
    let doc = PdfDocument::open(SAMPLE).expect("open sample");
    let meta = doc.metadata();
    assert_eq!(meta.creation_date.as_deref(), Some("D:20260421000000Z"));
    // form.pdf was generated without CreationDate, so that side should be None.
    let form = PdfDocument::open(FORM).expect("open form");
    assert!(form.metadata().creation_date.is_none());
}

#[test]
fn metadata_empty_on_doc_without_info() {
    // Create produces a doc with no /Info dict.
    let doc = PdfDocument::create();
    let meta = doc.metadata();
    assert!(meta.title.is_none());
    assert!(meta.author.is_none());
    assert!(meta.subject.is_none());
    assert!(meta.keywords.is_empty());
    assert!(meta.creation_date.is_none());
    assert!(meta.modification_date.is_none());
}

// ---------------------------------------------------------------------------
// metadata_mut() — write
// ---------------------------------------------------------------------------

#[test]
fn metadata_mut_chain_with_commit_flushes_and_roundtrips() {
    let tmp = std::env::temp_dir().join("pdfluent-test-metadata-roundtrip.pdf");
    let _ = std::fs::remove_file(&tmp);

    let mut doc = PdfDocument::open(SAMPLE).expect("open");
    doc.metadata_mut()
        .set_title("Processed Invoice")
        .set_author("Acme Corp")
        .set_subject("Q2 2026")
        .set_keywords(&["invoice", "processed", "q2"])
        .commit()
        .expect("commit");

    doc.save(&tmp).expect("save");

    let reloaded = PdfDocument::open(&tmp).expect("reopen saved");
    let meta = reloaded.metadata();
    assert_eq!(meta.title.as_deref(), Some("Processed Invoice"));
    assert_eq!(meta.author.as_deref(), Some("Acme Corp"));
    assert_eq!(meta.subject.as_deref(), Some("Q2 2026"));
    assert_eq!(meta.keywords, vec!["invoice", "processed", "q2"]);

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn metadata_mut_autocommits_on_drop() {
    let tmp = std::env::temp_dir().join("pdfluent-test-metadata-drop.pdf");
    let _ = std::fs::remove_file(&tmp);

    let mut doc = PdfDocument::open(SAMPLE).expect("open");
    {
        let mut m = doc.metadata_mut();
        m.set_title("Committed via drop");
        // No explicit commit — Drop must flush.
    }
    doc.save(&tmp).expect("save");

    let reloaded = PdfDocument::open(&tmp).expect("reopen");
    assert_eq!(
        reloaded.metadata().title.as_deref(),
        Some("Committed via drop")
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn metadata_mut_creates_info_dict_if_absent() {
    // `create()` returns a doc without /Info. Setting metadata must create
    // the dict on the fly.
    let mut doc = PdfDocument::create();
    doc.metadata_mut()
        .set_title("Fresh doc")
        .commit()
        .expect("commit");

    // Serialise + re-parse to verify persistence through the write path.
    let bytes = doc.to_bytes().expect("to_bytes");
    let reloaded = PdfDocument::from_bytes(&bytes).expect("reparse");
    assert_eq!(reloaded.metadata().title.as_deref(), Some("Fresh doc"));
}

#[test]
fn metadata_mut_subsequent_commits_update_values() {
    let tmp = std::env::temp_dir().join("pdfluent-test-metadata-multi.pdf");
    let _ = std::fs::remove_file(&tmp);

    let mut doc = PdfDocument::open(SAMPLE).expect("open");
    {
        let mut m = doc.metadata_mut();
        m.set_title("First").commit().expect("first commit");
        m.set_title("Second").commit().expect("second commit");
    }
    doc.save(&tmp).expect("save");

    let reloaded = PdfDocument::open(&tmp).expect("reopen");
    assert_eq!(reloaded.metadata().title.as_deref(), Some("Second"));

    let _ = std::fs::remove_file(&tmp);
}

// ---------------------------------------------------------------------------
// form_fields() — read
// ---------------------------------------------------------------------------

#[test]
fn form_fields_reads_three_fields_from_form_fixture() {
    let doc = PdfDocument::open(FORM).expect("open form fixture");
    let fields = doc.form_fields().expect("form_fields");
    assert_eq!(fields.len(), 3, "form.pdf has 3 AcroForm fields");

    let first_name = fields.iter().find(|f| f.name == "first_name").unwrap();
    assert_eq!(first_name.field_type, pdfluent::FieldType::Text);
    assert!(first_name.required);
    assert!(!first_name.read_only);

    let last_name = fields.iter().find(|f| f.name == "last_name").unwrap();
    assert_eq!(last_name.field_type, pdfluent::FieldType::Text);
    assert!(!last_name.required);

    let agree = fields.iter().find(|f| f.name == "agree_terms").unwrap();
    assert_eq!(agree.field_type, pdfluent::FieldType::Checkbox);
}

#[test]
fn form_fields_returns_empty_on_doc_without_form() {
    let doc = PdfDocument::open(SAMPLE).expect("open sample (no form)");
    let fields = doc.form_fields().expect("form_fields");
    assert!(fields.is_empty());
}

#[test]
fn form_fields_returns_empty_on_empty_doc() {
    let doc = PdfDocument::create();
    assert!(doc.form_fields().expect("form_fields").is_empty());
}
