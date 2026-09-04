#![cfg(feature = "xfa-js-sandboxed")]
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! F-2 — Sandbox forbidden-globals regression tests.
//!
//! Verifies that the XFA JS sandbox (QuickJsRuntime) does NOT expose any of
//! the capability-bearing globals that would allow filesystem, network, or
//! process access from untrusted XFA script content:
//!
//!   - `fetch`            — browser/Node network API
//!   - `XMLHttpRequest`   — browser network API
//!   - `process`          — Node.js process object (env, spawn, exit, …)
//!   - `require`          — CommonJS module loader
//!   - `Deno`             — Deno runtime namespace
//!   - `Bun`              — Bun runtime namespace
//!   - `globalThis` escalation — accessing the above through globalThis
//!
//! Each test runs a script that throws an Error if the forbidden name is
//! accessible (typeof !== "undefined"). All tests must pass without a
//! js_runtime_errors increment, proving that the sandbox correctly
//! withholds the capability.
//!
//! Implementation baseline: `rquickjs_backend.rs` — `init()` explicitly
//! sets each forbidden name to `rquickjs::Undefined` before registering
//! any host bindings (defense-in-depth: strips anything QuickJS or a
//! third-party crate may have registered).
//!
//! Reference: SECURITY2-01 audit §4.1; TRACK_CRASH_SECURITY_SANDBOX.md F-2.

use pdf_xfa::js_runtime::{QuickJsRuntime, SandboxError, XfaJsRuntime};

// ---------------------------------------------------------------------------
// Helper: build a fresh, fully-initialised runtime ready for script execution.
// ---------------------------------------------------------------------------

fn fresh_rt() -> QuickJsRuntime {
    let mut rt = QuickJsRuntime::new().expect("QuickJsRuntime::new");
    rt.init().expect("init");
    rt.reset_for_new_document().expect("reset_for_new_document");
    rt
}

/// Run a single script body under the `calculate` activity and expect it
/// to execute cleanly (no SandboxError). This helper is used for scripts
/// that assert a global is undefined by throwing on the opposite condition.
fn assert_executes_clean(rt: &mut QuickJsRuntime, script: &str) {
    match rt.execute_script(Some("calculate"), script) {
        Ok(outcome) => assert!(
            outcome.executed,
            "expected script to be marked executed, script: {script}"
        ),
        Err(SandboxError::ScriptError(msg)) => {
            panic!(
                "Script threw an error — sandbox boundary violation? msg: {msg}\nscript: {script}"
            );
        }
        Err(other) => {
            panic!("Unexpected SandboxError::{other:?}\nscript: {script}");
        }
    }
}

// ---------------------------------------------------------------------------
// F-2-1  `fetch` must be undefined
// ---------------------------------------------------------------------------

/// The `fetch` global (browser/Node network API) must not be accessible from
/// XFA script context. The sandbox strips it in `init()` before any host
/// binding is registered.
#[test]
fn forbidden_global_fetch_is_undefined() {
    let mut rt = fresh_rt();
    assert_executes_clean(
        &mut rt,
        "if (typeof fetch !== 'undefined') throw new Error('fetch leaked into sandbox');",
    );
}

// ---------------------------------------------------------------------------
// F-2-2  `XMLHttpRequest` must be undefined
// ---------------------------------------------------------------------------

/// `XMLHttpRequest` (browser XHR network API) must not be accessible from
/// XFA script context. XFA forms must not be able to initiate HTTP requests.
#[test]
fn forbidden_global_xmlhttprequest_is_undefined() {
    let mut rt = fresh_rt();
    assert_executes_clean(
        &mut rt,
        "if (typeof XMLHttpRequest !== 'undefined') throw new Error('XMLHttpRequest leaked into sandbox');",
    );
}

// ---------------------------------------------------------------------------
// F-2-3  `process` must be undefined
// ---------------------------------------------------------------------------

/// The Node.js `process` object (env vars, exit, spawn) must not be
/// accessible from XFA script context. Its presence would allow an adversarial
/// form to read environment variables or terminate the host process.
#[test]
fn forbidden_global_process_is_undefined() {
    let mut rt = fresh_rt();
    assert_executes_clean(
        &mut rt,
        "if (typeof process !== 'undefined') throw new Error('process leaked into sandbox');",
    );
}

