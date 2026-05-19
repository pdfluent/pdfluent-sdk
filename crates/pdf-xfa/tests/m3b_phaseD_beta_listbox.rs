#![cfg(feature = "xfa-js-sandboxed")]

//! M3-B Phase D-β integration tests for XFA listbox API (clearItems + addItem).

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{
    HostBindings, QuickJsRuntime, MAX_ITEMS_PER_LISTBOX, MAX_MUTATIONS_PER_DOC,
};
use pdf_xfa::JsExecutionMode;
use xfa_layout_engine::form::{
    EventScript, FormNode, FormNodeId, FormNodeType, FormTree, Occur, ScriptLanguage,
};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

fn add_node(tree: &mut FormTree, name: &str, node_type: FormNodeType) -> FormNodeId {
    tree.add_node(FormNode {
        name: name.to_string(),
        node_type,
        box_model: BoxModel::default(),
        layout: LayoutStrategy::TopToBottom,
        children: Vec::new(),
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: Vec::new(),
        col_span: 1,
    })
}

fn add_child(
    tree: &mut FormTree,
    parent: FormNodeId,
    name: &str,
    node_type: FormNodeType,
) -> FormNodeId {
    let child = add_node(tree, name, node_type);
    tree.get_mut(parent).children.push(child);
    child
}

fn add_field(tree: &mut FormTree, parent: FormNodeId, name: &str, value: &str) -> FormNodeId {
    add_child(
        tree,
        parent,
        name,
        FormNodeType::Field {
            value: value.to_string(),
        },
    )
}

fn add_js_script(tree: &mut FormTree, node_id: FormNodeId, activity: &str, script: &str) {
    tree.meta_mut(node_id).event_scripts = vec![EventScript::new(
        script.to_string(),
        ScriptLanguage::JavaScript,
        Some(activity.to_string()),
        None,
        None,
    )];
}

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> pdf_xfa::DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

fn one_field_form() -> (FormTree, FormNodeId, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let field = add_field(&mut tree, root, "State", "");
    (tree, root, field)
}

fn field_value(tree: &FormTree, node_id: FormNodeId) -> &str {
    match &tree.get(node_id).node_type {
        FormNodeType::Field { value } => value,
        _ => panic!("expected field"),
    }
}

/// Form with a primary field plus a separate `Out` sink the test scripts
/// write their result to. Returned tuple is `(tree, root, primary, out)`.
fn one_field_with_out() -> (FormTree, FormNodeId, FormNodeId, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let primary = add_field(&mut tree, root, "Primary", "");
    let out = add_field(&mut tree, root, "Out", "");
    (tree, root, primary, out)
}

#[test]
fn clear_items_on_empty_list_succeeds() {
    let (mut tree, root, field) = one_field_form();
    add_js_script(&mut tree, field, "calculate", "this.clearItems();");

    let outcome = run_sandbox(&mut tree, root);

    assert!(tree.meta(field).runtime_listbox_items.is_empty());
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_list_writes, 1);
    assert_eq!(outcome.js_binding_errors, 0);
}

#[test]
fn add_item_with_display_and_save_adds_tuple() {
    let (mut tree, root, field) = one_field_form();
    add_js_script(
        &mut tree,
        field,
        "calculate",
        r#"this.addItem("California", "CA");"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    let items = &tree.meta(field).runtime_listbox_items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0], ("California".to_string(), "CA".to_string()));
    assert_eq!(outcome.js_list_writes, 1);
    assert_eq!(outcome.js_binding_errors, 0);
}

#[test]
fn add_item_without_save_uses_display_as_save() {
    let (mut tree, root, field) = one_field_form();
    add_js_script(
        &mut tree,
        field,
        "calculate",
        r#"this.addItem("California");"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    let items = &tree.meta(field).runtime_listbox_items;
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0],
        ("California".to_string(), "California".to_string())
    );
    assert_eq!(outcome.js_list_writes, 1);
}

#[test]
fn clear_items_after_multiple_adds_leaves_empty() {
    let (mut tree, root, field) = one_field_form();
    add_js_script(
        &mut tree,
        field,
        "calculate",
        r#"
this.addItem("A", "1");
this.addItem("B", "2");
this.clearItems();
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert!(tree.meta(field).runtime_listbox_items.is_empty());
    assert_eq!(outcome.js_list_writes, 3);
}

#[test]
fn add_item_past_max_items_per_listbox_returns_err() {
    let (mut tree, root, field) = one_field_form();
    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);

    for _ in 0..MAX_ITEMS_PER_LISTBOX {
        host.reset_per_script(field, Some("calculate"));
        assert_eq!(
            host.list_add(field, "A".to_string(), Some("1".to_string())),
            Ok(())
        );
    }
    host.reset_per_script(field, Some("calculate"));
    assert_eq!(
        host.list_add(field, "overflow".to_string(), Some("ov".to_string())),
        Err(())
    );

    let metadata = host.take_metadata();
    assert_eq!(metadata.list_writes, MAX_ITEMS_PER_LISTBOX as usize);
    assert_eq!(metadata.binding_errors, 1);
}

