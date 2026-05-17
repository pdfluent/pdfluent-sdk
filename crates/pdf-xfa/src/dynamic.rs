use std::collections::HashMap;

use crate::error::{Result, XfaError};
use crate::javascript_policy::{self, JavaScriptEntryPoint};
use crate::js_runtime::{
    activity_allowed_for_sandbox, NullRuntime, RuntimeMetadata, SandboxError, XfaJsRuntime,
};
use formcalc_interpreter::{
    interpreter::Interpreter, lexer::tokenize, parser, som_bridge::SomResolver,
    value::Value as FormCalcValue,
};
use xfa_dom_resolver::som::{parse_som, SomExpression, SomIndex, SomRoot, SomSelector};
use xfa_layout_engine::form::{
    EventScript, FormNodeId, FormNodeType, FormTree, GroupKind, Presence, ScriptLanguage,
};

// XFA Spec 3.3 §9.3 — Dynamic Forms Re-Layout: after script execution the
// layout processor must re-run layout.  The spec does not prescribe a fixed
// pass limit; Adobe typically converges in 2-3 passes.  Our limit of 3 is a
// pragmatic cap that matches observed Adobe behavior.
const MAX_SCRIPT_PASSES: usize = 3;

/// Controls only the pre-flight handling of parsed JavaScript-bearing XFA
/// event hooks. JavaScript execution remains denied by `javascript_policy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsExecutionMode {
    /// Abort before script execution when any JavaScript or unsupported script
    /// language is present. This preserves the original policy-gate behavior.
    Strict,
    /// Skip JavaScript and unsupported-language scripts, then continue running
    /// FormCalc and the layout pipeline. Skipped scripts are reported in the
    /// returned outcome.
    #[default]
    BestEffortStatic,
    /// **M3-B Phase B opt-in.** Route JavaScript scripts through the sandboxed
    /// runtime adapter (`crate::js_runtime`). Requires the Cargo feature
    /// `xfa-js-sandboxed` to be compiled in for any script to actually
    /// execute; without the feature the runtime returns
    /// [`SandboxError::NotCompiledIn`] and the dispatch path falls back to
    /// the same skip behaviour as [`Self::BestEffortStatic`] while incrementing
    /// `js_runtime_errors` so callers can observe the dead-code state.
    /// Phase B registers no host bindings; see
    /// `benchmarks/runs/M3B_HOST_BINDINGS_MINIMUM_SET.md` for the Phase C
    /// roadmap.
    SandboxedRuntime,
}
/// Fidelity level of the flattened output relative to the source XFA data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputQuality {
    /// All data was bound and rendered without skipping any scripts or content.
    #[default]
    Exact,
    /// Some scripts were skipped (e.g. JavaScript with `BestEffortStatic` mode);
    /// output may differ from a full Adobe Reader render.
    BestEffort,
    /// All JavaScript scripts on the document executed inside the sandbox
    /// without runtime / timeout / OOM errors (requires `xfa-js-sandboxed` feature).
    Sandboxed,
}

impl OutputQuality {
    /// Return a short lowercase string label suitable for logging and metrics.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::BestEffort => "best_effort",
            Self::Sandboxed => "sandboxed",
        }
    }
}
/// Aggregate outcome of the dynamic script processing pass.
///
/// Returned by [`flatten_xfa_to_pdf_with_metadata`](crate::flatten_xfa_to_pdf_with_metadata)
/// and embedded in [`FlattenMetadata`](crate::FlattenMetadata).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynamicScriptOutcome {
    /// Number of form field values that were mutated by scripts.
    pub changes: usize,
    /// True when the document contains at least one JavaScript event hook.
    pub js_present: bool,
    /// Number of JavaScript scripts that were skipped (not executed).
    pub js_skipped: usize,
    /// Number of scripts in unsupported languages (not FormCalc, not JavaScript) skipped.
    pub other_skipped: usize,
    /// Number of FormCalc scripts that ran successfully.
    pub formcalc_run: usize,
    /// Number of FormCalc scripts that produced an error.
    pub formcalc_errors: usize,
    /// Overall output quality after script processing.
    pub output_quality: OutputQuality,
    /// **M3-B Phase B.** Scripts that ran to completion in the sandboxed
    /// runtime. Always 0 when mode != [`JsExecutionMode::SandboxedRuntime`]
    /// or when the `xfa-js-sandboxed` feature is not compiled in.
    pub js_executed: usize,
    /// **M3-B Phase B.** Sandbox errors that did not fall under timeout / OOM
    /// (parse error, throw, missing host binding in Phase B, FFI panic). The
    /// dispatch path treats these as a script skip; the parent flatten never
    /// aborts because of them (S-17 fail-open).
    pub js_runtime_errors: usize,
    /// **M3-B Phase B.** Per-script time-budget exhaustions.
    pub js_timeouts: usize,
    /// **M3-B Phase B.** Per-document memory-budget exhaustions.
    pub js_oom: usize,
    /// **M3-B Phase C.** Host-binding invocations.
    pub js_host_calls: usize,
    /// **M3-B Phase C.** Successful host-side `field.rawValue` writes.
    pub js_mutations: usize,
    /// **M3-B Phase D.** Successful host-side instanceManager writes.
    pub js_instance_writes: usize,
    /// **M3-B Phase D-β.** Successful host-side listbox clearItems / addItem writes.
    pub js_list_writes: usize,
    /// **M3-B Phase C.** Binding-level failures.
    pub js_binding_errors: usize,
    /// **M3-B Phase C.** SOM resolution misses / failures.
    pub js_resolve_failures: usize,
    /// **M3-B Phase D-γ.** Successful DataDom reads (children / value / child-by-name).
    pub js_data_reads: usize,
    /// **M3-B Phase E (XFA-JS-HOST-STUBS).** Scripts touched a host capability
    /// that requires real viewer / user interaction (UI dialogs, signature,
    /// submit, openList, beep, ...). The stub returned a deterministic safe
    /// default so the script kept running; this counter records how often
    /// such a touch happened so callers can distinguish "would-have-been
    /// interactive" from genuine runtime errors.
    pub js_unsupported_host_calls: usize,
    /// **M3-B Phase D-θ.2.** Strict probe calls skipped because
    /// `parentIds.length == 1 && chain.length == 1` (no same-name
    /// sibling ambiguity possible). Each skipped call saves one
    /// `resolveWithFullChainStrict` host round-trip.
    pub js_probe_skips: usize,
}

impl Default for DynamicScriptOutcome {
    fn default() -> Self {
        Self {
            changes: 0,
            js_present: false,
            js_skipped: 0,
            other_skipped: 0,
            formcalc_run: 0,
            formcalc_errors: 0,
            output_quality: OutputQuality::Exact,
            js_executed: 0,
            js_runtime_errors: 0,
            js_timeouts: 0,
            js_oom: 0,
            js_host_calls: 0,
            js_mutations: 0,
            js_instance_writes: 0,
            js_list_writes: 0,
            js_binding_errors: 0,
            js_resolve_failures: 0,
            js_data_reads: 0,
            js_unsupported_host_calls: 0,
            js_probe_skips: 0,
        }
    }
}

