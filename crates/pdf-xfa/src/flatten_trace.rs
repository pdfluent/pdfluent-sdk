//! XFA flatten traceability (env-gated, default OFF).
//!
//! Emits a per-document JSON trace when `XFA_FLATTEN_TRACE` is set, capturing
//! parse / bind / script / layout / paint / writer-stage counts so it is
//! traceable **where** XFA content disappears during flatten. Schema:
//! `benchmarks/runs/xfa_enterprise_plan/dynamic_reflow_traceability/XFA_FLATTEN_TRACE_SCHEMA.md`.
//!
//! Cost contract: zero work unless `XFA_FLATTEN_TRACE` is set — [`enabled`]
//! short-circuits before any counting. This is a debugging aid, not a
//! production hot-path feature. Output goes to the file named by
//! `XFA_FLATTEN_TRACE_PATH`, or, if that is unset, a one-line summary to stderr.

use std::fmt::Write as _;

use xfa_layout_engine::form::{DrawContent, FormNodeId, FormNodeType, FormTree, Presence};
use xfa_layout_engine::layout::{LayoutContent, LayoutDom, LayoutNode};

use crate::dynamic::DynamicScriptOutcome;
use crate::render_bridge::PageOverlay;

/// True when `XFA_FLATTEN_TRACE` is set to a non-empty value other than `"0"`.
pub(crate) fn enabled() -> bool {
    matches!(std::env::var("XFA_FLATTEN_TRACE"), Ok(v) if !v.is_empty() && v != "0")
}

/// Counts derived from the merged FormTree (bind stage).
#[derive(Default)]
struct BindCounts {
    form_nodes_total: usize,
    subforms: usize,
    subform_sets: usize,
    areas: usize,
    excl_groups: usize,
    page_areas: usize,
    page_sets: usize,
    fields: usize,
    fields_with_value: usize,
    fields_without_value: usize,
    draws: usize,
    draws_with_text: usize,
    images: usize,
    hidden_nodes: usize,
    /// Nodes reachable only through a hidden/invisible/inactive ancestor — these
    /// are pruned wholesale by the layout engine's `queue_content` filter.
    nodes_under_hidden: usize,
    /// Text-bearing draws that are NOT under any hidden ancestor (i.e. SHOULD be
    /// laid out). A large gap between this and `layout_draw_nodes` indicates a
    /// bounded admission bug rather than a JS/visibility (broad) cause.
    visible_draws_with_text: usize,
}

fn count_bind(tree: &FormTree) -> BindCounts {
    let mut c = BindCounts {
        form_nodes_total: tree.nodes.len(),
        ..Default::default()
    };
    for i in 0..tree.nodes.len() {
        let id = FormNodeId(i);
        let node = tree.get(id);
        if tree.meta(id).presence != Presence::Visible {
            c.hidden_nodes += 1;
        }
        match &node.node_type {
            FormNodeType::Subform => c.subforms += 1,
            FormNodeType::SubformSet => c.subform_sets += 1,
            FormNodeType::Area => c.areas += 1,
            FormNodeType::ExclGroup => c.excl_groups += 1,
            FormNodeType::PageArea { .. } => c.page_areas += 1,
            FormNodeType::PageSet => c.page_sets += 1,
            FormNodeType::Field { value } => {
                c.fields += 1;
                if value.trim().is_empty() {
                    c.fields_without_value += 1;
                } else {
                    c.fields_with_value += 1;
                }
            }
            FormNodeType::Draw(content) => {
                c.draws += 1;
                if let DrawContent::Text(t) = content {
                    if !t.trim().is_empty() {
                        c.draws_with_text += 1;
                    }
                }
            }
            FormNodeType::Image { .. } => c.images += 1,
            FormNodeType::Root => {}
        }
    }

    // DFS from root node(s) to attribute hidden-subtree pruning. A node reached
    // only through a hidden/invisible/inactive ancestor is pruned wholesale by
    // the layout engine's `queue_content` filter, so the entire subtree is lost.
    let roots: Vec<FormNodeId> = (0..tree.nodes.len())
        .map(FormNodeId)
        .filter(|&id| matches!(tree.get(id).node_type, FormNodeType::Root))
        .collect();
    let roots = if roots.is_empty() {
        vec![FormNodeId(0)]
    } else {
        roots
    };
    let mut visited = vec![false; tree.nodes.len()];
    let mut stack: Vec<(FormNodeId, bool)> = roots.into_iter().map(|r| (r, false)).collect();
    while let Some((id, under_hidden)) = stack.pop() {
        if id.0 >= tree.nodes.len() || visited[id.0] {
            continue;
        }
        visited[id.0] = true;
        let node = tree.get(id);
        if under_hidden {
            c.nodes_under_hidden += 1;
        }
        let eff_hidden = under_hidden || tree.meta(id).presence.is_layout_hidden();
        if let FormNodeType::Draw(DrawContent::Text(t)) = &node.node_type {
            if !eff_hidden && !t.trim().is_empty() {
                c.visible_draws_with_text += 1;
            }
        }
        for &child in &node.children {
            stack.push((child, eff_hidden));
        }
    }
    c
}