#[test]
fn clear_items_on_non_field_returns_err() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _subform = add_child(&mut tree, root, "Sub", FormNodeType::Subform);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
var result = Sub.clearItems();
Out.rawValue = result === false ? "err" : "ok";
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(outcome.js_list_writes, 0);
    assert_eq!(outcome.js_binding_errors, 1);
}

#[test]
fn activity_gate_blocks_add_item_outside_allowed_activity() {
    let (mut tree, root, field) = one_field_form();
    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);
    host.reset_per_script(field, Some("click"));

    assert_eq!(
        host.list_add(field, "A".to_string(), Some("1".to_string())),
        Err(())
    );

    let metadata = host.take_metadata();
    assert_eq!(metadata.list_writes, 0);
    assert_eq!(metadata.binding_errors, 1);
}

#[test]
fn mutation_cap_blocks_add_item_at_max_mutations() {
    let (mut tree, root, field) = one_field_form();
    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);

    for _ in 0..MAX_MUTATIONS_PER_DOC {
        host.reset_per_script(field, Some("calculate"));
        assert_eq!(
            host.list_add(field, "A".to_string(), Some("1".to_string())),
            Ok(())
        );
    }
    host.reset_per_script(field, Some("calculate"));
    assert_eq!(
        host.list_add(field, "A".to_string(), Some("1".to_string())),
        Err(())
    );

    let metadata = host.take_metadata();
    assert_eq!(metadata.list_writes, MAX_MUTATIONS_PER_DOC);
    assert_eq!(metadata.binding_errors, 1);
}

