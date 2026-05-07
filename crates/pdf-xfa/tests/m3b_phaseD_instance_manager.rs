#![cfg(feature = "xfa-js-sandboxed")]

//! M3-B Phase D integration tests for XFA instanceManager host bindings.

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{
    HostBindings, QuickJsRuntime, XfaJsRuntime, MAX_INSTANCES_PER_SUBFORM, MAX_MUTATIONS_PER_DOC,
};
use pdf_xfa::JsExecutionMode;
use xfa_dom_resolver::data_dom::{DataDom, DataNodeId};
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

fn add_row(
    tree: &mut FormTree,
    root: FormNodeId,
    min: u32,
    max: Option<u32>,
) -> (FormNodeId, FormNodeId) {
    let row = add_child(tree, root, "Row", FormNodeType::Subform);
    tree.get_mut(row).occur = Occur::repeating(min, max, 1);
    let value = add_field(tree, row, "Value", "");
    (row, value)
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

fn field_value(tree: &FormTree, node_id: FormNodeId) -> &str {
    match &tree.get(node_id).node_type {
        FormNodeType::Field { value } => value,
        _ => panic!("expected field"),
    }
}

fn live_children_named(tree: &FormTree, parent: FormNodeId, name: &str) -> Vec<FormNodeId> {
    tree.get(parent)
        .children
        .iter()
        .copied()
        .filter(|child_id| tree.get(*child_id).name == name)
        .collect()
}

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> pdf_xfa::DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

fn run_sandbox_with_data(
    tree: &mut FormTree,
    root: FormNodeId,
    data_dom: &DataDom,
) -> pdf_xfa::DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    runtime.set_data_handle(data_dom as *const DataDom);
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

fn data_dom_from_xml(xml: &str) -> DataDom {
    DataDom::from_xml(xml).expect("data dom parse")
}

fn find_group_named(dom: &DataDom, start: DataNodeId, name: &str) -> Option<DataNodeId> {
    if dom
        .get(start)
        .is_some_and(|node| node.name() == name && node.is_group())
    {
        return Some(start);
    }
    for &child in dom.children(start) {
        if let Some(found) = find_group_named(dom, child, name) {
            return Some(found);
        }
    }
    None
}

fn one_row_form(min: u32, max: Option<u32>) -> (FormTree, FormNodeId, FormNodeId, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let (row, _value) = add_row(&mut tree, root, min, max);
    let out = add_field(&mut tree, root, "Out", "");
    (tree, root, row, out)
}

#[test]
fn instance_manager_count_single_prototype_is_one() {
    let (mut tree, root, _row, out) = one_row_form(0, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Row.instanceManager.count;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "1");
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_instance_writes, 0);
}

#[test]
fn set_instances_three_updates_count() {
    let (mut tree, root, _row, out) = one_row_form(0, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
var count = Row.instanceManager.setInstances(3);
Out.rawValue = count + ":" + Row.instanceManager.count;
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "3:3");
    assert_eq!(live_children_named(&tree, root, "Row").len(), 3);
    assert_eq!(outcome.js_instance_writes, 1);
}

#[test]
fn set_instances_zero_clamps_to_min_occur() {
    let (mut tree, root, _row, out) = one_row_form(1, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"Out.rawValue = Row.instanceManager.setInstances(0);"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "1");
    assert_eq!(live_children_named(&tree, root, "Row").len(), 1);
    assert_eq!(outcome.js_instance_writes, 1);
}

#[test]
fn set_instances_one_reduces_existing_run() {
    let (mut tree, root, _row, out) = one_row_form(0, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
Row.instanceManager.setInstances(3);
Out.rawValue = Row.instanceManager.setInstances(1);
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "1");
    assert_eq!(live_children_named(&tree, root, "Row").len(), 1);
    assert_eq!(outcome.js_instance_writes, 2);
}

#[test]
fn set_instances_clamps_to_safety_cap() {
    let (mut tree, root, _row, out) = one_row_form(0, None);
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"Out.rawValue = Row.instanceManager.setInstances(1000);"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(
        field_value(&tree, out),
        MAX_INSTANCES_PER_SUBFORM.to_string()
    );
    assert_eq!(
        live_children_named(&tree, root, "Row").len(),
        MAX_INSTANCES_PER_SUBFORM as usize
    );
    assert_eq!(outcome.js_instance_writes, 1);
}