/// Snapshot of field values and presence states, used for rollback.
/// NOTE: This rollback mechanism is our own heuristic — the XFA spec does not
/// define a rollback model.  It protects against scripts that blank out all
/// fields (broken SOM resolution, etc.).
struct FormSnapshot {
    field_values: Vec<(usize, String)>,
    presences: Vec<(usize, Presence)>,
    populated_count: usize,
}

fn snapshot_form(form: &FormTree) -> FormSnapshot {
    let mut field_values = Vec::new();
    let mut presences = Vec::new();
    let mut populated_count = 0usize;
    for (idx, node) in form.nodes.iter().enumerate() {
        if let FormNodeType::Field { value } = &node.node_type {
            field_values.push((idx, value.clone()));
            if !value.trim().is_empty() {
                populated_count += 1;
            }
        }
        presences.push((idx, form.metadata[idx].presence));
    }
    FormSnapshot {
        field_values,
        presences,
        populated_count,
    }
}

fn restore_snapshot(form: &mut FormTree, snapshot: &FormSnapshot) {
    for (idx, value) in &snapshot.field_values {
        if let FormNodeType::Field { value: fv } = &mut form.nodes[*idx].node_type {
            *fv = value.clone();
        }
    }
    for (idx, presence) in &snapshot.presences {
        form.metadata[*idx].presence = *presence;
    }
}

fn should_rollback(
    form: &FormTree,
    snapshot: &FormSnapshot,
    errors: usize,
    successes: usize,
) -> bool {
    if errors > 0 && errors > successes {
        return true;
    }
    if snapshot.populated_count >= 2 {
        let mut now_empty = 0usize;
        for (idx, old_value) in &snapshot.field_values {
            if old_value.trim().is_empty() {
                continue;
            }
            if let FormNodeType::Field { value } = &form.nodes[*idx].node_type {
                if value.trim().is_empty() {
                    now_empty += 1;
                }
            }
        }
        if now_empty * 2 > snapshot.populated_count {
            return true;
        }
    }
    false
}
/// apply_dynamic_scripts.
// XFA Spec 3.3 §9.3 — Dynamic Forms: after data binding, scripts run in
// two phases: (1) initialize events fire once, (2) calculate events may
// iterate until stable (convergence) or MAX_SCRIPT_PASSES is reached.
// The spec (§14.3.2) defines the event model; our implementation runs
// initialize then calculate, matching Adobe's processing order.
//
// NOTE: §10.6 Rule 3 states the merge-completion order as:
//   value calcs → property calcs → validations → initialize events.
// Our order (initialize first) differs from the spec but matches Adobe's
// observed behavior on our 20K test corpus (97%+ SSIM). §28.2 (p1231)
// documents Adobe's event execution insert-at-position-2 algorithm.
//
// The default JavaScript handling is best-effort static flattening: parsed
// JavaScript and unsupported-language scripts are skipped and reported, while
// FormCalc continues to run. Use `apply_dynamic_scripts_with_mode(..., Strict)`
// when callers need the legacy whole-form JavaScript policy gate.
pub fn apply_dynamic_scripts(
    form: &mut FormTree,
    root_id: FormNodeId,
) -> Result<DynamicScriptOutcome> {
    apply_dynamic_scripts_with_mode(form, root_id, JsExecutionMode::default())
}
/// apply_dynamic_scripts_with_mode.
pub fn apply_dynamic_scripts_with_mode(
    form: &mut FormTree,
    root_id: FormNodeId,
    mode: JsExecutionMode,
) -> Result<DynamicScriptOutcome> {
    // M3-B Phase C-α (2026-05-03): when the caller asks for SandboxedRuntime
    // and the `xfa-js-sandboxed` feature is compiled in, instantiate the
    // real QuickJS-backed adapter. Without the feature, fall back to
    // NullRuntime which surfaces `SandboxError::NotCompiledIn` per script
    // (existing Phase B fallback semantics).
    #[cfg(feature = "xfa-js-sandboxed")]
    {
        if mode == JsExecutionMode::SandboxedRuntime {
            match crate::js_runtime::QuickJsRuntime::new() {
                Ok(mut rt) => {
                    return apply_dynamic_scripts_with_runtime(form, root_id, mode, &mut rt);
                }
                Err(_e) => {
                    // QuickJS init failure is rare; fall through to NullRuntime
                    // so the dispatch path can still record per-script errors
                    // and the flatten succeeds in best-effort mode.
                }
            }
        }
    }
    apply_dynamic_scripts_with_runtime(form, root_id, mode, &mut NullRuntime::new())
}

