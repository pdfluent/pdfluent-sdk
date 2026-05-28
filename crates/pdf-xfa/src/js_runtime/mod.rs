//! M3-B Phase B — JavaScript runtime adapter (skeleton).
//!
//! This module is the integration boundary between `crates/pdf-xfa`'s
//! flatten pipeline and a sandboxed JavaScript runtime. Phase B ships
//! the boundary plus a `NullRuntime` stub. The rquickjs-backed runtime
//! is gated behind the `xfa-js-sandboxed` Cargo feature and registers
//! **no host bindings** — Phase C adds the first useful set per
//! `benchmarks/runs/M3B_HOST_BINDINGS_MINIMUM_SET.md`.
//!
//! The default behaviour of [`crate::dynamic::apply_dynamic_scripts`]
//! and `flatten_xfa_to_pdf` is unchanged: with the feature off and
//! mode `BestEffortStatic`, the runtime is never invoked. Adding the
//! adapter is intentionally behaviour-neutral.
//!
//! See `benchmarks/runs/M3B_RUNTIME_SECURITY_MODEL.md` for the 18
//! invariants the adapter must respect (S-1..S-18).

pub mod host;
pub mod null;
pub mod regex_guard;

#[cfg(feature = "xfa-js-sandboxed")]
pub mod rquickjs_backend;

pub use host::{
    HostBindings, MutationLogEntry, MAX_INSTANCES_PER_SUBFORM, MAX_ITEMS_PER_LISTBOX,
    MAX_MUTATIONS_PER_DOC, MAX_RESOLVE_CALLS_PER_SCRIPT, MAX_RESOLVE_RESULTS, MAX_SOM_DEPTH,
};
pub use null::NullRuntime;
#[cfg(feature = "xfa-js-sandboxed")]
pub use rquickjs_backend::QuickJsRuntime;

use xfa_dom_resolver::data_dom::DataDom;
use xfa_layout_engine::form::{FormNodeId, FormTree};

/// Outcome of evaluating one script body inside the sandbox.
#[derive(Debug, Clone, Default)]
pub struct RuntimeOutcome {
    /// True when the script ran to completion inside the sandbox.
    pub executed: bool,
    /// Number of host-tree mutations the script applied via the
    /// (currently empty) host-binding allowlist. Always 0 in Phase B.
    pub mutated_field_count: usize,
}

/// Errors the runtime adapter can emit. Every variant is recoverable
/// at the dispatch site — the parent flatten never aborts because of
/// a sandbox error (S-17 fail-open).
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum SandboxError {
    /// Cargo feature `xfa-js-sandboxed` not compiled in. Returned by
    /// the [`null::NullRuntime`] for every `execute_script` call.
    #[error("sandboxed runtime not compiled in")]
    NotCompiledIn,

    /// Script body exceeds the per-script size cap (S-11; default 64 KB).
    #[error("script body exceeds size cap")]
    BodyTooLarge,

    /// Per-script time budget exceeded (S-9; default 100 ms hard).
    #[error("script time budget exceeded")]
    Timeout,

    /// Per-document memory budget exceeded (S-10; default 32 MiB hard).
    #[error("document memory budget exceeded")]
    OutOfMemory,

    /// Call stack depth exceeded the configured maximum (S-12; default 64).
    #[error("call stack overflow")]
    StackOverflow,

    /// Activity not in the runtime allowlist for sandboxed dispatch
    /// (S-14). UI / submission activities skip the runtime entirely
    /// at the dispatch boundary and never reach this error path; this
    /// variant exists for explicit binding-level phase guards.
    #[error("activity {0:?} denied for sandbox dispatch")]
    PhaseDenied(String),

    /// Phase B: no host bindings registered. Returned when a script
    /// attempts to read or write any `xfa.*` / `field.*` binding the
    /// adapter has not yet exposed.
    #[error("no host bindings registered (Phase B skeleton)")]
    NoBindings,

    /// FFI panic captured via `std::panic::catch_unwind`. Used by the
    /// rquickjs backend to keep panics from crossing the FFI boundary
    /// into the Rust caller.
    #[error("sandbox panic captured: {0}")]
    PanicCaptured(String),

    /// Generic script-level error: parse, runtime, or thrown JS error.
    #[error("script error: {0}")]
    ScriptError(String),

    /// W3-A — REDOS-01 mitigation: the script body contains a regex
    /// pattern shape known to cause catastrophic backtracking in QuickJS's
    /// NFA-based engine (e.g. `(a+)+$`). The body is rejected before
    /// reaching the sandbox to bound CPU time. See
    /// `crates/pdf-xfa/src/js_runtime/regex_guard.rs` for the heuristic
    /// catalogue.
    #[error("regex rejected by ReDoS guard: {0}")]
    RegexRejected(String),

    /// QF1-E / SEC-01 — defence-in-depth wall-time fallback. The primary
    /// per-script time budget is enforced via the rquickjs interrupt
    /// handler that polls at JS opcode boundaries (see
    /// [`SandboxError::Timeout`]). When the interrupt callback fails to
    /// fire for an extended period — for example because execution is
    /// trapped inside a single C-level call (regex, JSON.parse on a
    /// pathological input, host binding routine) that does not yield
    /// opcode boundaries — the wall-clock can drift well beyond the
    /// configured budget. This variant is emitted when total elapsed
    /// time crossed the fallback threshold
    /// ([`WALLTIME_FALLBACK_MULTIPLIER_DEFAULT`] × the configured
    /// time budget). It is strictly a *post-hoc classification*:
    /// the primary interrupt path remains in charge of actually aborting
    /// script execution; this variant simply re-labels the error so
    /// observability can distinguish "clean stop at budget" from
    /// "stop dragged past 5×". See
    /// `crates/pdf-xfa/src/js_runtime/rquickjs_backend.rs` and the
    /// QF1_E report under `benchmarks/runs/xfa_enterprise_plan/quality_factory_v1/`.
    #[error("wall-time fallback fired: elapsed {0}")]
    WallTimeExceeded(String),
}