#[test]
fn xfa_event_newtext_defaults_to_empty_string() {
    // Phase D-δ.2 regression: scripts that read xfa.event.newText (or
    // event.newText, prevText, change, fullText, selStart, selEnd) on
    // activities where no actual change event fired must see deterministic
    // defaults instead of throwing "cannot read property 'X' of undefined".
    let (mut tree, root, field) = one_field_form();
    add_js_script(
        &mut tree,
        field,
        "calculate",
        r#"
var nt  = xfa.event.newText;
var pt  = xfa.event.prevText;
var ch  = xfa.event.change;
var ft  = xfa.event.fullText;
var ss  = xfa.event.selStart;
var se  = xfa.event.selEnd;
this.rawValue = "[" + nt + "][" + pt + "][" + ch + "][" + ft + "][" + ss + "][" + se + "]";
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, field), "[][][][][0][0]");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn bound_item_returns_save_value_for_known_display() {
    let (mut tree, root, _primary, out) = one_field_with_out();
    let listbox = add_field(&mut tree, root, "Listbox", "");
    {
        let meta = tree.meta_mut(listbox);
        meta.display_items = vec!["ALPHA".into(), "BETA".into(), "GAMMA".into()];
        meta.save_items = vec!["A".into(), "B".into(), "G".into()];
    }
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Listbox.boundItem(\"BETA\");",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "B");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn bound_item_returns_passthrough_for_unknown_display() {
    let (mut tree, root, _primary, out) = one_field_with_out();
    let listbox = add_field(&mut tree, root, "Listbox", "");
    {
        let meta = tree.meta_mut(listbox);
        meta.display_items = vec!["ALPHA".into(), "BETA".into()];
        meta.save_items = vec!["A".into(), "B".into()];
    }
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Listbox.boundItem(\"UNKNOWN\");",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "UNKNOWN");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn bound_item_handles_empty_input_safely() {
    let (mut tree, root, _primary, out) = one_field_with_out();
    let listbox = add_field(&mut tree, root, "Listbox", "");
    {
        let meta = tree.meta_mut(listbox);
        meta.display_items = vec!["ALPHA".into()];
        meta.save_items = vec!["A".into()];
    }
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = String(Listbox.boundItem(xfa.event.newText));",
    );

    let outcome = run_sandbox(&mut tree, root);

    // xfa.event.newText defaults to "" → boundItem("") returns "" passthrough.
    assert_eq!(field_value(&tree, out), "");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn bound_item_prefers_runtime_items_over_static() {
    // Order-independent variant: pre-populate `runtime_listbox_items`
    // directly so the test does not depend on the script-dispatch order
    // between two sibling fields. The original variant (one script
    // populating, another reading) was sensitive to the dispatcher
    // walking nodes in form-tree order.
    let (mut tree, root, _primary, out) = one_field_with_out();
    let listbox = add_field(&mut tree, root, "Listbox", "");
    {
        let meta = tree.meta_mut(listbox);
        meta.display_items = vec!["ALPHA".into()];
        meta.save_items = vec!["A".into()];
        meta.runtime_listbox_items = vec![("ALPHA".to_string(), "RUNTIME_A".to_string())];
    }
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Listbox.boundItem(\"ALPHA\");",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "RUNTIME_A");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn bound_item_does_not_mutate_listbox_items() {
    let (mut tree, root, _primary, out) = one_field_with_out();
    let listbox = add_field(&mut tree, root, "Listbox", "");
    {
        let meta = tree.meta_mut(listbox);
        meta.display_items = vec!["ALPHA".into()];
        meta.save_items = vec!["A".into()];
    }
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Listbox.boundItem(\"ALPHA\");",
    );

    let _ = run_sandbox(&mut tree, root);

    let meta = tree.meta(listbox);
    assert_eq!(meta.display_items, vec!["ALPHA".to_string()]);
    assert_eq!(meta.save_items, vec!["A".to_string()]);
    assert!(meta.runtime_listbox_items.is_empty());
}

#[test]
fn variables_script_global_is_visible_to_event_scripts() {
    // Phase D-ι regression: a <variables> <script name="X"> block's
    // top-level var/function declarations must be visible to subsequent
    // event/calculate scripts as `X.<topLevelDecl>`. Without D-ι, the
    // namespace is undefined and chained access throws.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    tree.variables_scripts.push((
        None,
        "Helpers".into(),
        r#"
var STATES = ["AK","AL","AZ"];
function greet() { return "hi"; }
"#
        .into(),
    ));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
this.rawValue = Helpers.STATES[1] + ":" + Helpers.greet();
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "AL:hi");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn variables_script_globals_clear_between_documents() {
    // Phase D-ι regression: registered <variables> namespaces must not
    // survive `reset_per_document`. Otherwise a previous flatten could
    // leak its `Helpers` object into the next.
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");

    // Document 1: register `Helpers`, run a script that uses it.
    {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        tree.variables_scripts
            .push((None, "Helpers".into(), "var X = 42;".into()));
        let out = add_field(&mut tree, root, "Out", "");
        add_js_script(
            &mut tree,
            out,
            "calculate",
            "this.rawValue = String(Helpers.X);",
        );
        apply_dynamic_scripts_with_runtime(
            &mut tree,
            root,
            JsExecutionMode::SandboxedRuntime,
            &mut runtime,
        )
        .expect("doc1 dispatch");
        assert_eq!(field_value(&tree, out), "42");
    }

    // Document 2: NO `Helpers` registered. Script reading `Helpers` must
    // see undefined (D-ι globals from doc1 must have been cleared).
    {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let out = add_field(&mut tree, root, "Out", "");
        add_js_script(
            &mut tree,
            out,
            "calculate",
            "this.rawValue = (typeof Helpers === \"undefined\") ? \"clean\" : \"leaked\";",
        );
        apply_dynamic_scripts_with_runtime(
            &mut tree,
            root,
            JsExecutionMode::SandboxedRuntime,
            &mut runtime,
        )
        .expect("doc2 dispatch");
        assert_eq!(field_value(&tree, out), "clean");
    }
}

#[test]
fn variables_script_runaway_body_does_not_hang_flatten() {
    // Phase D-ι, Codex P1 review on PR #1499: a malicious or buggy
    // `<variables>` body containing `while (true) {}` must not be able
    // to hang flatten. The script-time deadline + interrupt handler
    // applied to event scripts must also apply to variables-script
    // registration in `set_form_handle`.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    tree.variables_scripts
        .push((None, "RogueScript".into(), "while (true) {}".into()));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = (typeof RogueScript === \"undefined\") ? \"absorbed\" : \"leaked\";",
    );

    let start = std::time::Instant::now();
    let outcome = run_sandbox(&mut tree, root);
    let elapsed = start.elapsed();

    // Default per-script time budget is 250 ms; with a small overhead the
    // entire dispatch should finish well under a few seconds. We assert a
    // generous cap so noisy CI runners do not flake.
    assert!(
        elapsed.as_secs() < 10,
        "flatten took too long despite runaway variables script: {:?}",
        elapsed
    );
    // The runaway script timed out so its namespace was not registered;
    // event scripts see `RogueScript === undefined` and the calculate
    // script writes "absorbed".
    assert_eq!(field_value(&tree, out), "absorbed");
    // The interrupt may bubble up as a runtime error counted at
    // registration time — what matters is no hang and no leak.
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn variables_script_oversized_body_is_rejected() {
    // Phase D-ι, Codex P1 review on PR #1499: variables-script bodies
    // exceeding `MAX_VARIABLES_SCRIPT_BODY_BYTES` must be rejected,
    // mirroring the execute_script path.
    //
    // W2-B: variables-scripts use the dedicated, higher cap
    // `MAX_VARIABLES_SCRIPT_BODY_BYTES` so real-world XFA helper
    // libraries (e.g. `validateForm` ≈ 125 KB, `LOV` ≈ 507 KB) still
    // register. This test exercises the *upper* cap by constructing a
    // body strictly larger than it.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let unit = "var x = 1;\n"; // 11 bytes
    let target_bytes = pdf_xfa::js_runtime::MAX_VARIABLES_SCRIPT_BODY_BYTES + 1;
    let repeats = target_bytes.div_ceil(unit.len());
    let huge = unit.repeat(repeats);
    assert!(huge.len() > pdf_xfa::js_runtime::MAX_VARIABLES_SCRIPT_BODY_BYTES);
    tree.variables_scripts.push((None, "Huge".into(), huge));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = (typeof Huge === \"undefined\") ? \"absorbed\" : \"leaked\";",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "absorbed");
    assert_eq!(outcome.js_executed, 1);
}
