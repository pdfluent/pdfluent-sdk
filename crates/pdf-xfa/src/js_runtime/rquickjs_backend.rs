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

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};

use rquickjs::function::Opt;
use rquickjs::{CatchResultExt, Coerced, Context, Function, Object, Persistent, Runtime};
use xfa_layout_engine::form::{FormNodeId, FormTree};

use super::regex_guard::{scan_script_for_redos, RegexScanVerdict};
use super::{
    activity_allowed_for_sandbox_with_gate, walltime_fallback_multiplier, HostBindings,
    RuntimeMetadata, RuntimeOutcome, SandboxError, XfaJsRuntime, DEFAULT_MEMORY_BUDGET_BYTES,
    DEFAULT_TIME_BUDGET_MS, MAX_SCRIPT_BODY_BYTES, MAX_VARIABLES_SCRIPT_BODY_BYTES,
};

/// QuickJS-backed runtime adapter. One instance is reusable across many
/// documents; callers MUST invoke [`XfaJsRuntime::reset_for_new_document`]
/// at the start of each flatten.
pub struct QuickJsRuntime {
    eval_script: Option<Persistent<Function<'static>>>,
    /// Phase D-ι: hook that registers a `<variables>` `<script>` body as a
    /// form-level global. Called once per named script at document load.
    set_variables_script: Option<Persistent<Function<'static>>>,
    /// Phase D-ι: clears all registered variables-script globals at
    /// document boundary (`reset_per_document`).
    clear_variables_scripts: Option<Persistent<Function<'static>>>,
    /// W3-D RETRY: hook that registers a `<variables>` `<text>` data item
    /// (XFA 3.3 §5.5.2) as a form-level mutable string container. Called
    /// once per named data item at document load. Sibling to
    /// [`Self::set_variables_script`] — the `clear_variables_scripts` hook
    /// also drops the data-item namespace.
    set_variables_data_item: Option<Persistent<Function<'static>>>,
    context: Context,
    runtime: Runtime,
    metadata: RuntimeMetadata,
    time_budget: Duration,
    memory_budget_bytes: usize,
    script_deadline: Arc<AtomicU64>,
    script_started: Arc<AtomicBool>,
    /// QF1-E / SEC-01 — start-time of the currently executing script in
    /// nanoseconds since [`epoch`]. Set by [`Self::set_deadline`] before
    /// QuickJS is invoked and read by the post-execution classification
    /// branch to compute elapsed wall-clock and decide whether to surface
    /// [`SandboxError::WallTimeExceeded`] instead of
    /// [`SandboxError::Timeout`]. Cleared back to 0 by
    /// [`Self::clear_deadline`].
    script_start_nanos: Arc<AtomicU64>,
    /// QF1-E / SEC-01 — multiplier on `time_budget` used by the wall-time
    /// fallback. Snapshotted at construction time from
    /// [`super::walltime_fallback_multiplier`] so tests can toggle the env
    /// var per-instance without racing other tests.
    walltime_fallback_multiplier: u32,
    host: Rc<RefCell<HostBindings>>,
    bindings_registered: bool,
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

fn parse_node_id_csv(raw: &str) -> Vec<FormNodeId> {
    raw.split(',')
        .filter_map(|part| part.trim().parse::<usize>().ok())
        .map(FormNodeId)
        .collect()
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
        // QF1-E: independent of the deadline atomic — start-time is needed
        // for the post-execution elapsed comparison even when the deadline
        // has been cleared.
        let script_start_nanos = Arc::new(AtomicU64::new(0));

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
            eval_script: None,
            set_variables_script: None,
            clear_variables_scripts: None,
            set_variables_data_item: None,
            context,
            runtime,
            metadata: RuntimeMetadata::default(),
            time_budget: Duration::from_millis(DEFAULT_TIME_BUDGET_MS),
            memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
            script_deadline,
            script_started,
            script_start_nanos,
            walltime_fallback_multiplier: walltime_fallback_multiplier(),
            host: Rc::new(RefCell::new(HostBindings::new())),
            bindings_registered: false,
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

    /// QF1-E / SEC-01 — override the wall-time fallback multiplier. The
    /// default is read from
    /// [`super::ENV_WALLTIME_FALLBACK_MULTIPLIER`] / falls back to
    /// [`super::WALLTIME_FALLBACK_MULTIPLIER_DEFAULT`]. Values below `2`
    /// are clamped to `2` so a clean timeout cannot be re-labelled as
    /// `WallTimeExceeded` by accident. Tests use this to deterministically
    /// pin the multiplier without racing the process-global env var.
    pub fn with_walltime_fallback_multiplier(mut self, multiplier: u32) -> Self {
        self.walltime_fallback_multiplier = multiplier.max(2);
        self
    }

    fn set_deadline(&self) {
        let now = Instant::now()
            .checked_duration_since(epoch())
            .unwrap_or(Duration::ZERO);
        let deadline = now + self.time_budget;
        self.script_deadline
            .store(deadline.as_nanos() as u64, Ordering::Release);
        // QF1-E: capture start-time before flagging the script as started so
        // the post-execution branch can compute elapsed wall-clock even if
        // `clear_deadline` runs first.
        self.script_start_nanos
            .store(now.as_nanos() as u64, Ordering::Release);
        self.script_started.store(true, Ordering::Release);
    }

    fn clear_deadline(&self) {
        self.script_started.store(false, Ordering::Release);
        self.script_deadline.store(0, Ordering::Release);
        // Intentionally keep `script_start_nanos` populated until the next
        // `set_deadline` so the caller's elapsed-snapshot is stable.
    }

    /// QF1-E / SEC-01 — classify a timeout-shaped error as either the
    /// primary [`SandboxError::Timeout`] (interrupt fired within multiplier
    /// × budget) or the defence-in-depth [`SandboxError::WallTimeExceeded`]
    /// (elapsed ≥ multiplier × budget at error-classification time).
    ///
    /// This is intentionally a no-op in the common case: the rquickjs
    /// interrupt callback fires within a few opcodes of the deadline for
    /// any reasonable JS source, so the elapsed time at this point is
    /// ≈ `time_budget`, well below `multiplier × time_budget`. The
    /// fallback variant only surfaces when execution was trapped inside a
    /// non-interruptible C-level call long enough that the elapsed
    /// wall-clock crossed the multiplier threshold.
    ///
    /// `now_nanos` is the wall-clock at the moment the post-execution
    /// branch captured the deadline; passed in so the caller can reuse a
    /// single `Instant::now()` snapshot for both checks.
    fn classify_timeout_or_walltime(&self, now_nanos: u64) -> SandboxError {
        let start = self.script_start_nanos.load(Ordering::Acquire);
        if start == 0 || now_nanos <= start {
            // Defensive: if the start was never set or the snapshot went
            // backwards (impossible barring monotonic clock issues), keep
            // the primary classification.
            return SandboxError::Timeout;
        }
        let elapsed_nanos = now_nanos - start;
        let threshold_nanos = (self.time_budget.as_nanos() as u64)
            .saturating_mul(self.walltime_fallback_multiplier as u64);
        if elapsed_nanos >= threshold_nanos {
            let elapsed_ms = elapsed_nanos / 1_000_000;
            let budget_ms = self.time_budget.as_millis();
            SandboxError::WallTimeExceeded(format!(
                "{elapsed_ms} ms ≥ {mult}× budget ({budget_ms} ms)",
                mult = self.walltime_fallback_multiplier
            ))
        } else {
            SandboxError::Timeout
        }
    }

    fn register_host_bindings(&mut self) -> Result<(), String> {
        if self.bindings_registered {
            return Ok(());
        }

        let host = Rc::clone(&self.host);
        let eval_script = self.context.with(|ctx| {
            let globals = ctx.globals();
            let internal =
                Object::new(ctx.clone()).map_err(|e| format!("host internal object: {e}"))?;

            let resolve_host = Rc::clone(&host);
            let resolve_node_id = Function::new(ctx.clone(), move |path: Opt<Coerced<String>>| {
                let Some(path) = path.0 else {
                    let _ = resolve_host.borrow_mut().resolve_node("");
                    return -1i32;
                };
                resolve_host
                    .borrow_mut()
                    .resolve_node(&path.0)
                    .map(|node_id| node_id.0 as i32)
                    .unwrap_or(-1)
            })
            .map_err(|e| format!("resolveNodeId: {e}"))?;
            internal
                .set("resolveNodeId", resolve_node_id)
                .map_err(|e| format!("set resolveNodeId: {e}"))?;

            let resolve_nodes_host = Rc::clone(&host);
            let resolve_node_ids =
                Function::new(ctx.clone(), move |path: Opt<Coerced<String>>| -> Vec<i32> {
                    let Some(path) = path.0 else {
                        let _ = resolve_nodes_host.borrow_mut().resolve_nodes("");
                        return Vec::new();
                    };
                    resolve_nodes_host
                        .borrow_mut()
                        .resolve_nodes(&path.0)
                        .into_iter()
                        .map(|node_id| node_id.0 as i32)
                        .collect()
                })
                .map_err(|e| format!("resolveNodeIds: {e}"))?;
            internal
                .set("resolveNodeIds", resolve_node_ids)
                .map_err(|e| format!("set resolveNodeIds: {e}"))?;

            let generation_host = Rc::clone(&host);
            let generation = Function::new(ctx.clone(), move || {
                generation_host.borrow().generation() as i64
            })
            .map_err(|e| format!("generation: {e}"))?;
            internal
                .set("generation", generation)
                .map_err(|e| format!("set generation: {e}"))?;

            let current_host = Rc::clone(&host);
            let current_node = Function::new(ctx.clone(), move || {
                current_host
                    .borrow()
                    .current_node()
                    .map(|node_id| node_id.0 as i32)
                    .unwrap_or(-1)
            })
            .map_err(|e| format!("currentNodeId: {e}"))?;
            internal
                .set("currentNodeId", current_node)
                .map_err(|e| format!("set currentNodeId: {e}"))?;

            let implicit_host = Rc::clone(&host);
            let resolve_implicit_node_id = Function::new(
                ctx.clone(),
                move |current_id: i32, name: Opt<Coerced<String>>| -> i32 {
                    if current_id < 0 {
                        return -1;
                    }
                    let Some(name) = name.0 else {
                        return -1;
                    };
                    implicit_host
                        .borrow_mut()
                        .resolve_implicit(FormNodeId(current_id as usize), &name.0)
                        .map(|node_id| node_id.0 as i32)
                        .unwrap_or(-1)
                },
            )
            .map_err(|e| format!("resolveImplicitNodeId: {e}"))?;
            internal
                .set("resolveImplicitNodeId", resolve_implicit_node_id)
                .map_err(|e| format!("set resolveImplicitNodeId: {e}"))?;

            let implicit_candidates_host = Rc::clone(&host);
            let resolve_implicit_node_ids = Function::new(
                ctx.clone(),
                move |current_id: i32, name: Opt<Coerced<String>>| -> Vec<i32> {
                    if current_id < 0 {
                        return Vec::new();
                    }
                    let Some(name) = name.0 else {
                        return Vec::new();
                    };
                    implicit_candidates_host
                        .borrow_mut()
                        .resolve_implicit_candidates(FormNodeId(current_id as usize), &name.0)
                        .into_iter()
                        .map(|node_id| node_id.0 as i32)
                        .collect()
                },
            )
            .map_err(|e| format!("resolveImplicitNodeIds: {e}"))?;
            internal
                .set("resolveImplicitNodeIds", resolve_implicit_node_ids)
                .map_err(|e| format!("set resolveImplicitNodeIds: {e}"))?;

            // XFA-DATA-M3C: quiet probe for implicit identifier resolution.
            // Same scope walk as `resolveImplicitNodeIds`; a miss returns an
            // empty array without bumping `resolve_failures`. The JS proxy
            // uses this when probing schema-optional underscore-shorthand
            // globals (`_<Name>`).
            let implicit_quiet_host = Rc::clone(&host);
            let resolve_implicit_node_ids_quiet = Function::new(
                ctx.clone(),
                move |current_id: i32, name: Opt<Coerced<String>>| -> Vec<i32> {
                    if current_id < 0 {
                        return Vec::new();
                    }
                    let Some(name) = name.0 else {
                        return Vec::new();
                    };
                    implicit_quiet_host
                        .borrow_mut()
                        .resolve_implicit_candidates_quiet(FormNodeId(current_id as usize), &name.0)
                        .into_iter()
                        .map(|node_id| node_id.0 as i32)
                        .collect()
                },
            )
            .map_err(|e| format!("resolveImplicitNodeIdsQuiet: {e}"))?;
            internal
                .set(
                    "resolveImplicitNodeIdsQuiet",
                    resolve_implicit_node_ids_quiet,
                )
                .map_err(|e| format!("set resolveImplicitNodeIdsQuiet: {e}"))?;

            let child_host = Rc::clone(&host);
            let resolve_child_node_id = Function::new(
                ctx.clone(),
                move |parent_id: i32, name: Opt<Coerced<String>>| {
                    if parent_id < 0 {
                        return -1i32;
                    }
                    let Some(name) = name.0 else {
                        return -1;
                    };
                    child_host
                        .borrow_mut()
                        .resolve_child(FormNodeId(parent_id as usize), &name.0)
                        .map(|node_id| node_id.0 as i32)
                        .unwrap_or(-1)
                },
            )
            .map_err(|e| format!("resolveChildNodeId: {e}"))?;
            internal
                .set("resolveChildNodeId", resolve_child_node_id)
                .map_err(|e| format!("set resolveChildNodeId: {e}"))?;

            let child_candidates_host = Rc::clone(&host);
            let resolve_child_node_ids = Function::new(
                ctx.clone(),
                move |parent_ids: Opt<Coerced<String>>, name: Opt<Coerced<String>>| -> Vec<i32> {
                    let Some(parent_ids) = parent_ids.0 else {
                        return Vec::new();
                    };
                    let Some(name) = name.0 else {
                        return Vec::new();
                    };
                    child_candidates_host
                        .borrow_mut()
                        .resolve_child_candidates(&parse_node_id_csv(&parent_ids.0), &name.0)
                        .into_iter()
                        .map(|node_id| node_id.0 as i32)
                        .collect()
                },
            )
            .map_err(|e| format!("resolveChildNodeIds: {e}"))?;
            internal
                .set("resolveChildNodeIds", resolve_child_node_ids)
                .map_err(|e| format!("set resolveChildNodeIds: {e}"))?;

            // XFA-DATA-M3C: quiet variant of resolveChildNodeIds. Returns the
            // same ids on success and an empty array on miss, but does not
            // bump the `resolve_failures` metric — used by underscore-shorthand
            // probes where a miss is the expected schema-optional path and
            // would otherwise inflate the resolve-failure budget.
            let child_quiet_host = Rc::clone(&host);
            let resolve_child_node_ids_quiet = Function::new(
                ctx.clone(),
                move |parent_ids: Opt<Coerced<String>>, name: Opt<Coerced<String>>| -> Vec<i32> {
                    let Some(parent_ids) = parent_ids.0 else {
                        return Vec::new();
                    };
                    let Some(name) = name.0 else {
                        return Vec::new();
                    };
                    child_quiet_host
                        .borrow_mut()
                        .resolve_child_candidates_quiet(&parse_node_id_csv(&parent_ids.0), &name.0)
                        .into_iter()
                        .map(|node_id| node_id.0 as i32)
                        .collect()
                },
            )
            .map_err(|e| format!("resolveChildNodeIdsQuiet: {e}"))?;
            internal
                .set("resolveChildNodeIdsQuiet", resolve_child_node_ids_quiet)
                .map_err(|e| format!("set resolveChildNodeIdsQuiet: {e}"))?;

            // Phase D-θ: lookahead-hinted variants of the implicit and child
            // resolvers. The JS proxy carries the next-segment property name
            // and asks the host to keep only candidates whose subtree
            // contains a descendant matching that hint. Falls back to the
            // un-hinted candidate list when the hint disambiguates nothing,
            // so single-token reads remain byte-for-byte compatible.
            let implicit_hinted_host = Rc::clone(&host);
            let resolve_implicit_node_ids_hinted = Function::new(
                ctx.clone(),
                move |current_id: i32,
                      name: Opt<Coerced<String>>,
                      hint: Opt<Coerced<String>>|
                      -> Vec<i32> {
                    if current_id < 0 {
                        return Vec::new();
                    }
                    let Some(name) = name.0 else {
                        return Vec::new();
                    };
                    let hint = hint.0.map(|s| s.0).unwrap_or_default();
                    implicit_hinted_host
                        .borrow_mut()
                        .resolve_implicit_candidates_hinted(
                            FormNodeId(current_id as usize),
                            &name.0,
                            &hint,
                        )
                        .into_iter()
                        .map(|node_id| node_id.0 as i32)
                        .collect()
                },
            )
            .map_err(|e| format!("resolveImplicitNodeIdsHinted: {e}"))?;
            internal
                .set(
                    "resolveImplicitNodeIdsHinted",
                    resolve_implicit_node_ids_hinted,
                )
                .map_err(|e| format!("set resolveImplicitNodeIdsHinted: {e}"))?;

            let child_hinted_host = Rc::clone(&host);
            let resolve_child_node_ids_hinted = Function::new(
                ctx.clone(),
                move |parent_ids: Opt<Coerced<String>>,
                      name: Opt<Coerced<String>>,
                      hint: Opt<Coerced<String>>|
                      -> Vec<i32> {
                    let Some(parent_ids) = parent_ids.0 else {
                        return Vec::new();
                    };
                    let Some(name) = name.0 else {
                        return Vec::new();
                    };
                    let hint = hint.0.map(|s| s.0).unwrap_or_default();
                    child_hinted_host
                        .borrow_mut()
                        .resolve_child_candidates_hinted(
                            &parse_node_id_csv(&parent_ids.0),
                            &name.0,
                            &hint,
                        )
                        .into_iter()
                        .map(|node_id| node_id.0 as i32)
                        .collect()
                },
            )
            .map_err(|e| format!("resolveChildNodeIdsHinted: {e}"))?;
            internal
                .set("resolveChildNodeIdsHinted", resolve_child_node_ids_hinted)
                .map_err(|e| format!("set resolveChildNodeIdsHinted: {e}"))?;

            let scoped_candidates_host = Rc::clone(&host);
            let resolve_scoped_node_ids = Function::new(
                ctx.clone(),
                move |scope_ids: Opt<Coerced<String>>, name: Opt<Coerced<String>>| -> Vec<i32> {
                    let Some(scope_ids) = scope_ids.0 else {
                        return Vec::new();
                    };
                    let Some(name) = name.0 else {
                        return Vec::new();
                    };
                    scoped_candidates_host
                        .borrow_mut()
                        .resolve_scoped_candidates(&parse_node_id_csv(&scope_ids.0), &name.0)
                        .into_iter()
                        .map(|node_id| node_id.0 as i32)
                        .collect()
                },
            )
            .map_err(|e| format!("resolveScopedNodeIds: {e}"))?;
            internal
                .set("resolveScopedNodeIds", resolve_scoped_node_ids)
                .map_err(|e| format!("set resolveScopedNodeIds: {e}"))?;

            // Phase D-θ.2: full-chain SOM resolution with backtracking. The
            // JS proxy accumulates property names (the SOM chain) until a
            // terminal property is read, then asks the host to walk the
            // entire chain in one shot. `chain_csv` is a comma-separated
            // list of property names; commas are illegal in SOM names so the
            // delimiter is unambiguous. Passing a `current_id` of -1 + an
            // empty `parent_ids_csv` is invalid and yields an empty result;
            // callers must supply either explicit parents OR a current id.
            let full_chain_host = Rc::clone(&host);
            let resolve_with_full_chain = Function::new(
                ctx.clone(),
                move |parent_ids_csv: Opt<Coerced<String>>,
                      current_id: i32,
                      chain_csv: Opt<Coerced<String>>|
                      -> Vec<i32> {
                    let parents: Vec<FormNodeId> = parent_ids_csv
                        .0
                        .map(|s| parse_node_id_csv(&s.0))
                        .unwrap_or_default();
                    let chain: Vec<String> = chain_csv
                        .0
                        .map(|s| s.0.split(',').map(|part| part.trim().to_string()).collect())
                        .unwrap_or_default();
                    let implicit_origin = if current_id >= 0 {
                        Some(FormNodeId(current_id as usize))
                    } else {
                        None
                    };
                    full_chain_host
                        .borrow_mut()
                        .resolve_with_full_chain(&parents, &chain, implicit_origin)
                        .into_iter()
                        .map(|node_id| node_id.0 as i32)
                        .collect()
                },
            )
            .map_err(|e| format!("resolveWithFullChain: {e}"))?;
            internal
                .set("resolveWithFullChain", resolve_with_full_chain)
                .map_err(|e| format!("set resolveWithFullChain: {e}"))?;

            // Strict full-chain probe: returns empty when the chain cannot
            // be completed at full depth. Used by the JS chain proxy during
            // accumulation to surface `undefined` for unreachable paths
            // (so `A.B === undefined` keeps the byte-for-byte D-θ.1
            // behaviour for forms that test dead-end chains).
            let full_chain_strict_host = Rc::clone(&host);
            let resolve_with_full_chain_strict = Function::new(
                ctx.clone(),
                move |parent_ids_csv: Opt<Coerced<String>>,
                      current_id: i32,
                      chain_csv: Opt<Coerced<String>>|
                      -> Vec<i32> {
                    let parents: Vec<FormNodeId> = parent_ids_csv
                        .0
                        .map(|s| parse_node_id_csv(&s.0))
                        .unwrap_or_default();
                    let chain: Vec<String> = chain_csv
                        .0
                        .map(|s| s.0.split(',').map(|part| part.trim().to_string()).collect())
                        .unwrap_or_default();
                    let implicit_origin = if current_id >= 0 {
                        Some(FormNodeId(current_id as usize))
                    } else {
                        None
                    };
                    full_chain_strict_host
                        .borrow_mut()
                        .resolve_with_full_chain_strict(&parents, &chain, implicit_origin)
                        .into_iter()
                        .map(|node_id| node_id.0 as i32)
                        .collect()
                },
            )
            .map_err(|e| format!("resolveWithFullChainStrict: {e}"))?;
            internal
                .set("resolveWithFullChainStrict", resolve_with_full_chain_strict)
                .map_err(|e| format!("set resolveWithFullChainStrict: {e}"))?;

            let get_raw_host = Rc::clone(&host);
            let get_raw_value = Function::new(
                ctx.clone(),
                move |id: i32, generation: i64| -> Option<String> {
                    if id < 0 || generation < 0 {
                        return None;
                    }
                    get_raw_host
                        .borrow_mut()
                        .get_raw_value(FormNodeId(id as usize), generation as u64)
                },
            )
            .map_err(|e| format!("getRawValue: {e}"))?;
            internal
                .set("getRawValue", get_raw_value)
                .map_err(|e| format!("set getRawValue: {e}"))?;

            let set_raw_host = Rc::clone(&host);
            let set_raw_value = Function::new(
                ctx.clone(),
                move |id: i32, generation: i64, value: Coerced<String>| -> bool {
                    if id < 0 || generation < 0 {
                        return false;
                    }
                    set_raw_host.borrow_mut().set_raw_value(
                        FormNodeId(id as usize),
                        value.0,
                        generation as u64,
                    )
                },
            )
            .map_err(|e| format!("setRawValue: {e}"))?;
            internal
                .set("setRawValue", set_raw_value)
                .map_err(|e| format!("set setRawValue: {e}"))?;

            let instance_count_host = Rc::clone(&host);
            let instance_count =
                Function::new(ctx.clone(), move |id: i32, generation: i64| -> u32 {
                    if id < 0 || generation < 0 {
                        return 0;
                    }
                    instance_count_host
                        .borrow_mut()
                        .instance_count_for_handle(FormNodeId(id as usize), generation as u64)
                })
                .map_err(|e| format!("instanceCount: {e}"))?;
            internal
                .set("instanceCount", instance_count)
                .map_err(|e| format!("set instanceCount: {e}"))?;

            let zero_instance_host = Rc::clone(&host);
            let has_zero_instance_run = Function::new(
                ctx.clone(),
                move |id: i32, generation: i64, name: Opt<Coerced<String>>| -> bool {
                    if id < 0 || generation < 0 {
                        return false;
                    }
                    let Some(name) = name.0 else {
                        return false;
                    };
                    zero_instance_host.borrow_mut().has_zero_instance_run(
                        FormNodeId(id as usize),
                        generation as u64,
                        &name.0,
                    )
                },
            )
            .map_err(|e| format!("hasZeroInstanceRun: {e}"))?;
            internal
                .set("hasZeroInstanceRun", has_zero_instance_run)
                .map_err(|e| format!("set hasZeroInstanceRun: {e}"))?;

            let node_index_host = Rc::clone(&host);
            let node_index = Function::new(ctx.clone(), move |id: i32, generation: i64| -> u32 {
                if id < 0 || generation < 0 {
                    return 0;
                }
                node_index_host
                    .borrow_mut()
                    .instance_index_for_handle(FormNodeId(id as usize), generation as u64)
            })
            .map_err(|e| format!("nodeIndex: {e}"))?;
            internal
                .set("nodeIndex", node_index)
                .map_err(|e| format!("set nodeIndex: {e}"))?;

            let node_name_host = Rc::clone(&host);
            let node_name = Function::new(ctx.clone(), move |id: i32, generation: i64| -> String {
                if id < 0 || generation < 0 {
                    return String::new();
                }
                node_name_host
                    .borrow()
                    .node_name(FormNodeId(id as usize), generation as u64)
                    .unwrap_or_default()
            })
            .map_err(|e| format!("nodeName: {e}"))?;
            internal
                .set("nodeName", node_name)
                .map_err(|e| format!("set nodeName: {e}"))?;

            // XFA-DATA-M3C: container test used by the JS proxy to decide
            // whether `<handle>._<Name>` should fall back to an empty
            // instance-manager sentinel (containers) or stay `undefined`
            // (fields/draws). Mirrors XFA 3.3 §6.4.3.2.
            let is_container_host = Rc::clone(&host);
            let node_is_container =
                Function::new(ctx.clone(), move |id: i32, generation: i64| -> bool {
                    if id < 0 || generation < 0 {
                        return false;
                    }
                    is_container_host
                        .borrow_mut()
                        .node_is_container(FormNodeId(id as usize), generation as u64)
                })
                .map_err(|e| format!("nodeIsContainer: {e}"))?;
            internal
                .set("nodeIsContainer", node_is_container)
                .map_err(|e| format!("set nodeIsContainer: {e}"))?;

            // XFA-DATA-M3C: form-tree parent lookup used by the JS proxy to
            // materialise the bare global `parent` identifier and `<handle>.
            // parent` chain access. Returns -1 when no parent exists, when
            // the handle is stale, or when no form is installed.
            let parent_node_host = Rc::clone(&host);
            let parent_of_node =
                Function::new(ctx.clone(), move |id: i32, generation: i64| -> i32 {
                    if id < 0 || generation < 0 {
                        return -1;
                    }
                    parent_node_host
                        .borrow_mut()
                        .parent_of_node(FormNodeId(id as usize), generation as u64)
                        .map(|node_id| node_id.0 as i32)
                        .unwrap_or(-1)
                })
                .map_err(|e| format!("parentOfNode: {e}"))?;
            internal
                .set("parentOfNode", parent_of_node)
                .map_err(|e| format!("set parentOfNode: {e}"))?;

            let instance_set_host = Rc::clone(&host);
            let instance_set = Function::new(
                ctx.clone(),
                move |id: i32, generation: i64, n: Opt<i32>| -> i32 {
                    if id < 0 || generation < 0 {
                        return -1;
                    }
                    let n = n.0.unwrap_or(0).max(0) as u32;
                    instance_set_host
                        .borrow_mut()
                        .instance_set_for_handle(FormNodeId(id as usize), generation as u64, n)
                        .map(|count| count as i32)
                        .unwrap_or(-1)
                },
            )
            .map_err(|e| format!("instanceSet: {e}"))?;
            internal
                .set("instanceSet", instance_set)
                .map_err(|e| format!("set instanceSet: {e}"))?;

            let instance_add_host = Rc::clone(&host);
            let instance_add = Function::new(ctx.clone(), move |id: i32, generation: i64| -> i32 {
                if id < 0 || generation < 0 {
                    return -1;
                }
                instance_add_host
                    .borrow_mut()
                    .instance_add_for_handle(FormNodeId(id as usize), generation as u64)
                    .map(|node_id| node_id.0 as i32)
                    .unwrap_or(-1)
            })
            .map_err(|e| format!("instanceAdd: {e}"))?;
            internal
                .set("instanceAdd", instance_add)
                .map_err(|e| format!("set instanceAdd: {e}"))?;

            let instance_remove_host = Rc::clone(&host);
            let instance_remove = Function::new(
                ctx.clone(),
                move |id: i32, generation: i64, index: Opt<i32>| -> bool {
                    if id < 0 || generation < 0 {
                        return false;
                    }
                    let index = index.0.unwrap_or(0).max(0) as u32;
                    instance_remove_host
                        .borrow_mut()
                        .instance_remove_for_handle(
                            FormNodeId(id as usize),
                            generation as u64,
                            index,
                        )
                        .is_ok()
                },
            )
            .map_err(|e| format!("instanceRemove: {e}"))?;
            internal
                .set("instanceRemove", instance_remove)
                .map_err(|e| format!("set instanceRemove: {e}"))?;

            let list_clear_host = Rc::clone(&host);
            let list_clear = Function::new(ctx.clone(), move |id: i32, generation: i64| -> bool {
                if id < 0 || generation < 0 {
                    return false;
                }
                list_clear_host
                    .borrow_mut()
                    .list_clear_for_handle(FormNodeId(id as usize), generation as u64)
                    .is_ok()
            })
            .map_err(|e| format!("listClear: {e}"))?;
            internal
                .set("listClear", list_clear)
                .map_err(|e| format!("set listClear: {e}"))?;

            let list_add_host = Rc::clone(&host);
            let list_add = Function::new(
                ctx.clone(),
                move |id: i32,
                      generation: i64,
                      display: Coerced<String>,
                      save: Opt<Coerced<String>>|
                      -> bool {
                    if id < 0 || generation < 0 {
                        return false;
                    }
                    list_add_host
                        .borrow_mut()
                        .list_add_for_handle(
                            FormNodeId(id as usize),
                            generation as u64,
                            display.0,
                            save.0.map(|s| s.0),
                        )
                        .is_ok()
                },
            )
            .map_err(|e| format!("listAdd: {e}"))?;
            internal
                .set("listAdd", list_add)
                .map_err(|e| format!("set listAdd: {e}"))?;

            let bound_item_host = Rc::clone(&host);
            let bound_item = Function::new(
                ctx.clone(),
                move |id: i32, generation: i64, display: Coerced<String>| -> String {
                    if id < 0 || generation < 0 {
                        return display.0;
                    }
                    bound_item_host.borrow_mut().bound_item_for_handle(
                        FormNodeId(id as usize),
                        generation as u64,
                        display.0,
                    )
                },
            )
            .map_err(|e| format!("boundItem: {e}"))?;
            internal
                .set("boundItem", bound_item)
                .map_err(|e| format!("set boundItem: {e}"))?;

            let num_pages_host = Rc::clone(&host);
            let num_pages =
                Function::new(ctx.clone(), move || num_pages_host.borrow_mut().num_pages())
                    .map_err(|e| format!("numPages: {e}"))?;
            internal
                .set("numPages", num_pages)
                .map_err(|e| format!("set numPages: {e}"))?;

            let binding_error_host = Rc::clone(&host);
            let binding_error = Function::new(ctx.clone(), move || {
                binding_error_host.borrow_mut().metadata_binding_error();
            })
            .map_err(|e| format!("bindingError: {e}"))?;
            internal
                .set("bindingError", binding_error)
                .map_err(|e| format!("set bindingError: {e}"))?;

            let resolve_failure_host = Rc::clone(&host);
            let resolve_failure = Function::new(ctx.clone(), move || {
                resolve_failure_host.borrow_mut().metadata_resolve_failure();
            })
            .map_err(|e| format!("resolveFailure: {e}"))?;
            internal
                .set("resolveFailure", resolve_failure)
                .map_err(|e| format!("set resolveFailure: {e}"))?;

            // Phase E (XFA-JS-HOST-STUBS) — sandbox-safe accounting hook for
            // host capabilities that require viewer / user interaction. JS
            // callers pass an optional capability identifier purely for log
            // forensics; the host only increments the unsupported counter.
            // No filesystem / network / process access ever runs through
            // this path — see benchmarks/JS_SANDBOX_SECURITY_AUDIT.md.
            let unsupported_host_call_host = Rc::clone(&host);
            let unsupported_host_call =
                Function::new(ctx.clone(), move |_capability: Opt<Coerced<String>>| {
                    unsupported_host_call_host
                        .borrow_mut()
                        .metadata_unsupported_host_call();
                })
                .map_err(|e| format!("unsupportedHostCall: {e}"))?;
            internal
                .set("unsupportedHostCall", unsupported_host_call)
                .map_err(|e| format!("set unsupportedHostCall: {e}"))?;

            // Phase D-θ.2 probe-skip telemetry: JS calls this when the strict
            // probe is elided because parentIds.length == 1 && chain.length == 1
            // (no same-name sibling ambiguity is possible in that case).
            let probe_skip_host = Rc::clone(&host);
            let probe_skip = Function::new(ctx.clone(), move || {
                probe_skip_host.borrow_mut().metadata_probe_skip();
            })
            .map_err(|e| format!("probeSkip: {e}"))?;
            internal
                .set("probeSkip", probe_skip)
                .map_err(|e| format!("set probeSkip: {e}"))?;

            // Phase D-γ: DataDom host bindings --------------------------------

            let dc_host = Rc::clone(&host);
            let data_children = Function::new(ctx.clone(), move |raw_id: i32| -> Vec<i32> {
                if raw_id < 0 {
                    return Vec::new();
                }
                dc_host
                    .borrow_mut()
                    .data_children(raw_id as usize)
                    .into_iter()
                    .map(|x| x as i32)
                    .collect()
            })
            .map_err(|e| format!("dataChildren: {e}"))?;
            internal
                .set("dataChildren", data_children)
                .map_err(|e| format!("set dataChildren: {e}"))?;

            let dv_host = Rc::clone(&host);
            let data_value = Function::new(ctx.clone(), move |raw_id: i32| -> Option<String> {
                if raw_id < 0 {
                    return None;
                }
                dv_host.borrow_mut().data_value(raw_id as usize)
            })
            .map_err(|e| format!("dataValue: {e}"))?;
            internal
                .set("dataValue", data_value)
                .map_err(|e| format!("set dataValue: {e}"))?;

            let dcbn_host = Rc::clone(&host);
            let data_child_by_name = Function::new(
                ctx.clone(),
                move |parent_raw: i32, name: Opt<Coerced<String>>| -> i32 {
                    if parent_raw < 0 {
                        return -1;
                    }
                    let Some(name) = name.0 else {
                        return -1;
                    };
                    dcbn_host
                        .borrow_mut()
                        .data_child_by_name(parent_raw as usize, &name.0)
                        .map(|x| x as i32)
                        .unwrap_or(-1)
                },
            )
            .map_err(|e| format!("dataChildByName: {e}"))?;
            internal
                .set("dataChildByName", data_child_by_name)
                .map_err(|e| format!("set dataChildByName: {e}"))?;

            let dbr_host = Rc::clone(&host);
            let data_bound_record = Function::new(
                ctx.clone(),
                move |form_node_id: i32, generation: i64| -> i32 {
                    if form_node_id < 0 || generation < 0 {
                        return -1;
                    }
                    dbr_host
                        .borrow_mut()
                        .data_bound_record(FormNodeId(form_node_id as usize), generation as u64)
                        .map(|x| x as i32)
                        .unwrap_or(-1)
                },
            )
            .map_err(|e| format!("dataBoundRecord: {e}"))?;
            internal
                .set("dataBoundRecord", data_bound_record)
                .map_err(|e| format!("set dataBoundRecord: {e}"))?;

            let drn_host = Rc::clone(&host);
            let data_resolve_node =
                Function::new(ctx.clone(), move |path: Opt<Coerced<String>>| -> i32 {
                    let Some(path) = path.0 else {
                        return -1;
                    };
                    drn_host
                        .borrow_mut()
                        .data_resolve_node(&path.0)
                        .map(|x| x as i32)
                        .unwrap_or(-1)
                })
                .map_err(|e| format!("dataResolveNode: {e}"))?;
            internal
                .set("dataResolveNode", data_resolve_node)
                .map_err(|e| format!("set dataResolveNode: {e}"))?;

            let drns_host = Rc::clone(&host);
            let data_resolve_nodes =
                Function::new(ctx.clone(), move |path: Opt<Coerced<String>>| -> Vec<i32> {
                    let Some(path) = path.0 else {
                        return Vec::new();
                    };
                    drns_host
                        .borrow_mut()
                        .data_resolve_nodes(&path.0)
                        .into_iter()
                        .map(|x| x as i32)
                        .collect()
                })
                .map_err(|e| format!("dataResolveNodes: {e}"))?;
            internal
                .set("dataResolveNodes", data_resolve_nodes)
                .map_err(|e| format!("set dataResolveNodes: {e}"))?;

            // End Phase D-γ host bindings -------------------------------------

            let factory: Function = ctx
                .eval(PHASE_C_BINDINGS_JS.as_bytes())
                .map_err(|e| format!("binding factory parse: {e}"))?;
            let bridge: Object = factory
                .call((internal,))
                .catch(&ctx)
                .map_err(|e| format!("binding factory call: {e}"))?;
            let xfa: Object = bridge.get("xfa").map_err(|e| format!("get xfa: {e}"))?;
            let app: Object = bridge.get("app").map_err(|e| format!("get app: {e}"))?;
            // JS2-01 (Sprint 2 Batch B): `form` is a top-level alias of
            // `xfa.form`. Adobe Reader exposes both spellings; templates
            // such as 60df78fe_pdf_0012 reference bare `form.X`.
            let form: Object = bridge.get("form").map_err(|e| format!("get form: {e}"))?;
            let eval_script: Function = bridge
                .get("evalScript")
                .map_err(|e| format!("get evalScript: {e}"))?;
            let set_variables_script: Function = bridge
                .get("setVariablesScript")
                .map_err(|e| format!("get setVariablesScript: {e}"))?;
            let clear_variables_scripts: Function = bridge
                .get("clearVariablesScripts")
                .map_err(|e| format!("get clearVariablesScripts: {e}"))?;
            let set_variables_data_item: Function = bridge
                .get("setVariablesDataItem")
                .map_err(|e| format!("get setVariablesDataItem: {e}"))?;
            globals
                .set("xfa", xfa)
                .map_err(|e| format!("set xfa global: {e}"))?;
            globals
                .set("app", app)
                .map_err(|e| format!("set app global: {e}"))?;
            globals
                .set("form", form)
                .map_err(|e| format!("set form global: {e}"))?;
            Ok::<
                (
                    Persistent<Function<'static>>,
                    Persistent<Function<'static>>,
                    Persistent<Function<'static>>,
                    Persistent<Function<'static>>,
                ),
                String,
            >((
                Persistent::save(&ctx, eval_script),
                Persistent::save(&ctx, set_variables_script),
                Persistent::save(&ctx, clear_variables_scripts),
                Persistent::save(&ctx, set_variables_data_item),
            ))
        })?;

        self.eval_script = Some(eval_script.0);
        self.set_variables_script = Some(eval_script.1);
        self.clear_variables_scripts = Some(eval_script.2);
        self.set_variables_data_item = Some(eval_script.3);
        self.bindings_registered = true;
        Ok(())
    }

    /// Phase D-ι: extract top-level `var X` and `function X(` identifiers
    /// from a `<variables>` `<script>` body. Used to build the wrapper
    /// IIFE that returns the form-level global object. Best-effort regex
    /// scan — handles the common XFA authoring patterns; complex
    /// destructuring would be missed and the corresponding identifier
    /// would simply not appear in the namespace object.
    fn extract_top_level_idents(body: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        // Strip line and block comments to avoid extracting commented-out idents.
        let stripped = strip_js_comments(body);
        for token_kind in [
            ("var", false),
            ("let", false),
            ("const", false),
            ("function", true),
        ] {
            let kw = token_kind.0;
            let is_fn = token_kind.1;
            let mut search = stripped.as_str();
            while let Some(idx) = search.find(kw) {
                let (before, after) = search.split_at(idx);
                let head_ok = before
                    .chars()
                    .last()
                    .is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '$');
                let tail_after_kw = &after[kw.len()..];
                let tail_ok = tail_after_kw
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_whitespace());
                if !(head_ok && tail_ok) {
                    search = &after[1..];
                    continue;
                }
                // Skip whitespace, then read an identifier.
                let after_ws = tail_after_kw.trim_start();
                let mut chars = after_ws.chars();
                let ident: String = std::iter::once(chars.next())
                    .chain(chars.map(Some))
                    .map_while(|c| c.filter(|c| c.is_alphanumeric() || *c == '_' || *c == '$'))
                    .collect();
                if ident.is_empty() || ident.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    search = &after[kw.len()..];
                    continue;
                }
                if is_fn {
                    let after_ident = after_ws
                        .trim_start_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '$');
                    if !after_ident.trim_start().starts_with('(') {
                        search = &after[kw.len()..];
                        continue;
                    }
                }
                if seen.insert(ident.clone()) {
                    out.push(ident);
                }
                search = &after[kw.len()..];
            }
        }
        out
    }

    /// Phase D-ι: register a `<variables>` `<script name="name">` block
    /// as a form-level global. Called by the dispatch layer after
    /// `set_form_handle` so subsequent event/calculate scripts see the
    /// namespace.
    ///
    /// Variables-script bodies are evaluated through the same QuickJS
    /// context as event scripts, so they MUST run under the same
    /// per-script time budget (`set_deadline` / interrupt handler) and
    /// the same body-size cap (`MAX_SCRIPT_BODY_BYTES`). Without the
    /// budget, an untrusted/malformed XFA template that contains
    /// `while (true) {}` inside `<variables>` could hang flatten before
    /// any normal event script runs (Codex P1 review on PR #1499).
    fn register_variables_script(
        &self,
        name: &str,
        body: &str,
        subform_scope: Option<&str>,
    ) -> Result<(), SandboxError> {
        let Some(setter) = self.set_variables_script.clone() else {
            return Ok(());
        };
        // W2-B: variables-scripts use a higher body-size cap than event
        // scripts because they are form-level helper libraries (XFA 3.3
        // §5.5). The time- and memory-budgets still apply via the
        // QuickJS interrupt handler + runtime memory limit; the larger
        // body cap simply lets real-world government XFA libraries (e.g.
        // `validateForm` ≈ 125 KB, `LOV` ≈ 507 KB, `CoreFunctions` ≈ 115
        // KB) register so dependent event scripts can resolve
        // `<scriptName>.<top_level_decl>(...)` rather than failing as
        // `js_resolve_failure` (W1-B `implicit_function` cluster).
        if body.len() > MAX_VARIABLES_SCRIPT_BODY_BYTES {
            return Err(SandboxError::BodyTooLarge);
        }
        // W3-A — REDOS-01 mitigation also gates variables-scripts.
        // A library-level helper that defines a global function whose
        // body contains `/(a+)+$/` would be evaluated by QuickJS at
        // registration time, and the interrupt cannot bound it.
        if let RegexScanVerdict::Reject { reason } = scan_script_for_redos(body) {
            return Err(SandboxError::RegexRejected(reason));
        }
        let idents = Self::extract_top_level_idents(body);
        let scope = subform_scope.unwrap_or("").to_string();
        self.set_deadline();
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.context.with(|ctx| -> Result<(), rquickjs::Error> {
                let setter = setter.restore(&ctx)?;
                let _: bool = setter.call((name, body, idents, scope))?;
                Ok(())
            })
        }));
        // Detect timeouts the same way `execute_script` does: if the
        // deadline elapsed during evaluation, the interrupt handler aborts
        // QuickJS with an error.
        let deadline_now = self.script_deadline.load(Ordering::Acquire);
        let now_nanos = Instant::now()
            .checked_duration_since(epoch())
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let timed_out = deadline_now != 0 && now_nanos >= deadline_now;
        self.clear_deadline();
        match result {
            Ok(Ok(())) => Ok(()),
            // QF1-E / SEC-01: defence-in-depth wall-time fallback also
            // covers `<variables>` `<script>` registration. The same
            // classifier upgrades a `Timeout` to `WallTimeExceeded` when
            // elapsed ≥ multiplier × budget.
            Ok(Err(_)) if timed_out => Err(self.classify_timeout_or_walltime(now_nanos)),
            Ok(Err(e)) => Err(SandboxError::ScriptError(format!(
                "variables-script `{name}` register: {e}"
            ))),
            Err(_) => Err(SandboxError::PanicCaptured(format!(
                "panic registering variables-script `{name}`"
            ))),
        }
    }

    /// W3-D RETRY: register one `<variables>` `<text name="X">value</text>`
    /// data item (XFA 3.3 §5.5.2) as a form-level mutable string container.
    /// Sister of [`Self::register_variables_script`] but with no body cap,
    /// no REDOS scan, and no time budget — data items carry text payloads,
    /// not JavaScript code. Idempotent per `(subform_scope, name)`.
    fn register_variables_data_item(
        &self,
        name: &str,
        initial: &str,
        subform_scope: Option<&str>,
    ) -> Result<(), SandboxError> {
        let Some(setter) = self.set_variables_data_item.clone() else {
            return Ok(());
        };
        let initial_owned = initial.to_string();
        let scope = subform_scope.unwrap_or("").to_string();
        let name_owned = name.to_string();
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.context.with(|ctx| -> Result<(), rquickjs::Error> {
                let setter = setter.restore(&ctx)?;
                let _: bool = setter.call((name_owned, initial_owned, scope))?;
                Ok(())
            })
        }));
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(SandboxError::ScriptError(format!(
                "variables data item `{name}` register: {e}"
            ))),
            Err(_) => Err(SandboxError::PanicCaptured(format!(
                "panic registering variables data item `{name}`"
            ))),
        }
    }

    fn clear_variables_scripts_global(&self) -> Result<(), SandboxError> {
        let Some(clearer) = self.clear_variables_scripts.clone() else {
            return Ok(());
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.context.with(|ctx| -> Result<(), rquickjs::Error> {
                let clearer = clearer.restore(&ctx)?;
                let _: () = clearer.call(())?;
                Ok(())
            })
        }));
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(SandboxError::ScriptError(format!(
                "variables-script clear: {e}"
            ))),
            Err(_) => Err(SandboxError::PanicCaptured(
                "panic clearing variables-scripts".to_string(),
            )),
        }
    }
}