/// Cumulative metadata for a single document's flatten. The runtime
/// adapter accumulates counters across calls; the dispatch site reads
/// them via [`XfaJsRuntime::take_metadata`] when the document is done.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuntimeMetadata {
    /// Scripts that ran to completion inside the sandbox.
    pub executed: usize,
    /// Generic runtime / script errors (parse, throw, NoBindings, …).
    pub runtime_errors: usize,
    /// Time-budget exhaustions.
    pub timeouts: usize,
    /// Memory-budget exhaustions.
    pub oom: usize,
    /// Phase C host-binding invocations.
    pub host_calls: usize,
    /// Phase C successful `field.rawValue` writes.
    pub mutations: usize,
    /// Phase D successful instanceManager structure writes.
    pub instance_writes: usize,
    /// Phase D-β successful listbox clearItems / addItem writes.
    pub list_writes: usize,
    /// Phase C binding-level failures (type, activity, cap, parse).
    pub binding_errors: usize,
    /// Phase C SOM resolution misses / failures.
    pub resolve_failures: usize,
    /// Phase D-γ successful DataDom reads (children / value / child-by-name).
    pub data_reads: usize,
    /// Phase E (XFA-JS-HOST-STUBS): Calls into host capabilities that require
    /// genuine viewer / user interaction (UI dialogs, signature panels,
    /// network submit). The sandbox cannot honestly satisfy these during a
    /// non-interactive flatten; instead of raising a `TypeError` (which would
    /// abort the script and inflate [`runtime_errors`](Self::runtime_errors))
    /// the stubs return a safe default value and increment this counter so
    /// the dispatch site keeps observability of "would-have-been-interactive"
    /// touch points. Note: this counter is intentionally NOT folded into
    /// [`is_clean`](Self::is_clean) — a script that touched
    /// `xfa.host.messageBox` is still considered to have run cleanly because
    /// the sandbox did not error; embedders that care about UI gaps should
    /// inspect this field explicitly.
    pub unsupported_host_calls: usize,
    /// Phase D-θ.2 probe calls skipped because `parentIds.length == 1 &&
    /// chain.length == 1` (no same-name ambiguity possible).  Every skipped
    /// call saves one `resolveWithFullChainStrict` host round-trip.
    pub probe_skips: usize,
    /// D3 (trace-only): `<variables>` `<script>` objects collected from the
    /// template for this document (root + subform scopes).
    pub variables_scripts_collected: usize,
    /// D3 (trace-only): `<variables>` `<text>` data items collected.
    pub variables_data_items_collected: usize,
    /// D3 (trace-only): script objects whose JS-side registration returned
    /// success (namespace bound into `variablesScripts` / `subformVariables`).
    pub script_objects_registered: usize,
    /// D3 (trace-only): script objects that did NOT register — either a Rust
    /// skip (`BodyTooLarge` / `RegexRejected` / panic) or a JS-side eval
    /// failure (`setVariablesScript` returned `false`). Pure observability;
    /// never folded into [`is_clean`](Self::is_clean) or rollback.
    pub script_objects_register_failed: usize,
    /// D3 (trace-only): script objects collected under a NESTED subform scope
    /// (registered into `subformVariables` only, hence not reachable as a bare
    /// identifier today — the "scope_hidden" gap class).
    pub script_objects_subform_scoped: usize,
    /// D4: total SOM lookups observed at the host resolve boundary (successes +
    /// failures across the instrumented `resolve_*` entry points).
    pub som_lookups_total: usize,
    /// D4: SOM lookups that resolved to at least one node.
    pub som_lookup_successes: usize,
    /// D4: SOM lookups that returned NoMatch.
    pub som_lookup_failures: usize,
    /// D4: subform-scoped script-object names NOT exposed because the same name
    /// is declared by ≥2 subforms (fail-closed ambiguity).
    pub som_lookup_ambiguous: usize,
    /// D4: subform-scoped script objects exposed to bare-identifier lookup
    /// (unique-name, sandboxed-only).
    pub som_subform_scripts_exposed: usize,
    /// D4 (trace-only): SOM NoMatch references whose path is an `occur` path
    /// (`occur` / `occur.min` / `occur.max` …). Classified, NOT resolved.
    pub som_occur_path_refs: usize,
    /// D5: `node.occur` handle accesses (successes + failures).
    pub occur_lookups_total: usize,
    /// D5: `node.occur` accesses where the node handle was live.
    pub occur_lookup_successes: usize,
    /// D5: `node.occur` accesses where the node handle was not live.
    pub occur_lookup_failures: usize,
    /// D5: reads of an `occur` property (`min`/`max`/`initial`).
    pub occur_property_reads: usize,
    /// D5: writes to an `occur` property (captured, not applied).
    pub occur_property_writes: usize,
    /// D5: writes specifically to `occur.min`.
    pub occur_min_writes: usize,
    /// D5: writes specifically to `occur.max`.
    pub occur_max_writes: usize,
    /// D5: occur mutations captured as intent (no layout effect).
    pub occur_mutations_captured: usize,
    /// D5: occur mutations APPLIED to layout. **Always 0 in D5** (capture-only);
    /// D6 bumps this when `XFA_OCCUR_APPLY=1` applies a captured `occur.min`.
    pub occur_mutations_applied: usize,
    /// D6: captured occur mutations NOT applied (rollback, apply-flag off,
    /// dead/non-repeatable target, unsupported prop, negative value).
    pub occur_mutations_skipped: usize,
    /// D6: captured occur mutations skipped because the target node is not a
    /// repeatable container (Subform/Area/ExclGroup) — fail-closed.
    pub occur_application_ambiguous: usize,
    /// D6: distinct form nodes whose occur was applied.
    pub occur_application_targets: usize,
    /// BE-1: `$data` bare-global intercepts resolved successfully (JS layer).
    pub som_data_root_hits: usize,
    /// BE-1: `#items` property accesses resolved to a non-empty item list.
    pub som_items_path_hits: usize,
}