/// Phase B entry point that lets the caller inject a sandboxed runtime
/// adapter. When `mode == JsExecutionMode::SandboxedRuntime` the supplied
/// `runtime` is consulted for every JavaScript script whose `<event activity>`
/// is in [`crate::js_runtime::SANDBOX_ACTIVITY_ALLOWLIST`]. UI / submission
/// activities skip the runtime entirely and are recorded as `js_skipped`,
/// matching `BestEffortStatic` behaviour for those scripts.
///
/// Other modes ignore `runtime` entirely; callers that just need the
/// existing strict / best-effort behaviour should use
/// [`apply_dynamic_scripts_with_mode`] (which routes through
/// [`crate::js_runtime::NullRuntime`]).
pub fn apply_dynamic_scripts_with_runtime(
    form: &mut FormTree,
    root_id: FormNodeId,
    mode: JsExecutionMode,
    runtime: &mut dyn XfaJsRuntime,
) -> Result<DynamicScriptOutcome> {
    let parents = build_parent_map(form, root_id);
    let all_scripts: Vec<(FormNodeId, Vec<EventScript>)> = form
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, _)| {
            let node_id = FormNodeId(idx);
            let scripts = form.meta(node_id).event_scripts.clone();
            (!scripts.is_empty()).then_some((node_id, scripts))
        })
        .collect();

    let has_unsupported_script = all_scripts.iter().any(|(_, node_scripts)| {
        node_scripts
            .iter()
            .any(|script| script.language != ScriptLanguage::FormCalc)
    });

    if mode == JsExecutionMode::Strict && has_unsupported_script {
        return Err(javascript_policy::reject_execution(
            JavaScriptEntryPoint::XfaEventHook,
        ));
    }

    let mut js_skipped = 0usize;
    let mut other_skipped = 0usize;
    let mut sandbox_metadata = RuntimeMetadata::default();
    let mut scripts = Vec::new();
    let sandbox_active = mode == JsExecutionMode::SandboxedRuntime;
    let snapshot = snapshot_form(form);

    if sandbox_active {
        // Best-effort init / reset; init failures are non-fatal — the
        // dispatch path will record them as runtime_errors per script.
        let _ = runtime.init();
        let _ = runtime.reset_for_new_document();
        let _ = runtime.set_form_handle(form as *mut FormTree, root_id);
    }

    for (node_id, node_scripts) in all_scripts {
        let mut formcalc_scripts = Vec::new();
        for script in node_scripts {
            match script.language {
                ScriptLanguage::FormCalc => formcalc_scripts.push(script),
                ScriptLanguage::JavaScript => {
                    if sandbox_active && activity_allowed_for_sandbox(script.activity.as_deref()) {
                        let _ = runtime.reset_per_script(node_id, script.activity.as_deref());
                        match runtime.execute_script(script.activity.as_deref(), &script.script) {
                            Ok(_outcome) => {
                                // Counter increment lives on `take_metadata()`.
                            }
                            Err(SandboxError::Timeout) => js_skipped += 1,
                            Err(SandboxError::OutOfMemory) => js_skipped += 1,
                            Err(e) => {
                                // M3-B Phase C-α: surface the per-script error
                                // class via `log::debug!` for normal builds and
                                // also via stderr when `XFA_JS_DEBUG=1` is set
                                // (operator triage aid; xfa-cli does not init
                                // a logger that respects RUST_LOG).
                                log::debug!(
                                    "sandbox script error on activity={:?}: {}",
                                    script.activity.as_deref(),
                                    e
                                );
                                if std::env::var("XFA_JS_DEBUG").ok().as_deref() == Some("1") {
                                    eprintln!(
                                        "XFA_JS_DEBUG sandbox script error on activity={:?}: {}",
                                        script.activity.as_deref(),
                                        e
                                    );
                                }
                                js_skipped += 1;
                            }
                        }
                    } else {
                        js_skipped += 1;
                    }
                }
                ScriptLanguage::Other => other_skipped += 1,
            }
        }
        if !formcalc_scripts.is_empty() {
            scripts.push((node_id, formcalc_scripts));
        }
    }

    if sandbox_active {
        let _ = runtime.set_form_handle(std::ptr::null_mut(), root_id);
        sandbox_metadata = runtime.take_metadata();
    }

    let mut stats = ScriptStats::default();

    let mut changes = sandbox_metadata
        .mutations
        .saturating_add(sandbox_metadata.instance_writes)
        + run_script_phase(
            form,
            root_id,
            &parents,
            &scripts,
            ScriptPhase::Initialize,
            1,
            &mut stats,
        )?
        + run_script_phase(
            form,
            root_id,
            &parents,
            &scripts,
            ScriptPhase::Calculate,
            MAX_SCRIPT_PASSES,
            &mut stats,
        )?;

    let sandbox_rollback_errors = sandbox_metadata
        .runtime_errors
        .saturating_add(sandbox_metadata.timeouts)
        .saturating_add(sandbox_metadata.oom)
        .saturating_add(sandbox_metadata.binding_errors);
    let rollback_errors = stats.errors.saturating_add(sandbox_rollback_errors);
    let rollback_successes = stats.successes.saturating_add(sandbox_metadata.executed);

    if should_rollback(form, &snapshot, rollback_errors, rollback_successes) {
        restore_snapshot(form, &snapshot);
        changes = 0;
    }

    let js_seen_count = js_skipped + sandbox_metadata.executed;
    let js_present = js_seen_count > 0;
    let output_quality = if sandbox_active && js_present && sandbox_metadata.is_clean() {
        OutputQuality::Sandboxed
    } else if (sandbox_active && js_present) || js_skipped > 0 || other_skipped > 0 {
        OutputQuality::BestEffort
    } else {
        OutputQuality::Exact
    };

    Ok(DynamicScriptOutcome {
        changes,
        js_present,
        js_skipped,
        other_skipped,
        formcalc_run: stats.formcalc_run,
        formcalc_errors: stats.formcalc_errors,
        output_quality,
        js_executed: sandbox_metadata.executed,
        js_runtime_errors: sandbox_metadata.runtime_errors,
        js_timeouts: sandbox_metadata.timeouts,
        js_oom: sandbox_metadata.oom,
        js_host_calls: sandbox_metadata.host_calls,
        js_mutations: sandbox_metadata.mutations,
        js_instance_writes: sandbox_metadata.instance_writes,
        js_list_writes: sandbox_metadata.list_writes,
        js_binding_errors: sandbox_metadata.binding_errors,
        js_resolve_failures: sandbox_metadata.resolve_failures,
        js_data_reads: sandbox_metadata.data_reads,
        js_unsupported_host_calls: sandbox_metadata.unsupported_host_calls,
        js_probe_skips: sandbox_metadata.probe_skips,
    })
}