/// Counts derived from the laid-out pages (layout stage).
#[derive(Default)]
struct LayoutCounts {
    nodes_total: usize,
    text_nodes: usize,
    wrapped_text_nodes: usize,
    field_nodes: usize,
    draw_nodes: usize,
    image_nodes: usize,
    total_chars: usize,
}

fn walk_layout_node(n: &LayoutNode, c: &mut LayoutCounts) {
    c.nodes_total += 1;
    match &n.content {
        LayoutContent::Text(t) => {
            c.text_nodes += 1;
            c.total_chars += t.chars().count();
        }
        LayoutContent::WrappedText { lines, .. } => {
            c.wrapped_text_nodes += 1;
            c.total_chars += lines.iter().map(|l| l.chars().count()).sum::<usize>();
        }
        LayoutContent::Field { value, .. } => {
            c.field_nodes += 1;
            c.total_chars += value.chars().count();
        }
        LayoutContent::Draw(_) => c.draw_nodes += 1,
        LayoutContent::Image { .. } => c.image_nodes += 1,
        LayoutContent::None => {}
    }
    for child in &n.children {
        walk_layout_node(child, c);
    }
}

fn count_layout(layout: &LayoutDom) -> LayoutCounts {
    let mut c = LayoutCounts::default();
    for page in &layout.pages {
        for node in &page.nodes {
            walk_layout_node(node, &mut c);
        }
    }
    c
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Per-page suppression diagnostics (XFA §4.3 data-empty page suppression).
/// Captured before the suppression `retain` so the trace can explain every
/// keep/drop decision and the occur-instance-aware signal that distinguishes a
/// real continuation page from an empty repeated occur-instance.
#[derive(Clone)]
pub(crate) struct PageSuppressionDiag {
    pub page_index: usize,
    pub keep: bool,
    pub reason: &'static str,
    pub field_count: usize,
    pub empty_field_count: usize,
    pub nonempty_field_count: usize,
    pub static_draw_text_chars: usize,
    /// Distinct FormNode ids referenced on the page (occur-instance signature).
    pub distinct_form_nodes: usize,
    /// Index of an earlier page with an identical FormNode-id signature, or -1.
    /// A non-negative value marks this page as a repeated occur-instance.
    pub duplicate_of_page: i64,
    pub runtime_instantiated: bool,
    // --- Layout provenance (XFA_LAYOUT_PROVENANCE_ENGINE_WAVE) ---
    /// True when the page's content sits under a repeating (`occur` max>1)
    /// subform ancestor.
    pub under_repeating_subform: bool,
    /// FormNodeId of the nearest repeating-subform ancestor, or -1.
    pub occur_template_id: i64,
    /// Page nodes bound to a data node (`meta.bound_data_node.is_some()`).
    pub data_bound_nodes_count: usize,
    /// Why this page exists: root_page / continuation / occur_instance /
    /// repeated_empty_instance / static_page_area / unknown.
    pub page_reason: &'static str,
    /// True when this page is a data-empty repeated occur-instance — the only
    /// class that is provenance-safe to drop.
    pub suppression_safe_to_drop: bool,
    /// exact / inferred / unknown.
    pub provenance_confidence: &'static str,
}

/// All inputs needed to assemble a flatten trace. Borrowed; nothing is cloned
/// beyond the small derived count structs.
pub(crate) struct TraceInputs<'a> {
    pub suppression: &'a [PageSuppressionDiag],
    pub input_bytes: usize,
    pub template_bytes: usize,
    pub js_execution_mode: &'a str,
    pub flatten_path: &'a str,
    pub template_packet_found: bool,
    pub datasets_packet_found: bool,
    pub form_packet_found: bool,
    pub image_files: usize,
    pub tree: &'a FormTree,
    pub scripts: &'a DynamicScriptOutcome,
    pub layout: &'a LayoutDom,
    pub pages_produced: usize,
    pub pages_after_suppression: usize,
    pub runtime_instantiated_pages: usize,
    pub overlays: &'a [PageOverlay],
    pub n_layout: usize,
    pub n_existing: usize,
    pub is_static_form: bool,
    pub has_static_content: bool,
    pub preserve_static: bool,
    pub excess_pages_deleted: usize,
    pub widgets_baked: usize,
    pub acroform_removed: bool,
    pub xfa_removed_structural: bool,
    pub needs_rendering_removed: bool,
    pub javascript_actions_stripped: usize,
    pub output_bytes: usize,
    pub output_page_count: usize,
}

