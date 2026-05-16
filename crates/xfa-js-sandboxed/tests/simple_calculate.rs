use std::sync::{atomic::AtomicBool, Arc};
use xfa_js_sandboxed::{ExecCtx, FieldValues, JsValue, XfaJsRuntime};

fn no_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn fresh() -> XfaJsRuntime {
    XfaJsRuntime::new().expect("runtime init")
}

#[test]
fn arithmetic_result_is_number() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    let val = rt.execute_calculate("1 + 1", ctx).unwrap();
    assert_eq!(val, JsValue::Number(2.0));
}

#[test]
fn calculate_writes_raw_value_via_xfa_form() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    fields.set("Total", "0");
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    rt.execute_calculate("xfa.form.Total.rawValue = 1 + 1", ctx)
        .unwrap();
    assert_eq!(fields.get("Total"), Some("2"));
}

#[test]
fn string_concatenation_result() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    let val = rt.execute_calculate("\"hello\" + \" world\"", ctx).unwrap();
    assert_eq!(val, JsValue::String("hello world".to_string()));
}

#[test]
fn numeric_expression_with_decimal() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    let val = rt.execute_calculate("3.14 * 2", ctx).unwrap();
    assert_eq!(val, JsValue::Number(6.28));
}

#[test]
fn runtime_reusable_across_calls() {
    let mut rt = fresh();
    for i in 0..5_u32 {
        let mut fields = FieldValues::new();
        let ctx = ExecCtx::new(&mut fields, no_cancel());
        let val = rt.execute_calculate(&format!("{i} * {i}"), ctx).unwrap();
        assert_eq!(val, JsValue::Number((i * i) as f64));
    }
}