fn has_hidden_ancestor(
    form: &FormTree,
    parents: &HashMap<FormNodeId, FormNodeId>,
    node_id: FormNodeId,
) -> bool {
    let mut cursor = parents.get(&node_id).copied();
    while let Some(ancestor) = cursor {
        if form.meta(ancestor).presence.is_not_visible() {
            return true;
        }
        cursor = parents.get(&ancestor).copied();
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptPhase {
    Initialize,
    Calculate,
}

#[derive(Default)]
struct ScriptStats {
    errors: usize,
    successes: usize,
    formcalc_run: usize,
    formcalc_errors: usize,
}

#[derive(Debug)]
struct ScriptResult {
    changes: usize,
    error: bool,
}

fn run_script_phase(
    form: &mut FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    scripts: &[(FormNodeId, Vec<EventScript>)],
    phase: ScriptPhase,
    max_passes: usize,
    stats: &mut ScriptStats,
) -> Result<usize> {
    let mut total_changes = 0;

    for _ in 0..max_passes {
        let mut pass_changes = 0;

        for (node_id, node_scripts) in scripts {
            if has_hidden_ancestor(form, parents, *node_id) {
                continue;
            }

            for script in node_scripts
                .iter()
                .filter(|script| should_run_script(script, phase))
            {
                let result = execute_event_script(form, root_id, parents, *node_id, script, phase)?;
                stats.formcalc_run += 1;
                if result.error {
                    stats.errors += 1;
                    stats.formcalc_errors += 1;
                } else {
                    stats.successes += 1;
                }
                pass_changes += result.changes;
            }
        }

        total_changes += pass_changes;
        if pass_changes == 0 {
            break;
        }
    }

    Ok(total_changes)
}

fn should_run_script(script: &EventScript, phase: ScriptPhase) -> bool {
    match phase {
        ScriptPhase::Initialize => script.activity.as_deref() == Some("initialize"),
        ScriptPhase::Calculate => script.activity.as_deref() == Some("calculate"),
    }
}

fn execute_event_script(
    form: &mut FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    script: &EventScript,
    phase: ScriptPhase,
) -> Result<ScriptResult> {
    match script.language {
        ScriptLanguage::FormCalc => Ok(execute_formcalc_script(
            form, root_id, parents, current_id, script, phase,
        )),
        ScriptLanguage::JavaScript => Err(javascript_policy::reject_execution(
            JavaScriptEntryPoint::XfaEventHook,
        )),
        ScriptLanguage::Other => Err(XfaError::UnsupportedFeature("script language".to_string())),
    }
}

fn execute_formcalc_script(
    form: &mut FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    script: &EventScript,
    phase: ScriptPhase,
) -> ScriptResult {
    let Ok(tokens) = tokenize(&script.script) else {
        return ScriptResult {
            changes: 0,
            error: true,
        };
    };
    let Ok(ast) = parser::parse(tokens) else {
        return ScriptResult {
            changes: 0,
            error: true,
        };
    };

    let mut interpreter = Interpreter::new();
    let mut resolver = FormTreeSomResolver::new(form, root_id, parents, current_id);
    let Ok(result) = interpreter.exec_with_resolver(&ast, &mut resolver) else {
        return ScriptResult {
            changes: resolver.changes,
            error: true,
        };
    };

    if matches!(phase, ScriptPhase::Calculate) {
        resolver.changes += write_formcalc_value(
            resolver.form,
            current_id,
            ResolvedProperty::RawValue,
            result,
        );
    }

    ScriptResult {
        changes: resolver.changes,
        error: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedProperty {
    RawValue,
    Presence,
    SomExpression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolvedTarget {
    node_id: FormNodeId,
    property: ResolvedProperty,
}

struct FormTreeSomResolver<'a> {
    form: &'a mut FormTree,
    root_id: FormNodeId,
    parents: &'a HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    changes: usize,
}

impl<'a> FormTreeSomResolver<'a> {
    fn new(
        form: &'a mut FormTree,
        root_id: FormNodeId,
        parents: &'a HashMap<FormNodeId, FormNodeId>,
        current_id: FormNodeId,
    ) -> Self {
        Self {
            form,
            root_id,
            parents,
            current_id,
            changes: 0,
        }
    }

    fn resolve_target(&self, path: &str) -> Option<ResolvedTarget> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return None;
        }

        if matches!(trimmed, "rawValue" | "presence" | "somExpression") {
            return Some(ResolvedTarget {
                node_id: self.current_id,
                property: parse_property_name(trimmed)?,
            });
        }

        let (expr, property) = split_property_path(trimmed)?;
        let node_id = self.resolve_expression(&expr)?.into_iter().next()?;
        Some(ResolvedTarget { node_id, property })
    }

    fn count_targets(&self, path: &str) -> usize {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return 0;
        }
        if matches!(trimmed, "rawValue" | "presence" | "somExpression") {
            return 1;
        }
        let Some((expr, _property)) = split_property_path(trimmed) else {
            return 0;
        };
        self.resolve_expression(&expr)
            .map_or(0, |nodes| nodes.len())
    }

    fn resolve_expression(&self, expr: &SomExpression) -> Option<Vec<FormNodeId>> {
        match expr.root {
            SomRoot::Data | SomRoot::Record | SomRoot::Template => None,
            SomRoot::CurrentContainer => {
                if expr.segments.is_empty() {
                    Some(vec![self.current_id])
                } else {
                    Some(self.follow_absolute(vec![self.current_id], &expr.segments))
                }
            }
            SomRoot::Form => {
                if expr.segments.is_empty() {
                    Some(vec![self.root_id])
                } else {
                    Some(self.follow_absolute(vec![self.root_id], &expr.segments))
                }
            }
            SomRoot::Xfa => {
                let segments = strip_xfa_form_prefix(&expr.segments);
                if segments.is_empty() {
                    Some(vec![self.root_id])
                } else {
                    Some(self.follow_absolute(vec![self.root_id], segments))
                }
            }
            SomRoot::Unqualified => {
                if expr.segments.is_empty() {
                    Some(vec![self.current_id])
                } else {
                    Some(self.follow_unqualified(&expr.segments))
                }
            }
        }
    }

    fn follow_absolute(
        &self,
        mut current: Vec<FormNodeId>,
        segments: &[xfa_dom_resolver::som::SomSegment],
    ) -> Vec<FormNodeId> {
        for (idx, segment) in segments.iter().enumerate() {
            let allow_self = idx == 0;
            current = current
                .into_iter()
                .flat_map(|node_id| self.step_from_node(node_id, segment, allow_self))
                .collect();
            if current.is_empty() {
                break;
            }
        }
        current
    }

    fn follow_unqualified(
        &self,
        segments: &[xfa_dom_resolver::som::SomSegment],
    ) -> Vec<FormNodeId> {
        let Some((first, rest)) = segments.split_first() else {
            return vec![self.current_id];
        };

        let mut scope = Some(self.current_id);
        while let Some(scope_id) = scope {
            let anchors: Vec<_> = descendants_inclusive(self.form, scope_id)
                .into_iter()
                .filter(|node_id| self.node_matches_segment(*node_id, first))
                .collect();
            let matched = self.follow_remaining(anchors, rest);
            if !matched.is_empty() {
                return matched;
            }
            scope = self.parents.get(&scope_id).copied();
        }

        let anchors: Vec<_> = descendants_inclusive(self.form, self.root_id)
            .into_iter()
            .filter(|node_id| self.node_matches_segment(*node_id, first))
            .collect();
        self.follow_remaining(anchors, rest)
    }

    fn follow_remaining(
        &self,
        mut current: Vec<FormNodeId>,
        segments: &[xfa_dom_resolver::som::SomSegment],
    ) -> Vec<FormNodeId> {
        for segment in segments {
            current = current
                .into_iter()
                .flat_map(|node_id| self.step_from_node(node_id, segment, false))
                .collect();
            if current.is_empty() {
                break;
            }
        }
        current
    }

    fn step_from_node(
        &self,
        node_id: FormNodeId,
        segment: &xfa_dom_resolver::som::SomSegment,
        allow_self: bool,
    ) -> Vec<FormNodeId> {
        // XFA-F3-06: `..` (parent) navigation — a segment whose name is an
        // empty string (produced by the `.` separator after `..` in the raw path)
        // or literally ".." navigates to the parent node.
        if let SomSelector::Name(name) = &segment.selector {
            if name == ".." {
                // Navigate to parent
                if let Some(&parent_id) = self.parents.get(&node_id) {
                    return apply_index_to_single(parent_id, segment.index);
                }
                return Vec::new();
            }
        }

        if allow_self && self.node_matches_selector(node_id, &segment.selector) {
            return apply_index_to_single(node_id, segment.index);
        }

        let matches: Vec<_> = self
            .form
            .get(node_id)
            .children
            .iter()
            .copied()
            .filter(|child_id| self.node_matches_selector(*child_id, &segment.selector))
            .collect();

        apply_index(matches, segment.index)
    }

    fn node_matches_segment(
        &self,
        node_id: FormNodeId,
        segment: &xfa_dom_resolver::som::SomSegment,
    ) -> bool {
        if !self.node_matches_selector(node_id, &segment.selector) {
            return false;
        }

        match segment.index {
            SomIndex::All => true,
            SomIndex::None => self.sibling_position(node_id, &segment.selector) == Some(0),
            SomIndex::Specific(idx) => {
                self.sibling_position(node_id, &segment.selector) == Some(idx)
            }
        }
    }

    fn sibling_position(&self, node_id: FormNodeId, selector: &SomSelector) -> Option<usize> {
        let Some(parent_id) = self.parents.get(&node_id).copied() else {
            return self.node_matches_selector(node_id, selector).then_some(0);
        };

        self.form
            .get(parent_id)
            .children
            .iter()
            .copied()
            .filter(|candidate| self.node_matches_selector(*candidate, selector))
            .position(|candidate| candidate == node_id)
    }

    fn node_matches_selector(&self, node_id: FormNodeId, selector: &SomSelector) -> bool {
        match selector {
            SomSelector::Name(name) => self.form.get(node_id).name == *name,
            SomSelector::Class(class_name) => self.node_matches_class(node_id, class_name),
            SomSelector::AllChildren => true,
        }
    }

    fn node_matches_class(&self, node_id: FormNodeId, class_name: &str) -> bool {
        let class_name = class_name.to_ascii_lowercase();
        match class_name.as_str() {
            "subform" => matches!(
                self.form.get(node_id).node_type,
                FormNodeType::Root | FormNodeType::Subform
            ),
            "pageset" => {
                matches!(self.form.get(node_id).node_type, FormNodeType::PageSet)
            }
            "pagearea" => matches!(
                self.form.get(node_id).node_type,
                FormNodeType::PageArea { .. }
            ),
            "field" => matches!(self.form.get(node_id).node_type, FormNodeType::Field { .. }),
            "draw" => matches!(
                self.form.get(node_id).node_type,
                FormNodeType::Draw(_) | FormNodeType::Image { .. }
            ),
            "exclgroup" => self.form.meta(node_id).group_kind == GroupKind::ExclusiveChoice,
            _ => false,
        }
    }
}

impl SomResolver for FormTreeSomResolver<'_> {
    fn resolve_path(
        &mut self,
        path: &str,
    ) -> formcalc_interpreter::error::Result<Option<FormCalcValue>> {
        let Some(target) = self.resolve_target(path) else {
            // XFA-F3-06: log a warning instead of silently returning None so
            // that SOM path failures are diagnosable.
            if !path.trim().is_empty() {
                log::warn!("SOM bridge: path not resolved: {:?}", path.trim());
            }
            return Ok(None);
        };
        Ok(Some(read_formcalc_value(
            self.form,
            self.root_id,
            self.parents,
            target,
        )))
    }

    fn assign_path(
        &mut self,
        path: &str,
        value: FormCalcValue,
    ) -> formcalc_interpreter::error::Result<bool> {
        let Some(target) = self.resolve_target(path) else {
            // XFA-F3-06: descriptive warning on assignment failure.
            if !path.trim().is_empty() {
                log::warn!("SOM bridge: assignment target not found: {:?}", path.trim());
            }
            return Ok(false);
        };
        self.changes += write_formcalc_value(self.form, target.node_id, target.property, value);
        Ok(true)
    }

    fn count_path_matches(&mut self, path: &str) -> formcalc_interpreter::error::Result<usize> {
        Ok(self.count_targets(path))
    }
}

