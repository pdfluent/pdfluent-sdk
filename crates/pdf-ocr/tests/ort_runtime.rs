// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The shared ONNX Runtime layer, through its public API (#641).
//!
//! The failure-kind tests need no ONNX Runtime library: a missing model, a
//! missing library and a wrong checksum are all answered before anything is
//! loaded. The tests that load and run the `Add` fixture only run when
//! `PDFLUENT_ORT_TEST_DYLIB` points at a real ONNX Runtime 1.23.2 library --
//! locally `~/.local/share/pdfluent/ort-1.23.2-macos-arm64/
//! libonnxruntime.1.23.2.dylib`, a TEST source only -- and skip otherwise.

#![cfg(feature = "onnx-runtime")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pdf_ocr::runtime::{
    init, model_error, session, ModelError, ModelErrorKind, ModelSpec, RuntimeConfig, SharedSession,
};

/// `SharedSession` crosses threads by design (the pool), and `ModelError`
/// crosses them inside `io::Error`: both must be `Send + Sync`.
fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn shared_session_and_model_error_are_send_and_sync() {
    assert_send_sync::<SharedSession>();
    assert_send_sync::<ModelError>();
}

/// SHA-256 of `tests/fixtures/ort/add/add.onnx`, the committed single-`Add`
/// graph. Regenerate with `tests/fixtures/ort/generate_add_fixture.py`
/// (generator `onnx==1.19.0`) and update this pin with it.
const FIXTURE_SHA256: &str = "9cccf3aa4cc7df7ed0333594fbd6bf2df121ccbbbfeb6f53571cd1fa3e275917";

const FIXTURE_INSTALL: &str = "run crates/pdf-ocr/tests/fixtures/ort/generate_add_fixture.py";

fn fixture_spec() -> ModelSpec {
    ModelSpec::new("add", "add.onnx", FIXTURE_SHA256, FIXTURE_INSTALL)
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ort")
}

/// A config that resolves nothing from the environment: every path is
/// explicit, so these tests answer the same on any machine.
fn config_in(model_dir: &Path, ort_dylib: &Path) -> RuntimeConfig {
    RuntimeConfig::new()
        .with_model_dir(model_dir.to_path_buf())
        .with_ort_dylib(ort_dylib.to_path_buf())
}

#[test]
fn a_missing_model_dir_is_not_available_and_names_the_path_and_install() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let missing = tmp.path().join("no-such-dir");
    let config = config_in(&missing, &tmp.path().join("no-such-lib"));
    // The model is checked before the library: this names the MODEL even
    // though no runtime is installed either.
    let err = session(&fixture_spec(), &config).expect_err("a missing dir must fail");
    assert_eq!(err.kind, ModelErrorKind::ModelNotAvailable);
    assert_eq!(err.kind.code(), "MODEL_NOT_AVAILABLE");
    assert_eq!(err.model.as_deref(), Some("add"));
    let want = missing.join("add").join("add.onnx");
    assert_eq!(err.path, Some(want.clone()));
    let text = err.to_string();
    assert!(
        text.contains(&want.to_string_lossy().into_owned()),
        "display names the path: {text}"
    );
    assert!(
        text.contains(FIXTURE_INSTALL),
        "display names the install command: {text}"
    );
}

#[test]
fn a_missing_runtime_library_is_not_available_and_names_onnxruntime() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let missing_lib = tmp.path().join("no-such-lib.dylib");
    // The model file exists (the committed fixture, right checksum), so this
    // reaches the library -- which is not there.
    let config = config_in(&fixture_dir(), &missing_lib);
    let err = session(&fixture_spec(), &config).expect_err("a missing library must fail");
    assert_eq!(err.kind, ModelErrorKind::ModelNotAvailable);
    assert_eq!(err.model.as_deref(), Some("onnxruntime-1.23.2"));
    assert_eq!(err.path, Some(missing_lib.clone()));
    let text = err.to_string();
    assert!(
        text.contains("onnxruntime-1.23.2"),
        "display names the runtime: {text}"
    );
    assert!(
        text.contains(&missing_lib.to_string_lossy().into_owned()),
        "display names the path looked at: {text}"
    );
}

