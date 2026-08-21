// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! Constructing an OCR engine must never fetch anything by itself.
//!
//! Until 2026-08-19 it did: `PaddleOcrEngine::new()` downloaded up to ~84 MB of
//! ONNX weights from a third-party public repository, unverified, the first time
//! anyone built an engine. These tests pin the replacement behaviour so it cannot
//! quietly come back — the default must be an error that names the missing files,
//! not a network call.
//!
//! Deliberately no network access here. A test that needed the internet to prove we
//! do not use the internet would be skipped on an offline runner, and a skipped
//! security test is worse than none.

#![cfg(feature = "paddle")]

use std::collections::BTreeMap;

use pdf_ocr::paddle::models::{ensure_models, ModelSource};
use pdf_ocr::paddle::PaddleOcrConfig;

fn config_in(dir: &std::path::Path) -> PaddleOcrConfig {
    PaddleOcrConfig {
        model_dir: dir.to_path_buf(),
        ..Default::default()
    }
}

#[test]
fn the_default_source_is_local_only() {
    // If this ever flips, every other guarantee here is void.
    assert_eq!(
        PaddleOcrConfig::default().model_source,
        ModelSource::LocalOnly,
        "the default must not reach the network"
    );
}

#[test]
fn missing_models_are_an_error_not_a_download() {
    let dir = tempfile::tempdir().expect("tempdir");
    let err = ensure_models(&config_in(dir.path())).expect_err("must refuse");
    let msg = err.to_string();

    assert!(
        msg.contains("Nothing was downloaded"),
        "the error must say plainly that no fetch happened; got: {msg}"
    );
    // And nothing may have appeared on disk as a side effect.
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .expect("readable")
        .filter_map(Result::ok)
        .collect();
    assert!(
        entries.is_empty(),
        "ensure_models wrote {} entries into the model dir",
        entries.len()
    );
}

#[test]
fn the_error_names_the_missing_files_and_where_to_put_them() {
    let dir = tempfile::tempdir().expect("tempdir");
    let msg = ensure_models(&config_in(dir.path()))
        .expect_err("must refuse")
        .to_string();

    // A user hitting this needs to know what to obtain and where it goes.
    assert!(
        msg.contains("det.onnx"),
        "should name the detection model: {msg}"
    );
    assert!(
        msg.contains("rec.onnx"),
        "should name the recognition model: {msg}"
    );
    assert!(
        msg.contains(&dir.path().display().to_string()),
        "should name the directory it looked in: {msg}"
    );
}

#[test]
fn an_unpinned_file_is_refused_before_any_request() {
    // Verified source, but no digest for the files it would need. This must fail
    // on the missing pin rather than fetching and hoping.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut config = config_in(dir.path());
    config.model_source = ModelSource::Verified {
        // Unroutable by construction: if this ever performs a request, the test
        // fails on a connection error instead of passing for the wrong reason.
        base_url: "http://127.0.0.1:1/nothing-here".to_string(),
        digests: BTreeMap::new(),
    };

    let msg = ensure_models(&config).expect_err("must refuse").to_string();
    assert!(
        msg.contains("no sha256 pinned"),
        "must refuse on the missing pin, not on the failed request; got: {msg}"
    );
}