fn split_property_path(path: &str) -> Option<(SomExpression, ResolvedProperty)> {
    let normalized = if let Some(rest) = path.strip_prefix("this.") {
        format!("$.{rest}")
    } else if path == "this" {
        "$".to_string()
    } else {
        path.to_string()
    };

    let mut expr = parse_som(&normalized).ok()?;
    let property = if let Some(last) = expr.segments.last() {
        match &last.selector {
            SomSelector::Name(name) => {
                parse_property_name(name).unwrap_or(ResolvedProperty::RawValue)
            }
            _ => ResolvedProperty::RawValue,
        }
    } else {
        ResolvedProperty::RawValue
    };

    if matches!(
        expr.segments.last().map(|segment| &segment.selector),
        Some(SomSelector::Name(name)) if parse_property_name(name).is_some()
    ) {
        expr.segments.pop();
    }

    Some((expr, property))
}

fn parse_property_name(name: &str) -> Option<ResolvedProperty> {
    match name {
        "rawValue" => Some(ResolvedProperty::RawValue),
        "presence" => Some(ResolvedProperty::Presence),
        "somExpression" => Some(ResolvedProperty::SomExpression),
        _ => None,
    }
}

fn strip_xfa_form_prefix(
    segments: &[xfa_dom_resolver::som::SomSegment],
) -> &[xfa_dom_resolver::som::SomSegment] {
    match segments.first() {
        Some(segment)
            if matches!(&segment.selector, SomSelector::Name(name) if name == "form")
                && matches!(segment.index, SomIndex::None) =>
        {
            &segments[1..]
        }
        _ => segments,
    }
}

fn apply_index(matches: Vec<FormNodeId>, index: SomIndex) -> Vec<FormNodeId> {
    match index {
        SomIndex::None => matches.into_iter().take(1).collect(),
        SomIndex::Specific(idx) => matches.get(idx).copied().into_iter().collect(),
        SomIndex::All => matches,
    }
}

fn apply_index_to_single(node_id: FormNodeId, index: SomIndex) -> Vec<FormNodeId> {
    match index {
        SomIndex::None | SomIndex::Specific(0) | SomIndex::All => vec![node_id],
        SomIndex::Specific(_) => Vec::new(),
    }
}

fn read_formcalc_value(
    form: &FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    target: ResolvedTarget,
) -> FormCalcValue {
    match target.property {
        ResolvedProperty::RawValue => get_formcalc_raw_value(form, target.node_id),
        ResolvedProperty::Presence => FormCalcValue::String(
            match form.meta(target.node_id).presence {
                Presence::Visible => "visible",
                Presence::Hidden => "hidden",
                Presence::Invisible => "invisible",
                Presence::Inactive => "inactive",
            }
            .to_string(),
        ),
        ResolvedProperty::SomExpression => {
            FormCalcValue::String(build_som_expression(form, root_id, parents, target.node_id))
        }
    }
}

fn get_formcalc_raw_value(form: &FormTree, node_id: FormNodeId) -> FormCalcValue {
    match &form.get(node_id).node_type {
        FormNodeType::Field { value } => string_to_formcalc_value(value),
        _ if form.meta(node_id).group_kind == GroupKind::ExclusiveChoice => {
            for &child_id in &form.get(node_id).children {
                if let FormNodeType::Field { value } = &form.get(child_id).node_type {
                    if !value.is_empty() {
                        let selected = form.meta(child_id).item_value.as_deref().unwrap_or(value);
                        return string_to_formcalc_value(selected);
                    }
                }
            }
            FormCalcValue::Null
        }
        _ => FormCalcValue::Null,
    }
}

fn write_formcalc_value(
    form: &mut FormTree,
    node_id: FormNodeId,
    property: ResolvedProperty,
    value: FormCalcValue,
) -> usize {
    match property {
        ResolvedProperty::RawValue => set_raw_value(form, node_id, formcalc_to_script_value(value)),
        ResolvedProperty::Presence => {
            set_presence(form, node_id, ScriptValue::String(value.to_string_val()))
        }
        ResolvedProperty::SomExpression => 0,
    }
}

fn string_to_formcalc_value(value: &str) -> FormCalcValue {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        FormCalcValue::Null
    } else if let Ok(number) = trimmed.parse::<f64>() {
        FormCalcValue::Number(number)
    } else {
        FormCalcValue::String(value.to_string())
    }
}

fn formcalc_to_script_value(value: FormCalcValue) -> ScriptValue {
    match value {
        FormCalcValue::Null => ScriptValue::Null,
        FormCalcValue::Number(number) => ScriptValue::String(normalize_number(number)),
        FormCalcValue::String(value) => ScriptValue::String(value),
    }
}

fn build_som_expression(
    form: &FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    node_id: FormNodeId,
) -> String {
    let mut parts = Vec::new();
    let mut cursor = Some(node_id);
    while let Some(current) = cursor {
        let node = form.get(current);
        if !node.name.is_empty() {
            let index = if let Some(parent_id) = parents.get(&current).copied() {
                form.get(parent_id)
                    .children
                    .iter()
                    .copied()
                    .filter(|sibling_id| form.get(*sibling_id).name == node.name)
                    .position(|sibling_id| sibling_id == current)
                    .unwrap_or(0)
            } else {
                0
            };
            parts.push(format!("{}[{index}]", node.name));
        }
        if current == root_id {
            break;
        }
        cursor = parents.get(&current).copied();
    }
    parts.reverse();

    if parts.is_empty() {
        "$form".to_string()
    } else {
        format!("$form.{}", parts.join("."))
    }
}