impl RuntimeMetadata {
    /// True when the runtime never reported any error class.
    pub fn is_clean(&self) -> bool {
        self.runtime_errors == 0
            && self.timeouts == 0
            && self.oom == 0
            && self.binding_errors == 0
            && self.resolve_failures == 0
    }

    /// Add another metadata snapshot into this one.
    pub fn accumulate(&mut self, other: RuntimeMetadata) {
        self.executed = self.executed.saturating_add(other.executed);
        self.runtime_errors = self.runtime_errors.saturating_add(other.runtime_errors);
        self.timeouts = self.timeouts.saturating_add(other.timeouts);
        self.oom = self.oom.saturating_add(other.oom);
        self.host_calls = self.host_calls.saturating_add(other.host_calls);
        self.mutations = self.mutations.saturating_add(other.mutations);
        self.instance_writes = self.instance_writes.saturating_add(other.instance_writes);
        self.list_writes = self.list_writes.saturating_add(other.list_writes);
        self.binding_errors = self.binding_errors.saturating_add(other.binding_errors);
        self.resolve_failures = self.resolve_failures.saturating_add(other.resolve_failures);
        self.data_reads = self.data_reads.saturating_add(other.data_reads);
        self.unsupported_host_calls = self
            .unsupported_host_calls
            .saturating_add(other.unsupported_host_calls);
        self.probe_skips = self.probe_skips.saturating_add(other.probe_skips);
        self.variables_scripts_collected = self
            .variables_scripts_collected
            .saturating_add(other.variables_scripts_collected);
        self.variables_data_items_collected = self
            .variables_data_items_collected
            .saturating_add(other.variables_data_items_collected);
        self.script_objects_registered = self
            .script_objects_registered
            .saturating_add(other.script_objects_registered);
        self.script_objects_register_failed = self
            .script_objects_register_failed
            .saturating_add(other.script_objects_register_failed);
        self.script_objects_subform_scoped = self
            .script_objects_subform_scoped
            .saturating_add(other.script_objects_subform_scoped);
        self.som_lookups_total = self
            .som_lookups_total
            .saturating_add(other.som_lookups_total);
        self.som_lookup_successes = self
            .som_lookup_successes
            .saturating_add(other.som_lookup_successes);
        self.som_lookup_failures = self
            .som_lookup_failures
            .saturating_add(other.som_lookup_failures);
        self.som_lookup_ambiguous = self
            .som_lookup_ambiguous
            .saturating_add(other.som_lookup_ambiguous);
        self.som_subform_scripts_exposed = self
            .som_subform_scripts_exposed
            .saturating_add(other.som_subform_scripts_exposed);
        self.som_occur_path_refs = self
            .som_occur_path_refs
            .saturating_add(other.som_occur_path_refs);
        self.occur_lookups_total = self
            .occur_lookups_total
            .saturating_add(other.occur_lookups_total);
        self.occur_lookup_successes = self
            .occur_lookup_successes
            .saturating_add(other.occur_lookup_successes);
        self.occur_lookup_failures = self
            .occur_lookup_failures
            .saturating_add(other.occur_lookup_failures);
        self.occur_property_reads = self
            .occur_property_reads
            .saturating_add(other.occur_property_reads);
        self.occur_property_writes = self
            .occur_property_writes
            .saturating_add(other.occur_property_writes);
        self.occur_min_writes = self.occur_min_writes.saturating_add(other.occur_min_writes);
        self.occur_max_writes = self.occur_max_writes.saturating_add(other.occur_max_writes);
        self.occur_mutations_captured = self
            .occur_mutations_captured
            .saturating_add(other.occur_mutations_captured);
        self.occur_mutations_applied = self
            .occur_mutations_applied
            .saturating_add(other.occur_mutations_applied);
        self.occur_mutations_skipped = self
            .occur_mutations_skipped
            .saturating_add(other.occur_mutations_skipped);
        self.occur_application_ambiguous = self
            .occur_application_ambiguous
            .saturating_add(other.occur_application_ambiguous);
        self.occur_application_targets = self
            .occur_application_targets
            .saturating_add(other.occur_application_targets);
        self.som_data_root_hits = self
            .som_data_root_hits
            .saturating_add(other.som_data_root_hits);
        self.som_items_path_hits = self
            .som_items_path_hits
            .saturating_add(other.som_items_path_hits);
    }
}

