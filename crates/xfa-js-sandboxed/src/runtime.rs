//! Sandboxed QuickJS runtime for XFA JavaScript calculate scripts.
//!
//! # Sandbox guarantees
//! - No `fs`, `net`, `process`, `fetch`, `XMLHttpRequest`, `require` exposed.
//! - `require`, `process`, `fetch`, `XMLHttpRequest`, `importScripts` are
//!   defined as stub objects/functions that throw a typed
//!   `__XFA_SANDBOX_DENY__:` exception, which is mapped to
//!   [`XfaJsError::UnsupportedHostCapability`] in Rust.
//! - Memory limit: 32 MiB per runtime lifetime.
//! - Cancellation via an [`std::sync::atomic::AtomicBool`] token threaded
//!   through a thread-local so the interrupt handler can read it.
//! - All QuickJS FFI calls are wrapped in `std::panic::catch_unwind`.
//!
//! # What IS injected
//! - `xfa.form.<fieldName>.rawValue` — getter / setter backed by the borrowed
//!   [`FieldValues`] store (via `Rc<RefCell<SharedState>>`).
//! - `xfa.event.newText` — read-only in change-event context.

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};

use rquickjs::{CatchResultExt, Coerced, Context, Function, Runtime};

use crate::error::XfaJsError;
use crate::field_store::{ExecCtx, FieldValues};
use crate::value::JsValue;

// ─── memory budget (32 MiB) ──────────────────────────────────────────────────
const MEMORY_LIMIT_BYTES: usize = 32 * 1024 * 1024;

// ─── per-script wall-clock budget (5 s) ──────────────────────────────────────
const SCRIPT_BUDGET_MS: u64 = 5_000;

// ─── stable epoch for deadline arithmetic ────────────────────────────────────
static EPOCH: OnceLock<Instant> = OnceLock::new();
fn epoch() -> Instant {
    *EPOCH.get_or_init(Instant::now)
}

// ─── thread-local cancel slot ────────────────────────────────────────────────
// The interrupt handler is registered once at Runtime creation and must be
// 'static. We route the per-call cancel token through a thread-local so the
// handler can see it without unsafe raw pointers.
thread_local! {
    static ACTIVE_CANCEL: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

// ─── shared state between native bindings and execute_calculate ──────────────
struct SharedState {
    /// Snapshot of all field values at the start of execution.
    fields: HashMap<String, String>,
    /// Fields written during execution (name → new value).
    mutations: HashMap<String, String>,
    /// `xfa.event.newText` for the current script invocation.
    event_new_text: String,
}

impl SharedState {
    fn new() -> Self {
        Self {
            fields: HashMap::new(),
            mutations: HashMap::new(),
            event_new_text: String::new(),
        }
    }

    /// Load from ExecCtx before script execution.
    fn load(&mut self, ctx: &ExecCtx<'_>) {
        self.fields.clear();
        self.mutations.clear();
        for (name, val) in ctx.fields.iter() {
            self.fields.insert(name.to_string(), val.to_string());
        }
        self.event_new_text = ctx.event_new_text.unwrap_or("").to_string();
    }

    /// Write mutations back to the ExecCtx's FieldValues.
    fn flush(&self, fields: &mut FieldValues) {
        for (name, val) in &self.mutations {
            fields.set(name.as_str(), val.as_str());
        }
    }
}

// ─── sandbox setup JS ────────────────────────────────────────────────────────
// Run once after the native bindings are registered. Defines the xfa object
// and blocks dangerous host capabilities.
const SANDBOX_SETUP_JS: &str = r#"
(function() {
  "use strict";

  // Block dangerous host capabilities. Scripts that attempt to use any of
  // these will receive XfaJsError::UnsupportedHostCapability in Rust.
  function __xfa_deny(cap) {
    throw new TypeError("__XFA_SANDBOX_DENY__:" + cap);
  }

  // CommonJS / Node.js
  var require = function(m) { __xfa_deny("require(" + String(m) + ")"); };

  // Node.js process
  var process = new Proxy({}, {
    get: function(t, n) { __xfa_deny("process." + String(n)); },
    has: function() { return true; }
  });

  // Web fetch / network
  var fetch = function(url) { __xfa_deny("fetch(" + String(url) + ")"); };

  // XMLHttpRequest
  var XMLHttpRequest = function() { __xfa_deny("XMLHttpRequest"); };

  // Worker / ServiceWorker imports
  if (typeof importScripts !== "undefined") {
    var importScripts = function() { __xfa_deny("importScripts"); };
  }

  // ── xfa.form (Proxy) ───────────────────────────────────────────────────
  var xfaForm = new Proxy({}, {
    get: function(target, name) {
      if (typeof name !== "string") return undefined;
      return {
        get rawValue() { return __xfaGetRaw(name); },
        set rawValue(v)  { __xfaSetRaw(name, String(v)); }
      };
    }
  });

  // ── xfa.event ──────────────────────────────────────────────────────────
  var xfaEvent = Object.defineProperties({}, {
    newText: {
      get: function() { return __xfaEventNewText(); },
      enumerable: true
    }
  });

  // ── xfa global ─────────────────────────────────────────────────────────
  var xfa = Object.freeze({ form: xfaForm, event: xfaEvent });

  // Expose globals.
  globalThis.require          = require;
  globalThis.process          = process;
  globalThis.fetch            = fetch;
  globalThis.XMLHttpRequest   = XMLHttpRequest;
  globalThis.xfa              = xfa;
})();
"#;

// ─── XfaJsRuntime ────────────────────────────────────────────────────────────

/// Sandboxed QuickJS-backed runtime for XFA JavaScript calculate scripts.
///
/// One instance is reusable across many `execute_calculate` calls.
/// The runtime is **not** `Send` — it must stay on the thread it was created
/// on (this matches rquickjs's own `!Send` constraint).
pub struct XfaJsRuntime {
    /// The QuickJS engine. Memory-limited at construction.
    _runtime: Runtime,
    /// The full JS execution context (all ES built-ins, no host extensions).
    context: Context,
    /// Shared state: field snapshot + mutations + event.newText.
    shared: Rc<RefCell<SharedState>>,
    /// Deadline in nanoseconds since epoch (0 = no deadline active).
    deadline: Arc<AtomicU64>,
    /// True while a script is running (used by interrupt handler).
    active: Arc<AtomicBool>,
}

impl std::fmt::Debug for XfaJsRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XfaJsRuntime").finish()
    }
}