#[test]
fn add_instance_increments_count_and_returns_new_handle() {
    let (mut tree, root, _row, out) = one_row_form(0, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
var added = Row.instanceManager.addInstance();
added.Value.rawValue = "cloned";
Out.rawValue = Row.instanceManager.count + ":" + added.index;
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    let rows = live_children_named(&tree, root, "Row");
    let cloned_value = tree.get(rows[1]).children[0];

    assert_eq!(field_value(&tree, out), "2:1");
    assert_eq!(rows.len(), 2);
    assert_eq!(tree.get(rows[1]).name, "Row");
    assert_eq!(field_value(&tree, cloned_value), "cloned");
    assert_eq!(outcome.js_instance_writes, 1);
}

#[test]
fn remove_instance_decrements_and_refuses_at_min() {
    let (mut tree, root, _row, out) = one_row_form(1, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
Row.instanceManager.setInstances(2);
var removed = Row.instanceManager.removeInstance(1);
var refused = Row.instanceManager.removeInstance(0);
Out.rawValue = Row.instanceManager.count + ":" + removed + ":" + refused;
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "1:true:false");
    assert_eq!(live_children_named(&tree, root, "Row").len(), 1);
    assert_eq!(outcome.js_instance_writes, 2);
    assert_eq!(outcome.js_binding_errors, 1);
}

#[test]
fn mutation_cap_blocks_excessive_instance_writes() {
    let (mut tree, root, row, _out) = one_row_form(0, None);
    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);

    for _ in 0..MAX_MUTATIONS_PER_DOC {
        host.reset_per_script(row, Some("calculate"));
        assert_eq!(host.instance_set(row, 1), Ok(1));
    }
    host.reset_per_script(row, Some("calculate"));
    assert_eq!(host.instance_set(row, 1), Err(()));

    let metadata = host.take_metadata();
    assert_eq!(metadata.instance_writes, MAX_MUTATIONS_PER_DOC);
    assert_eq!(metadata.binding_errors, 1);
}

#[test]
fn activity_gate_blocks_instance_mutation_outside_allowed_activity() {
    let (mut tree, root, row, _out) = one_row_form(0, Some(10));
    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);
    host.reset_per_script(row, Some("click"));

    assert_eq!(host.instance_add(row), Err(()));

    let metadata = host.take_metadata();
    assert_eq!(metadata.instance_writes, 0);
    assert_eq!(metadata.binding_errors, 1);
}

#[test]
fn node_index_reports_first_and_last_same_name_sibling() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let (first, _first_value) = add_row(&mut tree, root, 0, Some(10));
    let (_second, _second_value) = add_row(&mut tree, root, 0, Some(10));
    let (last, _last_value) = add_row(&mut tree, root, 0, Some(10));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        first,
        "calculate",
        r#"Out.rawValue = this.index;"#,
    );
    add_js_script(
        &mut tree,
        last,
        "calculate",
        r#"Out.rawValue = Out.rawValue + ":" + this.index + ":" + this.instanceManager.count;"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "0:2:3");
    assert_eq!(outcome.js_instance_writes, 0);
    assert_eq!(outcome.js_binding_errors, 0);
}