/// Epic A E-2/E-3: verbose per-entry diagnostic logs drained alongside
/// [`RuntimeMetadata`] from [`HostBindings`].  Empty unless
/// `XFA_RUNTIME_DIAG=1` is set at the HostBindings call sites.
#[derive(Debug, Default)]
pub struct RuntimeDiagLogs {
    /// E-2: SOM resolution misses (capped at 200).
    pub som_fail_log: Vec<crate::dynamic::SomFailEntry>,
    /// E-3: instanceManager write events (capped at 200).
    pub instance_write_log: Vec<crate::dynamic::InstanceWriteEntry>,
}

/// Default per-script wall-clock budget enforced by the rquickjs
/// backend (S-9). Exposed as a constant so tests can reason about it
/// without depending on the runtime backend module.
pub const DEFAULT_TIME_BUDGET_MS: u64 = 100;

/// QF1-E / SEC-01 — default multiplier for the worker-level wall-time
/// fallback. The primary timeout enforcement is the rquickjs interrupt
/// callback (polled at JS opcode boundaries); when the script's elapsed
/// wall-clock exceeds this multiplier × the configured time budget, the
/// backend re-labels the resulting error as
/// [`SandboxError::WallTimeExceeded`] (instead of [`SandboxError::Timeout`]).
///
/// The default of `5` means: if the interrupt callback fires within ≤ 5×
/// the configured budget (the normal case for `while(true){}`), behaviour
/// is byte-identical to v1: `SandboxError::Timeout` is emitted.
///
/// Only when the abort drags past 5× the budget — indicating the primary
/// interrupt mechanism was unable to fire at the budget boundary, e.g.
/// because execution was trapped inside a single C-level call — does the
/// fallback variant surface. This is observable telemetry, not a new
/// kill switch: the interrupt still does the actual aborting.
///
/// The multiplier is generous on purpose: small CI scheduling jitter on
/// a 50 ms budget (1× = 50 ms; 5× = 250 ms) must never accidentally
/// trip the fallback for a script the interrupt aborted cleanly.
pub const WALLTIME_FALLBACK_MULTIPLIER_DEFAULT: u32 = 5;

