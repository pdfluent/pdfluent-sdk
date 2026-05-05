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

use super::{
    activity_allowed_for_sandbox, HostBindings, RuntimeMetadata, RuntimeOutcome, SandboxError,
    XfaJsRuntime, DEFAULT_MEMORY_BUDGET_BYTES, DEFAULT_TIME_BUDGET_MS, MAX_SCRIPT_BODY_BYTES,
};

/// QuickJS-backed runtime adapter. One instance is reusable across many
/// documents; callers MUST invoke [`XfaJsRuntime::reset_for_new_document`]
/// at the start of each flatten.
pub struct QuickJsRuntime {
    eval_script: Option<Persistent<Function<'static>>>,
    context: Context,
    runtime: Runtime,
    metadata: RuntimeMetadata,
    time_budget: Duration,
    memory_budget_bytes: usize,
    script_deadline: Arc<AtomicU64>,
    script_started: Arc<AtomicBool>,
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
            eval_script: None,
            context,
            runtime,
            metadata: RuntimeMetadata::default(),
            time_budget: Duration::from_millis(DEFAULT_TIME_BUDGET_MS),
            memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
            script_deadline,
            script_started,
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
            let eval_script: Function = bridge
                .get("evalScript")
                .map_err(|e| format!("get evalScript: {e}"))?;
            globals
                .set("xfa", xfa)
                .map_err(|e| format!("set xfa global: {e}"))?;
            globals
                .set("app", app)
                .map_err(|e| format!("set app global: {e}"))?;
            Ok::<Persistent<Function<'static>>, String>(Persistent::save(&ctx, eval_script))
        })?;

        self.eval_script = Some(eval_script);
        self.bindings_registered = true;
        Ok(())
    }
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
    "xfa", "event"
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
  ["rawValue", "somExpression", "isNull", "clearItems", "addItem", "$record",
   "nodes", "value", "length", "item"].forEach(function(name) {
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
        if (prop === "setInstances") {
          return function(n) {
            return host.instanceSet(id, generation, n);
          };
        }
        if (prop === "addInstance") {
          return function() {
            var newId = host.instanceAdd(id, generation);
            return newId < 0 ? null : makeHandle(newId, generation);
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
        if (prop === "$record") {
          var recRaw = host.dataBoundRecord(id, generation);
          if (recRaw < 0) return null;
          return makeDataHandle(recRaw);
        }
        if (shouldDeferHandleProperty(prop)) {
          return undefined;
        }
        // XFA 3.3 §6.4.3.2 underscore shorthand: `_<name>` on a subform
        // refers to the instanceManager of the same-named child subform.
        // Used in the wild as `parent._child.setInstances(N)`.
        if (prop.charAt(0) === "_" && prop.length > 1) {
          var bareName = prop.substring(1);
          var imChildId = host.resolveChildNodeId(id, bareName);
          if (imChildId >= 0) {
            return makeInstanceManager(imChildId, generation);
          }
        }
        var childId = host.resolveChildNodeId(id, prop);
        if (childId < 0) {
          return undefined;
        }
        return makeHandle(childId, generation);
      },
      set: function(_target, prop, value) {
        if (prop === "rawValue") {
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
          prop === "setInstances" ||
          prop === "addInstance" ||
          prop === "removeInstance" ||
          prop === "isNull" ||
          prop === "clearItems" ||
          prop === "addItem" ||
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

  // Phase D-γ: Data DOM handle — wraps a raw DataDom node index and exposes
  // `.value`, `.nodes`, `.length`, `.item(i)`, and named child access via Proxy.
  function makeDataHandle(rawId) {
    if (rawId === undefined || rawId < 0) return null;
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
        if (childId < 0) return undefined;
        return makeDataHandle(childId);
      }
    });
  }

  var xfaHost = nullProtoObject();
  Object.defineProperty(xfaHost, "numPages", {
    enumerable: true,
    configurable: false,
    get: function() {
      return host.numPages();
    }
  });
  Object.defineProperty(xfaHost, "messageBox", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: function() {
      host.bindingError();
      return null;
    }
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
    value: Object.freeze(xfaHost)
  });
  Object.defineProperty(xfa, "layout", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: Object.freeze(xfaLayout)
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

  var app = nullProtoObject();
  Object.defineProperty(app, "alert", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: function() {
      host.bindingError();
      return null;
    }
  });
  Object.defineProperty(app, "launchURL", {
    enumerable: true,
    configurable: false,
    writable: false,
    value: function() {
      host.bindingError();
      return null;
    }
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
    Object.defineProperty(ev, "change", {
      enumerable: true, configurable: false,
      get: function() { return ""; }
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
      var nodeId = host.resolveImplicitNodeId(currentId, name);
      if (nodeId < 0) {
        return undefined;
      }
      var handle = makeHandle(nodeId, generation);
      cachedHandles[name] = handle;
      return handle;
    }

    return new Proxy(Object.create(null), {
      has: function(_target, prop) {
        if (typeof prop !== "string") {
          return false;
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
        if (shouldDeferGlobalName(prop, localNames)) {
          return undefined;
        }
        // Phase D-γ: $record as a script-level global refers to the data
        // record bound to the current field's enclosing subform context.
        // Scripts write `var addr = $record.SECTION.nodes;` — we intercept
        // this here instead of letting resolveImplicitNodeId fail (-1).
        if (prop === "$record") {
          var recRaw = host.dataBoundRecord(currentId, generation);
          if (recRaw < 0) return null;
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
    xfa: Object.freeze(xfa),
    app: Object.freeze(app),
    consoleStub: Object.freeze(consoleStub),
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
        Ok(())
    }

    fn set_form_handle(
        &mut self,
        form: *mut FormTree,
        root_id: FormNodeId,
    ) -> Result<(), SandboxError> {
        self.host.borrow_mut().set_form_handle(form, root_id);
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
}
