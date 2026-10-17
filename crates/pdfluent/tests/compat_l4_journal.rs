//! Compat L4: document undo journal (#648).
//!
//! The journal is owned by the doors (the facade is rebuilt from bytes on
//! every door call): whole-document snapshots per op, a redo tail that a
//! new op truncates, and a file form that round-trips.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::compat::Journal;

/// One op around two synthetic states.
fn one_op_journal() -> Journal {
    let mut journal = Journal::default();
    journal.enable();
    journal.start_op(Some("op1"), vec![1]);
    journal.stop_op(vec![2]);
    journal
}

#[test]
fn journal_starts_disabled_with_no_steps() {
    let journal = Journal::default();
    assert_eq!(journal.is_enabled(), None);
    assert_eq!(journal.can_do(), (false, false));
    assert_eq!(journal.position(), (0, 0));
    assert_eq!(journal.op_name(0), None);
}

#[test]
fn journal_reports_enabled_once_on() {
    let mut journal = Journal::default();
    journal.enable();
    assert_eq!(journal.is_enabled(), Some(true));
    assert_eq!(journal.can_do(), (false, false));
}

#[test]
fn journal_op_advances_position_and_undo() {
    let journal = one_op_journal();
    assert_eq!(journal.position(), (1, 1));
    assert_eq!(journal.can_do(), (true, false));
    assert_eq!(journal.op_name(0), Some("op1"));
    assert_eq!(journal.op_name(99), None);
    assert_eq!(journal.op_name(-1), None);
}

#[test]
fn journal_undo_redo_restore_each_side() {
    let mut journal = one_op_journal();
    assert_eq!(journal.undo().expect("undo"), vec![1]);
    assert_eq!(journal.can_do(), (false, true));
    assert_eq!(journal.redo().expect("redo"), vec![2]);
    assert_eq!(journal.position(), (1, 1));
}

#[test]
fn journal_undo_without_steps_fails() {
    let mut journal = Journal::default();
    assert!(journal.undo().is_err());
    assert!(journal.redo().is_err());
    journal.enable();
    assert!(journal.undo().is_err());
    assert!(journal.redo().is_err());
}

#[test]
fn journal_stop_without_start_is_a_noop() {
    let mut journal = Journal::default();
    journal.enable();
    journal.stop_op(vec![9]);
    assert_eq!(journal.position(), (0, 0));
}

#[test]
fn journal_nested_start_keeps_the_outer_before_image() {
    let mut journal = Journal::default();
    journal.enable();
    journal.start_op(Some("outer"), vec![1]);
    journal.start_op(Some("inner"), vec![7]);
    journal.stop_op(vec![2]);
    assert_eq!(journal.op_name(0), Some("outer"));
    assert_eq!(journal.undo().expect("undo"), vec![1]);
}

#[test]
fn journal_new_op_truncates_the_redo_tail() {
    let mut journal = one_op_journal();
    journal.undo().expect("undo");
    journal.start_op(Some("op2"), vec![1]);
    journal.stop_op(vec![3]);
    assert_eq!(journal.position(), (1, 1));
    assert_eq!(journal.op_name(0), Some("op2"));
    assert_eq!(journal.can_do(), (true, false));
}

#[test]
fn journal_save_load_round_trips() {
    let journal = one_op_journal();
    let path = std::env::temp_dir().join(format!("pdfluent-journal-{}.bin", std::process::id()));
    journal.save(path.to_str().expect("path")).expect("save");
    let mut loaded = Journal::default();
    loaded.load(path.to_str().expect("path")).expect("load");
    std::fs::remove_file(&path).expect("cleanup");
    assert_eq!(loaded.is_enabled(), Some(true));
    assert_eq!(loaded.position(), (1, 1));
    assert_eq!(loaded.op_name(0), Some("op1"));
    assert_eq!(loaded.undo().expect("undo"), vec![1]);
}

#[test]
fn journal_load_refuses_missing_and_live_journals() {
    let missing =
        std::env::temp_dir().join(format!("pdfluent-no-journal-{}.bin", std::process::id()));
    let _ = std::fs::remove_file(&missing);
    let mut journal = Journal::default();
    assert!(journal.load(missing.to_str().expect("path")).is_err());

    let journal_saved = one_op_journal();
    let path =
        std::env::temp_dir().join(format!("pdfluent-journal-live-{}.bin", std::process::id()));
    journal_saved
        .save(path.to_str().expect("path"))
        .expect("save");
    let mut live = one_op_journal();
    assert!(live.load(path.to_str().expect("path")).is_err());
    std::fs::remove_file(&path).expect("cleanup");
}