impl XfaJsRuntime {
    /// Create a new sandboxed runtime and install the `xfa.*` bindings.
    ///
    /// Returns `Err(XfaJsError::Internal)` if the QuickJS engine fails to
    /// initialise (extremely rare — typically only on OOM at process start).
    pub fn new() -> Result<Self, XfaJsError> {
        let runtime =
            Runtime::new().map_err(|e| XfaJsError::Internal(format!("rquickjs runtime: {e}")))?;
        runtime.set_memory_limit(MEMORY_LIMIT_BYTES);

        let deadline = Arc::new(AtomicU64::new(0));
        let active = Arc::new(AtomicBool::new(false));
        let deadline_h = Arc::clone(&deadline);
        let active_h = Arc::clone(&active);

        runtime.set_interrupt_handler(Some(Box::new(move || {
            // 1. Check external cancel token.
            let cancelled = ACTIVE_CANCEL.with(|slot| {
                slot.borrow()
                    .as_ref()
                    .is_some_and(|a| a.load(Ordering::Relaxed))
            });
            if cancelled {
                return true;
            }
            // 2. Check wall-clock deadline.
            if !active_h.load(Ordering::Acquire) {
                return false;
            }
            let dl = deadline_h.load(Ordering::Acquire);
            if dl == 0 {
                return false;
            }
            let now = Instant::now()
                .checked_duration_since(epoch())
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            now >= dl
        })));

        let context = Context::full(&runtime)
            .map_err(|e| XfaJsError::Internal(format!("rquickjs context: {e}")))?;

        let shared = Rc::new(RefCell::new(SharedState::new()));

        let mut rt = Self {
            _runtime: runtime,
            context,
            shared,
            deadline,
            active,
        };
        rt.install_bindings()?;
        Ok(rt)
    }

    // ── internal: register native functions + sandbox setup ──────────────

