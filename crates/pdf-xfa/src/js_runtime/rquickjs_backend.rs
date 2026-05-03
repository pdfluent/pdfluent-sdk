//! M3-B Phase B — rquickjs backend for [`super::XfaJsRuntime`].
//!
//! Compiled in only when the `xfa-js-sandboxed` Cargo feature is enabled.
//!
//! This backend is intentionally minimal:
//!
//! - No host bindings registered. Phase C adds the first useful ones per
//!   `benchmarks/runs/M3B_HOST_BINDINGS_MINIMUM_SET.md`.
//! - `Date.now`, `Math.random`, `fetch`, `require`, `process` and friends
//!   are absent because they are never registered (rquickjs default
//!   contexts expose only spec-mandated ECMAScript built-ins).
//! - `JS_SetMemoryLimit` enforces the per-document memory budget; any
//!   allocation that pushes the runtime over the limit fails with
//!   [`super::SandboxError::OutOfMemory`].
//! - Per-script time budget is enforced with the rquickjs interrupt handler
//!   that polls a wall-clock deadline; `eval_with_options` bails out with
//!   [`super::SandboxError::Timeout`] when the deadline elapses.
//! - All FFI is wrapped in `std::panic::catch_unwind` so a QuickJS panic
//!   never crosses into the parent flatten path
//!   (`benchmarks/runs/M3B_RUNTIME_SECURITY_MODEL.md` §1 S-17).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};

use rquickjs::{Context, Function, Runtime};

use super::{
    activity_allowed_for_sandbox, RuntimeMetadata, RuntimeOutcome, SandboxError, XfaJsRuntime,
    DEFAULT_MEMORY_BUDGET_BYTES, DEFAULT_TIME_BUDGET_MS, MAX_SCRIPT_BODY_BYTES,
};

/// QuickJS-backed runtime adapter. One instance is reusable across many
/// documents; callers MUST invoke [`XfaJsRuntime::reset_for_new_document`]
/// at the start of each flatten.
pub struct QuickJsRuntime {
    runtime: Runtime,
    context: Context,
    metadata: RuntimeMetadata,
    time_budget: Duration,
    memory_budget_bytes: usize,
    script_deadline: Arc<AtomicU64>,
    script_started: Arc<AtomicBool>,
}

impl std::fmt::Debug for QuickJsRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuickJsRuntime")
            .field("metadata", &self.metadata)
            .field("time_budget_ms", &self.time_budget.as_millis())
            .field("memory_budget_bytes", &self.memory_budget_bytes)
            .finish()
    }
}

impl QuickJsRuntime {
    /// Construct a new sandboxed runtime with default budgets.
    pub fn new() -> Result<Self, SandboxError> {
        let runtime =
            Runtime::new().map_err(|e| SandboxError::ScriptError(format!("rquickjs init: {e}")))?;
        runtime.set_memory_limit(DEFAULT_MEMORY_BUDGET_BYTES);
        let context = Context::full(&runtime)
            .map_err(|e| SandboxError::ScriptError(format!("rquickjs context: {e}")))?;

        let script_deadline = Arc::new(AtomicU64::new(0));
        let script_started = Arc::new(AtomicBool::new(false));

        // Interrupt handler: poll the deadline. When the started flag is set
        // and the wall-clock has crossed the deadline, return `true` to abort
        // script execution. QuickJS turns this into a JS-level interrupt that
        // surfaces as `eval` returning `Err`.
        let deadline_for_handler = Arc::clone(&script_deadline);
        let started_for_handler = Arc::clone(&script_started);
        runtime.set_interrupt_handler(Some(Box::new(move || {
            if !started_for_handler.load(Ordering::Acquire) {
                return false;
            }
            let deadline_nanos = deadline_for_handler.load(Ordering::Acquire);
            if deadline_nanos == 0 {
                return false;
            }
            let now_nanos = Instant::now()
                .checked_duration_since(epoch())
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            now_nanos >= deadline_nanos
        })));

        Ok(Self {
            runtime,
            context,
            metadata: RuntimeMetadata::default(),
            time_budget: Duration::from_millis(DEFAULT_TIME_BUDGET_MS),
            memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
            script_deadline,
            script_started,
        })
    }

    /// Override the per-script wall-clock budget. Must be called before any
    /// `execute_script` invocation; takes effect on the next call.
    pub fn with_time_budget(mut self, budget: Duration) -> Self {
        self.time_budget = budget;
        self
    }

    /// Override the per-document memory budget. Must be called before any
    /// `execute_script` invocation; takes effect on the next document.
    pub fn with_memory_budget(mut self, bytes: usize) -> Self {
        self.memory_budget_bytes = bytes;
        self.runtime.set_memory_limit(bytes);
        self
    }

