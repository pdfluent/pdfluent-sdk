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

pub mod null;

#[cfg(feature = "xfa-js-sandboxed")]
pub mod rquickjs_backend;

pub use null::NullRuntime;
#[cfg(feature = "xfa-js-sandboxed")]
pub use rquickjs_backend::QuickJsRuntime;

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
}

impl RuntimeMetadata {
    /// True when the runtime never reported any error class.
    pub fn is_clean(&self) -> bool {
        self.runtime_errors == 0 && self.timeouts == 0 && self.oom == 0
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
}
