use std::sync::{atomic::AtomicBool, Arc};
use xfa_js_sandboxed::{ExecCtx, FieldValues, JsValue, XfaJsRuntime};

fn no_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn fresh() -> XfaJsRuntime {
    XfaJsRuntime::new().expect("runtime init")
}

#[test]
fn read_existing_field_via_xfa_form() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    fields.set("Amount", "42");
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    let val = rt
        .execute_calculate("xfa.form.Amount.rawValue", ctx)
        .unwrap();
    assert_eq!(val, JsValue::String("42".to_string()));
}

#[test]
fn write_field_and_verify_flush() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    fields.set("Tax", "0");
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    rt.execute_calculate("xfa.form.Tax.rawValue = \"21.00\"", ctx)
        .unwrap();
    assert_eq!(fields.get("Tax"), Some("21.00"));
}

#[test]
fn script_copies_one_field_to_another() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    fields.set("A", "100");
    fields.set("B", "0");
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    rt.execute_calculate("xfa.form.B.rawValue = xfa.form.A.rawValue;", ctx)
        .unwrap();
    assert_eq!(fields.get("B"), Some("100"));
}

#[test]
fn script_uses_field_value_in_calculation() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    fields.set("Price", "10");
    fields.set("Qty", "3");
    fields.set("Total", "0");
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    rt.execute_calculate(
        "xfa.form.Total.rawValue = Number(xfa.form.Price.rawValue) * Number(xfa.form.Qty.rawValue);",
        ctx,
    )
    .unwrap();
    assert_eq!(fields.get("Total"), Some("30"));
}

#[test]
fn unknown_field_returns_empty_string() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    let val = rt
        .execute_calculate("xfa.form.NonExistent.rawValue", ctx)
        .unwrap();
    // Unknown fields return "" (empty string from HashMap miss)
    assert_eq!(val, JsValue::String(String::new()));
}

#[test]
fn multiple_writes_in_one_script() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    fields.set("X", "0");
    fields.set("Y", "0");
    let ctx = ExecCtx::new(&mut fields, no_cancel());
    rt.execute_calculate("xfa.form.X.rawValue = 10; xfa.form.Y.rawValue = 20;", ctx)
        .unwrap();
    assert_eq!(fields.get("X"), Some("10"));
    assert_eq!(fields.get("Y"), Some("20"));
}

#[test]
fn event_new_text_is_accessible() {
    let mut rt = fresh();
    let mut fields = FieldValues::new();
    fields.set("Out", "");
    let mut ctx = ExecCtx::new(&mut fields, no_cancel());
    ctx.event_new_text = Some("typed-value");
    rt.execute_calculate("xfa.form.Out.rawValue = xfa.event.newText;", ctx)
        .unwrap();
    assert_eq!(fields.get("Out"), Some("typed-value"));
}
