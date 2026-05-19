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
    }
}

/// Default per-script wall-clock budget enforced by the rquickjs
/// backend (S-9). Exposed as a constant so tests can reason about it
/// without depending on the runtime backend module.
pub const DEFAULT_TIME_BUDGET_MS: u64 = 100;

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
        let mut m = RuntimeMetadata::default();
        m.executed = 5;
        assert!(m.is_clean(), "executed counter does not flip cleanliness");
        m.runtime_errors = 1;
        assert!(!m.is_clean());
    }

    #[test]
    fn budget_constants_are_sane() {
        assert!(MAX_SCRIPT_BODY_BYTES >= 4096);
        assert!(DEFAULT_TIME_BUDGET_MS >= 25);
        assert!(DEFAULT_MEMORY_BUDGET_BYTES >= 1024 * 1024);
    }

    // W2-B: variables-script body cap must be strictly higher than the
    // event-script cap so XFA helper libraries (validateForm, LOV,
    // CoreFunctions) register; it must still be bounded so an oversize
    // body cannot bypass static defence-in-depth before the per-document
    // memory budget engages.
    #[test]
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