    fn set_deadline(&self) {
        let deadline = Instant::now()
            .checked_duration_since(epoch())
            .map(|d| d + self.time_budget)
            .unwrap_or(self.time_budget);
        self.script_deadline
            .store(deadline.as_nanos() as u64, Ordering::Release);
        self.script_started.store(true, Ordering::Release);
    }

    fn clear_deadline(&self) {
        self.script_started.store(false, Ordering::Release);
        self.script_deadline.store(0, Ordering::Release);
    }
}

// Process-wide reference epoch; used together with `Instant::now() - EPOCH`
// to materialise a u64 nanosecond timestamp comparable across the interrupt
// handler closure and the dispatch path. We never expose this to scripts.
static EPOCH_CELL: OnceLock<Instant> = OnceLock::new();
fn epoch() -> Instant {
    *EPOCH_CELL.get_or_init(Instant::now)
}

impl XfaJsRuntime for QuickJsRuntime {
    fn init(&mut self) -> Result<(), SandboxError> {
        // Defensive: ensure no host binding leaked into globalThis.
        // We call this in a `with` because rquickjs Contexts borrow a
        // !Send handle; the catch_unwind crosses the FFI boundary.
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.context.with(|ctx| {
                let globals = ctx.globals();
                // Strip non-deterministic / capability-bearing globals if
                // any third-party crate ever registered them. Phase B
                // registers nothing, but defence in depth is cheap.
                for forbidden in [
                    "fetch",
                    "XMLHttpRequest",
                    "WebSocket",
                    "process",
                    "require",
                    "Deno",
                    "Bun",
                ] {
                    let _ = globals.set(forbidden, rquickjs::Undefined);
                }
                // Replace Date.now and Math.random with deterministic stubs.
                if let Ok(date_ctor) = globals.get::<_, rquickjs::Object>("Date") {
                    let zero_now = Function::new(ctx.clone(), || 0i64)
                        .map_err(|e| format!("date stub: {e}"))?;
                    let _ = date_ctor.set("now", zero_now);
                }
                if let Ok(math_ns) = globals.get::<_, rquickjs::Object>("Math") {
                    let _ = math_ns.set("random", rquickjs::Undefined);
                }
                Ok::<(), String>(())
            })
        }));
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(SandboxError::ScriptError(e)),
            Err(_) => Err(SandboxError::PanicCaptured(
                "panic while initialising sandbox globals".to_string(),
            )),
        }
    }

    fn reset_for_new_document(&mut self) -> Result<(), SandboxError> {
        self.metadata = RuntimeMetadata::default();
        self.clear_deadline();
        // Memory limit is per-document; re-set to clear any prior accounting.
        self.runtime.set_memory_limit(self.memory_budget_bytes);
        Ok(())
    }

    fn execute_script(
        &mut self,
        activity: Option<&str>,
        body: &str,
    ) -> Result<RuntimeOutcome, SandboxError> {
        if !activity_allowed_for_sandbox(activity) {
            return Err(SandboxError::PhaseDenied(
                activity.unwrap_or("None").to_string(),
            ));
        }
        if body.len() > MAX_SCRIPT_BODY_BYTES {
            self.metadata.runtime_errors = self.metadata.runtime_errors.saturating_add(1);
            return Err(SandboxError::BodyTooLarge);
        }

        self.set_deadline();
        let script_owned = body.to_string();
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.context.with(|ctx| -> Result<(), rquickjs::Error> {
                ctx.eval::<(), _>(script_owned.into_bytes())?;
                Ok(())
            })
        }));
        // Capture deadline state BEFORE clearing so the error-classification
        // branch below can distinguish a timeout from a genuine ScriptError.
        let captured_deadline = self.script_deadline.load(Ordering::Acquire);
        let captured_now = Instant::now()
            .checked_duration_since(epoch())
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let timed_out = captured_deadline != 0 && captured_now >= captured_deadline;
        self.clear_deadline();

        match result {
            Ok(Ok(())) => {
                self.metadata.executed = self.metadata.executed.saturating_add(1);
                Ok(RuntimeOutcome {
                    executed: true,
                    mutated_field_count: 0,
                })
            }
            Ok(Err(other)) => {
                // rquickjs ≤ 0.8 collapses interrupts, OOM, and thrown
                // exceptions into a small set of Error variants. We
                // distinguish a Timeout via the deadline snapshot captured
                // before clear_deadline() above; OOM via a substring scan of
                // the error message; everything else is ScriptError.
                if timed_out {
                    self.metadata.timeouts = self.metadata.timeouts.saturating_add(1);
                    Err(SandboxError::Timeout)
                } else {
                    let msg = other.to_string();
                    if msg.to_ascii_lowercase().contains("memory") {
                        self.metadata.oom = self.metadata.oom.saturating_add(1);
                        Err(SandboxError::OutOfMemory)
                    } else {
                        self.metadata.runtime_errors =
                            self.metadata.runtime_errors.saturating_add(1);
                        Err(SandboxError::ScriptError(msg))
                    }
                }
            }
            Err(_) => {
                self.metadata.runtime_errors = self.metadata.runtime_errors.saturating_add(1);
                Err(SandboxError::PanicCaptured(
                    "panic during sandboxed script execution".to_string(),
                ))
            }
        }
    }

    fn take_metadata(&mut self) -> RuntimeMetadata {
        std::mem::take(&mut self.metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_runtime() -> QuickJsRuntime {
        let mut rt = QuickJsRuntime::new().expect("rquickjs init");
        rt.init().expect("init");
        rt.reset_for_new_document().expect("reset");
        rt
    }

    #[test]
    fn harmless_calculate_script_executes() {
        let mut rt = fresh_runtime();
        let outcome = rt
            .execute_script(Some("calculate"), "var x = 1 + 1; x")
            .expect("ok");
        assert!(outcome.executed);
        let md = rt.take_metadata();
        assert_eq!(md.executed, 1);
        assert!(md.is_clean());
    }

    #[test]
    fn ui_activity_is_phase_denied() {
        let mut rt = fresh_runtime();
        let err = rt.execute_script(Some("click"), "1+1").unwrap_err();
        assert!(matches!(err, SandboxError::PhaseDenied(_)));
    }

    #[test]
    fn oversized_body_rejected_before_parse() {
        let mut rt = fresh_runtime();
        let body = "1;\n".repeat(MAX_SCRIPT_BODY_BYTES);
        let err = rt.execute_script(Some("calculate"), &body).unwrap_err();
        assert_eq!(err, SandboxError::BodyTooLarge);
    }

    #[test]
    fn fetch_is_undefined() {
        let mut rt = fresh_runtime();
        // Reading `typeof fetch` from a fresh context should return
        // "undefined" because we never register it. Surface as a thrown
        // error if it isn't, by using `if (typeof fetch !== 'undefined') throw 0`.
        rt.execute_script(
            Some("calculate"),
            "if (typeof fetch !== 'undefined') throw new Error('fetch leaked');",
        )
        .expect("must run cleanly with fetch undefined");
    }

    #[test]
    fn require_is_undefined() {
        let mut rt = fresh_runtime();
        rt.execute_script(
            Some("calculate"),
            "if (typeof require !== 'undefined') throw new Error('require leaked');",
        )
        .expect("must run cleanly with require undefined");
    }

    #[test]
    fn process_is_undefined() {
        let mut rt = fresh_runtime();
        rt.execute_script(
            Some("calculate"),
            "if (typeof process !== 'undefined') throw new Error('process leaked');",
        )
        .expect("must run cleanly with process undefined");
    }

    #[test]
    fn date_now_is_zero() {
        let mut rt = fresh_runtime();
        rt.execute_script(
            Some("calculate"),
            "if (Date.now() !== 0) throw new Error('Date.now not stubbed');",
        )
        .expect("Date.now must return 0");
    }

    #[test]
    fn math_random_is_undefined() {
        let mut rt = fresh_runtime();
        rt.execute_script(
            Some("calculate"),
            "if (typeof Math.random !== 'undefined') throw new Error('Math.random leaked');",
        )
        .expect("Math.random must be undefined");
    }

    #[test]
    fn infinite_loop_times_out() {
        let mut rt = QuickJsRuntime::new()
            .expect("init")
            .with_time_budget(Duration::from_millis(50));
        rt.init().unwrap();
        rt.reset_for_new_document().unwrap();
        let err = rt
            .execute_script(Some("calculate"), "while(true){}")
            .unwrap_err();
        assert_eq!(err, SandboxError::Timeout);
        let md = rt.take_metadata();
        assert_eq!(md.timeouts, 1);
        assert_eq!(md.executed, 0);
    }

    #[test]
    fn syntax_error_is_recoverable() {
        let mut rt = fresh_runtime();
        let err = rt
            .execute_script(Some("calculate"), "this is not javascript {{")
            .unwrap_err();
        assert!(matches!(err, SandboxError::ScriptError(_)));
        // Subsequent script should still run.
        rt.execute_script(Some("calculate"), "var ok = 1;")
            .expect("recovered");
    }
}