/// Compute the earliest stage at which the content chain breaks. Cheap
/// heuristic over already-computed counts; documented in the schema.
fn divergence_hint(i: &TraceInputs, bind: &BindCounts, lay: &LayoutCounts) -> &'static str {
    if !i.template_packet_found {
        return "parse";
    }
    if bind.draws_with_text == 0 && bind.fields == 0 {
        return "bind";
    }

    // Text-bearing nodes that actually reached the laid-out tree. A static draw
    // carrying text becomes a Text/WrappedText leaf (or, rarely, a Draw leaf);
    // so visible text-draws should each yield at least one such node.
    let laid_text = lay.text_nodes + lay.wrapped_text_nodes + lay.draw_nodes;
    let not_hidden_dominated = bind.nodes_under_hidden * 3 < bind.form_nodes_total;

    // (1) BOUNDED, JS-independent: many *visible* static draws (no hidden
    // ancestor) produced no corresponding laid-out text node. This is the safe
    // fix target — content is lost at layout admission, not behind visibility
    // scripts. Threshold: fewer than ~2/3 of visible text-draws survived.
    if bind.visible_draws_with_text >= 8
        && laid_text * 3 < bind.visible_draws_with_text * 2
        && not_hidden_dominated
    {
        return "layout_admission_visible_draw_drop";
    }

    // (2) BROAD, JS/visibility-driven: most nodes are pruned because they sit
    // under a `presence="hidden"` ancestor that runtime scripts (not executed in
    // BestEffortStatic) would reveal. Static recovery is not possible.
    if bind.nodes_under_hidden * 2 >= bind.form_nodes_total {
        return "hidden_subtree_prune";
    }

    // (3) Page suppression removed real pages.
    if i.pages_after_suppression < i.pages_produced {
        return "layout_suppression";
    }

    // (4) Generic large drop of text-bearing bound nodes not explained above.
    let bind_text_bearing = bind.draws_with_text + bind.fields_with_value;
    let layout_text_bearing = laid_text + lay.field_nodes;
    if bind_text_bearing >= 8 && (layout_text_bearing * 2) < bind_text_bearing {
        return "layout_admission";
    }
    if lay.nodes_total > 0 && i.overlays.iter().all(|o| o.content_stream.is_empty()) {
        return "paint";
    }
    if i.excess_pages_deleted > 0 {
        return "writer_excess_delete";
    }
    if i.preserve_static && i.n_layout > i.n_existing {
        return "writer_static_preserve_clamp";
    }
    "none"
}