/// QF1-E / SEC-01 — environment variable that overrides the wall-time
/// fallback multiplier ([`WALLTIME_FALLBACK_MULTIPLIER_DEFAULT`]). Only
/// values that parse as a positive `u32` ≥ 2 are honoured; anything else
/// (absent, empty, `"0"`, `"1"`, non-numeric) keeps the default. The
/// minimum of 2 prevents an operator from accidentally collapsing the
/// fallback onto the primary timeout boundary, which would re-classify
/// every clean timeout as a wall-time fallback and corrupt observability.
pub const ENV_WALLTIME_FALLBACK_MULTIPLIER: &str = "XFA_JS_WALLTIME_FALLBACK_MULTIPLIER";

/// QF1-E / SEC-01 — read the wall-time fallback multiplier from the
/// environment, clamped to a sane range. Returns
/// [`WALLTIME_FALLBACK_MULTIPLIER_DEFAULT`] when the env var is absent,
/// not a valid `u32`, or below the safety minimum of `2`.
///
/// Reads the env var once per call; the backend snapshots the result at
/// construction time so per-test toggling is deterministic.
pub fn walltime_fallback_multiplier() -> u32 {
    std::env::var(ENV_WALLTIME_FALLBACK_MULTIPLIER)
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&n| n >= 2)
        .unwrap_or(WALLTIME_FALLBACK_MULTIPLIER_DEFAULT)
}

/// Default per-document memory budget enforced by the rquickjs backend
/// (S-10).
pub const DEFAULT_MEMORY_BUDGET_BYTES: usize = 32 * 1024 * 1024;

/// Hard cap on script body size (S-11). Bodies above this length are
/// rejected before any parse attempt.
pub const MAX_SCRIPT_BODY_BYTES: usize = 64 * 1024;

/// Hard cap on `<variables><script>` body size (W2-B).
///
/// Variables-scripts are form-level helper libraries (XFA 3.3 §5.5):
/// they hold the top-level `var` / `function` declarations that event
/// scripts call as `<scriptName>.<top_level_decl>(...)`. They run once
/// per document at registration time, under the same per-script time
/// budget ([`DEFAULT_TIME_BUDGET_MS`]) and same per-document memory
/// budget ([`DEFAULT_MEMORY_BUDGET_BYTES`]) as event scripts.
///
/// Real-world government XFA forms (Canadian IRCC `imm5709e` / `imm5710e`,
/// Canadian Revenue Agency `t2200` / `t2-fill`) ship variables-scripts
/// such as `validateForm` (~125 KB), `CoreFunctions` (~115 KB) and
/// `LOV` (~507 KB) — well above the 64 KB event-script cap. With the
/// event-script cap applied to variables-scripts, registration fails
/// silently with [`SandboxError::BodyTooLarge`] and the dependent
/// event scripts cannot resolve `validateForm.X()` / `CoreFunctions.X()`,
/// surfacing as the W1-B `implicit_function` cluster (impact 79 across
/// 10 docs).
///
/// 1 MiB is intentionally above the largest observed real-world body
/// (`LOV` ≈ 507 KB on `imm5710e`) so the cap remains a defence-in-depth
/// stop and never a routine failure path. The time and memory budgets
/// still bound runaway parse / execute cost.
pub const MAX_VARIABLES_SCRIPT_BODY_BYTES: usize = 1024 * 1024;

/// The activities for which the sandboxed runtime accepts dispatch.
/// Other activities (`click`, `preSubmit`, `mouseEnter`, …) skip the
/// runtime entirely at the [`crate::dynamic::apply_dynamic_scripts_with_mode`]
/// boundary because they do not fire during static flatten (S-14).
pub const SANDBOX_ACTIVITY_ALLOWLIST: &[&str] = &[
    "initialize",
    "calculate",
    "validate",
    "docReady",
    "layoutReady",
];

/// True when `activity` is in [`SANDBOX_ACTIVITY_ALLOWLIST`].
pub fn activity_allowed_for_sandbox(activity: Option<&str>) -> bool {
    matches!(activity, Some(a) if SANDBOX_ACTIVITY_ALLOWLIST.contains(&a))
}

/// **D1.B gated allow.** Environment variable that opts an operator into the
/// `preSave` dispatch path during flatten. Default OFF; any value other than
/// `"1"` keeps the W3-B closure semantics (deny `preSave` at the dispatch
/// gate AND at the host-binding gate).
///
/// Cross-ref: `docs/INST_MGR_ACTIVITY_POLICY.md` v5 §6.1 (D1.B).
///
/// **Stop-rules (enforced by tests, never by code):**
/// 1. Only `preSave` is affected. `preSubmit`, `click`, and every other
///    denylist activity stay denied regardless of this flag.
/// 2. Default-OFF. Behaviour is byte-identical to v4 (W3-B closure) when
///    the variable is absent or unset, or set to any value other than `"1"`.
/// 3. Flipping requires an operator-signed waiver per the policy doc; the
///    flag is read at dispatch (one snapshot per flatten), never globally
///    memoised, so test harnesses can toggle it per-test.
pub const ENV_PRESAVE_DURING_FLATTEN: &str = "XFA_PRESAVE_DURING_FLATTEN";