fn descendants_inclusive(form: &FormTree, root_id: FormNodeId) -> Vec<FormNodeId> {
    let mut out = Vec::new();
    collect_descendants(form, root_id, &mut out);
    out
}

fn collect_descendants(form: &FormTree, node_id: FormNodeId, out: &mut Vec<FormNodeId>) {
    out.push(node_id);
    for &child_id in &form.get(node_id).children {
        collect_descendants(form, child_id, out);
    }
}

fn build_parent_map(form: &FormTree, root_id: FormNodeId) -> HashMap<FormNodeId, FormNodeId> {
    let mut parents = HashMap::new();
    populate_parent_map(form, root_id, &mut parents);
    parents
}

fn populate_parent_map(
    form: &FormTree,
    node_id: FormNodeId,
    parents: &mut HashMap<FormNodeId, FormNodeId>,
) {
    for &child_id in &form.get(node_id).children {
        parents.insert(child_id, node_id);
        populate_parent_map(form, child_id, parents);
    }
}

fn set_raw_value(form: &mut FormTree, node_id: FormNodeId, value: ScriptValue) -> usize {
    let value = match value {
        ScriptValue::Null => String::new(),
        ScriptValue::String(value) => value,
    };

    if form.meta(node_id).group_kind == GroupKind::ExclusiveChoice {
        let mut changes = 0;
        for &child_id in &form.get(node_id).children.clone() {
            let item_value = form.meta(child_id).item_value.clone();
            let next = if item_value.as_deref() == Some(value.as_str()) {
                value.clone()
            } else {
                String::new()
            };
            if let FormNodeType::Field { value: field_value } =
                &mut form.get_mut(child_id).node_type
            {
                if *field_value != next {
                    *field_value = next;
                    changes += 1;
                }
            }
        }
        return changes;
    }

    if let FormNodeType::Field { value: field_value } = &mut form.get_mut(node_id).node_type {
        if *field_value != value {
            *field_value = value;
            return 1;
        }
    }

    0
}

fn set_presence(form: &mut FormTree, node_id: FormNodeId, value: ScriptValue) -> usize {
    let value = match value {
        ScriptValue::Null => return 0,
        ScriptValue::String(value) => value,
    };
    let normalized = value.trim().to_ascii_lowercase();
    let new_presence = match normalized.as_str() {
        "visible" | "open" => Presence::Visible,
        "hidden" => Presence::Hidden,
        "invisible" => Presence::Invisible,
        "inactive" => Presence::Inactive,
        _ => return 0,
    };

    let meta = form.meta_mut(node_id);
    if meta.presence == new_presence {
        return 0;
    }
    meta.presence = new_presence;
    1
}