// ---------------------------------------------------------------------------
// F-2-4  `require` must be undefined
// ---------------------------------------------------------------------------

/// CommonJS `require` must not be accessible from XFA script context.
/// A `require('fs')` call would allow filesystem access; a `require('child_process')`
/// call would allow process spawning.
#[test]
fn forbidden_global_require_is_undefined() {
    let mut rt = fresh_rt();
    assert_executes_clean(
        &mut rt,
        "if (typeof require !== 'undefined') throw new Error('require leaked into sandbox');",
    );
}

// ---------------------------------------------------------------------------
// F-2-5  `Deno` must be undefined
// ---------------------------------------------------------------------------

/// The Deno runtime namespace must not be accessible from XFA script context.
/// `Deno.readFile`, `Deno.run`, `Deno.env.get` etc. would escape the sandbox.
#[test]
fn forbidden_global_deno_is_undefined() {
    let mut rt = fresh_rt();
    assert_executes_clean(
        &mut rt,
        "if (typeof Deno !== 'undefined') throw new Error('Deno leaked into sandbox');",
    );
}

// ---------------------------------------------------------------------------
// F-2-6  `Bun` must be undefined
// ---------------------------------------------------------------------------

/// The Bun runtime namespace must not be accessible from XFA script context.
/// `Bun.file`, `Bun.spawn` etc. would escape the sandbox.
#[test]
fn forbidden_global_bun_is_undefined() {
    let mut rt = fresh_rt();
    assert_executes_clean(
        &mut rt,
        "if (typeof Bun !== 'undefined') throw new Error('Bun leaked into sandbox');",
    );
}

// ---------------------------------------------------------------------------
// F-2-7  `globalThis` escalation — forbidden names not accessible via
//         `globalThis[name]` either
// ---------------------------------------------------------------------------

/// Even if `typeof fetch === 'undefined'` on its own, an attacker could
/// try `globalThis['fetch']` or `globalThis.fetch` as an alternative path.
/// This test verifies all forbidden names are absent from `globalThis`.
#[test]
fn forbidden_globals_not_accessible_via_globalthis() {
    let mut rt = fresh_rt();
    assert_executes_clean(
        &mut rt,
        r#"
var forbidden = ['fetch', 'XMLHttpRequest', 'process', 'require', 'Deno', 'Bun'];
for (var i = 0; i < forbidden.length; i++) {
    var name = forbidden[i];
    if (typeof globalThis[name] !== 'undefined') {
        throw new Error('globalThis.' + name + ' is accessible — sandbox boundary violation');
    }
}
"#,
    );
}

// ---------------------------------------------------------------------------
// F-2-8  Capability-bearing globals stay absent across multiple scripts
//         (documents do not leak between reset cycles).
// ---------------------------------------------------------------------------

/// Verifies that a second `reset_for_new_document` cycle does not accidentally
/// re-introduce forbidden globals. This guards against a registration path
/// that runs during `reset_for_new_document` rather than only during `init`.
#[test]
fn forbidden_globals_absent_after_document_reset() {
    let mut rt = fresh_rt();

    // First document: verify clean.
    assert_executes_clean(
        &mut rt,
        "if (typeof fetch !== 'undefined') throw new Error('fetch on doc-1');",
    );

    // Reset to simulate a new document.
    rt.reset_for_new_document()
        .expect("second reset_for_new_document");

    // Second document: forbidden names must still be absent.
    assert_executes_clean(
        &mut rt,
        r#"
if (typeof fetch !== 'undefined')          throw new Error('fetch leaked on doc-2');
if (typeof XMLHttpRequest !== 'undefined') throw new Error('XMLHttpRequest leaked on doc-2');
if (typeof process !== 'undefined')        throw new Error('process leaked on doc-2');
if (typeof require !== 'undefined')        throw new Error('require leaked on doc-2');
if (typeof Deno !== 'undefined')           throw new Error('Deno leaked on doc-2');
if (typeof Bun !== 'undefined')            throw new Error('Bun leaked on doc-2');
"#,
    );
}