/// True when `XFA_PRESAVE_DURING_FLATTEN=1`. Any other value (absent, empty,
/// `"0"`, `"true"`, `"yes"`, casing variants) returns false. This is the
/// only place that reads the environment for the D1.B gate — every other
/// site receives a `bool` argument so tests can toggle deterministically.
pub fn presave_during_flatten_enabled() -> bool {
    std::env::var(ENV_PRESAVE_DURING_FLATTEN).ok().as_deref() == Some("1")
}

/// **D1.B gated allow.** Same as [`activity_allowed_for_sandbox`], but
/// also accepts `Some("preSave")` when `presave_gate` is true.
///
/// `presave_gate` is computed once per flatten (via
/// [`presave_during_flatten_enabled`]) and threaded down so the dispatch
/// path can decide deterministically per script; the host-binding layer
/// receives the same bool via [`HostBindings::set_presave_gate`].
///
/// Cross-ref: `docs/INST_MGR_ACTIVITY_POLICY.md` v5 §6.1 (D1.B).
pub fn activity_allowed_for_sandbox_with_gate(activity: Option<&str>, presave_gate: bool) -> bool {
    if activity_allowed_for_sandbox(activity) {
        return true;
    }
    presave_gate && matches!(activity, Some("preSave"))
}

/// The host-side adapter the dispatch path calls. A minimal contract
/// chosen so that swapping backends (rquickjs ↔ boa ↔ external sandbox)
/// is one Cargo feature flag away.
pub trait XfaJsRuntime {
    /// One-time initialisation. Idempotent.
    fn init(&mut self) -> Result<(), SandboxError>;

    /// Reset per-document state (memory budget, instruction counter,
    /// any cached compiled scripts). Called once per flatten.
    fn reset_for_new_document(&mut self) -> Result<(), SandboxError>;

    /// Phase C: install the `FormTree` the runtime should resolve paths
    /// against and mutate. The dispatch path owns the mutable borrow and clears
    /// the handle before returning.
    fn set_form_handle(
        &mut self,
        _form: *mut FormTree,
        _root_id: FormNodeId,
    ) -> Result<(), SandboxError> {
        Ok(())
    }

    /// Phase D-γ: install a read-only view of the `DataDom` for the current
    /// document. Called once per document after `set_form_handle`, before any
    /// scripts run. Default: no-op (backends without DataDom support ignore it).
    ///
    /// # Safety
    /// Callers **must** guarantee that `dom` outlives all script execution for
    /// this document (i.e. it must remain alive until `set_form_handle(null)`
    /// is called). The runtime stores the pointer read-only and never writes
    /// through it.
    fn set_data_handle(&mut self, _dom: *const DataDom) {}

    /// BE-1 tranche #1 (benign zero-instance SOM): install the set of
    /// template-declared container names (`subform`/`subformSet`/`exclGroup`/
    /// `area`) for the current document. Backends that resolve implicit SOM
    /// identifiers use it to return a benign empty-node façade for a
    /// declared-but-absent reference instead of `undefined` (Adobe semantics),
    /// so guarded scripts (`if (!Sub.Child.isNull) {...} else {...}`) run their
    /// else branch instead of throwing. Like [`set_data_handle`], the caller
    /// installs this before script execution. Default: no-op (the static
    /// `NullRuntime` ignores it, so the default/non-sandboxed path is
    /// unaffected and stays byte-identical).
    ///
    /// [`set_data_handle`]: XfaJsRuntime::set_data_handle
    fn set_declared_subform_names(&mut self, _names: std::collections::HashSet<String>) {}

    /// Phase C: reset per-script host counters and install the current script
    /// context node / activity. Backends without host bindings ignore it.
    fn reset_per_script(
        &mut self,
        _current_id: FormNodeId,
        _activity: Option<&str>,
    ) -> Result<(), SandboxError> {
        Ok(())
    }

    /// Phase C page-count foundation. The current flatten order runs scripts
    /// before layout, so callers normally leave this at 0.
    fn set_static_page_count(&mut self, _page_count: u32) -> Result<(), SandboxError> {
        Ok(())
    }