#[test]
fn underscore_shorthand_resolves_before_handle_property_deferral() {
    // Regression: the `_<child>` underscore shorthand on a subform proxy must
    // run *before* `shouldDeferHandleProperty`, otherwise every underscore-
    // prefixed access (e.g. `parent._Row.count`) returns `undefined` and
    // throws on the next chained property read.
    //
    // Pre-fix, `Time_Confirmation._BodyRow.count` failed with
    // "cannot read property 'count' of undefined" on real corpus docs even
    // though the form tree had `Time_Confirmation.BodyRow` populated.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let parent = add_child(&mut tree, root, "Parent", FormNodeType::Subform);
    let row = add_child(&mut tree, parent, "Row", FormNodeType::Subform);
    tree.get_mut(row).occur = Occur::repeating(1, Some(10), 1);
    let _row_value = add_field(&mut tree, row, "Value", "");
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Parent._Row.count;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "1");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn implicit_resolver_prefers_populated_sibling_over_empty_stub() {
    // Phase D-η regression: when a form has multiple subforms with the
    // same name (a common authoring pattern producing layout stubs +
    // populated content), implicit identifier resolution must pick the
    // populated one (children > 0) rather than the first DFS hit.
    //
    // Real-world repro: 13275420c3c9afbb has four subforms named "F",
    // three of which are empty layout stubs and one (with 14 children)
    // is the meaningful one with `pageSet`, `P1`, `P2`, … as direct
    // children. Pre-D-η, `F.P1.X` chained through the stub and threw
    // "cannot read property 'P1' of undefined".
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    // Empty F stub first — the position the pre-D-η resolver picked.
    let _stub = add_child(&mut tree, root, "Container", FormNodeType::Subform);
    // Populated F second — what scripts mean.
    let populated = add_child(&mut tree, root, "Container", FormNodeType::Subform);
    let _payload = add_field(&mut tree, populated, "Payload", "WANT");
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Container.Payload.rawValue;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "WANT");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn implicit_resolver_returns_self_when_script_node_matches_name() {
    // Phase D-η regression: when a script inside subform `F` reads the
    // bare identifier `F`, that must resolve to the enclosing `F`, not
    // to an inner empty-stub same-named descendant. This is the case for
    // 13275420c3c9afbb where node 1423 (the populated F) is itself the
    // script's lexical `this` ancestor and contains stub `F` descendants.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let outer_form = add_child(&mut tree, root, "F", FormNodeType::Subform);
    let p1 = add_child(&mut tree, outer_form, "P1", FormNodeType::Subform);
    let _payload = add_field(&mut tree, p1, "Payload", "FOUND");
    // Inner stub also named F — must NOT win the resolution.
    let _inner_stub = add_child(&mut tree, outer_form, "F", FormNodeType::Subform);
    // Script anchored on a deep descendant of `outer_form`.
    let trigger = add_field(&mut tree, p1, "Trigger", "");
    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        "this.rawValue = F.P1.Payload.rawValue;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, trigger), "FOUND");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn implicit_resolver_falls_back_to_stub_when_only_stubs_exist() {
    // Phase D-η: if all same-name candidates are empty stubs and no
    // ancestor matches, Pass 3 returns the first stub so that property
    // reads like `.somExpression` or `.presence` still resolve to a
    // valid handle (matching pre-D-η behaviour for stub-only forms).
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _stub_a = add_child(&mut tree, root, "Placeholder", FormNodeType::Subform);
    let _stub_b = add_child(&mut tree, root, "Placeholder", FormNodeType::Subform);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = (Placeholder ? \"resolved\" : \"missing\");",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "resolved");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn implicit_resolver_prefers_field_over_sibling_container() {
    // Phase D-η, Codex review feedback: when same-name candidates include
    // a Field and a sibling subform/container, the Field must win.
    // `Amount.rawValue` means the field's value, not a container that
    // happens to share the name; picking the container would silently
    // return null for reads and refuse writes via set_raw_value.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let amount_subform = add_child(&mut tree, root, "Amount", FormNodeType::Subform);
    let _filler = add_field(&mut tree, amount_subform, "Filler", "");
    let amount_field = add_field(&mut tree, root, "Amount", "42");
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Amount.rawValue;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "42");
    assert_eq!(field_value(&tree, amount_field), "42");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn resolver_ambiguous_first_segment_picks_candidate_matching_second_segment() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _empty = add_child(&mut tree, root, "A", FormNodeType::Subform);
    let populated = add_child(&mut tree, root, "A", FormNodeType::Subform);
    let _b = add_field(&mut tree, populated, "B", "lookahead");
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(&mut tree, out, "calculate", "Out.rawValue = A.B.rawValue;");

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "lookahead");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn resolver_two_populated_candidates_lookahead_picks_correct_branch() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let wrong = add_child(&mut tree, root, "A", FormNodeType::Subform);
    let _wrong_child = add_field(&mut tree, wrong, "C", "wrong");
    let right = add_child(&mut tree, root, "A", FormNodeType::Subform);
    let _right_child = add_field(&mut tree, right, "B", "right");
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(&mut tree, out, "calculate", "Out.rawValue = A.B.rawValue;");

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "right");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn resolver_duplicate_child_candidates_survive_until_next_segment() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let parent = add_child(&mut tree, root, "Parent", FormNodeType::Subform);
    let wrong = add_child(&mut tree, parent, "Row", FormNodeType::Subform);
    let _wrong_child = add_field(&mut tree, wrong, "Other", "wrong");
    let right = add_child(&mut tree, parent, "Row", FormNodeType::Subform);
    let _right_child = add_field(&mut tree, right, "Value", "right");
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Parent.Row.Value.rawValue;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "right");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn resolver_chain_one_token_unchanged_when_no_lookahead() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let amount_container = add_child(&mut tree, root, "Amount", FormNodeType::Subform);
    let _nested = add_field(&mut tree, amount_container, "Nested", "wrong");
    let _amount = add_field(&mut tree, root, "Amount", "42");
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Amount.rawValue;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "42");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn resolver_unresolvable_chain_fails_soft() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _a = add_child(&mut tree, root, "A", FormNodeType::Subform);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
var b = A.B;
Out.rawValue = (b === undefined ? "undefined" : b.C.rawValue);
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "undefined");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn resolver_scoped_fallback_handles_ancestor_segment_inside_chain() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let parent = add_child(&mut tree, root, "Parent", FormNodeType::Subform);
    let child = add_child(&mut tree, parent, "Child", FormNodeType::Subform);
    let _target = add_field(&mut tree, parent, "Target", "ancestor");
    let out = add_field(&mut tree, root, "Out", "");
    let trigger = add_field(&mut tree, child, "Trigger", "");
    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        "Out.rawValue = Child.Parent.Target.rawValue;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "ancestor");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn resolver_preserves_underscore_shorthand() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let parent = add_child(&mut tree, root, "Parent", FormNodeType::Subform);
    let child = add_child(&mut tree, parent, "Child", FormNodeType::Subform);
    tree.get_mut(child).occur = Occur::repeating(0, Some(10), 1);
    let _value = add_field(&mut tree, child, "Value", "");
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
Parent._Child.setInstances(0);
Out.rawValue = Parent._Child.count;
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "0");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_instance_writes, 1);
}

