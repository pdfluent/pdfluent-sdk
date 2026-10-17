// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `Document.journal_*` through the facade (#643).
//!
//! The facade owns one [`pdfluent::compat::edit::Journal`] per open
//! document, so persistent-handle bindings (node, C-ABI) journal without a
//! door-side log. Steps snapshot whole-document bytes: undo and redo
//! restore exact states, and a saved log reloads onto a fresh handle.

use pdfluent::compat::{Page, PageMut};
use pdfluent::prelude::*;

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");

fn open_sample() -> PdfDocument {
    PdfDocument::from_bytes(SAMPLE_PDF).expect("sample opens")
}

fn rotation_of(doc: &PdfDocument) -> i32 {
    Page::new(doc, 0).rotation().expect("read rotation")
}

fn set_rotation(doc: &mut PdfDocument, rotation: i32) {
    PageMut::new(doc, 0)
        .set_rotation(rotation)
        .expect("set rotation");
}

fn journal_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "pdfluent-compat-journal-{name}-{}-{}.bin",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock runs")
            .as_nanos()
    ))
}

#[test]
fn enable_starts_an_empty_log() {
    let mut doc = open_sample();
    assert!(!doc.journal_is_enabled().expect("read flag"));
    assert_eq!(
        doc.journal_can_do().expect("read availability"),
        (false, false)
    );
    assert_eq!(doc.journal_position().expect("read position"), 0);

    doc.journal_enable().expect("enable");

    assert!(doc.journal_is_enabled().expect("read flag"));
    assert_eq!(
        doc.journal_can_do().expect("read availability"),
        (false, false)
    );
    assert_eq!(doc.journal_position().expect("read position"), 0);
}

#[test]
fn op_records_its_name_and_position() {
    let mut doc = open_sample();
    doc.journal_enable().expect("enable");
    doc.journal_start_op(Some("op1")).expect("start");
    set_rotation(&mut doc, 90);
    doc.journal_stop_op().expect("stop");

    assert_eq!(doc.journal_position().expect("read position"), 1);
    assert_eq!(doc.journal_op_name(0).expect("read name"), "op1");
    assert_eq!(
        doc.journal_can_do().expect("read availability"),
        (true, false)
    );
}

#[test]
fn undo_and_redo_restore_exact_states() {
    let mut doc = open_sample();
    assert_eq!(rotation_of(&doc), 0);
    doc.journal_enable().expect("enable");
    doc.journal_start_op(Some("op1")).expect("start");
    set_rotation(&mut doc, 90);
    doc.journal_stop_op().expect("stop");
    assert_eq!(rotation_of(&doc), 90);

    doc.journal_undo().expect("undo");
    assert_eq!(rotation_of(&doc), 0);
    assert_eq!(doc.journal_position().expect("read position"), 0);
    assert_eq!(
        doc.journal_can_do().expect("read availability"),
        (false, true)
    );

    doc.journal_redo().expect("redo");
    assert_eq!(rotation_of(&doc), 90);
    assert_eq!(doc.journal_position().expect("read position"), 1);
}

#[test]
fn undo_without_a_step_fails_loudly() {
    let mut doc = open_sample();
    let err = doc.journal_undo().expect_err("fresh undo fails");
    assert!(err.to_string().contains("nothing to undo"), "{err}");
    let err = doc.journal_redo().expect_err("fresh redo fails");
    assert!(err.to_string().contains("nothing to redo"), "{err}");
}

#[test]
fn op_name_past_the_end_is_out_of_range() {
    let mut doc = open_sample();
    doc.journal_enable().expect("enable");
    doc.journal_start_op(Some("op1")).expect("start");
    doc.journal_stop_op().expect("stop");
    let err = doc.journal_op_name(99).expect_err("past-end name fails");
    assert_eq!(
        err.compat_kind(),
        Some(&pdfluent::compat::ErrorKind::IndexOutOfRange),
        "{err:?}"
    );
}

#[test]
fn save_load_round_trip_restores_the_log() {
    let path = journal_path("round-trip");
    let mut doc = open_sample();
    doc.journal_enable().expect("enable");
    doc.journal_start_op(Some("op1")).expect("start");
    set_rotation(&mut doc, 90);
    doc.journal_stop_op().expect("stop");
    doc.journal_save(path.to_str().expect("temp path is utf-8"))
        .expect("save");

    let mut loaded = open_sample();
    loaded
        .journal_load(path.to_str().expect("temp path is utf-8"))
        .expect("load");
    assert!(loaded.journal_is_enabled().expect("read flag"));
    assert_eq!(loaded.journal_position().expect("read position"), 1);
    assert_eq!(loaded.journal_op_name(0).expect("read name"), "op1");
    loaded.journal_undo().expect("undo after load");
    assert_eq!(rotation_of(&loaded), 0);

    std::fs::remove_file(&path).expect("remove temp log");
}

#[test]
fn load_refuses_a_live_journal_and_a_missing_file() {
    let path = journal_path("live");
    let mut doc = open_sample();
    doc.journal_enable().expect("enable");
    doc.journal_start_op(Some("op1")).expect("start");
    doc.journal_stop_op().expect("stop");
    doc.journal_save(path.to_str().expect("temp path is utf-8"))
        .expect("save");

    let err = doc
        .journal_load(path.to_str().expect("temp path is utf-8"))
        .expect_err("load over a live journal fails");
    assert!(
        err.to_string().contains("journal is already enabled"),
        "{err}"
    );
    let err = doc
        .journal_load(
            path.with_file_name("pdfluent-compat-journal-missing.bin")
                .to_str()
                .expect("temp path is utf-8"),
        )
        .expect_err("missing log fails");
    assert!(err.to_string().contains("file not found"), "{err}");

    std::fs::remove_file(&path).expect("remove temp log");
}

#[test]
fn stop_without_start_changes_nothing() {
    let mut doc = open_sample();
    doc.journal_enable().expect("enable");
    doc.journal_stop_op().expect("bare stop is a no-op");
    assert_eq!(doc.journal_position().expect("read position"), 0);
    assert_eq!(
        doc.journal_can_do().expect("read availability"),
        (false, false)
    );
}
