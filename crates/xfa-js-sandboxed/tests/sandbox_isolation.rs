// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::sync::{atomic::AtomicBool, Arc};
use xfa_js_sandboxed::{ExecCtx, FieldValues, XfaJsError, XfaJsRuntime};

fn no_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn fresh() -> XfaJsRuntime {
    XfaJsRuntime::new().expect("runtime init")
}

fn run(rt: &mut XfaJsRuntime, script: &str) -> Result<xfa_js_sandboxed::JsValue, XfaJsError> {
    let mut fields = FieldValues::new();
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    rt.execute_calculate(script, ctx)
}

// ── negative cases: each must return UnsupportedHostCapability ───────────────

#[test]
fn require_fs_is_denied() {
    let mut rt = fresh();
    let err = run(&mut rt, "require('fs')").unwrap_err();
    assert!(
        matches!(err, XfaJsError::UnsupportedHostCapability(_)),
        "expected UnsupportedHostCapability, got {err:?}"
    );
}

#[test]
fn require_path_is_denied() {
    let mut rt = fresh();
    let err = run(&mut rt, "require('path')").unwrap_err();
    assert!(
        matches!(err, XfaJsError::UnsupportedHostCapability(_)),
        "expected UnsupportedHostCapability, got {err:?}"
    );
}

#[test]
fn process_access_is_denied() {
    let mut rt = fresh();
    let err = run(&mut rt, "process.env").unwrap_err();
    assert!(
        matches!(err, XfaJsError::UnsupportedHostCapability(_)),
        "expected UnsupportedHostCapability, got {err:?}"
    );
}

#[test]
fn process_exit_is_denied() {
    let mut rt = fresh();
    let err = run(&mut rt, "process.exit(0)").unwrap_err();
    assert!(
        matches!(err, XfaJsError::UnsupportedHostCapability(_)),
        "expected UnsupportedHostCapability, got {err:?}"
    );
}

#[test]
fn fetch_is_denied() {
    let mut rt = fresh();
    let err = run(&mut rt, "fetch('http://example.com')").unwrap_err();
    assert!(
        matches!(err, XfaJsError::UnsupportedHostCapability(_)),
        "expected UnsupportedHostCapability, got {err:?}"
    );
}

#[test]
fn global_this_fetch_is_denied() {
    let mut rt = fresh();
    let err = run(&mut rt, "globalThis.fetch('http://evil.example')").unwrap_err();
    assert!(
        matches!(err, XfaJsError::UnsupportedHostCapability(_)),
        "expected UnsupportedHostCapability, got {err:?}"
    );
}

#[test]
fn xmlhttprequest_is_denied() {
    let mut rt = fresh();
    let err = run(&mut rt, "new XMLHttpRequest()").unwrap_err();
    assert!(
        matches!(err, XfaJsError::UnsupportedHostCapability(_)),
        "expected UnsupportedHostCapability, got {err:?}"
    );
}

#[test]
fn fetch_with_file_url_is_denied() {
    let mut rt = fresh();
    let err = run(&mut rt, "fetch('file:///etc/passwd')").unwrap_err();
    assert!(
        matches!(err, XfaJsError::UnsupportedHostCapability(_)),
        "expected UnsupportedHostCapability, got {err:?}"
    );
}

// ── positive case: xfa.form still works after a denied call ─────────────────

#[test]
fn runtime_recovers_after_denied_capability() {
    let mut rt = fresh();
    // First: denied
    let _ = run(&mut rt, "require('evil')");
    // Then: normal calculation succeeds
    let mut fields = FieldValues::new();
    fields.set("V", "5");
    let cancel = no_cancel();
    let ctx = ExecCtx::new(&mut fields, cancel);
    let val = rt
        .execute_calculate("Number(xfa.form.V.rawValue) + 1", ctx)
        .unwrap();
    use xfa_js_sandboxed::JsValue;
    assert_eq!(val, JsValue::Number(6.0));
}

// ── property: no __XFA_SANDBOX_DENY__ leakage in success path ───────────────

#[test]
fn deny_marker_not_present_in_normal_strings() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    let val = rt.execute_calculate("\"hello\"", ctx).unwrap();
    let s = match &val {
        xfa_js_sandboxed::JsValue::String(s) => s.clone(),
        other => panic!("expected String, got {other:?}"),
    };
    assert!(!s.contains("__XFA_SANDBOX_DENY__"));
}