#[test]
fn a_wrong_checksum_is_a_mismatch_naming_expectation_and_path() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let package = tmp.path().join("add");
    std::fs::create_dir(&package).expect("a package dir");
    let file = package.join("add.onnx");
    std::fs::write(&file, b"these are not the weights").expect("a corrupt weight");
    // The spec pins the REAL fixture's digest, so these bytes must fail.
    let config = config_in(tmp.path(), &tmp.path().join("no-such-lib"));
    let err = session(&fixture_spec(), &config).expect_err("a wrong checksum must fail");
    assert_eq!(err.kind, ModelErrorKind::ChecksumMismatch);
    assert_eq!(err.kind.code(), "MODEL_CHECKSUM_MISMATCH");
    assert_eq!(err.expected_sha256.as_deref(), Some(FIXTURE_SHA256));
    assert_eq!(err.path, Some(file.clone()));
    let text = err.to_string();
    assert!(
        text.contains(FIXTURE_SHA256),
        "display names the expectation: {text}"
    );
    assert!(
        text.contains(&file.to_string_lossy().into_owned()),
        "display names the path: {text}"
    );
}

/// `PDFLUENT_MODEL_DIR` is honoured between the explicit config and the
/// default. The ONLY test that touches the environment: every other test
/// resolves explicitly, so no parallel test can observe this variable.
#[test]
fn the_model_dir_env_override_is_honoured() {
    struct Restore {
        key: &'static str,
        previous: Option<String>,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
    let tmp = tempfile::tempdir().expect("a temp dir");
    let from_env = tmp.path().join("from-env");
    let _restore = Restore {
        key: "PDFLUENT_MODEL_DIR",
        previous: std::env::var("PDFLUENT_MODEL_DIR").ok(),
    };
    std::env::set_var("PDFLUENT_MODEL_DIR", &from_env);
    let config = RuntimeConfig::new().with_ort_dylib(tmp.path().join("no-such-lib"));
    let err = session(&fixture_spec(), &config).expect_err("a missing env dir must fail");
    assert_eq!(err.kind, ModelErrorKind::ModelNotAvailable);
    assert_eq!(err.path, Some(from_env.join("add").join("add.onnx")));
}

#[test]
fn a_model_error_round_trips_through_io_error_and_back() {
    // What pdf-engine carries as `EngineError::Io`: `io::Error::other` keeps
    // the typed cause, and `model_error` walks back to it.
    let tmp = tempfile::tempdir().expect("a temp dir");
    let config = config_in(
        &tmp.path().join("no-such-dir"),
        &tmp.path().join("no-such-lib"),
    );
    let err = session(&fixture_spec(), &config).expect_err("a missing dir must fail");
    let io = std::io::Error::from(err);
    let back = io
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<ModelError>())
        .expect("io::Error::other keeps the typed cause");
    assert_eq!(back.kind, ModelErrorKind::ModelNotAvailable);
    assert_eq!(
        model_error(&io).map(|found| found.kind),
        Some(ModelErrorKind::ModelNotAvailable)
    );
}

/// The real library, when the environment names one. `None` skips the
/// dylib-backed tests: CI sets no such variable, and the step that runs this
/// file without it proves the failure kinds, not the loads.
fn test_dylib() -> Option<PathBuf> {
    match std::env::var("PDFLUENT_ORT_TEST_DYLIB") {
        Ok(path) if !path.is_empty() => Some(PathBuf::from(path)),
        _ => None,
    }
}

#[test]
fn init_is_idempotent_and_rejects_a_different_library() {
    let Some(dylib) = test_dylib() else {
        eprintln!("skipped: PDFLUENT_ORT_TEST_DYLIB is not set");
        return;
    };
    let config = config_in(&fixture_dir(), &dylib);
    init(&config).expect("init succeeds against the real library");
    // Same library again: Ok, no once-guard needed by the caller.
    init(&config).expect("a second init with the same library is Ok");
    // A different EXISTING path: the process keeps the first library.
    // (A missing path would be ModelNotAvailable, not a conflict: it names
    // nothing to conflict with.)
    let other = tempfile::NamedTempFile::new().expect("an existing other file");
    let config = config_in(&fixture_dir(), other.path());
    let err = init(&config).expect_err("a different library must conflict");
    assert_eq!(err.kind, ModelErrorKind::ConflictingInit);
    assert_eq!(err.kind.code(), "RUNTIME_CONFLICTING_INIT");
}