/// JSON array of repeating (`occur` max>1/unbounded) subforms: their occur
/// params and how many instances actually reached the laid-out tree. Lets the
/// trace show whether under-/over-pagination is occur/instance driven (it is the
/// runtime-instance signal the suppression problem was missing).
fn repeating_subforms_json(tree: &FormTree, layout: &LayoutDom) -> String {
    use xfa_layout_engine::form::FormNodeType;
    let mut inst = vec![0usize; tree.nodes.len()];
    fn walk(n: &LayoutNode, inst: &mut [usize]) {
        if n.form_node.0 < inst.len() {
            inst[n.form_node.0] += 1;
        }
        for c in &n.children {
            walk(c, inst);
        }
    }
    for p in &layout.pages {
        for n in &p.nodes {
            walk(n, &mut inst);
        }
    }
    let mut out = String::from("[");
    let mut first = true;
    for (idx, &inst_count) in inst.iter().enumerate() {
        let node = tree.get(FormNodeId(idx));
        if !matches!(
            node.node_type,
            FormNodeType::Subform
                | FormNodeType::SubformSet
                | FormNodeType::Area
                | FormNodeType::ExclGroup
        ) || !node.occur.is_repeating()
        {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        let max = node.occur.max.map_or(-1i64, |m| m as i64);
        let _ = write!(
            out,
            "{{\"template_id\":{},\"occur_min\":{},\"occur_max\":{},\"occur_initial\":{},\"layout_instances\":{}}}",
            idx, node.occur.min, max, node.occur.initial, inst_count
        );
    }
    out.push(']');
    out
}

/// Build the JSON trace and emit it per the env contract. Call only when
/// [`enabled`] returned true.
pub(crate) fn emit(i: &TraceInputs) {
    let bind = count_bind(i.tree);
    let lay = count_layout(i.layout);
    let s = i.scripts;
    let overlay_total_bytes: usize = i.overlays.iter().map(|o| o.content_stream.len()).sum();
    let overlay_substantial = i.overlays.iter().any(|o| o.content_stream.len() > 1000);
    let hint = divergence_hint(i, &bind, &lay);

    let mut per_page = String::from("[");
    for (idx, o) in i.overlays.iter().enumerate() {
        if idx > 0 {
            per_page.push(',');
        }
        let _ = write!(
            per_page,
            "{{\"page\":{},\"overlay_bytes\":{}}}",
            idx + 1,
            o.content_stream.len()
        );
    }
    per_page.push(']');

    let mut supp = String::from("[");
    for (idx, d) in i.suppression.iter().enumerate() {
        if idx > 0 {
            supp.push(',');
        }
        let _ = write!(
            supp,
            "{{\"page\":{},\"keep\":{},\"reason\":{},\"field_count\":{},\"empty_field_count\":{},\"nonempty_field_count\":{},\"static_draw_text_chars\":{},\"distinct_form_nodes\":{},\"duplicate_of_page\":{},\"runtime_instantiated\":{},\"under_repeating_subform\":{},\"occur_template_id\":{},\"data_bound_nodes_count\":{},\"page_reason\":{},\"suppression_safe_to_drop\":{},\"provenance_confidence\":{}}}",
            d.page_index + 1,
            d.keep,
            json_str(d.reason),
            d.field_count,
            d.empty_field_count,
            d.nonempty_field_count,
            d.static_draw_text_chars,
            d.distinct_form_nodes,
            d.duplicate_of_page,
            d.runtime_instantiated,
            d.under_repeating_subform,
            d.occur_template_id,
            d.data_bound_nodes_count,
            json_str(d.page_reason),
            d.suppression_safe_to_drop,
            json_str(d.provenance_confidence),
        );
    }
    supp.push(']');

    let mut j = String::with_capacity(2048);
    let _ = write!(
        j,
        "{{\"schema_version\":\"1.0\",\
\"input_bytes\":{},\"template_bytes\":{},\
\"js_execution_mode\":{},\"flatten_path\":{},\
\"parse\":{{\"template_packet_found\":{},\"datasets_packet_found\":{},\"form_packet_found\":{},\"image_files\":{}}},\
\"bind\":{{\"form_nodes_total\":{},\"subforms\":{},\"subform_sets\":{},\"areas\":{},\"excl_groups\":{},\"page_areas\":{},\"page_sets\":{},\"fields\":{},\"fields_with_value\":{},\"fields_without_value\":{},\"draws\":{},\"draws_with_text\":{},\"images\":{},\"hidden_nodes\":{},\"nodes_under_hidden\":{},\"visible_draws_with_text\":{}}},\
\"script\":{{\"output_quality\":{},\"js_present\":{},\"js_skipped\":{},\"other_skipped\":{},\"formcalc_run\":{},\"formcalc_errors\":{},\"js_executed\":{},\"js_runtime_errors\":{},\"js_timeouts\":{},\"js_oom\":{},\"js_host_calls\":{},\"js_mutations\":{},\"js_instance_writes\":{},\"js_list_writes\":{},\"js_binding_errors\":{},\"js_resolve_failures\":{},\"js_data_reads\":{},\"js_unsupported_host_calls\":{},\"js_probe_skips\":{}}},\
\"layout\":{{\"pages_produced\":{},\"pages_after_suppression\":{},\"pages_suppressed\":{},\"runtime_instantiated_pages\":{},\"layout_nodes_total\":{},\"layout_text_nodes\":{},\"layout_wrapped_text_nodes\":{},\"layout_field_nodes\":{},\"layout_draw_nodes\":{},\"layout_image_nodes\":{},\"layout_total_chars\":{}}},\
\"paint\":{{\"overlays_generated\":{},\"overlay_total_bytes\":{},\"overlay_substantial\":{},\"per_page\":{}}},\
\"writer\":{{\"n_layout\":{},\"n_existing\":{},\"is_static_form\":{},\"has_static_content\":{},\"preserve_static\":{},\"excess_pages_deleted\":{},\"widgets_baked\":{},\"acroform_removed\":{},\"xfa_removed_structural\":{},\"needs_rendering_removed\":{},\"javascript_actions_stripped\":{},\"output_bytes\":{},\"output_page_count\":{}}},\
\"suppression\":{},\
\"repeating_subforms\":{},\
\"stage_first_divergence_hint\":{}}}",
        i.input_bytes, i.template_bytes,
        json_str(i.js_execution_mode), json_str(i.flatten_path),
        i.template_packet_found, i.datasets_packet_found, i.form_packet_found, i.image_files,
        bind.form_nodes_total, bind.subforms, bind.subform_sets, bind.areas, bind.excl_groups, bind.page_areas, bind.page_sets, bind.fields, bind.fields_with_value, bind.fields_without_value, bind.draws, bind.draws_with_text, bind.images, bind.hidden_nodes, bind.nodes_under_hidden, bind.visible_draws_with_text,
        json_str(s.output_quality.as_str()), s.js_present, s.js_skipped, s.other_skipped, s.formcalc_run, s.formcalc_errors, s.js_executed, s.js_runtime_errors, s.js_timeouts, s.js_oom, s.js_host_calls, s.js_mutations, s.js_instance_writes, s.js_list_writes, s.js_binding_errors, s.js_resolve_failures, s.js_data_reads, s.js_unsupported_host_calls, s.js_probe_skips,
        i.pages_produced, i.pages_after_suppression, i.pages_produced.saturating_sub(i.pages_after_suppression), i.runtime_instantiated_pages, lay.nodes_total, lay.text_nodes, lay.wrapped_text_nodes, lay.field_nodes, lay.draw_nodes, lay.image_nodes, lay.total_chars,
        i.overlays.len(), overlay_total_bytes, overlay_substantial, per_page,
        i.n_layout, i.n_existing, i.is_static_form, i.has_static_content, i.preserve_static, i.excess_pages_deleted, i.widgets_baked, i.acroform_removed, i.xfa_removed_structural, i.needs_rendering_removed, i.javascript_actions_stripped, i.output_bytes, i.output_page_count,
        supp,
        repeating_subforms_json(i.tree, i.layout),
        json_str(hint),
    );

    match std::env::var("XFA_FLATTEN_TRACE_PATH") {
        Ok(path) if !path.is_empty() => {
            if let Err(e) = std::fs::write(&path, j.as_bytes()) {
                eprintln!("XFA_FLATTEN_TRACE: failed to write {path}: {e}");
            }
        }
        _ => {
            eprintln!(
                "XFA_FLATTEN_TRACE summary: path={} bind_draws_text={} layout_chars={} pages {}->{}/{} overlays={} out_pages={} divergence={}",
                i.flatten_path, bind.draws_with_text, lay.total_chars,
                i.pages_produced, i.pages_after_suppression, i.n_existing,
                i.overlays.len(), i.output_page_count, hint,
            );
        }
    }
}