fn normalize_number(number: f64) -> String {
    if number.fract().abs() < f64::EPSILON {
        (number as i64).to_string()
    } else {
        number.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScriptValue {
    Null,
    String(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_layout_engine::form::{
        FieldKind, FormNode, FormNodeMeta, FormNodeStyle, GroupKind, Occur,
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

    fn empty_meta() -> FormNodeMeta {
        FormNodeMeta {
            field_kind: FieldKind::Text,
            group_kind: GroupKind::None,
            style: FormNodeStyle::default(),
            ..Default::default()
        }
    }

    fn formcalc_script(script: &str, activity: &str) -> EventScript {
        EventScript::formcalc(script, Some(activity))
    }

    fn javascript_script(script: &str, activity: &str) -> EventScript {
        EventScript::javascript(script, Some(activity))
    }

    fn other_script(script: &str, activity: &str) -> EventScript {
        EventScript::new(
            script.to_string(),
            ScriptLanguage::Other,
            Some(activity.to_string()),
            None,
            None,
        )
    }

    fn script_policy_fixture(include_js: bool) -> (FormTree, FormNodeId, FormNodeId) {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let js_hook = add_node(
            &mut tree,
            "JsHook",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        let runner = add_node(
            &mut tree,
            "Runner",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        let target = add_node(
            &mut tree,
            "Target",
            FormNodeType::Field {
                value: String::new(),
            },
        );

        tree.get_mut(root).children = vec![js_hook, runner, target];
        if include_js {
            tree.meta_mut(js_hook).event_scripts = vec![javascript_script(
                "xfa.host.messageBox('skip');",
                "initialize",
            )];
        }
        tree.meta_mut(runner).event_scripts =
            vec![formcalc_script(r#"Target.rawValue = "ran""#, "initialize")];

        (tree, root, target)
    }

    fn other_language_policy_fixture() -> (FormTree, FormNodeId, FormNodeId) {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let other = add_node(&mut tree, "OtherHook", FormNodeType::Subform);
        let runner = add_node(&mut tree, "Runner", FormNodeType::Subform);
        let target = add_node(
            &mut tree,
            "Target",
            FormNodeType::Field {
                value: String::new(),
            },
        );

        tree.get_mut(root).children = vec![other, runner, target];
        tree.meta_mut(other).event_scripts = vec![other_script("MsgBox \"skip\"", "initialize")];
        tree.meta_mut(runner).event_scripts =
            vec![formcalc_script(r#"Target.rawValue = "ran""#, "initialize")];

        (tree, root, target)
    }

    fn field_value(tree: &FormTree, node_id: FormNodeId) -> &str {
        match &tree.get(node_id).node_type {
            FormNodeType::Field { value } => value,
            _ => panic!("expected field"),
        }
    }

    #[test]
    fn change_event_toggles_relative_hidden_subform() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let section = add_node(&mut tree, "Section", FormNodeType::Subform);
        let group = add_node(&mut tree, "Choice", FormNodeType::Subform);
        let option1 = add_node(
            &mut tree,
            "Option1",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );
        let option2 = add_node(
            &mut tree,
            "Option2",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        let details = add_node(&mut tree, "Details", FormNodeType::Subform);

        tree.get_mut(root).children = vec![section];
        tree.get_mut(section).children = vec![group, details];
        tree.get_mut(group).children = vec![option1, option2];

        tree.meta_mut(group).group_kind = GroupKind::ExclusiveChoice;
        tree.meta_mut(group).event_scripts = vec![formcalc_script(
            r#"
Details.presence = "hidden"
if (this.rawValue == 1) then
  Details.presence = "visible"
endif
"#,
            "initialize",
        )];
        tree.meta_mut(option1).item_value = Some("1".into());
        tree.meta_mut(option2).item_value = Some("2".into());
        tree.meta_mut(details).presence = Presence::Hidden;

        apply_dynamic_scripts(&mut tree, root).unwrap();

        assert_eq!(tree.meta(details).presence, Presence::Visible);
    }

    #[test]
    fn calculate_script_on_hidden_block_uses_sibling_values() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let section = add_node(&mut tree, "Section", FormNodeType::Subform);
        let option1 = add_node(
            &mut tree,
            "Opt1",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );
        let option2 = add_node(
            &mut tree,
            "Opt2",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        let details = add_node(&mut tree, "Details", FormNodeType::Subform);

        tree.get_mut(root).children = vec![section];
        tree.get_mut(section).children = vec![option1, option2, details];
        tree.meta_mut(details).presence = Presence::Hidden;
        tree.meta_mut(details).event_scripts = vec![formcalc_script(
            r#"
this.presence = "hidden"
if ((Opt1.rawValue == 1) or (Opt2.rawValue == 1)) then
  this.presence = "visible"
endif
"#,
            "calculate",
        )];

        apply_dynamic_scripts(&mut tree, root).unwrap();

        assert_eq!(tree.meta(details).presence, Presence::Visible);
    }

    #[test]
    fn multi_pass_scripts_propagate_raw_values() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let section = add_node(&mut tree, "Section", FormNodeType::Subform);
        let controller = add_node(
            &mut tree,
            "Controller",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );
        let target = add_node(
            &mut tree,
            "Target",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        let details = add_node(&mut tree, "Details", FormNodeType::Subform);

        tree.get_mut(root).children = vec![section];
        tree.get_mut(section).children = vec![controller, target, details];

        tree.meta_mut(controller).event_scripts = vec![formcalc_script(
            r#"
if (this.rawValue == 1) then
  Target.rawValue = 1
endif
"#,
            "calculate",
        )];
        tree.meta_mut(details).presence = Presence::Hidden;
        tree.meta_mut(details).event_scripts = vec![formcalc_script(
            r#"
this.presence = "hidden"
if (Target.rawValue == 1) then
  this.presence = "visible"
endif
"#,
            "calculate",
        )];

        apply_dynamic_scripts(&mut tree, root).unwrap();

        if let FormNodeType::Field { value } = &tree.get(target).node_type {
            assert_eq!(value, "1");
        } else {
            panic!("expected field");
        }
        assert_eq!(tree.meta(details).presence, Presence::Visible);
    }

    // ─── #1097: FormCalc SOM bridge hardening ────────────────────────────────

    /// SOM path `form1.#subform[0].field1.rawValue` resolves correctly on a
    /// simple form tree.
    #[test]
    fn som_path_resolves_on_simple_form_tree() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let form1 = add_node(&mut tree, "form1", FormNodeType::Subform);
        let subform = add_node(&mut tree, "subform1", FormNodeType::Subform);
        let field1 = add_node(
            &mut tree,
            "field1",
            FormNodeType::Field {
                value: "hello".to_string(),
            },
        );

        tree.get_mut(root).children = vec![form1];
        tree.get_mut(form1).children = vec![subform];
        tree.get_mut(subform).children = vec![field1];

        // Use a calculate script to read the value via absolute SOM path
        tree.meta_mut(root).event_scripts = vec![formcalc_script(
            "form1.subform1.field1.rawValue",
            "calculate",
        )];

        let parents = super::build_parent_map(&tree, root);
        let resolver = FormTreeSomResolver::new(&mut tree, root, &parents, root);
        let target = resolver.resolve_target("form1.subform1.field1.rawValue");
        assert!(target.is_some(), "SOM path must resolve to a node");
        let target = target.unwrap();
        let val = super::read_formcalc_value(&tree, root, &parents, target);
        match val {
            formcalc_interpreter::value::Value::String(s) => assert_eq!(s, "hello"),
            formcalc_interpreter::value::Value::Number(n) => {
                // number coercion: not expected here
                panic!("expected string, got number {n}")
            }
            _ => panic!("expected string value"),
        }
    }

    /// An invalid SOM path returns `None` (descriptive non-panic failure).
    #[test]
    fn invalid_som_path_returns_none_not_panic() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let parents = super::build_parent_map(&tree, root);
        let resolver = FormTreeSomResolver::new(&mut tree, root, &parents, root);

        // This should not panic — it should return None
        let result = resolver.resolve_target("nonexistent.deep.path.rawValue");
        assert!(
            result.is_none(),
            "invalid SOM path must return None, not panic"
        );
    }

    #[test]
    fn best_effort_skips_javascript_and_runs_formcalc() {
        let (mut tree, root, target) = script_policy_fixture(true);

        let outcome = apply_dynamic_scripts(&mut tree, root).unwrap();

        assert_eq!(field_value(&tree, target), "ran");
        assert!(outcome.js_present);
        assert_eq!(outcome.js_skipped, 1);
        assert_eq!(outcome.other_skipped, 0);
        assert_eq!(outcome.formcalc_run, 1);
        assert_eq!(outcome.formcalc_errors, 0);
        assert_eq!(outcome.output_quality, OutputQuality::BestEffort);
    }

    #[test]
    fn strict_mode_preserves_javascript_reject() {
        let (mut tree, root, target) = script_policy_fixture(true);

        let err =
            apply_dynamic_scripts_with_mode(&mut tree, root, JsExecutionMode::Strict).unwrap_err();

        assert!(matches!(err, XfaError::UnsupportedFeature(feature) if feature == "javascript"));
        assert_eq!(field_value(&tree, target), "");
    }

    #[test]
    fn formcalc_only_reports_exact_quality() {
        let (mut tree, root, target) = script_policy_fixture(false);

        let outcome = apply_dynamic_scripts(&mut tree, root).unwrap();

        assert_eq!(field_value(&tree, target), "ran");
        assert!(!outcome.js_present);
        assert_eq!(outcome.js_skipped, 0);
        assert_eq!(outcome.other_skipped, 0);
        assert_eq!(outcome.formcalc_run, 1);
        assert_eq!(outcome.formcalc_errors, 0);
        assert_eq!(outcome.output_quality, OutputQuality::Exact);
    }

    #[test]
    fn other_language_scripts_skip_in_best_effort_and_reject_in_strict() {
        let (mut tree, root, target) = other_language_policy_fixture();
        let (mut strict_tree, strict_root, _) = other_language_policy_fixture();

        let outcome = apply_dynamic_scripts(&mut tree, root).unwrap();
        assert_eq!(field_value(&tree, target), "ran");
        assert_eq!(outcome.js_skipped, 0);
        assert_eq!(outcome.other_skipped, 1);
        assert_eq!(outcome.formcalc_run, 1);
        assert_eq!(outcome.output_quality, OutputQuality::BestEffort);

        let err =
            apply_dynamic_scripts_with_mode(&mut strict_tree, strict_root, JsExecutionMode::Strict)
                .unwrap_err();
        assert!(matches!(err, XfaError::UnsupportedFeature(feature) if feature == "javascript"));
    }

    #[test]
    fn javascript_direct_executor_call_is_still_denied() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let trigger = add_node(
            &mut tree,
            "Trigger",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        tree.get_mut(root).children = vec![trigger];

        let parents = build_parent_map(&tree, root);
        let script = javascript_script("xfa.host.messageBox('deny');", "initialize");
        let err = execute_event_script(
            &mut tree,
            root,
            &parents,
            trigger,
            &script,
            ScriptPhase::Initialize,
        )
        .unwrap_err();

        assert!(matches!(err, XfaError::UnsupportedFeature(feature) if feature == "javascript"));
    }

    #[test]
    fn javascript_resolve_node_call_is_explicitly_denied() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let form = add_node(&mut tree, "formulier1", FormNodeType::Subform);
        let admin = add_node(&mut tree, "ADMIN", FormNodeType::Subform);
        let lock = add_node(
            &mut tree,
            "LockForm_AD",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );
        let reset = add_node(
            &mut tree,
            "Reset",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );

        tree.get_mut(root).children = vec![form];
        tree.get_mut(form).children = vec![admin, reset];
        tree.get_mut(admin).children = vec![lock];
        tree.meta_mut(reset).event_scripts = vec![javascript_script(
            r#"xfa.resolveNode("formulier1.ADMIN.LockForm_AD").rawValue = 0;"#,
            "initialize",
        )];

        let err =
            apply_dynamic_scripts_with_mode(&mut tree, root, JsExecutionMode::Strict).unwrap_err();
        assert!(matches!(err, XfaError::UnsupportedFeature(feature) if feature == "javascript"));

        if let FormNodeType::Field { value } = &tree.get(lock).node_type {
            assert_eq!(value, "1");
        } else {
            panic!("expected field");
        }
    }

    #[test]
    fn javascript_utils_hide_if_empty_is_explicitly_denied() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let empty = add_node(
            &mut tree,
            "EmptyField",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        tree.get_mut(root).children = vec![empty];
        tree.meta_mut(empty).event_scripts =
            vec![javascript_script("Utils.hideIfEmpty(this);", "initialize")];

        let err =
            apply_dynamic_scripts_with_mode(&mut tree, root, JsExecutionMode::Strict).unwrap_err();
        assert!(matches!(err, XfaError::UnsupportedFeature(feature) if feature == "javascript"));

        assert!(!tree.meta(empty).presence.is_not_visible());
    }

    #[test]
    fn malformed_javascript_payload_is_explicitly_denied_without_panic() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let container = add_node(&mut tree, "Container", FormNodeType::Subform);
        let empty = add_node(
            &mut tree,
            "EmptyField",
            FormNodeType::Field {
                value: String::new(),
            },
        );

        tree.get_mut(root).children = vec![container];
        tree.get_mut(container).children = vec![empty];
        tree.meta_mut(empty).event_scripts = vec![javascript_script(
            "\0}{{not.valid.javascript(",
            "initialize",
        )];

        let err =
            apply_dynamic_scripts_with_mode(&mut tree, root, JsExecutionMode::Strict).unwrap_err();
        assert!(matches!(err, XfaError::UnsupportedFeature(feature) if feature == "javascript"));

        assert!(!tree.meta(container).presence.is_not_visible());
    }

    #[test]
    fn default_meta_helper_is_constructible() {
        let meta = empty_meta();
        assert_eq!(meta.group_kind, GroupKind::None);
    }

    #[test]
    fn calculate_event_applies_formcalc_return_value() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let total = add_node(
            &mut tree,
            "Total",
            FormNodeType::Field {
                value: String::new(),
            },
        );

        tree.get_mut(root).children = vec![total];
        tree.meta_mut(total).event_scripts = vec![formcalc_script("40 + 2", "calculate")];

        apply_dynamic_scripts(&mut tree, root).unwrap();

        match &tree.get(total).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "42"),
            _ => panic!("expected field"),
        }
    }

    #[test]
    fn calculate_event_resolves_bare_field_names_as_raw_values() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let section = add_node(&mut tree, "Section", FormNodeType::Subform);
        let number1 = add_node(
            &mut tree,
            "Number1",
            FormNodeType::Field {
                value: "40".to_string(),
            },
        );
        let number2 = add_node(
            &mut tree,
            "Number2",
            FormNodeType::Field {
                value: "2".to_string(),
            },
        );
        let total = add_node(
            &mut tree,
            "Total",
            FormNodeType::Field {
                value: String::new(),
            },
        );

        tree.get_mut(root).children = vec![section];
        tree.get_mut(section).children = vec![number1, number2, total];
        tree.meta_mut(total).event_scripts =
            vec![formcalc_script("Number1 + Number2", "calculate")];

        apply_dynamic_scripts(&mut tree, root).unwrap();

        match &tree.get(total).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "42"),
            _ => panic!("expected field"),
        }
    }

    #[test]
    fn click_events_are_skipped_during_flatten() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let trigger = add_node(
            &mut tree,
            "Trigger",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );
        let details = add_node(&mut tree, "Details", FormNodeType::Subform);

        tree.get_mut(root).children = vec![trigger, details];
        tree.meta_mut(details).presence = Presence::Hidden;
        tree.meta_mut(trigger).event_scripts = vec![formcalc_script(
            r#"
Details.presence = "visible"
"#,
            "click",
        )];

        apply_dynamic_scripts(&mut tree, root).unwrap();

        assert_eq!(tree.meta(details).presence, Presence::Hidden);
    }

    #[test]
    fn rollback_when_scripts_mostly_error() {
        // Set up a form with fields that have values.  Attach scripts that
        // will fail to parse so that errors > successes.  After
        // apply_dynamic_scripts the field values must be unchanged.
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let field_a = add_node(
            &mut tree,
            "FieldA",
            FormNodeType::Field {
                value: "hello".to_string(),
            },
        );
        let field_b = add_node(
            &mut tree,
            "FieldB",
            FormNodeType::Field {
                value: "world".to_string(),
            },
        );

        tree.get_mut(root).children = vec![field_a, field_b];

        // Two scripts that fail parsing (invalid FormCalc), zero successes.
        tree.meta_mut(field_a).event_scripts = vec![formcalc_script("@@INVALID@@", "initialize")];
        tree.meta_mut(field_b).event_scripts =
            vec![formcalc_script("@@ALSO_BROKEN@@", "initialize")];

        apply_dynamic_scripts(&mut tree, root).unwrap();

        // Fields should retain their original values (rollback).
        match &tree.get(field_a).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "hello"),
            _ => panic!("expected field"),
        }
        match &tree.get(field_b).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "world"),
            _ => panic!("expected field"),
        }
    }

    #[test]
    fn rollback_when_populated_fields_go_empty() {
        // Calculate scripts returning Null clear field values. When >50%
        // of populated fields go empty, the rollback heuristic fires.
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let field_a = add_node(
            &mut tree,
            "FieldA",
            FormNodeType::Field {
                value: "keep".to_string(),
            },
        );
        let field_b = add_node(
            &mut tree,
            "FieldB",
            FormNodeType::Field {
                value: "also_keep".to_string(),
            },
        );

        tree.get_mut(root).children = vec![field_a, field_b];

        // Calculate scripts whose return value (Null) is written to the field,
        // blanking it.  The expression `Null()` is not a real FormCalc builtin,
        // but `0` would set the field to "0" (not empty).  Instead we use the
        // snapshot/rollback logic directly.
        // We test the heuristic by manually setting up the condition.
        let snapshot = super::snapshot_form(&tree);

        // Simulate scripts clearing both fields.
        if let FormNodeType::Field { value } = &mut tree.get_mut(field_a).node_type {
            *value = String::new();
        }
        if let FormNodeType::Field { value } = &mut tree.get_mut(field_b).node_type {
            *value = String::new();
        }

        assert!(super::should_rollback(&tree, &snapshot, 0, 2));

        super::restore_snapshot(&mut tree, &snapshot);

        match &tree.get(field_a).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "keep"),
            _ => panic!("expected field"),
        }
        match &tree.get(field_b).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "also_keep"),
            _ => panic!("expected field"),
        }
    }

    #[test]
    fn no_rollback_when_scripts_succeed() {
        // A single working script with no errors → no rollback, change persists.
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let total = add_node(
            &mut tree,
            "Total",
            FormNodeType::Field {
                value: String::new(),
            },
        );

        tree.get_mut(root).children = vec![total];
        tree.meta_mut(total).event_scripts = vec![formcalc_script("40 + 2", "calculate")];

        apply_dynamic_scripts(&mut tree, root).unwrap();

        match &tree.get(total).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "42"),
            _ => panic!("expected field"),
        }
    }
}