#[test]
fn a_second_session_for_the_same_key_is_the_same_arc() {
    let Some(dylib) = test_dylib() else {
        eprintln!("skipped: PDFLUENT_ORT_TEST_DYLIB is not set");
        return;
    };
    let config = config_in(&fixture_dir(), &dylib);
    let first = session(&fixture_spec(), &config).expect("the fixture loads");
    let second = session(&fixture_spec(), &config).expect("the cached fixture loads");
    assert!(Arc::ptr_eq(&first, &second), "one key, one Arc");
    assert_eq!(first.model_sha256(), FIXTURE_SHA256);
}

#[test]
fn the_add_fixture_runs_and_returns_the_sum() {
    use ort::value::Tensor;

    let Some(dylib) = test_dylib() else {
        eprintln!("skipped: PDFLUENT_ORT_TEST_DYLIB is not set");
        return;
    };
    let config = config_in(&fixture_dir(), &dylib);
    let shared = session(&fixture_spec(), &config).expect("the fixture loads");
    let sum = shared
        .run(|ort_session| {
            let x = Tensor::from_array(ndarray::array![[1.0f32, 2.0], [3.0, 4.0]])?;
            let y = Tensor::from_array(ndarray::array![[10.0f32, 20.0], [30.0, 40.0]])?;
            let outputs = ort_session.run(ort::inputs!["x" => x, "y" => y])?;
            let got = outputs[0].try_extract_array::<f32>()?;
            Ok(got.to_owned())
        })
        .expect("the fixture runs");
    assert_eq!(sum.shape(), &[2, 2]);
    let expected = [11.0f32, 22.0, 33.0, 44.0];
    assert!(
        sum.iter().copied().zip(expected).all(|(a, b)| a == b),
        "x + y must come back element-wise, got {sum:?}"
    );
}

#[test]
fn two_sessions_run_concurrently_while_one_serialises() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    fn spin_until(flag: &AtomicBool, what: &str) {
        let start = Instant::now();
        while !flag.load(Ordering::SeqCst) {
            if start.elapsed() > Duration::from_secs(20) {
                panic!("timed out waiting for {what}");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    let Some(dylib) = test_dylib() else {
        eprintln!("skipped: PDFLUENT_ORT_TEST_DYLIB is not set");
        return;
    };

    // With a pool of 2, both threads are inside `run` at once: A waits for B
    // to arrive, which only happens when B holds the second session. The spin
    // has a timeout so a regression fails instead of hanging the suite.
    let config = config_in(&fixture_dir(), &dylib).with_sessions_per_model(2);
    let shared = session(&fixture_spec(), &config).expect("the pool of 2 loads");
    let entered = std::sync::Arc::new(AtomicBool::new(false));
    let arrived = std::sync::Arc::new(AtomicBool::new(false));
    let probe = std::sync::Arc::clone(&shared);
    let entered_in = std::sync::Arc::clone(&entered);
    let arrived_in = std::sync::Arc::clone(&arrived);
    let a = std::thread::spawn(move || {
        probe
            .run(|_| {
                entered_in.store(true, Ordering::SeqCst);
                spin_until(&arrived_in, "the second thread to enter run");
                Ok::<(), ort::Error>(())
            })
            .expect("A runs");
    });
    spin_until(&entered, "A to enter run");
    let probe = std::sync::Arc::clone(&shared);
    let b = std::thread::spawn(move || {
        probe
            .run(|_| {
                arrived.store(true, Ordering::SeqCst);
                Ok::<(), ort::Error>(())
            })
            .expect("B runs");
    });
    a.join().expect("A joins");
    b.join().expect("B joins");

    // With a pool of 1, the closures never overlap: an atomic swap catches any
    // overlap whatever the schedule, so this half is timing-free.
    let config = config_in(&fixture_dir(), &dylib).with_sessions_per_model(1);
    let shared = session(&fixture_spec(), &config).expect("the pool of 1 loads");
    let inside = std::sync::Arc::new(AtomicBool::new(false));
    let overlap = std::sync::Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let probe = std::sync::Arc::clone(&shared);
        let inside = std::sync::Arc::clone(&inside);
        let overlap = std::sync::Arc::clone(&overlap);
        handles.push(std::thread::spawn(move || {
            probe
                .run(|_| {
                    if inside.swap(true, Ordering::SeqCst) {
                        overlap.store(true, Ordering::SeqCst);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                    inside.store(false, Ordering::SeqCst);
                    Ok::<(), ort::Error>(())
                })
                .expect("serial run");
        }));
    }
    for handle in handles {
        handle.join().expect("a serial thread joins");
    }
    assert!(
        !overlap.load(Ordering::SeqCst),
        "a pool of 1 must serialise"
    );
}