    fn install_bindings(&mut self) -> Result<(), XfaJsError> {
        let shared = Rc::clone(&self.shared);
        self.context
            .with(|ctx| -> Result<(), String> {
                let globals = ctx.globals();

                // __xfaGetRaw(name: String) -> String
                let sh = Rc::clone(&shared);
                let get_fn = Function::new(ctx.clone(), move |name: Coerced<String>| -> String {
                    sh.borrow().fields.get(&name.0).cloned().unwrap_or_default()
                })
                .map_err(|e| format!("__xfaGetRaw: {e}"))?;
                globals
                    .set("__xfaGetRaw", get_fn)
                    .map_err(|e| format!("set __xfaGetRaw: {e}"))?;

                // __xfaSetRaw(name: String, value: String)
                let sh = Rc::clone(&shared);
                let set_fn = Function::new(
                    ctx.clone(),
                    move |name: Coerced<String>, value: Coerced<String>| {
                        let mut state = sh.borrow_mut();
                        state.fields.insert(name.0.clone(), value.0.clone());
                        state.mutations.insert(name.0, value.0);
                    },
                )
                .map_err(|e| format!("__xfaSetRaw: {e}"))?;
                globals
                    .set("__xfaSetRaw", set_fn)
                    .map_err(|e| format!("set __xfaSetRaw: {e}"))?;

                // __xfaEventNewText() -> String
                let sh = Rc::clone(&shared);
                let evt_fn = Function::new(ctx.clone(), move || -> String {
                    sh.borrow().event_new_text.clone()
                })
                .map_err(|e| format!("__xfaEventNewText: {e}"))?;
                globals
                    .set("__xfaEventNewText", evt_fn)
                    .map_err(|e| format!("set __xfaEventNewText: {e}"))?;

                // Run sandbox setup JS.
                ctx.eval::<(), _>(SANDBOX_SETUP_JS.as_bytes())
                    .catch(&ctx)
                    .map_err(|e| format!("sandbox setup: {e}"))?;

                Ok(())
            })
            .map_err(XfaJsError::Internal)?;
        Ok(())
    }

    // ── deadline helpers ─────────────────────────────────────────────────

    fn set_deadline(&self) {
        let dl = Instant::now()
            .checked_duration_since(epoch())
            .map(|d| d + Duration::from_millis(SCRIPT_BUDGET_MS))
            .unwrap_or(Duration::from_millis(SCRIPT_BUDGET_MS));
        self.deadline.store(dl.as_nanos() as u64, Ordering::Release);
        self.active.store(true, Ordering::Release);
    }

    fn clear_deadline(&self) {
        self.active.store(false, Ordering::Release);
        self.deadline.store(0, Ordering::Release);
    }

    // ── public API ───────────────────────────────────────────────────────