    /// **D1.B gated allow.** Inform the runtime whether the
    /// `XFA_PRESAVE_DURING_FLATTEN=1` opt-in is active for the current
    /// flatten. The dispatch path computes this once per document via
    /// [`presave_during_flatten_enabled`] and forwards it here so the host
    /// binding layer can mirror the dispatch decision (defence-in-depth).
    ///
    /// Default: no-op. Backends without a host-binding gate ignore it.
    /// The contract for backends that DO mirror the gate:
    ///
    /// 1. Default OFF: every call to [`HostBindings::write_activity_allowed`]
    ///    with `current_activity = Some("preSave")` MUST return false.
    /// 2. Gate ON: the same call MUST return true ONLY for `Some("preSave")`.
    ///    Every other denylist activity (`preSubmit`, `click`, …) MUST
    ///    continue to return false.
    ///
    /// Cross-ref: `docs/INST_MGR_ACTIVITY_POLICY.md` v5 §6.1 (D1.B).
    fn set_presave_gate(&mut self, _enabled: bool) {}

    /// Execute one script body inside the sandbox.
    ///
    /// `activity` is the enclosing `<event activity="...">` value if
    /// any. The dispatch site has already filtered against
    /// [`activity_allowed_for_sandbox`]; backends may treat unknown
    /// activities as `PhaseDenied` for defence-in-depth.
    fn execute_script(
        &mut self,
        activity: Option<&str>,
        body: &str,
    ) -> Result<RuntimeOutcome, SandboxError>;

    /// Take the cumulative metadata since the last `take_metadata`
    /// call (or since `reset_for_new_document`, whichever was later).
    fn take_metadata(&mut self) -> RuntimeMetadata;

    /// D6: drain captured `occur.min`/`occur.max` write intents
    /// `(node_index, prop, value)` recorded during the script pass. The
    /// dispatch path applies them (only when `XFA_OCCUR_APPLY=1`) after the
    /// rollback decision. Default: none (non-sandboxed runtimes capture nothing).
    fn take_occur_mutations(&mut self) -> Vec<(usize, String, i64)> {
        Vec::new()
    }