/// Phase D-ι: strip `//` and `/* */` comments from a JS source body so
/// the regex scan for top-level `var` / `function` declarations does not
/// match commented-out tokens. Conservative — preserves strings (so a
/// `"//"` substring inside a string literal is left alone). XFA scripts
/// almost never have comments inside string literals, but the guard
/// matters for correctness.
fn strip_js_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' || b == b'\'' {
            // Copy string literal verbatim.
            let quote = b;
            out.push(b as char);
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                out.push(c as char);
                if c == b'\\' && i + 1 < bytes.len() {
                    out.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                i += 1;
                if c == quote {
                    break;
                }
            }
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() {
            let n = bytes[i + 1];
            if n == b'/' {
                // Line comment.
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if n == b'*' {
                // Block comment.
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
                continue;
            }
        }
        out.push(b as char);
        i += 1;
    }
    out
}

const PHASE_C_BINDINGS_JS: &str = r#"
(function(host) {
  function protoGuard() {
    return Object.freeze(Object.create(null));
  }

  function nullProtoObject() {
    var obj = Object.create(null);
    Object.defineProperty(obj, "__proto__", {
      value: protoGuard(),
      enumerable: false,
      configurable: false,
      writable: false
    });
    return obj;
  }

  function lookupObject() {
    return Object.create(null);
  }

  var deferredGlobalNames = lookupObject();
  [
    "app", "arguments", "Array", "Boolean", "Bun", "console", "Date",
    "decodeURI", "decodeURIComponent", "Deno", "encodeURI",
    "encodeURIComponent", "Error", "eval", "EvalError", "fetch",
    "Function", "globalThis", "Infinity", "isFinite", "isNaN", "JSON",
    "Map", "Math", "NaN", "Number", "Object", "parseFloat", "parseInt",
    "process", "RangeError", "ReferenceError", "RegExp", "require",
    "Set", "String", "Symbol", "SyntaxError", "TypeError", "undefined",
    "URIError", "WeakMap", "WeakSet", "WebSocket", "XMLHttpRequest",
    // JS2-01 (Sprint 2 Batch B): `form` is a top-level alias of `xfa.form`.
    // It must defer to globalThis so the host SOM resolver does not first
    // try to resolve `form` as a form-tree node, find nothing, and then
    // surface a `js_resolve_failure` for what is really a viewer global.
    "xfa", "event", "form"
  ].forEach(function(name) {
    deferredGlobalNames[name] = true;
  });

  var reservedHandleProperties = lookupObject();
  [
    "__defineGetter__", "__defineSetter__", "__lookupGetter__",
    "__lookupSetter__", "__proto__", "constructor", "hasOwnProperty",
    "isPrototypeOf", "propertyIsEnumerable", "then", "toJSON",
    "toLocaleString", "toString", "valueOf"
  ].forEach(function(name) {
    reservedHandleProperties[name] = true;
  });

  function shouldDeferGlobalName(name, localNames) {
    return name.charAt(0) === "_" ||
      localNames[name] === true ||
      deferredGlobalNames[name] === true;
  }

  // Properties that must NOT be deferred so their specific implementations run.
  var handlePropertyExclusions = lookupObject();
  ["rawValue", "somExpression", "isNull", "clearItems", "addItem", "boundItem",
   "$record", "nodes", "value", "length", "item", "choiceList"].forEach(function(name) {
    handlePropertyExclusions[name] = true;
  });

  function shouldDeferHandleProperty(name) {
    if (handlePropertyExclusions[name] === true) {
      return false;
    }
    return name.charAt(0) === "_" || reservedHandleProperties[name] === true;
  }

  function collectLocalNames(body) {
    var locals = lookupObject();
    var match;
    var decls = /\b(?:var|let|const)\s+([^;]+)/g;
    while ((match = decls.exec(body)) !== null) {
      match[1].split(",").forEach(function(part) {
        var ident = /^\s*([A-Za-z_$][0-9A-Za-z_$]*)/.exec(part);
        if (ident) {
          locals[ident[1]] = true;
        }
      });
    }
    var funcs = /\bfunction\s+([A-Za-z_$][0-9A-Za-z_$]*)/g;
    while ((match = funcs.exec(body)) !== null) {
      locals[match[1]] = true;
    }
    return locals;
  }

  // XFA-DATA-M3C: `<handle>.ui` returns the ui-config sub-object that
  // Adobe templates expose for choiceList / textEdit / checkButton widget
  // tweaking. During static flatten the widget tree is not materialised, so
  // we hand back a minimal stub whose `choiceList` is an empty frozen
  // array (matching the bare `handle.choiceList` fallback). Property
  // writes are absorbed silently so scripts like
  // `field.ui.choiceList.commitOn = "exit"` complete without TypeError.
  function makeUiStub() {
    var stub = nullProtoObject();
    Object.defineProperty(stub, "choiceList", {
      enumerable: true,
      configurable: false,
      get: function() { return Object.freeze([]); }
    });
    Object.defineProperty(stub, "textEdit", {
      enumerable: true,
      configurable: false,
      get: function() { return Object.freeze({}); }
    });
    Object.defineProperty(stub, "checkButton", {
      enumerable: true,
      configurable: false,
      get: function() { return Object.freeze({}); }
    });
    return new Proxy(stub, {
      get: function(target, prop) {
        if (typeof prop !== "string") return undefined;
        if (prop in target) return target[prop];
        // Sub-property writes / reads are absorbed: scripts walking
        // unknown widget config (e.g. `ui.imageEdit.<x>`) get a chainable
        // viewer-stub rather than crashing on undefined.
        return makeViewerStub();
      },
      set: function(_t, _p, _v) { return true; },
      has: function(_t, _p) { return true; }
    });
  }

  function makeInstanceManager(id, generation) {
    var manager = nullProtoObject();
    Object.defineProperty(manager, "count", {
      enumerable: true,
      configurable: false,
      get: function() {
        return host.instanceCount(id, generation);
      }
    });
    Object.defineProperty(manager, "setInstances", {
      enumerable: true,
      configurable: false,
      writable: false,
      value: function(n) {
        return host.instanceSet(id, generation, n);
      }
    });
    Object.defineProperty(manager, "addInstance", {
      enumerable: true,
      configurable: false,
      writable: false,
      value: function() {
        var newId = host.instanceAdd(id, generation);
        return newId < 0 ? null : makeHandle(newId, generation);
      }
    });
    Object.defineProperty(manager, "removeInstance", {
      enumerable: true,
      configurable: false,
      writable: false,
      value: function(idx) {
        return host.instanceRemove(id, generation, idx);
      }
    });
    return Object.freeze(manager);
  }

  function makeEmptyInstanceManager() {
    var manager = nullProtoObject();
    Object.defineProperty(manager, "count", {
      enumerable: true,
      configurable: false,
      value: 0
    });
    Object.defineProperty(manager, "setInstances", {
      enumerable: true,
      configurable: false,
      writable: false,
      value: function() { return 0; }
    });
    Object.defineProperty(manager, "addInstance", {
      enumerable: true,
      configurable: false,
      writable: false,
      value: function() { return null; }
    });
    Object.defineProperty(manager, "removeInstance", {
      enumerable: true,
      configurable: false,
      writable: false,
      value: function() { return false; }
    });
    return Object.freeze(manager);
  }

  function uniqueNodeIds(ids) {
    var out = [];
    if (!ids) return out;
    for (var i = 0; i < ids.length; i++) {
      var id = ids[i] | 0;
      if (id < 0) continue;
      var seen = false;
      for (var j = 0; j < out.length; j++) {
        if (out[j] === id) {
          seen = true;
          break;
        }
      }
      if (!seen) out.push(id);
    }
    return out;
  }

  function nodeIdListArg(ids) {
    return uniqueNodeIds(ids).join(",");
  }

  function resolveHandleChildIds(ids, prop) {
    var list = nodeIdListArg(ids);
    if (list.length === 0) return [];
    var childIds = uniqueNodeIds(host.resolveChildNodeIds(list, prop));
    if (childIds.length > 0) return childIds;
    return uniqueNodeIds(host.resolveScopedNodeIds(list, prop));
  }

  function makeNodeHandleFromIds(ids, generation) {
    var unique = uniqueNodeIds(ids);
    if (unique.length === 0) return undefined;
    if (unique.length === 1) return makeHandle(unique[0], generation);
    return makeCandidateSet(unique, generation);
  }

  // Phase D-θ: single-segment SOM lookahead. The proxy chain `F.P1.X.rawValue`
  // can resolve the leading `F.P1` correctly yet pick the wrong `P1` when
  // multiple same-named siblings exist — the resolver lacks the next-segment
  // context needed to disambiguate. These helpers return a one-step deferred
  // wrapper that, on its next property access, asks the host's hinted
  // resolver to drop candidates whose subtree does not contain the lookahead
  // name. Terminal properties (rawValue, instanceManager, …) bypass the hint
  // path entirely so single-token reads remain byte-for-byte identical.
  function isTerminalHandleProp(name) {
    if (handlePropertyExclusions[name] === true) return true;
    if (reservedHandleProperties[name] === true) return true;
    if (name.charAt(0) === "_") return true;
    switch (name) {
      case "rawValue":
      case "somExpression":
      case "instanceManager":
      case "index":
      case "setInstances":
      case "addInstance":
      case "removeInstance":
      case "isNull":
      case "clearItems":
      case "addItem":
      case "boundItem":
      case "$record":
      case "variables":
      case "nodes":
      case "length":
      case "value":
      case "item":
        return true;
      default:
        return false;
    }
  }

  function makeImplicitDeferred(currentId, name, eagerIds, generation) {
    function realize(hint) {
      if (!hint) return eagerIds;
      var refined = uniqueNodeIds(host.resolveImplicitNodeIdsHinted(currentId, name, hint));
      return refined.length > 0 ? refined : eagerIds;
    }
    var sentinel = nullProtoObject();
    return new Proxy(sentinel, {
      get: function(target, prop, receiver) {
        if (typeof prop !== "string") {
          return Reflect.get(target, prop, receiver);
        }
        // XFA-DATA2-02: short-circuit caption / resolveNode pair before any
        // host call. See captionSentinel + handleResolveNode definitions.
        if (prop === "caption") { return captionSentinel; }
        if (prop === "resolveNode") { return handleResolveNode; }
        if (prop === "resolveNodes") { return handleResolveNodes; }
        var ids = isTerminalHandleProp(prop) ? eagerIds : realize(prop);
        var handle = makeNodeHandleFromIds(ids, generation);
        if (handle === undefined) return undefined;
        return handle[prop];
      },
      set: function(_target, prop, value) {
        var handle = makeNodeHandleFromIds(eagerIds, generation);
        if (handle === undefined) return true;
        handle[prop] = value;
        return true;
      },
      has: function(_target, prop) {
        if (typeof prop !== "string") return Reflect.has(sentinel, prop);
        return true;
      }
    });
  }

  function makeChildDeferred(parentIds, childName, eagerChildIds, generation) {
    var parentList = nodeIdListArg(parentIds);
    function realize(hint) {
      if (!hint || parentList.length === 0) return eagerChildIds;
      var refined = uniqueNodeIds(host.resolveChildNodeIdsHinted(parentList, childName, hint));
      return refined.length > 0 ? refined : eagerChildIds;
    }
    var sentinel = nullProtoObject();
    return new Proxy(sentinel, {
      get: function(target, prop, receiver) {
        if (typeof prop !== "string") {
          return Reflect.get(target, prop, receiver);
        }
        // XFA-DATA2-02: short-circuit caption / resolveNode pair before any
        // host call. See captionSentinel + handleResolveNode definitions.
        if (prop === "caption") { return captionSentinel; }
        if (prop === "resolveNode") { return handleResolveNode; }
        if (prop === "resolveNodes") { return handleResolveNodes; }
        var ids = isTerminalHandleProp(prop) ? eagerChildIds : realize(prop);
        var handle = makeNodeHandleFromIds(ids, generation);
        if (handle === undefined) return undefined;
        return handle[prop];
      },
      set: function(_target, prop, value) {
        var handle = makeNodeHandleFromIds(eagerChildIds, generation);
        if (handle === undefined) return true;
        handle[prop] = value;
        return true;
      },
      has: function(_target, prop) {
        if (typeof prop !== "string") return Reflect.has(sentinel, prop);
        return true;
      }
    });
  }

  function makeChainHandle(parentIds, childName, generation) {
    var eagerChildIds = resolveHandleChildIds(parentIds, childName);
    if (eagerChildIds.length === 0) return undefined;
    if (eagerChildIds.length === 1) {
      // Singleton child with a single parent leaves the host resolver no
      // alternatives to filter even when a hint is provided — same-name
      // sibling disambiguation is impossible. Skip the wrapper so the
      // common path keeps its original Proxy identity and resolve budget.
      return makeHandle(eagerChildIds[0], generation);
    }
    // Phase D-θ.2: same-name siblings exist; defer through the full-chain
    // accumulator so a deeper segment (still unread) can disambiguate.
    return makeChainProxy(parentIds, [childName], eagerChildIds, generation, -1);
  }

  // Phase D-θ.2: lazy chain proxy. Accumulates a SOM chain (string[]) as the
  // script walks property access; on the first terminal property read it
  // asks the host to resolve the FULL accumulated chain in one call,
  // letting the host backtrack and pick the same-name candidate at any
  // depth whose subtree actually completes the chain.
  //
  // Compared to D-θ.1 (single-segment hint), this lets `A.B.C.D.rawValue`
  // pick the correct same-name `A` not only when one is the parent of `B`
  // but specifically the one whose subtree contains the full B.C.D chain.
  //
  // Parameters:
  //   parentIds            — entry parent set; empty array means implicit walk.
  //   chain                — string[] of segments accumulated so far (>=1).
  //   eagerIds             — fallback ids returned if the host's full-chain
  //                          resolve produces no candidate at any depth
  //                          (preserves D-θ.1 byte-for-byte behaviour for
  //                          chains the host cannot improve on).
  //   generation           — handle generation to bind to.
  //   currentId            — implicit-walk anchor (used when parentIds is empty).
  //
  // Terminal property names trigger immediate resolution. Non-terminal
  // string property names return a NEW chain proxy with the property name
  // appended; no host call happens at that point.
  function makeChainProxy(parentIds, chain, eagerIds, generation, currentId) {
    var sentinel = nullProtoObject();
    function fullChain(extraSeg) {
      var parentCsv = nodeIdListArg(parentIds);
      var chainArr = chain;
      if (extraSeg !== undefined) {
        chainArr = chain.slice();
        chainArr.push(extraSeg);
      }
      var chainCsv = chainArr.join(",");
      var resolved = uniqueNodeIds(
        host.resolveWithFullChain(parentCsv, currentId | 0, chainCsv)
      );
      if (resolved.length === 0) return eagerIds;
      return resolved;
    }
    return new Proxy(sentinel, {
      get: function(target, prop, receiver) {
        if (typeof prop !== "string") {
          // Symbol.toPrimitive and similar non-string keys must force
          // terminal resolution so coercion (e.g. via Number(handle)) sees
          // the resolved candidate's primitive view rather than the proxy
          // sentinel itself.
          var primIds = fullChain();
          var primHandle = makeNodeHandleFromIds(primIds, generation);
          if (primHandle === undefined) {
            return Reflect.get(target, prop, receiver);
          }
          return primHandle[prop];
        }
        // XFA-DATA2-02: caption / resolveNode / resolveNodes short-circuit on
        // the chain proxy. Without this branch the chain accumulator would
        // append "caption" as a non-terminal segment, call
        // resolveWithFullChainStrict, miss, and return `undefined` —
        // `field.caption.value` then TypeErrors. Same null/empty-list
        // contract as the global pair.
        if (prop === "caption") {
          return captionSentinel;
        }
        if (prop === "resolveNode") {
          return handleResolveNode;
        }
        if (prop === "resolveNodes") {
          return handleResolveNodes;
        }
        if (isTerminalHandleProp(prop)) {
          var ids = fullChain();
          var handle = makeNodeHandleFromIds(ids, generation);
          if (handle === undefined) return undefined;
          return handle[prop];
        }
        // Non-terminal: probe the extended chain in STRICT mode. If the
        // host cannot complete `[chain..., prop]` at full depth anywhere
        // in the form, return `undefined` — mirroring the eager D-θ.1
        // behaviour where `A.B` resolved to `undefined` when no `A.B`
        // existed. When the probe succeeds, return a new chain proxy
        // with the segment appended; the lazy accumulation lets a
        // deeper segment still disambiguate which same-name `A` to keep.
        //
        // D-θ.2 probe-skip: when there is exactly one parent and the chain
        // so far has exactly one segment, no same-name sibling ambiguity is
        // possible — the parent uniquely identifies the node, and there are
        // no alternative subtrees for the host to choose between.  In that
        // case the strict probe would always return the same set as eagerIds,
        // so we skip the host round-trip and immediately build the next proxy
        // using eagerIds as the fallback.  The host-call budget is preserved
        // and disambiguation is unaffected because a single-parent / single-
        // segment chain cannot be disambiguated further by the host anyway.
        var nextChain = chain.slice();
        nextChain.push(prop);
        if (parentIds.length === 1 && chain.length === 1) {
          host.probeSkip();
          return makeChainProxy(parentIds, nextChain, eagerIds, generation, currentId);
        }
        var probe = host.resolveWithFullChainStrict(
          nodeIdListArg(parentIds),
          currentId | 0,
          nextChain.join(",")
        );
        if (!probe || probe.length === 0) {
          return undefined;
        }
        // The probe result becomes the new eager-fallback so future
        // probes that the host cannot improve on still degrade
        // gracefully. We must NOT collapse to a single makeHandle here:
        // the chain may continue to grow and disambiguate further.
        var nextEager = uniqueNodeIds(probe);
        return makeChainProxy(parentIds, nextChain, nextEager, generation, currentId);
      },
      set: function(_target, prop, value) {
        // Writes are always terminal: resolve the full chain and forward.
        var ids = fullChain();
        var handle = makeNodeHandleFromIds(ids, generation);
        if (handle === undefined) return true;
        handle[prop] = value;
        return true;
      },
      has: function(_target, prop) {
        if (typeof prop !== "string") return Reflect.has(sentinel, prop);
        return true;
      }
    });
  }

  function makeCandidateSet(ids, generation) {
    var candidates = uniqueNodeIds(ids);
    if (candidates.length === 0) return undefined;
    var firstId = candidates[0];
    var obj = nullProtoObject();
    return new Proxy(obj, {
      get: function(target, prop, receiver) {
        if (typeof prop !== "string") {
          return Reflect.get(target, prop, receiver);
        }
        if (prop === "rawValue") {
          var value = host.getRawValue(firstId, generation);
          return value === undefined ? null : value;
        }
        if (prop === "somExpression") {
          return "xfa[0].form[0].placeholder";
        }
        if (prop === "instanceManager") {
          return makeInstanceManager(firstId, generation);
        }
        if (prop === "index") {
          return host.nodeIndex(firstId, generation);
        }
        // XFA-DATA-M3C: `<candidateSet>.parent` walks one form-tree level up
        // from the leading candidate. Same semantics as `<handle>.parent`;
        // see makeHandle for rationale.
        if (prop === "parent") {
          var parentId = host.parentOfNode(firstId, generation);
          if (parentId < 0) return undefined;
          return makeHandle(parentId, generation);
        }
        if (prop === "setInstances") {
          return function(n) {
            return host.instanceSet(firstId, generation, n);
          };
        }
        if (prop === "addInstance") {
          return function() {
            var newId = host.instanceAdd(firstId, generation);
            // WP-3 F3: return a chainable null-safe sentinel rather than null
            // so scripts that immediately access `.index` / `.value` on the
            // newly-added handle do not throw when the instance manager
            // refuses (min/max reached, unbound).
            return newId < 0 ? makeNullDataHandle() : makeHandle(newId, generation);
          };
        }
        if (prop === "removeInstance") {
          return function(idx) {
            return host.instanceRemove(firstId, generation, idx);
          };
        }
        if (prop === "isNull") {
          var raw = host.getRawValue(firstId, generation);
          return raw === undefined || raw === null || raw === "";
        }
        if (prop === "clearItems") {
          return function() {
            return host.listClear(firstId, generation);
          };
        }
        if (prop === "addItem") {
          return function(display, save) {
            if (save === undefined) {
              return host.listAdd(firstId, generation, String(display));
            }
            return host.listAdd(firstId, generation, String(display), String(save));
          };
        }
        if (prop === "boundItem") {
          return function(displayValue) {
            var coerced = displayValue === null || displayValue === undefined ?
              "" : String(displayValue);
            return host.boundItem(firstId, generation, coerced);
          };
        }
        if (prop === "$record") {
          var recRaw = host.dataBoundRecord(firstId, generation);
          if (recRaw < 0) return makeNullDataHandle();
          return makeDataHandle(recRaw);
        }
        if (prop === "variables") {
          var csNodeName = host.nodeName(firstId, generation);
          if (typeof csNodeName === "string" && csNodeName.length > 0 &&
              subformVariables[csNodeName] !== undefined) {
            return subformVariables[csNodeName];
          }
          return Object.create(null);
        }
        if (prop.charAt(0) === "_" && prop.length > 1) {
          var bareName = prop.substring(1);
          var imIds = uniqueNodeIds(
            host.resolveChildNodeIdsQuiet(nodeIdListArg(candidates), bareName)
          );
          if (imIds.length > 0) {
            return makeInstanceManager(imIds[0], generation);
          }
          for (var imIdx = 0; imIdx < candidates.length; imIdx++) {
            if (host.hasZeroInstanceRun(candidates[imIdx], generation, bareName)) {
              return makeEmptyInstanceManager();
            }
          }
          // XFA-DATA-M3C: same fallthrough as makeHandle underscore branch —
          // expose an empty instanceManager for schema-optional same-named
          // siblings that the merged tree omits, keeping schema-bound
          // scripts chainable. Restricted to container candidates so
          // field/draw candidate sets keep the original `undefined`
          // semantics for underscored property reads.
          if (host.nodeIsContainer(firstId, generation)) {
            return makeEmptyInstanceManager();
          }
        }
        // WP-3: choiceList is a listbox/combobox property not surfaced through
        // the FormTree. Return an empty frozen array.
        if (prop === "choiceList") {
          return Object.freeze([]);
        }
        // XFA-DATA-M3C: `<candidateSet>.ui` returns the widget-config stub.
        // See makeUiStub on makeHandle for rationale.
        if (prop === "ui") {
          return makeUiStub();
        }
        if (prop === "formattedValue") {
          var fvc = host.getRawValue(firstId, generation);
          return fvc === undefined || fvc === null ? "" : String(fvc);
        }
        if (prop === "execEvent") {
          return function() { return undefined; };
        }
        // XFA-DATA2-02: caption / resolveNode / resolveNodes intercept on the
        // candidate-set path. Same rationale as makeHandle: skip the child
        // resolver host call and its resolve_failures bump, return a frozen
        // sentinel + the chainable method pair. Null contract preserved.
        if (prop === "caption") {
          return captionSentinel;
        }
        if (prop === "resolveNode") {
          return handleResolveNode;
        }
        if (prop === "resolveNodes") {
          return handleResolveNodes;
        }
        if (shouldDeferHandleProperty(prop)) {
          return undefined;
        }
        return makeChainHandle(candidates, prop, generation);
      },
      set: function(_target, prop, value) {
        // WP-3: `value` is an alias for rawValue on the set side.
        if (prop === "rawValue" || prop === "value") {
          host.setRawValue(firstId, generation, value);
        }
        return true;
      },
      has: function(target, prop) {
        if (typeof prop !== "string") {
          return Reflect.has(target, prop);
        }
        return prop === "rawValue" ||
          prop === "somExpression" ||
          prop === "instanceManager" ||
          prop === "index" ||
          prop === "parent" ||
          prop === "setInstances" ||
          prop === "addInstance" ||
          prop === "removeInstance" ||
          prop === "isNull" ||
          prop === "clearItems" ||
          prop === "addItem" ||
          prop === "boundItem" ||
          Reflect.has(target, prop);
      }
    });
  }

  // XFA-DATA2-02: shared caption sentinel — singleton, frozen, null-safe.
  //
  // XFA templates commonly inspect `field.caption.value` or
  // `field.caption.text` to drive labels and conditional formatting. The
  // merged FormTree does not carry the optional `<caption>` sub-element, so
  // looking up `caption` as a normal child name would call
  // `resolve_child_candidates` → NoMatch → bump `resolve_failures`, return
  // `undefined`, and the very next `.value` access would throw TypeError
  // ("cannot read property 'value' of undefined") and abort the rest of the
  // script's initializers.
  //
  // We intercept `caption` BEFORE the child-resolver call and return a
  // chainable sentinel whose terminal reads are empty strings and whose
  // deeper reads keep chaining via `makeNullDataHandle`. This is purely
  // read-side: writes are silently absorbed (caption sub-element is a
  // viewer-only field, never a flatten mutation channel). The sentinel is
  // allocated once at factory build-time and frozen, so per-call access is
  // an `Object.freeze`d Proxy hit with no heap allocation.
  var captionSentinel = (function() {
    var base = nullProtoObject();
    Object.defineProperty(base, "value",   { enumerable: true, configurable: false,
      get: function() { return ""; }, set: function(_v) { /* viewer-only */ } });
    Object.defineProperty(base, "text",    { enumerable: true, configurable: false,
      get: function() { return ""; }, set: function(_v) { /* viewer-only */ } });
    Object.defineProperty(base, "name",    { enumerable: true, configurable: false,
      get: function() { return "caption"; } });
    Object.defineProperty(base, "rawValue",{ enumerable: true, configurable: false,
      get: function() { return ""; }, set: function(_v) { /* viewer-only */ } });
    Object.defineProperty(base, "isNull",  { enumerable: true, configurable: false,
      get: function() { return true; } });
    return new Proxy(base, {
      get: function(target, prop) {
        if (prop in target || typeof prop !== "string") return target[prop];
        // Unknown reads stay chainable so `caption.font.typeface` etc. do
        // not throw. The sentinel is read-only and write-absorbing.
        return makeNullDataHandle();
      },
      set: function(_t, _p, _v) { return true; },
      has: function() { return true; }
    });
  })();

  // XFA-DATA2-02: shared `resolveNode` / `resolveNodes` instance methods.
  //
  // Beyond the global `xfa.resolveNode(path)`, XFA scripts also call these
  // as methods on individual handles (`field.resolveNode("Subform.X")`,
  // `subform.resolveNodes("$.field[*]")`). Without an explicit shortcut these
  // property reads would fall through to `makeChainHandle`, miss in the
  // child resolver and inflate `js_resolve_failures` — the very pattern the
  // 60df78fe corpus replay flagged.
  //
  // Behaviour mirrors the global pair, including the data-path routing for
  // `data.`, `$data.`, and `xfa.datasets.data.` prefixes. The Cluster C
  // null-return contract is preserved: a miss in the host returns native
  // `null` for the singular form and a frozen empty array for the plural.
  function handleResolveNode(path) {
    if (typeof path === "string" &&
        (path.indexOf("data.") === 0 || path.indexOf("$data.") === 0 ||
         path.indexOf("xfa.datasets.data.") === 0)) {
      var rawId = host.dataResolveNode(path);
      if (rawId < 0) return null;
      return makeDataHandle(rawId);
    }
    var nid = host.resolveNodeId(path);
    if (nid < 0) return null;
    return makeHandle(nid, host.generation());
  }
  function handleResolveNodes(path) {
    if (typeof path === "string" &&
        (path.indexOf("data.") === 0 || path.indexOf("$data.") === 0 ||
         path.indexOf("xfa.datasets.data.") === 0)) {
      var rawIds = host.dataResolveNodes(path);
      var out = [];
      for (var i = 0; i < rawIds.length; i++) out.push(makeDataHandle(rawIds[i]));
      return Object.freeze(out);
    }
    var generation = host.generation();
    var ids = host.resolveNodeIds(path);
    var out2 = [];
    for (var j = 0; j < ids.length; j++) {
      out2.push(makeHandle(ids[j], generation));
    }
    return Object.freeze(out2);
  }

  function makeHandle(id, generation) {
    var obj = nullProtoObject();
    Object.defineProperty(obj, "rawValue", {
      enumerable: true,
      configurable: false,
      get: function() {
        var value = host.getRawValue(id, generation);
        return value === undefined ? null : value;
      },
      set: function(value) {
        host.setRawValue(id, generation, value);
      }
    });
    // Phase C-α: defensive stub. Real Adobe forms call
    // `this.somExpression` to obtain the SOM path string. We don't expose
    // the real SOM path (introspection capability), but returning a
    // placeholder lets viewer-tweak scripts (e.g. acroSOM substr(15))
    // proceed without ReferenceError. Mutations via this handle still
    // require the rawValue setter, which is the only side-effect channel.
    Object.defineProperty(obj, "somExpression", {
      enumerable: false,
      configurable: false,
      get: function() {
        return "xfa[0].form[0].placeholder";
      }
    });
    return new Proxy(obj, {
      get: function(target, prop, receiver) {
        if (typeof prop !== "string") {
          return Reflect.get(target, prop, receiver);
        }
        if (prop === "rawValue" || prop === "somExpression") {
          return Reflect.get(target, prop, receiver);
        }
        if (prop === "instanceManager") {
          return makeInstanceManager(id, generation);
        }
        if (prop === "index") {
          return host.nodeIndex(id, generation);
        }
        // XFA-DATA-M3C: `<handle>.parent` returns the form-tree parent.
        // Adobe scripts commonly chain `parent.somExpression`,
        // `parent.index`, or `parent._Sibling.setInstances(...)` to walk
        // one level up from a field or subform without authoring an
        // explicit SOM path.
        if (prop === "parent") {
          var parentId = host.parentOfNode(id, generation);
          if (parentId < 0) return undefined;
          return makeHandle(parentId, generation);
        }
        if (prop === "setInstances") {
          return function(n) {
            return host.instanceSet(id, generation, n);
          };
        }
        if (prop === "addInstance") {
          return function() {
            var newId = host.instanceAdd(id, generation);
            // WP-3 F3: chainable null-safe sentinel on failure (see candidateSet).
            return newId < 0 ? makeNullDataHandle() : makeHandle(newId, generation);
          };
        }
        if (prop === "removeInstance") {
          return function(idx) {
            return host.instanceRemove(id, generation, idx);
          };
        }
        if (prop === "isNull") {
          var value = host.getRawValue(id, generation);
          return value === undefined || value === null || value === "";
        }
        if (prop === "clearItems") {
          return function() {
            return host.listClear(id, generation);
          };
        }
        if (prop === "addItem") {
          return function(display, save) {
            if (save === undefined) {
              return host.listAdd(id, generation, String(display));
            }
            return host.listAdd(id, generation, String(display), String(save));
          };
        }
        // XFA 3.3 §App A `boundItem` — listbox display→save lookup. Used as
        // `field.boundItem(xfa.event.newText)` to translate a user-visible
        // option label into its underlying save value. Falls back to the
        // input string when no match exists (Adobe behaviour).
        if (prop === "boundItem") {
          return function(displayValue) {
            var coerced;
            if (displayValue === null || displayValue === undefined) {
              coerced = "";
            } else {
              coerced = String(displayValue);
            }
            return host.boundItem(id, generation, coerced);
          };
        }
        if (prop === "$record") {
          var recRaw = host.dataBoundRecord(id, generation);
          if (recRaw < 0) return makeNullDataHandle();
          return makeDataHandle(recRaw);
        }
        // Phase D-ι.2: `subformHandle.variables` returns the namespace object
        // holding all `<variables><script>` entries registered for this
        // subform by name. Enables `Page2.variables.ValidationScript.fn()`.
        if (prop === "variables") {
          var nodeName = host.nodeName(id, generation);
          if (typeof nodeName === "string" && nodeName.length > 0 &&
              subformVariables[nodeName] !== undefined) {
            return subformVariables[nodeName];
          }
          return Object.create(null);
        }
        // XFA 3.3 §6.4.3.2 underscore shorthand: `_<name>` on a subform
        // refers to the instanceManager of the same-named child subform.
        // Used in the wild as `parent._child.setInstances(N)`. This MUST
        // run before `shouldDeferHandleProperty`, which otherwise returns
        // `undefined` for every underscore-prefixed property and makes the
        // shorthand unreachable for real bound subforms.
        if (prop.charAt(0) === "_" && prop.length > 1) {
          var bareName = prop.substring(1);
          var imChildIds = uniqueNodeIds(host.resolveChildNodeIdsQuiet(String(id), bareName));
          if (imChildIds.length > 0) {
            return makeInstanceManager(imChildIds[0], generation);
          }
          if (host.hasZeroInstanceRun(id, generation, bareName)) {
            return makeEmptyInstanceManager();
          }
          // XFA-DATA-M3C: when the same-named child subform is absent from
          // the merged form tree (occur.initial=0, optional bind, or
          // never-instantiated schema option), Adobe still hands the
          // script an empty instanceManager so `parent._Foo.setInstances(N)`
          // is a chainable no-op. This sentinel is ONLY safe on container
          // handles (root, subform, area, subformSet, exclGroup) — XFA
          // 3.3 §6.4.3.2 limits the underscore-shorthand to those node
          // classes. Fields and draws stay `undefined` so existing
          // narrow-handle tests (m3b_phaseC_bindings::field_handle_is_frozen_and_narrow)
          // keep their property-isolation contract.
          if (host.nodeIsContainer(id, generation)) {
            return makeEmptyInstanceManager();
          }
        }
        // WP-3: choiceList is a listbox/combobox property not surfaced through
        // the FormTree. Return an empty frozen array.
        if (prop === "choiceList") {
          return Object.freeze([]);
        }
        // XFA-DATA-M3C: `<handle>.ui` exposes the widget-config sub-object.
        // We materialise a chainable stub (see makeUiStub) so scripts like
        // `field.ui.choiceList.commitOn = "exit"` complete without throwing.
        if (prop === "ui") {
          return makeUiStub();
        }
        // XFA-DATA-M3C: `<handle>.formattedValue` (Adobe SDK §JS A) reads
        // the field value formatted by its picture clause. Static flatten
        // has no live picture-clause formatter; return the raw value so
        // scripts can compare-and-branch without TypeError.
        if (prop === "formattedValue") {
          var fv = host.getRawValue(id, generation);
          return fv === undefined || fv === null ? "" : String(fv);
        }
        // XFA-DATA-M3C: `<handle>.execEvent("activity")` (Adobe SDK) fires
        // an event handler. Static flatten cannot dispatch new events
        // mid-script; absorb the call as a no-op returning undefined.
        if (prop === "execEvent") {
          return function() { return undefined; };
        }
        // XFA-DATA2-02: caption sub-element + resolveNode/resolveNodes
        // instance methods. Intercepted BEFORE shouldDeferHandleProperty +
        // makeChainHandle so the child-resolver host call (and its
        // resolve_failures bump) never happens. See captionSentinel /
        // handleResolveNode/Nodes definitions above for rationale.
        if (prop === "caption") {
          return captionSentinel;
        }
        if (prop === "resolveNode") {
          return handleResolveNode;
        }
        if (prop === "resolveNodes") {
          return handleResolveNodes;
        }
        if (shouldDeferHandleProperty(prop)) {
          return undefined;
        }
        return makeChainHandle([id], prop, generation);
      },
      set: function(_target, prop, value) {
        // WP-3: `value` is an alias for rawValue on the set side.
        if (prop === "rawValue" || prop === "value") {
          host.setRawValue(id, generation, value);
        }
        return true;
      },
      has: function(target, prop) {
        if (typeof prop !== "string") {
          return Reflect.has(target, prop);
        }
        return prop === "rawValue" ||
          prop === "somExpression" ||
          prop === "instanceManager" ||
          prop === "index" ||
          prop === "parent" ||
          prop === "ui" ||
          prop === "formattedValue" ||
          prop === "execEvent" ||
          prop === "setInstances" ||
          prop === "addInstance" ||
          prop === "removeInstance" ||
          prop === "isNull" ||
          prop === "clearItems" ||
          prop === "addItem" ||
          prop === "boundItem" ||
          prop === "caption" ||
          prop === "resolveNode" ||
          prop === "resolveNodes" ||
          Reflect.has(target, prop);
      }
    });
  }

  // Phase C-α: viewer-stub that absorbs property writes silently.
  // Used as the return value of `event.target.getField()` so
  // AcroForm widget-tweak scripts (`field.doNotScroll = true`,
  // `field.required = false`, etc.) complete without error and
  // without mutating any flatten-relevant state.
  function makeViewerStub() {
    return new Proxy({}, {
      get: function(_t, _prop) { return undefined; },
      set: function(_t, _prop, _val) {
        
        return true;
      },
      has: function() { return true; }
    });
  }

  function toListIndex(value) {
    var n = Number(value);
    if (!isFinite(n) || n < 0) return -1;
    return Math.floor(n);
  }

  // XFA §8.5: a `$record` reference when there is no bound data node must return
  // an empty object that chains safely (`.value` → null, `.nodes.length` → 0)
  // rather than null, which would throw TypeError on any property access.
  function makeNullDataHandle() {
    var emptyNodes = [];
    // XFA null-safe: item() on an empty sentinel list returns a chainable null handle,
    // not native null, so callers can do nodes.item(0).value without TypeError.
    emptyNodes.item = function() { return makeNullDataHandle(); };
    Object.freeze(emptyNodes);
    var sentinel = nullProtoObject();
    // WP-3: configurable:true so the Proxy set-trap can return true without
    // triggering the ECMAScript "non-configurable accessor without setter" invariant.
    Object.defineProperty(sentinel, "value",
      { get: function() { return null; }, enumerable: true, configurable: true });
    Object.defineProperty(sentinel, "rawValue",
      { get: function() { return null; }, enumerable: true, configurable: true });
    Object.defineProperty(sentinel, "length",
      { get: function() { return 0; }, enumerable: true, configurable: true });
    Object.defineProperty(sentinel, "nodes",
      { get: function() { return emptyNodes; }, enumerable: true, configurable: true });
    // WP-3: instance.index on an unbound node → 0
    Object.defineProperty(sentinel, "index",
      { get: function() { return 0; }, enumerable: true, configurable: true });
    sentinel.item = function() { return makeNullDataHandle(); };
    return new Proxy(sentinel, {
      get: function(target, prop) {
        if (prop in target) return target[prop];
        if (typeof prop !== "string") return undefined;
        return makeNullDataHandle();
      },
      // WP-3: absorb all writes — null data handles are read-only sentinels.
      set: function(_target, _prop, _value) {
        return true;
      }
    });
  }

  // Phase D-γ: Data DOM handle — wraps a raw DataDom node index and exposes
  // `.value`, `.nodes`, `.length`, `.item(i)`, and named child access via Proxy.
  function makeDataHandle(rawId) {
    // WP-3: return null-safe sentinel so callers can chain .value / .nodes safely.
    if (rawId === undefined || rawId < 0) return makeNullDataHandle();
    var handle = nullProtoObject();
    Object.defineProperty(handle, "value", {
      get: function() {
        var v = host.dataValue(rawId);
        return (v === undefined || v === null) ? null : v;
      },
      enumerable: true, configurable: false
    });
    // rawValue is an alias for value — scripts use both forms on data handles.
    Object.defineProperty(handle, "rawValue", {
      get: function() {
        var v = host.dataValue(rawId);
        return (v === undefined || v === null) ? null : v;
      },
      enumerable: true, configurable: false
    });
    Object.defineProperty(handle, "length", {
      get: function() { return host.dataChildren(rawId).length; },
      enumerable: true, configurable: false
    });
    Object.defineProperty(handle, "nodes", {
      get: function() {
        var ids = host.dataChildren(rawId);
        var arr = [];
        for (var i = 0; i < ids.length; i++) arr.push(makeDataHandle(ids[i]));
        // Phase D-γ fix: XFA scripts call `nodeList.item(i)` on the array
        // returned by `.nodes`. Plain JS arrays have no `.item()` method —
        // add one that mirrors the W3C NodeList API. Out-of-bounds indices
        // return a null-safe sentinel so `.value` access never throws.
        var NULLNODE = Object.freeze({ value: null, rawValue: null });
        arr.item = function(idx) {
          var index = toListIndex(idx);
          if (index < 0 || index >= arr.length) return NULLNODE;
          return arr[index];
        };
        return Object.freeze(arr);
      },
      enumerable: true, configurable: false
    });
    handle.item = function(i) {
      var ids = host.dataChildren(rawId);
      var index = toListIndex(i);
      if (index < 0 || index >= ids.length) return null;
      return makeDataHandle(ids[index]);
    };
    return new Proxy(handle, {
      get: function(target, prop) {
        if (prop in target || typeof prop !== "string") return target[prop];
        if (prop === "rawValue" || prop === "value") return target.value;
        var childId = host.dataChildByName(rawId, prop);
        if (childId < 0) return makeNullDataHandle();
        return makeDataHandle(childId);
      }
    });
  }

  // Phase E (XFA-JS-HOST-STUBS): `xfa.host` is the most heavily used Adobe
  // Reader viewer namespace. The previous frozen stub only exposed
  // `numPages` + `messageBox`, so initializer scripts that touch
  // `xfa.host.title = "..."`, `xfa.host.openList(...)`, `xfa.host.beep()`,
  // etc. raised TypeError on the very first property access and inflated
  // `runtime_errors`. We expose a Proxy that:
  //   * resolves known read-only viewer properties to deterministic defaults
  //     (numPages, version, language, platform, name, title, validationsEnabled,
  //     calculationsEnabled, currentPage, pageCount);
  //   * absorbs every other write silently (viewer-only state has no flatten
  //     side-effects);
  //   * returns a null-safe sentinel for unknown reads so chains like
  //     `xfa.host.someVendorExt.message` don't TypeError;
  //   * exposes the interactive function family (messageBox, openList, beep,
  //     response, print, gotoURL, setFocus, exportData, importData,
  //     resetData) as safe-default thunks that ALSO bump the
  //     `unsupported_host_calls` counter so dispatch keeps observability
  //     without claiming a fake success (no "user clicked OK" lies).
  // No filesystem / network / process syscalls reach the host — every
  // interactive thunk is a pure JS no-op that delegates to host.unsupported
  // for accounting only. See benchmarks/JS_SANDBOX_SECURITY_AUDIT.md.
  var XFA_HOST_INTERACTIVE_CALLS = {
    "messageBox":   { kind: "ret", value: 0 },
    "openList":     { kind: "ret", value: -1 },
    "beep":         { kind: "ret", value: undefined },
    "response":     { kind: "ret", value: "" },
    "print":        { kind: "ret", value: undefined },
    "gotoURL":      { kind: "ret", value: undefined },
    "setFocus":     { kind: "ret", value: undefined },
    "exportData":   { kind: "ret", value: undefined },
    "importData":   { kind: "ret", value: undefined },
    "resetData":    { kind: "ret", value: undefined },
    "documentCountInBatch": { kind: "ret", value: 1 },
    "documentInBatch":      { kind: "ret", value: 0 },
    // JS2-01 (Sprint 2 Batch B): closeDoc is referenced by Canadian IMM
    // and Quebec dol4n templates as an interactive viewer entry point.
    // Static flatten cannot dismiss a document; safe-default undefined
    // with `unsupported_host_calls` accounting keeps observability without
    // a TypeError. Pure JS no-op — no filesystem / network reach the host.
    "closeDoc":     { kind: "ret", value: undefined }
  };

  // Read-side defaults for viewer-readable host properties. These are pure
  // accessors — no host call leaves the sandbox. The values mirror Adobe
  // Reader behaviour during static, non-interactive rendering.
  function makeXfaHostBase() {
    var base = nullProtoObject();
    Object.defineProperty(base, "numPages", {
      enumerable: true, configurable: false,
      get: function() { return host.numPages(); }
    });
    Object.defineProperty(base, "currentPage", {
      enumerable: true, configurable: true,
      get: function() { return 0; },
      set: function(_v) { /* viewer-only */ }
    });
    Object.defineProperty(base, "pageCount", {
      enumerable: true, configurable: false,
      get: function() { return host.numPages(); }
    });
    // Static deterministic identity strings. We deliberately avoid claiming
    // Acrobat compatibility — scripts that branch on `xfa.host.name` for
    // Adobe-specific behaviour should fall through to a non-Acrobat path.
    var SCALAR_DEFAULTS = {
      "version":    "PDFluent-XFA",
      "language":   "ENU",
      "platform":   "PDFluent",
      "name":       "PDFluent",
      "title":      "",
      "appType":    "Reader",
      "variation":  "Reader",
      "calculationsEnabled": true,
      "validationsEnabled":  true,
      "runtimeHighlight":    false,
      "runtimeHighlightColor": "",
      "viewerType":          "PDFluent",
      "fullScreen":          false
    };
    Object.keys(SCALAR_DEFAULTS).forEach(function(k) {
      var v = SCALAR_DEFAULTS[k];
      Object.defineProperty(base, k, {
        enumerable: true, configurable: true,
        get: function() { return v; },
        set: function(_v) { /* viewer-only, silently absorb */ }
      });
    });
    return base;
  }

  // Install interactive function thunks. Each thunk increments the
  // unsupported-host-call counter and returns a deterministic safe default.
  function installXfaHostInteractive(base) {
    Object.keys(XFA_HOST_INTERACTIVE_CALLS).forEach(function(name) {
      var spec = XFA_HOST_INTERACTIVE_CALLS[name];
      Object.defineProperty(base, name, {
        enumerable: true, configurable: false, writable: false,
        value: function() {
          host.unsupportedHostCall(name);
          return spec.value;
        }
      });
    });
  }

  var xfaHostBase = makeXfaHostBase();
  installXfaHostInteractive(xfaHostBase);
  // Wrap in a Proxy so unknown property reads return a null-safe sentinel
  // and unknown writes silently absorb. This is the "no TypeError" guarantee
  // for vendor-specific xfa.host extensions referenced by templated scripts.
  var xfaHost = new Proxy(xfaHostBase, {
    get: function(target, prop) {
      if (prop in target || typeof prop !== "string") return target[prop];
      // Unknown read — never throw, never claim a value. Sentinel only.
      return makeNullDataHandle();
    },
    set: function(_t, _p, _v) { return true; },
    has: function() { return true; }
  });

  var xfaLayout = nullProtoObject();
  Object.defineProperty(xfaLayout, "pageCount", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: function() {
      return host.numPages();
    }
  });
  // Phase D-γ: xfa.layout.page(node) — page number (1-based) of a form node.
  // During static flatten the layout is not yet run, so return a bounded
  // placeholder and mark the metadata as approximate.
  Object.defineProperty(xfaLayout, "page", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: function() {
      host.resolveFailure();
      return 1;
    }
  });
  // Phase D-γ: xfa.layout.pageSpan(node) — number of pages a node spans.
  // Always 1 during static flatten; metadata records the approximation.
  Object.defineProperty(xfaLayout, "pageSpan", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: function() {
      host.resolveFailure();
      return 1;
    }
  });
  Object.defineProperty(xfaLayout, "absPage", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: function() {
      host.bindingError();
      return null;
    }
  });

  var xfa = nullProtoObject();
  Object.defineProperty(xfa, "host", {
    enumerable: true,
    configurable: false,
    writable: false,
    // Note: deliberately NOT Object.freeze'd. `xfaHost` is already a Proxy
    // with sealed semantics (writes absorbed, unknown reads return sentinels).
    // Freezing would force every Set trap to throw under strict-mode scripts.
    value: xfaHost
  });
  Object.defineProperty(xfa, "layout", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: Object.freeze(xfaLayout)
  });

  // Phase E (XFA-JS-HOST-STUBS): viewer / interaction sub-namespaces. Each
  // is a Proxy that silently absorbs writes and returns chainable sentinels
  // on read so scripts can complete:
  //
  //   xfa.viewer        — Reader UI state (zoom, scrollbar, toolbar, ...)
  //   xfa.appState      — Reader-wide preference cache
  //   xfa.appearanceFilter — accessibility / high-contrast hints
  //   xfa.connection    — `<connection>` outbound data binding stubs
  //   xfa.signature     — interactive digital-signature panel
  //   xfa.aliasNode     — script-time DOM alias rebinding
  //   xfa.form          — top-level FormDOM root reference (we expose the
  //                       same resolveNode/resolveNodes pair so the script
  //                       feels uniform; legacy property reads sentinel)
  //
  // These never call out to the host other than for accounting; no
  // filesystem / network / process syscalls reach the host bindings. Reads
  // for known property names return spec-conforming defaults; everything
  // else falls back to the null-safe sentinel chain.
  function makeViewerStubNamespace(staticReads) {
    var base = nullProtoObject();
    if (staticReads) {
      Object.keys(staticReads).forEach(function(k) {
        var v = staticReads[k];
        Object.defineProperty(base, k, {
          enumerable: true, configurable: true,
          get: function() { return v; },
          set: function(_v) { /* viewer-only — silent absorb */ }
        });
      });
    }
    return new Proxy(base, {
      get: function(target, prop) {
        if (prop in target || typeof prop !== "string") return target[prop];
        return makeNullDataHandle();
      },
      set: function(_t, _p, _v) { return true; },
      has: function() { return true; }
    });
  }

  function makeInteractiveStubNamespace(funcs) {
    // funcs: { "sign": defaultRet, "verify": defaultRet, ... }
    var base = nullProtoObject();
    Object.keys(funcs).forEach(function(name) {
      var def = funcs[name];
      Object.defineProperty(base, name, {
        enumerable: true, configurable: false, writable: false,
        value: function() {
          host.unsupportedHostCall(name);
          return def;
        }
      });
    });
    return new Proxy(base, {
      get: function(target, prop) {
        if (prop in target || typeof prop !== "string") return target[prop];
        return makeNullDataHandle();
      },
      set: function(_t, _p, _v) { return true; },
      has: function() { return true; }
    });
  }

  Object.defineProperty(xfa, "viewer", {
    enumerable: true, configurable: false, writable: false,
    value: makeViewerStubNamespace({
      "zoomType":  "FitWidth",
      "zoom":      100,
      "scrollbar": "auto",
      "toolbar":   true,
      "menubar":   true,
      "statusbar": true
    })
  });
  Object.defineProperty(xfa, "appState", {
    enumerable: true, configurable: false, writable: false,
    value: makeViewerStubNamespace({
      "highlightRequiredFields": false,
      "fieldHighlightColor":     "",
      "ariaEnabled":             false
    })
  });
  Object.defineProperty(xfa, "appearanceFilter", {
    enumerable: true, configurable: false, writable: false,
    value: makeViewerStubNamespace(null)
  });
  Object.defineProperty(xfa, "aliasNode", {
    enumerable: true, configurable: false, writable: false,
    value: makeViewerStubNamespace(null)
  });
  Object.defineProperty(xfa, "connection", {
    enumerable: true, configurable: false, writable: false,
    // `<connection>` calls hit a real service endpoint in Adobe Reader.
    // Static flatten cannot honour that — every method is unsupported.
    value: makeInteractiveStubNamespace({
      "execute":   null,
      "open":      null,
      "close":     null,
      "send":      null
    })
  });
  Object.defineProperty(xfa, "signature", {
    enumerable: true, configurable: false, writable: false,
    // Digital signatures require an interactive certificate picker.
    value: makeInteractiveStubNamespace({
      "sign":     false,
      "verify":   "unknown",
      "enumerate": ""
    })
  });
  // WP-3 F3: `xfa.validate` is a viewer-only namespace controlling form-wide
  // validation behaviour (Reader UI prompts on field constraint violations).
  // Adobe initializers commonly do `xfa.validate.override = 0` /
  // `xfa.validate.max = N` to suppress modal dialogs that have no meaning
  // in a static flatten context. Expose a Proxy that silently absorbs every
  // property write (`override`, `max`, `messageMode`, etc.) and returns
  // empty defaults on read, so scripts complete without TypeError.
  Object.defineProperty(xfa, "validate", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: new Proxy(nullProtoObject(), {
      get: function(_t, prop) {
        if (prop === "override" || prop === "max") return 0;
        if (prop === "messageMode") return "";
        if (typeof prop !== "string") return undefined;
        // Unknown reads return a chainable null-safe sentinel so deeper
        // access like `xfa.validate.scriptTest.message` does not throw.
        return makeNullDataHandle();
      },
      set: function(_t, _prop, _value) { return true; },
      has: function() { return true; }
    })
  });
  Object.defineProperty(xfa, "resolveNode", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: function(path) {
      // Phase D-γ: data paths are routed to the DataDom, not the FormTree.
      if (typeof path === "string" &&
          (path.indexOf("data.") === 0 || path.indexOf("$data.") === 0 ||
           path.indexOf("xfa.datasets.data.") === 0)) {
        var rawId = host.dataResolveNode(path);
        if (rawId < 0) return null;
        return makeDataHandle(rawId);
      }
      var id = host.resolveNodeId(path);
      if (id < 0) {
        return null;
      }
      return makeHandle(id, host.generation());
    }
  });
  // JS2-01 (Sprint 2 Batch B): `xfa.form` is the FormDOM root surface
  // Adobe Reader exposes (XFA 3.3 §6.1.4 — the result of merging template
  // + datasets). The previous build only commented that it should be
  // exposed (rquickjs_backend.rs:2477) but no actual binding existed —
  // scripts that did `xfa.form.resolveNode("Form1.Subform1")` or
  // `form.resolveNode(...)` ran into "cannot read property of undefined"
  // or "form is not defined". We expose a Proxy that:
  //   * forwards `resolveNode` / `resolveNodes` to the existing
  //     `xfa.resolveNode` / `xfa.resolveNodes` pair (cluster C contract
  //     preserved — missing paths still return `null`, not a sentinel);
  //   * absorbs unknown property writes (viewer-only flags) silently;
  //   * returns a chainable null-safe sentinel for unknown reads.
  // No new Rust closure is registered; the binding is pure JS routing.
  function makeFormNamespace() {
    var base = nullProtoObject();
    Object.defineProperty(base, "resolveNode", {
      enumerable: true, configurable: false, writable: false,
      value: function(path) { return xfa.resolveNode(path); }
    });
    Object.defineProperty(base, "resolveNodes", {
      enumerable: true, configurable: false, writable: false,
      value: function(path) { return xfa.resolveNodes(path); }
    });
    // Adobe's `form` exposes `recalculate(true/false)` as a no-op trigger
    // for the calculation cascade. Templates pass `1` (force) commonly.
    // Static flatten already ran calculate scripts — accept the call,
    // return undefined, no counter (it is not an interactive prompt).
    Object.defineProperty(base, "recalculate", {
      enumerable: true, configurable: false, writable: false,
      value: function() { return undefined; }
    });
    Object.defineProperty(base, "execValidate", {
      enumerable: true, configurable: false, writable: false,
      value: function() { return true; }
    });
    Object.defineProperty(base, "execInitialize", {
      enumerable: true, configurable: false, writable: false,
      value: function() { return undefined; }
    });
    Object.defineProperty(base, "execCalculate", {
      enumerable: true, configurable: false, writable: false,
      value: function() { return undefined; }
    });
    return new Proxy(base, {
      get: function(target, prop) {
        if (prop in target || typeof prop !== "string") return target[prop];
        return makeNullDataHandle();
      },
      set: function(_t, _p, _v) { return true; },
      has: function() { return true; }
    });
  }
  var formNamespace = makeFormNamespace();
  Object.defineProperty(xfa, "form", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: formNamespace
  });
  Object.defineProperty(xfa, "resolveNodes", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: function(path) {
      // Phase D-γ: data paths are routed to the DataDom, not the FormTree.
      if (typeof path === "string" &&
          (path.indexOf("data.") === 0 || path.indexOf("$data.") === 0 ||
           path.indexOf("xfa.datasets.data.") === 0)) {
        var rawIds = host.dataResolveNodes(path);
        var out = [];
        for (var i = 0; i < rawIds.length; i++) out.push(makeDataHandle(rawIds[i]));
        return Object.freeze(out);
      }
      var generation = host.generation();
      var ids = host.resolveNodeIds(path);
      var out = [];
      for (var i = 0; i < ids.length; i++) {
        out.push(makeHandle(ids[i], generation));
      }
      return Object.freeze(out);
    }
  });
  // Phase D-δ.2: expose `xfa.event` as an alias for the per-script event
  // global. Real Adobe Reader populates this with the firing event;
  // during static flatten there is no dispatched UI event, so the
  // accessor returns the same defensive event-stub that the per-script
  // `event` parameter receives — newText/prevText/change default to
  // empty strings, target resolves to the firing field handle.
  Object.defineProperty(xfa, "event", {
    enumerable: true,
    configurable: false,
    get: function() {
      return makeEvent();
    }
  });

  // Phase E (XFA-JS-HOST-STUBS): Acrobat / Reader `app` global. Adobe
  // initializer scripts commonly do:
  //   app.calculate.override = true;       // viewer-only flag — silent absorb
  //   app.runtimeHighlight   = false;      // viewer-only flag — silent absorb
  //   app.alert("submitted"); app.launchURL("…")  // interactive — counted
  //
  // The previous frozen `app` only stubbed two functions; every other
  // property touch surfaced as TypeError. Wrap the whole namespace in a
  // Proxy with the same semantics as `xfa.host`:
  //   * known viewer properties resolve to deterministic defaults;
  //   * known interactive functions return a safe default + bump
  //     unsupported_host_calls (NOT runtime_errors);
  //   * unknown reads return a null-safe sentinel chain;
  //   * unknown writes are absorbed.
  //
  // `app.calculate` is itself an absorbing sub-Proxy — every assignment
  // (`app.calculate.override = true`, `app.calculate.suspend = false`, ...)
  // is viewer-only and has no flatten side-effect. We document this as an
  // explicit silent no-op (not UnsupportedHostCapability) because
  // suppressing the viewer's calculation cascade is the script author's
  // ASKING the viewer to stop, not a UI prompt to the user. See
  // benchmarks/JS_SANDBOX_SECURITY_AUDIT.md for the full classification.
  var APP_INTERACTIVE_CALLS = {
    "alert":       { kind: "ret", value: 0 },     // dialog OK pressed = 0
    "launchURL":   { kind: "ret", value: undefined },
    "execMenuItem":{ kind: "ret", value: undefined },
    "beep":        { kind: "ret", value: undefined },
    "openDoc":     { kind: "ret", value: null },
    "response":    { kind: "ret", value: "" },
    "mailMsg":     { kind: "ret", value: undefined },
    // JS2-01 (Sprint 2 Batch B): Adobe Acrobat SDK exposes `app.messageBox`
    // as a richer alias of `app.alert` (modal dialog, returns the pressed
    // button index — 0 = OK). Safe default mirrors `app.alert`. No new
    // capability surface; pure JS no-op with counter accounting.
    "messageBox":  { kind: "ret", value: 0 },
    // JS2-01: `app.closeDoc` is the Acrobat alias of `xfa.host.closeDoc`.
    // Templates targeting both Reader and Acrobat reference both spellings;
    // we counter-bump on each call and return undefined.
    "closeDoc":    { kind: "ret", value: undefined }
  };
  function makeAppBase() {
    var base = nullProtoObject();
    // Read-side static defaults — never call out, never lie about identity.
    var SCALAR_DEFAULTS = {
      "viewerType":     "PDFluent",
      "viewerVariation":"Reader",
      "viewerVersion":  0,
      "language":       "ENU",
      "platform":       "PDFluent",
      "fs":             null,  // file system gateway — explicitly null
      "media":          null,  // multimedia controller — explicitly null
      "fullscreen":     false,
      "runtimeHighlight": false,
      "runtimeHighlightColor": ""
    };
    Object.keys(SCALAR_DEFAULTS).forEach(function(k) {
      var v = SCALAR_DEFAULTS[k];
      Object.defineProperty(base, k, {
        enumerable: true, configurable: true,
        get: function() { return v; },
        set: function(_v) { /* viewer-only, silently absorb */ }
      });
    });
    // app.calculate sub-namespace: writes absorb, reads return defaults.
    Object.defineProperty(base, "calculate", {
      enumerable: true, configurable: false, writable: false,
      value: makeViewerStubNamespace({
        "override": true,
        "suspend":  false
      })
    });
    // Install interactive function thunks (same pattern as xfa.host).
    Object.keys(APP_INTERACTIVE_CALLS).forEach(function(name) {
      var spec = APP_INTERACTIVE_CALLS[name];
      Object.defineProperty(base, name, {
        enumerable: true, configurable: false, writable: false,
        value: function() {
          host.unsupportedHostCall(name);
          return spec.value;
        }
      });
    });
    return base;
  }
  var app = new Proxy(makeAppBase(), {
    get: function(target, prop) {
      if (prop in target || typeof prop !== "string") return target[prop];
      return makeNullDataHandle();
    },
    set: function(_t, _p, _v) { return true; },
    has: function() { return true; }
  });

  // Phase C-α: viewer-only `event` global. Real Adobe Reader populates
  // this with the firing event; during static flatten there is no
  // dispatched UI event, so the object is a defensive stub that:
  // - exposes `target` resolving to the firing field handle (≈ `this`),
  //   so scripts like `event.target.getField(somPath)` complete without
  //   ReferenceError;
  // - exposes `change` as an empty string (the spec default);
  // - returns viewer-stubs from `target.getField()` so AcroForm widget
  //   tweaks (`doNotScroll`, `required`, etc.) silently absorb.
  function makeEvent() {
    var id = host.currentNodeId();
    var fieldHandle = id < 0 ? null : makeHandle(id, host.generation());
    var target = nullProtoObject();
    Object.defineProperty(target, "getField", {
      enumerable: true, configurable: false, writable: false,
      value: function() {
        
        return makeViewerStub();
      }
    });
    Object.defineProperty(target, "name", {
      enumerable: true, configurable: false,
      get: function() { return ""; }
    });
    Object.defineProperty(target, "self", {
      enumerable: true, configurable: false,
      get: function() { return fieldHandle; }
    });
    var ev = nullProtoObject();
    Object.defineProperty(ev, "target", {
      enumerable: true, configurable: false,
      get: function() { return target; }
    });
    // Phase D-δ.2: stable empty-string defaults for the change-event property
    // family so scripts that read `xfa.event.newText` / `prevText` /
    // `change` on initialize/calculate (where no real change event occurred)
    // do not throw `cannot read property 'X' of undefined`. Adobe populates
    // these on actual `change`/`exit` events; we run those activities later
    // (or never) and surface deterministic empty defaults instead.
    Object.defineProperty(ev, "change", {
      enumerable: true, configurable: false,
      get: function() { return ""; }
    });
    Object.defineProperty(ev, "newText", {
      enumerable: true, configurable: false,
      get: function() { return ""; }
    });
    Object.defineProperty(ev, "prevText", {
      enumerable: true, configurable: false,
      get: function() { return ""; }
    });
    Object.defineProperty(ev, "fullText", {
      enumerable: true, configurable: false,
      get: function() { return ""; }
    });
    Object.defineProperty(ev, "selStart", {
      enumerable: true, configurable: false,
      get: function() { return 0; }
    });
    Object.defineProperty(ev, "selEnd", {
      enumerable: true, configurable: false,
      get: function() { return 0; }
    });
    // JS2-01 (Sprint 2 Batch B): the Phase E security audit §3.9 promised
    // that `event` exposes the full Adobe Acrobat SDK event surface
    // (`target`, `change`, `newText`, `prevText`, `fullText`, `selStart`,
    // `selEnd`, plus `name`, `type`, `shift`, `modifier`, `commitKey`,
    // `willCommit`, `rc`, `keyDown`, `value`, `reenter`). Only the first
    // seven were implemented; scripts that probe `event.name` for the
    // firing activity (`if (event.name === "calculate") {...}`) silently
    // saw `undefined`, and scripts that chained off `event.X` could trip
    // a "not a function" if `X` was expected to be callable.
    //
    // All values are deterministic spec defaults that match Adobe's
    // behaviour during a non-interactive flatten (no firing event):
    //   name       — current activity if known, else ""
    //   type       — event type label (Adobe ≈ "Field"), "" if unknown
    //   shift      — modifier-key state (false)
    //   modifier   — modifier-key state (false)
    //   commitKey  — 0 (no commit key pressed)
    //   willCommit — false (no commit pending)
    //   rc         — true (accept-by-default — same as Adobe's validate)
    //   keyDown    — false (no key pressed)
    //   value      — "" (current widget value, unknown during static flatten)
    //   reenter    — false (Acrobat re-entry flag)
    //
    // Read-only via getter, like the existing seven. The frozen ev object
    // means writes to these from scripts running in strict mode would throw
    // a TypeError — but no real script writes event scalars (the only
    // observed pattern is `event.value = ...` on a stale `event` reference
    // which is undefined, not the frozen object). If we ever observe
    // legitimate writes, swap getter for configurable getter+setter.
    Object.defineProperty(ev, "name", {
      enumerable: true, configurable: false,
      get: function() { return ""; }
    });
    Object.defineProperty(ev, "type", {
      enumerable: true, configurable: false,
      get: function() { return ""; }
    });
    Object.defineProperty(ev, "shift", {
      enumerable: true, configurable: false,
      get: function() { return false; }
    });
    Object.defineProperty(ev, "modifier", {
      enumerable: true, configurable: false,
      get: function() { return false; }
    });
    Object.defineProperty(ev, "commitKey", {
      enumerable: true, configurable: false,
      get: function() { return 0; }
    });
    Object.defineProperty(ev, "willCommit", {
      enumerable: true, configurable: false,
      get: function() { return false; }
    });
    Object.defineProperty(ev, "rc", {
      enumerable: true, configurable: false,
      get: function() { return true; }
    });
    Object.defineProperty(ev, "keyDown", {
      enumerable: true, configurable: false,
      get: function() { return false; }
    });
    Object.defineProperty(ev, "value", {
      enumerable: true, configurable: false,
      get: function() { return ""; }
    });
    Object.defineProperty(ev, "reenter", {
      enumerable: true, configurable: false,
      get: function() { return false; }
    });
    return Object.freeze(ev);
  }

  // Phase D-γ: XFA global `util` (Acrobat SDK §Util).  Provides date/number
  // formatting helpers used by many XFA templates. Only the subset required
  // by real-corpus scripts is implemented; unknown methods return "".
  //
  // util.printd(sFormat, dDate)  — format a Date per sFormat using UTC fields.
  //   Supported tokens: yyyy (year), yy (two-digit year), mm (month 01-12),
  //   m (1-12), dd (day 01-31), d (1-31), HH (hour 00-23),
  //   MM (minute 00-59), SS (second 00-59).
  // util.printx(cPicture, cValue) — picture format; returns cValue as-is.
  // util.scand(sFormat, cDate)   — deterministic numeric parser for the same
  //   token subset; unsupported or invalid input returns Invalid Date.
  var xfaUtil = (function() {
    var DateCtor = Date;
    var dateUtc = Date.UTC;
    function pad2(n) { return (n < 10 ? "0" : "") + n; }
    function invalidDate() { return new DateCtor(NaN); }
    function escapeRegex(text) {
      return String(text).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    }
    function parseDate(fmt, input) {
      var fmtText, text;
      try {
        fmtText = String(fmt);
        text = String(input);
      } catch (_e) {
        return invalidDate();
      }

      var groups = [];
      var pattern = "^";
      for (var i = 0; i < fmtText.length;) {
        var rest = fmtText.substring(i);
        if (rest.indexOf("yyyy") === 0) {
          pattern += "(\\d{4})";
          groups.push("yyyy");
          i += 4;
        } else if (rest.indexOf("yy") === 0) {
          pattern += "(\\d{2})";
          groups.push("yy");
          i += 2;
        } else if (rest.indexOf("mm") === 0) {
          pattern += "(\\d{2})";
          groups.push("mm");
          i += 2;
        } else if (rest.indexOf("dd") === 0) {
          pattern += "(\\d{2})";
          groups.push("dd");
          i += 2;
        } else if (rest.indexOf("HH") === 0) {
          pattern += "(\\d{2})";
          groups.push("HH");
          i += 2;
        } else if (rest.indexOf("MM") === 0) {
          pattern += "(\\d{2})";
          groups.push("MM");
          i += 2;
        } else if (rest.indexOf("SS") === 0) {
          pattern += "(\\d{2})";
          groups.push("SS");
          i += 2;
        } else if (rest.indexOf("m") === 0) {
          pattern += "(\\d{1,2})";
          groups.push("m");
          i += 1;
        } else if (rest.indexOf("d") === 0) {
          pattern += "(\\d{1,2})";
          groups.push("d");
          i += 1;
        } else {
          pattern += escapeRegex(fmtText.charAt(i));
          i += 1;
        }
      }

      var match = new RegExp(pattern + "$").exec(text);
      if (!match) return invalidDate();

      var year = NaN, month = 1, day = 1, hour = 0, minute = 0, second = 0;
      for (var g = 0; g < groups.length; g++) {
        var value = parseInt(match[g + 1], 10);
        if (!isFinite(value)) return invalidDate();
        if (groups[g] === "yyyy") year = value;
        else if (groups[g] === "yy") year = 2000 + value;
        else if (groups[g] === "mm" || groups[g] === "m") month = value;
        else if (groups[g] === "dd" || groups[g] === "d") day = value;
        else if (groups[g] === "HH") hour = value;
        else if (groups[g] === "MM") minute = value;
        else if (groups[g] === "SS") second = value;
      }

      if (!isFinite(year) || month < 1 || month > 12 || day < 1 || day > 31 ||
          hour < 0 || hour > 23 || minute < 0 || minute > 59 ||
          second < 0 || second > 59) {
        return invalidDate();
      }
      var out = new DateCtor(dateUtc(year, month - 1, day, hour, minute, second));
      if (out.getUTCFullYear() !== year ||
          out.getUTCMonth() + 1 !== month ||
          out.getUTCDate() !== day ||
          out.getUTCHours() !== hour ||
          out.getUTCMinutes() !== minute ||
          out.getUTCSeconds() !== second) {
        return invalidDate();
      }
      return out;
    }
    var u = nullProtoObject();
    u.printd = function(fmt, date) {
      if (!(date instanceof DateCtor) || isNaN(date.getTime())) return "";
      var y = date.getUTCFullYear();
      var mo = date.getUTCMonth() + 1;
      var d  = date.getUTCDate();
      var h  = date.getUTCHours();
      var mi = date.getUTCMinutes();
      var s  = date.getUTCSeconds();
      var result = String(fmt);
      result = result.replace(/yyyy/g, y)
                     .replace(/yy/g,   String(y).slice(-2))
                     .replace(/mm/g,   pad2(mo))
                     .replace(/m/g,    mo)
                     .replace(/dd/g,   pad2(d))
                     .replace(/d/g,    d)
                     .replace(/HH/g,   pad2(h))
                     .replace(/MM/g,   pad2(mi))
                     .replace(/SS/g,   pad2(s));
      return result;
    };
    u.printx = function(_fmt, val) { return val === null || val === undefined ? "" : String(val); };
    u.scand  = function(fmt, str) { return parseDate(fmt, str); };
    return Object.freeze(u);
  }());

  // Phase C-α: minimal `console` no-op. Many forms guard with
  // `if (typeof console !== "undefined") console.log(...)` and proceed
  // when the symbol exists. Stub returns undefined; never writes
  // anywhere observable to the script.
  var consoleStub = nullProtoObject();
  ["log","warn","error","info","debug","trace"].forEach(function(name) {
    Object.defineProperty(consoleStub, name, {
      enumerable: true, configurable: false, writable: false,
      value: function() {  return undefined; }
    });
  });

  // Phase D-ι: form-level globals registered from `<variables>` `<script>`
  // blocks. Each entry is `name -> frozen object`. Populated by the host
  // once per document via `setVariablesScript`; cleared by
  // `clearVariablesScripts` at `reset_per_document`.
  var variablesScripts = lookupObject();
  // Phase D-ι.2: subform-scoped variables. Maps subform name -> namespace
  // object containing that subform's named scripts. Enables
  // `subformHandle.variables.ScriptName.method()` access paths.
  var subformVariables = lookupObject();
  // W3-D RETRY: form-level mutable string data items registered from
  // `<variables>` `<text name="X">value</text>` blocks (XFA 3.3 §5.5.2).
  // Each entry is `name -> { value: <string> }`. Adobe Reader exposes these
  // as global mutable string containers — the canonical IMM5709 pattern
  // is `<text name="globValidatePressed"/>` referenced from event scripts
  // as `globValidatePressed.value = "true";`. Populated by
  // `setVariablesDataItem`; cleared by `clearVariablesScripts` (shared
  // teardown — same per-document lifecycle).
  var variablesDataItems = lookupObject();
  // W3-D RETRY: subform-scoped variant of `variablesDataItems`, parallel to
  // `subformVariables` for scripts. Reserved for future subform-level data
  // items; currently populated only at root scope but the structure is
  // here so an upcoming pass can light it up without further refactor.
  var subformVariablesDataItems = lookupObject();

  function makeImplicitGlobals(body) {
    var currentId = host.currentNodeId();
    var generation = host.generation();
    var localNames = collectLocalNames(String(body));
    var cachedHandles = lookupObject();
    var dynamicLocals = lookupObject();

    function lookup(name) {
      if (cachedHandles[name] !== undefined) {
        return cachedHandles[name];
      }
      var nodeIds = uniqueNodeIds(host.resolveImplicitNodeIds(currentId, name));
      if (nodeIds.length === 0) {
        return undefined;
      }
      // Phase D-θ.2: wrap an implicit hit in the full-chain accumulator. The
      // resulting proxy accumulates SOM property names without contacting
      // the host until a terminal property is read; only then is the full
      // chain resolved with backtracking. This lets `A.B.C.D.rawValue`
      // disambiguate the same-name `A` whose subtree completes the chain,
      // rather than relying on a single-segment hint as in D-θ.1. Terminal
      // properties still resolve byte-for-byte identically to the
      // un-hinted walk so single-token reads keep their existing
      // semantics.
      var handle = makeChainProxy([], [name], nodeIds, generation, currentId);
      cachedHandles[name] = handle;
      return handle;
    }

    return new Proxy(Object.create(null), {
      has: function(_target, prop) {
        if (typeof prop !== "string") {
          return false;
        }
        // XFA-DATA-M3C: `_<Name>` and `parent` are XFA-defined globals that
        // must be visible to `with()` lookup even when the local-names
        // collector treats them as locals (they shadow no real var/let).
        if (prop === "parent") {
          return true;
        }
        if (prop.charAt(0) === "_" && prop.length > 1 &&
            localNames[prop] !== true) {
          // Probe — only claim presence when the bare-name resolves to a
          // real subform/container; otherwise fall through to the standard
          // defer rules so unrelated underscored locals stay undefined.
          // Quiet probe: a miss here is the schema-optional path and must
          // not count as a script-level resolve failure.
          var bare = prop.substring(1);
          if (uniqueNodeIds(host.resolveImplicitNodeIdsQuiet(currentId, bare)).length > 0 ||
              host.hasZeroInstanceRun(currentId, generation, bare)) {
            return true;
          }
        }
        if (shouldDeferGlobalName(prop, localNames)) {
          return false;
        }
        return true;
      },
      get: function(_target, prop) {
        if (typeof prop !== "string") {
          return undefined;
        }
        // XFA-DATA-M3C: bare `parent` resolves to the form-tree parent of
        // the current script node. Common in calculate/initialize scripts
        // for `parent.index`, `parent.rawValue`, and chain navigation that
        // mirrors Adobe's implicit-scope walk one step upward.
        if (prop === "parent") {
          var parentId = host.parentOfNode(currentId, generation);
          if (parentId < 0) return undefined;
          return makeHandle(parentId, generation);
        }
        // XFA 3.3 §6.4.3.2 underscore shorthand at the global scope:
        // `_<Name>` referenced as a bare identifier denotes the
        // instanceManager of a same-named subform reachable from the
        // current implicit scope. Adobe Reader exposes this both as a
        // child property (`parent._Foo`) and as a bare global. Without
        // this branch the `shouldDeferGlobalName` rule below returns
        // `undefined` for every underscored bare ident and the calling
        // script throws `ReferenceError: _Foo is not defined`.
        //
        // Only triggers when the target subform actually exists; otherwise
        // fall through so unrelated underscored locals keep their
        // existing deferred-undefined semantics.
        if (prop.charAt(0) === "_" && prop.length > 1 &&
            localNames[prop] !== true) {
          var bareName = prop.substring(1);
          // Quiet probe: schema-optional misses are NOT a script-level
          // resolve failure. We surface either a live manager, an empty
          // manager (zero-instance run or never-instantiated optional
          // subform), or fall through to the defer rules.
          var imIds = uniqueNodeIds(
            host.resolveImplicitNodeIdsQuiet(currentId, bareName)
          );
          if (imIds.length > 0) {
            return makeInstanceManager(imIds[0], generation);
          }
          if (host.hasZeroInstanceRun(currentId, generation, bareName)) {
            return makeEmptyInstanceManager();
          }
        }
        if (shouldDeferGlobalName(prop, localNames)) {
          return undefined;
        }
        // Phase D-γ: $record as a script-level global refers to the data
        // record bound to the current field's enclosing subform context.
        // Scripts write `var addr = $record.SECTION.nodes;` — we intercept
        // this here instead of letting resolveImplicitNodeId fail (-1).
        if (prop === "$record") {
          var recRaw = host.dataBoundRecord(currentId, generation);
          if (recRaw < 0) return makeNullDataHandle();
          return makeDataHandle(recRaw);
        }
        // Phase D-γ: `util` is an XFA global (Acrobat SDK §Util) that provides
        // date/number formatting functions.  `util.printd(fmt, date)` is widely
        // used by XFA templates to format Date objects.  We intercept it here so
        // scripts can complete without a TypeError instead of throwing and
        // aborting all later mutations in the same script body.
        if (prop === "util") {
          return xfaUtil;
        }
        if (dynamicLocals[prop] !== undefined) {
          return dynamicLocals[prop];
        }
        // Phase D-ι: form-level named-script globals from <variables>
        // outrank the form-tree implicit lookup. Adobe XFA spec §5.5
        // exposes `<scriptName>.<topLevelDecl>` to all event/calculate
        // scripts in the same document.
        if (variablesScripts[prop] !== undefined) {
          return variablesScripts[prop];
        }
        // W3-D RETRY: form-level data items (XFA 3.3 §5.5.2) outrank the
        // form-tree implicit lookup, same as scripts. This lets templates
        // like Canadian IMM5709 reference `globValidatePressed.value`
        // without the bare ident falling through to the SOM resolver as a
        // `js_resolve_failure`. Scripts take precedence over data items in
        // the unlikely case both share a name (spec-undefined; we follow
        // declaration order, which is `setVariablesScript` first in the
        // host loop).
        if (variablesDataItems[prop] !== undefined) {
          return variablesDataItems[prop];
        }
        return lookup(prop);
      },
      set: function(_target, prop, value) {
        if (typeof prop !== "string") {
          return true;
        }
        if (shouldDeferGlobalName(prop, localNames)) {
          return false;
        }
        if (cachedHandles[prop] !== undefined) {
          return true;
        }
        dynamicLocals[prop] = value;
        return true;
      }
    });
  }

  return {
    // Phase E (XFA-JS-HOST-STUBS): `xfa` itself is still frozen because all
    // of its properties were installed with `configurable: false` and writes
    // go through inner-Proxy `set` traps that we control. `app` is a Proxy
    // whose underlying target is non-extensible after we install the
    // function thunks; freezing the Proxy itself would force the absorbing
    // `set` trap to throw a TypeError (proxy invariants: writes to
    // non-extensible targets must reject), defeating the whole point of the
    // silent-absorb design. We therefore expose `app` unfrozen — its safety
    // is enforced by the Proxy traps, not by Object.freeze.
    xfa: Object.freeze(xfa),
    app: app,
    // JS2-01 (Sprint 2 Batch B): top-level `form` global alias for
    // `xfa.form`. Adobe Reader exposes both spellings; templates such as
    // 60df78fe_pdf_0012 reference bare `form.X`. The alias is identity-equal
    // to `xfa.form` (same Proxy), so cluster C contract preservation is
    // shared between the two surfaces. No new Rust closure introduced.
    form: formNamespace,
    consoleStub: Object.freeze(consoleStub),
    // Phase D-ι: register a `<variables>` `<script name="X">…` block as a
    // form-level global. Called by the host once per script body at
    // document load. `body` is the raw script source; `identNames` is a
    // pre-extracted array of top-level `var` / `function` identifiers
    // (Rust-side regex). The body is wrapped in an IIFE that returns a
    // frozen object whose properties are those identifiers. Variables
    // scripts share the same time/memory budget enforcement as event
    // scripts but emit no field mutations of their own. Errors during
    // evaluation are absorbed: the namespace remains undefined and
    // dependent event scripts will fail naturally at first use.
    // Phase D-ι / D-ι.2: register a named `<variables><script>` body.
    // `subformName` (4th param, optional) is non-empty for subform-scoped
    // scripts; omit or pass "" for root-level scripts.
    //
    // Root-level scripts (empty subformName) go into the flat
    // `variablesScripts` dict only — accessible as `ScriptName.X` from
    // any event script in the document.
    //
    // Subform-scoped scripts go into `subformVariables[subformName][name]`
    // ONLY — accessible as `subformHandle.variables.ScriptName.X`. They
    // are intentionally NOT written to the flat dict: two subforms may
    // define the same script name, and writing both to the flat map would
    // let the second registration silently shadow the first.
    setVariablesScript: function(name, body, identNames, subformName) {
      if (typeof name !== "string" || name.length === 0) return false;
      if (typeof body !== "string") return false;
      var idents = Array.isArray(identNames) ? identNames : [];
      var props = "";
      for (var i = 0; i < idents.length; i++) {
        var id = idents[i];
        if (typeof id !== "string" || id.length === 0) continue;
        if (i > 0) props += ",";
        props += JSON.stringify(id) + ": typeof " + id +
                 " !== \"undefined\" ? " + id + " : undefined";
      }
      // XFA-DATA2-02: inject `console` and `util` into the variables-script
      // closure so the form-level helper functions registered here can
      // reference them lexically and the dependent event scripts that call
      // those helpers do not blow up with `console is not defined` /
      // `util is not defined`. Both are the same sandbox-safe singletons
      // exposed to event scripts (silent no-op console, deterministic util).
      var wrapper = "(function(__console, __util){\n" +
                    "var console = __console; var util = __util;\n" +
                    body +
                    "\nreturn Object.freeze({" + props + "});\n})";
      try {
        var ns = (Function("return " + wrapper))()(consoleStub, xfaUtil);
        if (typeof subformName === "string" && subformName.length > 0) {
          if (subformVariables[subformName] === undefined) {
            subformVariables[subformName] = lookupObject();
          }
          subformVariables[subformName][name] = ns;
        } else {
          variablesScripts[name] = ns;
        }
        return true;
      } catch (_e) {
        return false;
      }
    },
    clearVariablesScripts: function() {
      var keys = Object.keys(variablesScripts);
      for (var i = 0; i < keys.length; i++) {
        delete variablesScripts[keys[i]];
      }
      var skeys = Object.keys(subformVariables);
      for (var j = 0; j < skeys.length; j++) {
        delete subformVariables[skeys[j]];
      }
      // W3-D RETRY: data items share the same per-document lifecycle as
      // scripts. Dropping them here keeps a single bridge entry-point at
      // `reset_for_new_document` and prevents per-document drift.
      var dkeys = Object.keys(variablesDataItems);
      for (var k = 0; k < dkeys.length; k++) {
        delete variablesDataItems[dkeys[k]];
      }
      var sdkeys = Object.keys(subformVariablesDataItems);
      for (var m = 0; m < sdkeys.length; m++) {
        delete subformVariablesDataItems[sdkeys[m]];
      }
    },
    // W3-D RETRY: register a `<variables>` `<text name="X">value</text>`
    // data item (XFA 3.3 §5.5.2) as a form-level mutable string container.
    // Idempotent per (subformName?, name): a later registration overwrites
    // the earlier one, mirroring Adobe Reader's last-wins template merge.
    //
    // The returned shape is `{ value: <string> }` — minimal because real
    // templates only read/write `.value`. Writes coerce non-string inputs
    // to string (matching Adobe Reader's text-field semantics) so a
    // calculate script that does `globValidatePressed.value = true` does
    // not silently store a boolean.
    setVariablesDataItem: function(name, initial, subformName) {
      if (typeof name !== "string" || name.length === 0) return false;
      var initialStr = (typeof initial === "string") ? initial : "";
      // Use a closure-bound private slot so the `value` getter / setter
      // can coerce on write without exposing a plain-object property that
      // would let `delete item.value` succeed.
      var item = (function(initStr) {
        var current = initStr;
        var holder = Object.create(null);
        Object.defineProperty(holder, "value", {
          enumerable: true,
          configurable: false,
          get: function() { return current; },
          set: function(v) {
            current = (v === undefined || v === null) ? "" : String(v);
          }
        });
        // XFA 3.3 §5.5.2: data items also expose `.name`. Read-only.
        Object.defineProperty(holder, "name", {
          enumerable: true, configurable: false, writable: false,
          value: name
        });
        return holder;
      })(initialStr);
      if (typeof subformName === "string" && subformName.length > 0) {
        if (subformVariablesDataItems[subformName] === undefined) {
          subformVariablesDataItems[subformName] = lookupObject();
        }
        subformVariablesDataItems[subformName][name] = item;
      } else {
        variablesDataItems[name] = item;
      }
      return true;
    },
    evalScript: function(body) {
      var id = host.currentNodeId();
      var thisArg = id < 0 ? undefined : makeHandle(id, host.generation());
      // Phase C-α: install per-script `event` global in the function
      // closure so `event.target` resolves to the current field. Wrapping
      // body inside a function lets us pass `event` as a parameter
      // without leaking it to globalThis (where it would persist across
      // unrelated scripts).
      var ev = makeEvent();
      var consoleArg = consoleStub;
      var globals = makeImplicitGlobals(body);
      return (Function(
        "event",
        "console",
        "__globals",
        "with(__globals){\n" + String(body) + "\n}"
      )).call(thisArg, ev, consoleArg, globals);
    }
  };
})
"#;

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
            })?;
            self.register_host_bindings()?;
            Ok::<(), String>(())
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
        self.host.borrow_mut().reset_per_document();
        self.clear_deadline();
        // Memory limit is per-document; re-set to clear any prior accounting.
        self.runtime.set_memory_limit(self.memory_budget_bytes);
        // Phase D-ι: drop all `<variables>` namespace globals from the
        // previous document so they do not leak into the next. Failure
        // here is non-fatal — it just means a slightly polluted global
        // namespace, never a correctness issue, but log via metadata.
        if let Err(e) = self.clear_variables_scripts_global() {
            log::debug!("D-ι clear failed: {e:?}");
        }
        Ok(())
    }

    // The `*mut FormTree` parameter is part of the existing
    // `XfaJsRuntime` trait — the caller in `flatten.rs` already enforces
    // that the pointer outlives this call. Clippy's
    // `not_unsafe_ptr_arg_deref` would require this signature to be
    // `unsafe fn`, which the trait does not allow.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn set_form_handle(
        &mut self,
        form: *mut FormTree,
        root_id: FormNodeId,
    ) -> Result<(), SandboxError> {
        self.host.borrow_mut().set_form_handle(form, root_id);
        // Phase D-ι: register every `<variables>` `<script name="X">…` body
        // collected during merge as a form-level JS global. Done here
        // because by this point the form pointer is valid and the JS
        // runtime is initialised. Errors registering one script do not
        // block the others.
        if !form.is_null() {
            // SAFETY: caller guarantees `form` outlives this call.
            let scripts: Vec<(Option<String>, String, String)> =
                unsafe { (*form).variables_scripts.clone() };
            for (subform_scope, name, body) in scripts {
                if let Err(e) =
                    self.register_variables_script(&name, &body, subform_scope.as_deref())
                {
                    log::debug!("D-ι register `{name}` failed: {e:?}");
                }
            }
            // W3-D RETRY: register `<variables><text name="X">…</text>`
            // data items after scripts so a same-named script (declaration
            // order) keeps precedence — matches the JS-side `get` trap
            // which consults `variablesScripts` before `variablesDataItems`.
            // SAFETY: same lifetime guarantee as above.
            let data_items: Vec<(Option<String>, String, String)> =
                unsafe { (*form).variables_data_items.clone() };
            for (subform_scope, name, initial) in data_items {
                if let Err(e) =
                    self.register_variables_data_item(&name, &initial, subform_scope.as_deref())
                {
                    log::debug!("W3-D register data item `{name}` failed: {e:?}");
                }
            }
        }
        Ok(())
    }

    fn set_data_handle(&mut self, dom: *const xfa_dom_resolver::data_dom::DataDom) {
        self.host.borrow_mut().set_data_handle(dom);
    }

    fn reset_per_script(
        &mut self,
        current_id: FormNodeId,
        activity: Option<&str>,
    ) -> Result<(), SandboxError> {
        self.host
            .borrow_mut()
            .reset_per_script(current_id, activity);
        Ok(())
    }

    fn set_static_page_count(&mut self, page_count: u32) -> Result<(), SandboxError> {
        self.host.borrow_mut().set_static_page_count(page_count);
        Ok(())
    }

    fn set_presave_gate(&mut self, enabled: bool) {
        // D1.B: mirror the dispatch decision into the host-binding layer so
        // mutating host calls see the same gate.
        self.host.borrow_mut().set_presave_gate(enabled);
    }

    fn execute_script(
        &mut self,
        activity: Option<&str>,
        body: &str,
    ) -> Result<RuntimeOutcome, SandboxError> {
        // D1.B: when the per-flatten gate is ON, the QuickJS backend's
        // defence-in-depth check accepts `preSave` exactly like the dispatch
        // gate. Without the gate, behaviour is byte-identical to v4 (W3-B
        // closure). Hard-stop: only `preSave` is unlocked; every other
        // denylist activity stays denied. The gate value is owned by the
        // host bindings (set via `set_presave_gate` from dispatch).
        let presave_gate = self.host.borrow().presave_gate();
        if !activity_allowed_for_sandbox_with_gate(activity, presave_gate) {
            return Err(SandboxError::PhaseDenied(
                activity.unwrap_or("None").to_string(),
            ));
        }
        if body.len() > MAX_SCRIPT_BODY_BYTES {
            self.metadata.runtime_errors = self.metadata.runtime_errors.saturating_add(1);
            return Err(SandboxError::BodyTooLarge);
        }

        // W3-A — REDOS-01 mitigation. Reject script bodies containing
        // catastrophic-backtracking regex shapes BEFORE handing them to
        // QuickJS, because QuickJS's interrupt handler is polled at JS
        // opcode boundaries — not inside the regex C code — and cannot
        // bound a single pathological `test()` call.
        if let RegexScanVerdict::Reject { reason } = scan_script_for_redos(body) {
            self.metadata.runtime_errors = self.metadata.runtime_errors.saturating_add(1);
            return Err(SandboxError::RegexRejected(reason));
        }

        self.set_deadline();
        let script_owned = body.to_string();
        // Phase D-γ: capture the actual JS exception message while still inside
        // the QuickJS context, so we get "TypeError: foo is not a function"
        // rather than the generic "Exception generated by QuickJS".
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.context.with(|ctx| -> Result<(), rquickjs::Error> {
                let Some(eval_script) = self.eval_script.clone() else {
                    return Err(rquickjs::Error::new_from_js_message(
                        "host bindings",
                        "Function",
                        "Phase C eval bridge not registered",
                    ));
                };
                let eval_script = eval_script.restore(&ctx)?;
                if let Err(e) = eval_script.call::<_, ()>((script_owned,)) {
                    // rquickjs stores the thrown value as a pending exception in
                    // the context. `ctx.catch()` pops it and lets us stringify it
                    // for much more useful diagnostic output.
                    let exc_msg = if matches!(e, rquickjs::Error::Exception) {
                        let val = ctx.catch();
                        // Try to get a string representation of the exception.
                        if let Some(exc) = val.as_exception() {
                            exc.message().unwrap_or_else(|| exc.to_string())
                        } else {
                            e.to_string()
                        }
                    } else {
                        e.to_string()
                    };
                    return Err(rquickjs::Error::new_from_js_message(
                        "script", "Error", exc_msg,
                    ));
                }
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
                let host_metadata = self.host.borrow_mut().take_metadata();
                self.metadata.accumulate(host_metadata);
                Ok(RuntimeOutcome {
                    executed: true,
                    mutated_field_count: host_metadata.mutations,
                })
            }
            Ok(Err(other)) => {
                let host_metadata = self.host.borrow_mut().take_metadata();
                self.metadata.accumulate(host_metadata);
                // rquickjs ≤ 0.8 collapses interrupts, OOM, and thrown
                // exceptions into a small set of Error variants. We
                // distinguish a Timeout via the deadline snapshot captured
                // before clear_deadline() above; OOM via a substring scan of
                // the error message; everything else is ScriptError.
                if timed_out {
                    // QF1-E / SEC-01: defence-in-depth wall-time fallback.
                    // The interrupt fired (so this *is* a timeout shape);
                    // re-label as `WallTimeExceeded` only if elapsed
                    // crossed `multiplier × budget`. Both classifications
                    // bump the same `timeouts` counter — observability of
                    // the typed variant is via error type, not metrics.
                    self.metadata.timeouts = self.metadata.timeouts.saturating_add(1);
                    Err(self.classify_timeout_or_walltime(captured_now))
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
                let host_metadata = self.host.borrow_mut().take_metadata();
                self.metadata.accumulate(host_metadata);
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

    #[test]
    fn null_receiver_silent_setter() {
        let mut rt = fresh_runtime();
        rt.execute_script(
            Some("calculate"),
            r#"
$record.NONEXISTENT_FIELD.rawValue = "hello";
$record.NONEXISTENT_FIELD.value = "world";
"#,
        )
        .expect("WP-3 null_receiver_silent_setter: set on null DataHandle must not throw");
    }

    #[test]
    fn null_record_returns_empty_nodelist() {
        let mut rt = fresh_runtime();
        rt.execute_script(
            Some("calculate"),
            r#"
var nodes = $record.FIELD.nodes;
if (nodes.length !== 0) {
  throw new Error("expected nodes.length == 0, got " + nodes.length);
}
// item() on an empty null-sentinel NodeList returns a chainable null handle (not
// native null) so callers can safely do .item(0).value without TypeError.
var itemVal = nodes.item(0).value;
if (itemVal !== null) {
  throw new Error("expected nodes.item(0).value == null, got " + itemVal);
}
"#,
        )
        .expect("WP-3 null_record_returns_empty_nodelist: $record.FIELD.nodes must be empty and chainable");
    }

    // WP-3 F3: xfa.validate is a no-op viewer stub absorbing writes silently.
    // Adobe-generated initialize scripts call `xfa.validate.override = 0`,
    // `xfa.validate.max = N`, `xfa.validate.scriptTest.message = "…"`; without
    // the stub these throw "Cannot set property X of undefined" because
    // `xfa.validate` was previously not defined on the host xfa object.
    #[test]
    fn xfa_validate_stub_absorbs_writes_silently() {
        let mut rt = fresh_runtime();
        rt.execute_script(
            Some("initialize"),
            r#"
xfa.validate.override = 0;
xfa.validate.max = 5;
xfa.validate.messageMode = "warning";
xfa.validate.scriptTest.message = "ignored";
xfa.validate.scriptTest.nested.deeper = true;
// Reads must return harmless defaults, never throw.
if (xfa.validate.override !== 0) throw new Error("override default");
if (xfa.validate.max !== 0) throw new Error("max default");
if (xfa.validate.messageMode !== "") throw new Error("messageMode default");
"#,
        )
        .expect("WP-3 F3 xfa_validate_stub: writes must absorb and reads must not throw");
    }

    // WP-3 F3: addInstance failure returns chainable sentinel so common
    // pattern `parent._Child.addInstance().rawValue = X` does not throw when
    // the instance manager refuses (occur/max bounds reached or unbound).
    // Note: this test exercises only the JS-side null-safety contract; full
    // end-to-end instance addition lives in m3b_phaseD_instance_manager.rs.
    #[test]
    fn add_instance_failure_returns_chainable_null_handle() {
        let mut rt = fresh_runtime();
        // No FormTree is installed, so any addInstance call must fail at the
        // host shim and surface a sentinel rather than native null.
        rt.execute_script(
            Some("initialize"),
            r#"
// Resolve a non-existent subform, addInstance() on an unbound instance
// manager must yield a chainable handle whose `.index` is a number and
// whose `.rawValue` setter is silent.
var im = xfa.resolveNode("NoSuchSubform");
if (im !== null) {
  // If a real handle was returned (shouldn't be), addInstance still must
  // either return a real handle or a null-safe sentinel — never throw.
  var added = im.addInstance();
  if (typeof added.index !== "number") {
    throw new Error("added.index must be a number, got " + typeof added.index);
  }
  added.rawValue = "chained";  // silent on sentinel
  added.value = "chained";     // silent on sentinel
}
"#,
        )
        .expect("WP-3 F3 add_instance_failure: chainable sentinel on failure path");
    }
}