#[test]
fn resolver_preserves_dollar_record() {
    let dom = data_dom_from_xml(
        r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
             <xfa:data>
               <Root><X>from-data</X></Root>
             </xfa:data>
           </xfa:datasets>"#,
    );
    let data_root = dom.root().expect("root");
    let bound = find_group_named(&dom, data_root, "Root").unwrap_or(data_root);

    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let out = add_field(&mut tree, root, "Out", "");
    tree.meta_mut(out).bound_data_node = Some(bound.as_raw());
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = $record.X.value;",
    );

    let outcome = run_sandbox_with_data(&mut tree, root, &dom);

    assert_eq!(field_value(&tree, out), "from-data");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert!(outcome.js_data_reads > 0);
}

#[test]
fn resolver_preserves_variables_globals() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    tree.variables_scripts.push((
        None,
        "Helpers".into(),
        r#"
var VALUE = "from-vars";
function suffix() { return "-ok"; }
"#
        .into(),
    ));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Helpers.VALUE + Helpers.suffix();",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "from-vars-ok");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn resolver_preserves_clean_doc_333f4a55_path() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let clean = add_child(&mut tree, root, "Clean", FormNodeType::Subform);
    let section = add_child(&mut tree, clean, "Section", FormNodeType::Subform);
    let _value = add_field(&mut tree, section, "Value", "clean");
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Clean.Section.Value.rawValue;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "clean");
    assert_eq!(outcome.js_runtime_errors, 0);
}

// Gap B: $record null-coalesce regression tests (XFA §8.5)
// When no bound data node exists, $record must return a safe sentinel that
// chains without TypeError instead of returning null.

