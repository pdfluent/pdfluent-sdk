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

#[test]
fn thrown_string_is_runtime_error() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    let err = rt
        .execute_calculate("throw \"something went wrong\";", ctx)
        .unwrap_err();
    assert!(
        matches!(err, XfaJsError::Runtime(_)),
        "expected Runtime, got {err:?}"
    );
}

#[test]
fn thrown_error_object_is_runtime_error() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    let err = rt
        .execute_calculate("throw new Error(\"oops\");", ctx)
        .unwrap_err();
    let XfaJsError::Runtime(msg) = &err else {
        panic!("expected Runtime, got {err:?}");
    };
    assert!(msg.contains("oops"), "message should contain 'oops': {msg}");
}

#[test]
fn syntax_error_is_runtime_error() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    assert!(matches!(
        rt.execute_calculate("{{{{", ctx),
        Err(XfaJsError::Runtime(_))
    ));
}

#[test]
fn type_error_from_script_is_runtime_error() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    // Calling null as a function causes a TypeError.
    let err = rt.execute_calculate("(null)()", ctx).unwrap_err();
    assert!(
        matches!(err, XfaJsError::Runtime(_)),
        "expected Runtime, got {err:?}"
    );
}

#[test]
fn runtime_is_usable_after_error() {
    let mut rt = fresh();
    // First call: error
    {
        let mut fields = FieldValues::new();
        let ctx = ExecCtx::new(&mut fields, no_cancel());
        let _ = rt.execute_calculate("throw 42;", ctx);
    }
    // Second call: should succeed
    {
        let mut fields = FieldValues::new();
        let ctx = ExecCtx::new(&mut fields, no_cancel());
        let val = rt.execute_calculate("1 + 2", ctx).unwrap();
        use xfa_js_sandboxed::JsValue;
        assert_eq!(val, JsValue::Number(3.0));
    }
}

#[test]
fn divide_by_zero_is_not_an_error_in_js() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    // JS: 1/0 = Infinity, not an exception
    let val = rt.execute_calculate("1 / 0", ctx).unwrap();
    use xfa_js_sandboxed::JsValue;
    // Infinity is a float
    if let JsValue::Number(n) = val {
        assert!(n.is_infinite(), "expected Infinity");
    } else {
        panic!("expected Number(Infinity), got {val:?}");
    }
}