    /// Epic A E-2/E-3: drain verbose per-entry diagnostic logs. Only
    /// populated when `XFA_RUNTIME_DIAG=1` is set at the host call sites.
    /// Default impl returns empty logs (NullRuntime, non-sandboxed paths).
    fn take_diag_logs(&mut self) -> RuntimeDiagLogs {
        RuntimeDiagLogs::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_accepts_initialize_and_calculate() {
        assert!(activity_allowed_for_sandbox(Some("initialize")));
        assert!(activity_allowed_for_sandbox(Some("calculate")));
        assert!(activity_allowed_for_sandbox(Some("validate")));
        assert!(activity_allowed_for_sandbox(Some("docReady")));
        assert!(activity_allowed_for_sandbox(Some("layoutReady")));
    }

    // D1.B gate: when the flag is OFF (the default in tests via env::var
    // being unset for the controlled key), `activity_allowed_for_sandbox_with_gate`
    // is byte-identical to `activity_allowed_for_sandbox`. When the gate
    // bool is wired ON, ONLY `preSave` flips to allowed — `preSubmit`,
    // `click`, etc. stay denied (hard stop in §6.1 of the policy doc).
    #[test]
    fn presave_gate_off_matches_base_allowlist() {
        for allowed in SANDBOX_ACTIVITY_ALLOWLIST {
            assert!(activity_allowed_for_sandbox_with_gate(Some(allowed), false));
        }
        for denied in [
            "preSave",
            "preSubmit",
            "click",
            "mouseEnter",
            "exit",
            "postSave",
        ] {
            assert!(!activity_allowed_for_sandbox_with_gate(Some(denied), false));
        }
        assert!(!activity_allowed_for_sandbox_with_gate(None, false));
    }

    #[test]
    fn presave_gate_on_unlocks_only_presave() {
        // preSave flips from deny -> allow when the gate is ON.
        assert!(activity_allowed_for_sandbox_with_gate(
            Some("preSave"),
            true
        ));
        // Hard-stop: every other denylist activity MUST stay denied.
        for still_denied in [
            "preSubmit",
            "click",
            "mouseEnter",
            "mouseExit",
            "exit",
            "enter",
            "change",
            "postSave",
            "postSubmit",
            "ready",
            "prePrint",
            "postPrint",
            "preOpen",
            "full",
        ] {
            assert!(
                !activity_allowed_for_sandbox_with_gate(Some(still_denied), true),
                "{still_denied} must stay denied even with D1.B gate ON",
            );
        }
        assert!(!activity_allowed_for_sandbox_with_gate(None, true));
        // Default-OFF behaviour unchanged for the 5 lifecycle activities.
        for allowed in SANDBOX_ACTIVITY_ALLOWLIST {
            assert!(activity_allowed_for_sandbox_with_gate(Some(allowed), true));
        }
    }

    // Note: the env-var helper `presave_during_flatten_enabled` is pinned
    // by integration tests in `tests/m3b_phasePQ_presave_gated_w3repair_d1b.rs`
    // (`d1b_default_off_keeps_presave_denied_at_dispatch`,
    // `d1b_env_var_parsing_only_one_enables_gate`). Inline unit tests would
    // race with `std::env` because cargo test runs in-process; the
    // integration tests serialise env mutations behind a mutex guard.
    //
    // ENV_PRESAVE_DURING_FLATTEN constant is canonicalised at the const
    // declaration site and never re-spelled in code.

    #[test]
    fn presave_env_var_constant_is_canonical_name() {
        assert_eq!(ENV_PRESAVE_DURING_FLATTEN, "XFA_PRESAVE_DURING_FLATTEN");
    }

    #[test]
    fn allowlist_rejects_ui_and_submit_activities() {
        for ui in [
            "click",
            "mouseEnter",
            "mouseExit",
            "enter",
            "exit",
            "preSubmit",
            "postSubmit",
            "ready",
        ] {
            assert!(
                !activity_allowed_for_sandbox(Some(ui)),
                "{ui} must not be allowed",
            );
        }
        assert!(!activity_allowed_for_sandbox(None));
    }

    #[test]
    fn metadata_is_clean_when_zero() {
        assert!(RuntimeMetadata::default().is_clean());
        let mut m = RuntimeMetadata {
            executed: 5,
            ..Default::default()
        };
        assert!(m.is_clean(), "executed counter does not flip cleanliness");
        m.runtime_errors = 1;
        assert!(!m.is_clean());
    }

    #[test]
    // Intentional const-floor contract assertions: these pin the safety
    // floors of compile-time budget constants. assertions_on_constants is
    // expected and desired here.
    #[allow(clippy::assertions_on_constants)]
    fn budget_constants_are_sane() {
        assert!(MAX_SCRIPT_BODY_BYTES >= 4096);
        assert!(DEFAULT_TIME_BUDGET_MS >= 25);
        assert!(DEFAULT_MEMORY_BUDGET_BYTES >= 1024 * 1024);
    }

    // QF1-E: the wall-time fallback multiplier must be strictly > 1 so a
    // clean interrupt at the budget boundary cannot be reclassified as
    // WallTimeExceeded. 5 is the documented default; this test pins it as
    // a contract between this module and the backend.
    // Compile-time const-assert avoids `clippy::assertions_on_constants`
    // while still failing the build if the safety floor is broken.
    const _WALLTIME_SAFE_MIN: () = assert!(
        WALLTIME_FALLBACK_MULTIPLIER_DEFAULT >= 2,
        "multiplier < 2 would re-label normal Timeouts as WallTimeExceeded"
    );

    #[test]
    fn walltime_fallback_multiplier_default_is_safe() {
        // The compile-time `_WALLTIME_SAFE_MIN` const above is the real
        // contract; this runtime test pins the concrete value so a
        // refactor that changes the default but leaves the floor intact
        // is still caught here.
        assert_eq!(WALLTIME_FALLBACK_MULTIPLIER_DEFAULT, 5);
    }

    #[test]
    fn walltime_fallback_env_var_name_is_canonical() {
        assert_eq!(
            ENV_WALLTIME_FALLBACK_MULTIPLIER,
            "XFA_JS_WALLTIME_FALLBACK_MULTIPLIER"
        );
    }

    // W2-B: variables-script body cap must be strictly higher than the
    // event-script cap so XFA helper libraries (validateForm, LOV,
    // CoreFunctions) register; it must still be bounded so an oversize
    // body cannot bypass static defence-in-depth before the per-document
    // memory budget engages.
    #[test]
    // Intentional const-floor contract assertions on compile-time caps.
    #[allow(clippy::assertions_on_constants)]
    fn variables_script_cap_is_above_event_cap_and_bounded() {
        assert!(
            MAX_VARIABLES_SCRIPT_BODY_BYTES > MAX_SCRIPT_BODY_BYTES,
            "variables-script cap must exceed event-script cap"
        );
        // Sanity: must be high enough to register the largest observed
        // real-world variables-script library (LOV ≈ 507 KB).
        assert!(
            MAX_VARIABLES_SCRIPT_BODY_BYTES >= 768 * 1024,
            "variables-script cap below observed real-world max"
        );
        // Sanity: must be bounded well under the per-document memory
        // budget so a single oversize body cannot consume the entire
        // budget on parse alone.
        assert!(
            MAX_VARIABLES_SCRIPT_BODY_BYTES <= DEFAULT_MEMORY_BUDGET_BYTES / 8,
            "variables-script cap must stay an order of magnitude below memory budget"
        );
    }
}
