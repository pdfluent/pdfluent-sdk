use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use xfa_js_sandboxed::{ExecCtx, FieldValues, XfaJsError, XfaJsRuntime};

fn fresh() -> XfaJsRuntime {
    XfaJsRuntime::new().expect("runtime init")
}

#[test]
fn pre_cancelled_token_returns_cancelled() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let cancel = Arc::new(AtomicBool::new(true)); // already cancelled
    let ctx = ExecCtx::new(&mut fields, cancel);
    assert!(matches!(
        rt.execute_calculate("1 + 1", ctx),
        Err(XfaJsError::Cancelled)
    ));
}

#[test]
fn cancel_during_infinite_loop() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let cancel = Arc::new(AtomicBool::new(false));

    // Set the cancel flag to true from a different thread while the script runs.
    let cancel_clone = Arc::clone(&cancel);
    let handle = std::thread::spawn(move || {
        // Give the script a moment to start executing.
        std::thread::sleep(std::time::Duration::from_millis(10));
        cancel_clone.store(true, Ordering::Relaxed);
    });

    let ctx = ExecCtx::new(&mut fields, cancel);
    let result = rt.execute_calculate("while (true) {}", ctx);

    handle.join().unwrap();
    assert!(
        matches!(result, Err(XfaJsError::Cancelled)),
        "expected Cancelled, got {result:?}"
    );
}

#[test]
fn cancel_flag_cleared_between_calls() {
    let mut rt = fresh();

    // First call: cancelled
    {
        let mut fields = FieldValues::new();
        let cancel = Arc::new(AtomicBool::new(true));
        let ctx = ExecCtx::new(&mut fields, cancel);
        assert!(matches!(
            rt.execute_calculate("1 + 1", ctx),
            Err(XfaJsError::Cancelled)
        ));
    }

    // Second call with a fresh non-cancelled token: should succeed.
    {
        let mut fields = FieldValues::new();
        let cancel = Arc::new(AtomicBool::new(false));
        let ctx = ExecCtx::new(&mut fields, cancel);
        use xfa_js_sandboxed::JsValue;
        let val = rt.execute_calculate("7 * 6", ctx).unwrap();
        assert_eq!(val, JsValue::Number(42.0));
    }
}

#[test]
fn cancel_does_not_corrupt_field_state() {
    let mut rt = fresh();

    // Prime with a successful write.
    {
        let mut fields = FieldValues::new();
        fields.set("Counter", "0");
        let cancel = Arc::new(AtomicBool::new(false));
        let ctx = ExecCtx::new(&mut fields, cancel);
        rt.execute_calculate("xfa.form.Counter.rawValue = 5;", ctx)
            .unwrap();
        // Nothing to check here — just warming up.
    }

    // Cancelled call should not write anything.
    {
        let mut fields = FieldValues::new();
        fields.set("Counter", "999");
        let cancel = Arc::new(AtomicBool::new(true));
        let ctx = ExecCtx::new(&mut fields, cancel);
        let _ = rt.execute_calculate("xfa.form.Counter.rawValue = 0;", ctx);
        // Value should be unchanged because execution never started.
        assert_eq!(fields.get("Counter"), Some("999"));
    }
}