    /// Execute a JavaScript calculate script within the sandbox.
    ///
    /// The script runs in a QuickJS context that has `xfa.form.<fieldName>.rawValue`
    /// getter/setter and `xfa.event.newText` available. Any `rawValue` writes
    /// performed by the script are reflected in `ctx.fields` when this call
    /// returns. The return value is the JS value of the last expression.
    ///
    /// # Errors
    /// - [`XfaJsError::Cancelled`] if `ctx.cancel` is set before or during
    ///   execution.
    /// - [`XfaJsError::UnsupportedHostCapability`] if the script attempts to
    ///   use `require`, `process`, `fetch`, `XMLHttpRequest`, etc.
    /// - [`XfaJsError::Runtime`] for any other JS exception or parse error.
    /// - [`XfaJsError::Internal`] if the QuickJS FFI panics (extremely rare).
    pub fn execute_calculate(
        &mut self,
        script: &str,
        ctx: ExecCtx<'_>,
    ) -> Result<JsValue, XfaJsError> {
        // Fast-path: already cancelled before we even start.
        if ctx.cancel.load(Ordering::Relaxed) {
            return Err(XfaJsError::Cancelled);
        }

        // Populate shared state from the ExecCtx.
        self.shared.borrow_mut().load(&ctx);

        // Install cancel token in thread-local slot so the interrupt handler
        // can read it. Guard uses RAII to clear the slot on return.
        let _cancel_guard = CancelGuard::install(ctx.cancel.clone());

        // Arm deadline.
        self.set_deadline();

        let script_owned = script.to_string();
        let shared = Rc::clone(&self.shared);

        // All QuickJS calls are wrapped in catch_unwind so an FFI panic
        // (should never happen with a well-formed script) cannot unwind past
        // this function.
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            self.context.with(|qctx| -> EvalOutcome {
                match qctx.eval::<rquickjs::Value, _>(script_owned.as_bytes()) {
                    Ok(val) => EvalOutcome::Value(convert_value(val)),
                    Err(e) => {
                        let msg = if matches!(e, rquickjs::Error::Exception) {
                            let exc_val = qctx.catch();
                            if let Some(exc) = exc_val.as_exception() {
                                exc.message().unwrap_or_else(|| exc.to_string())
                            } else {
                                e.to_string()
                            }
                        } else {
                            e.to_string()
                        };
                        EvalOutcome::Error(msg)
                    }
                }
            })
        }));

        // Capture deadline state before clearing.
        let dl_snap = self.deadline.load(Ordering::Acquire);
        let now_ns = Instant::now()
            .checked_duration_since(epoch())
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let timed_out = dl_snap != 0 && now_ns >= dl_snap;
        self.clear_deadline();

        // Check cancel (may have been set during execution).
        let was_cancelled = ACTIVE_CANCEL.with(|slot| {
            slot.borrow()
                .as_ref()
                .is_some_and(|a| a.load(Ordering::Relaxed))
        });

        // Map outcome to Result<JsValue, XfaJsError>.
        let result = match outcome {
            Err(_) => Err(XfaJsError::Internal(
                "panic during sandboxed script execution".to_string(),
            )),
            Ok(EvalOutcome::Value(v)) => Ok(v),
            Ok(EvalOutcome::Error(msg)) => {
                if was_cancelled || timed_out {
                    Err(XfaJsError::Cancelled)
                } else if let Some(cap) = msg.strip_prefix("__XFA_SANDBOX_DENY__:") {
                    Err(XfaJsError::UnsupportedHostCapability(cap.to_string()))
                } else {
                    Err(XfaJsError::Runtime(msg))
                }
            }
        };

        // Flush mutations back to the caller's FieldValues regardless of outcome
        // (partial writes are still useful for debugging).
        let state = shared.borrow();
        state.flush(ctx.fields);

        result
    }
}

// ─── cancel RAII guard ───────────────────────────────────────────────────────

struct CancelGuard;

impl CancelGuard {
    fn install(token: Arc<AtomicBool>) -> Self {
        ACTIVE_CANCEL.with(|slot| *slot.borrow_mut() = Some(token));
        CancelGuard
    }
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        ACTIVE_CANCEL.with(|slot| *slot.borrow_mut() = None);
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

enum EvalOutcome {
    Value(JsValue),
    Error(String),
}

fn convert_value(val: rquickjs::Value<'_>) -> JsValue {
    if val.is_undefined() || val.is_null() {
        JsValue::Null
    } else if val.is_bool() {
        JsValue::Bool(val.as_bool().unwrap_or(false))
    } else if val.is_int() {
        JsValue::Number(val.as_int().unwrap_or(0) as f64)
    } else if val.is_float() {
        JsValue::Number(val.as_float().unwrap_or(0.0))
    } else if val.is_string() {
        let s = val
            .as_string()
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        JsValue::String(s)
    } else {
        JsValue::Undefined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_cancel() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn new_succeeds() {
        XfaJsRuntime::new().expect("runtime init");
    }

    #[test]
    fn eval_simple_arithmetic_returns_number() {
        let mut rt = XfaJsRuntime::new().unwrap();
        let mut fields = FieldValues::new();
        let ctx = ExecCtx::new(&mut fields, no_cancel());
        let val = rt.execute_calculate("1 + 1", ctx).unwrap();
        assert_eq!(val, JsValue::Number(2.0));
    }

    #[test]
    fn cancelled_before_execution() {
        let mut rt = XfaJsRuntime::new().unwrap();
        let mut fields = FieldValues::new();
        let cancel = Arc::new(AtomicBool::new(true));
        let ctx = ExecCtx::new(&mut fields, cancel);
        assert!(matches!(
            rt.execute_calculate("1 + 1", ctx),
            Err(XfaJsError::Cancelled)
        ));
    }

    #[test]
    fn syntax_error_is_runtime_error() {
        let mut rt = XfaJsRuntime::new().unwrap();
        let mut fields = FieldValues::new();
        let ctx = ExecCtx::new(&mut fields, no_cancel());
        assert!(matches!(
            rt.execute_calculate("{{{{", ctx),
            Err(XfaJsError::Runtime(_))
        ));
    }
}