#[test]
fn record_no_binding_nodes_length_is_zero() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = String($record.Section.nodes.length);",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "0");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn record_no_binding_chained_value_is_null() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = String($record.Podmiot1.value);",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "null");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn record_no_binding_nodes_item_chains_safely() {
    // nodes.item(0) must return a chainable sentinel so that
    // $record.Section.nodes.item(0).value does not throw TypeError.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = String($record.Section.nodes.item(0).value);",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "null");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn record_real_data_binding_still_resolves() {
    // Regression guard: when a DataDom IS present and bound, $record.Podmiot1.value
    // must still return the real bound value (not the null sentinel).
    let dom = data_dom_from_xml(
        r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
             <xfa:data>
               <Root><Podmiot1>real-value</Podmiot1></Root>
             </xfa:data>
           </xfa:datasets>"#,
    );
    let data_root = dom.root().expect("root");
    let bound = find_group_named(&dom, data_root, "Root").unwrap_or(data_root);

    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let out = add_field(&mut tree, root, "Out", "");
    tree.meta_mut(out).bound_data_node = Some(bound.as_raw());
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = String($record.Podmiot1.value);",
    );

    let outcome = run_sandbox_with_data(&mut tree, root, &dom);

    assert_eq!(field_value(&tree, out), "real-value");
    assert_eq!(outcome.js_runtime_errors, 0);
}

// Phase D-ι.2 — subform-scoped <variables> tests

#[test]
fn subform_variables_accessible_via_variables_property() {
    // `Page2.variables.ValidationScript.getCounty()` must resolve when
    // ValidationScript is registered as a Page2-scoped variables entry.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _page2 = add_child(&mut tree, root, "Page2", FormNodeType::Subform);
    let out = add_field(&mut tree, root, "Out", "");
    tree.variables_scripts.push((
        Some("Page2".into()),
        "ValidationScript".into(),
        r#"
function getCounty() { return "Bavaria"; }
var TAG = "page2-scoped";
"#
        .into(),
    ));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Page2.variables.ValidationScript.getCounty() + \
         \"-\" + Page2.variables.ValidationScript.TAG;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "Bavaria-page2-scoped");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn subform_variables_also_accessible_flat() {
    // Root-level scripts registered without a subform scope remain accessible
    // directly as `ScriptName.X` from any calculate script (D-ι behaviour
    // unchanged by D-ι.2).
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let out = add_field(&mut tree, root, "Out", "");
    tree.variables_scripts
        .push((None, "Global".into(), r#"var ANSWER = "42";"#.into()));
    add_js_script(&mut tree, out, "calculate", "Out.rawValue = Global.ANSWER;");

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "42");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn subform_variables_unknown_subform_returns_empty_object() {
    // Accessing `.variables` on a subform that has no registered variables
    // scripts must return an empty object rather than throwing.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _page1 = add_child(&mut tree, root, "Page1", FormNodeType::Subform);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        // typeof check avoids TypeError; empty object has no .Missing property
        "var ns = Page1.variables; \
         Out.rawValue = (ns !== null && typeof ns === 'object') ? 'ok' : 'fail';",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "ok");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn multiple_subforms_variables_are_isolated() {
    // Page2.variables and Page3.variables must each return their own
    // namespace objects, not each other's. Same script name on two subforms
    // must NOT collide in the flat global dict.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _page2 = add_child(&mut tree, root, "Page2", FormNodeType::Subform);
    let _page3 = add_child(&mut tree, root, "Page3", FormNodeType::Subform);
    let out = add_field(&mut tree, root, "Out", "");
    tree.variables_scripts.push((
        Some("Page2".into()),
        "Util".into(),
        r#"var ID = "p2";"#.into(),
    ));
    tree.variables_scripts.push((
        Some("Page3".into()),
        "Util".into(),
        r#"var ID = "p3";"#.into(),
    ));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Page2.variables.Util.ID + \"-\" + Page3.variables.Util.ID;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "p2-p3");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn subform_scoped_variables_not_in_flat_global() {
    // Subform-scoped scripts must NOT be accessible via bare `ScriptName.X`
    // — only via `subformHandle.variables.ScriptName.X`. This prevents
    // silent shadowing when two subforms define the same script name.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _page2 = add_child(&mut tree, root, "Page2", FormNodeType::Subform);
    let out = add_field(&mut tree, root, "Out", "");
    tree.variables_scripts.push((
        Some("Page2".into()),
        "ScopedOnly".into(),
        r#"var VAL = "scoped";"#.into(),
    ));
    // Access ScopedOnly directly (flat path) — must be undefined, not the
    // scoped namespace.
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = (typeof ScopedOnly === 'undefined') ? 'not-flat' : 'leaked';",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "not-flat");
    assert_eq!(outcome.js_runtime_errors, 0);
}
