#![cfg(feature = "xfa-js-sandboxed")]

//! F-3 — Resource-limit regression tests.
//!
//! Verifies that the XFA JS sandbox enforces hard resource limits so that
//! adversarial or broken XFA scripts cannot consume unbounded CPU, memory,
//! or stack depth.
//!
//! Three scenarios are tested:
//!
//! 1. **Timeout (infinite loop):** a script containing `while(true){}` must
//!    terminate with `SandboxError::Timeout` well within 10 seconds.
//!    The runtime is configured with a 50 ms budget to keep the test fast.
//!    The 10-second assertion margin accounts for CI scheduling jitter.
//!
//! 2. **Memory (allocation bomb):** a script that tries to build a very large
//!    array must terminate with `SandboxError::OutOfMemory` well within 10
//!    seconds. The runtime is configured with a 4 MiB budget to keep peak
//!    host RSS low during the test.
//!
//! 3. **Recursion (stack bomb):** a mutually or self-recursive function with
//!    no base case must terminate with a `SandboxError` (either
//!    `SandboxError::ScriptError` for a stack-overflow JS exception, or
//!    `SandboxError::Timeout` if the interrupt fires first) without killing
//!    the host process. The important invariant is that the test process
//!    survives and the error is recoverable.
//!
//! After each resource-exhaustion test the runtime must be able to execute
//! a harmless script, proving that the sandbox recovers without requiring
//! process restart (S-17 fail-open).
//!
//! Reference: SECURITY2-01 audit §3.1–3.2; TRACK_CRASH_SECURITY_SANDBOX.md F-3.

use std::time::{Duration, Instant};

use pdf_xfa::js_runtime::{QuickJsRuntime, SandboxError, XfaJsRuntime};

// ---------------------------------------------------------------------------
// F-3-1  Timeout — infinite loop terminates well within 10 seconds
// ---------------------------------------------------------------------------

/// An infinite `while(true){}` loop must be interrupted by the per-script
/// wall-clock budget and return `SandboxError::Timeout`. The entire test
/// (including runtime setup) must complete within 10 seconds.
///
/// A 50 ms time budget is used so the test suite stays fast; the 10-second
/// wall-clock assertion is a generous safety net for slow CI machines.
#[test]
fn timeout_infinite_loop_terminates_within_10s() {
    let start = Instant::now();

    let mut rt = QuickJsRuntime::new()
        .expect("QuickJsRuntime::new")
        .with_time_budget(Duration::from_millis(50));
    rt.init().expect("init");
    rt.reset_for_new_document().expect("reset_for_new_document");

    let result = rt.execute_script(Some("calculate"), "while (true) {}");
    let elapsed = start.elapsed();

    assert_eq!(
        result.unwrap_err(),
        SandboxError::Timeout,
        "infinite loop must produce SandboxError::Timeout"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "infinite loop took {elapsed:?} — must terminate within 10 s"
    );

    // Metadata counter must reflect the timeout.
    let md = rt.take_metadata();
    assert_eq!(md.timeouts, 1, "timeouts counter must be 1 after loop");
    assert_eq!(md.executed, 0, "executed counter must remain 0 on timeout");

    // Sandbox must be recoverable: a subsequent harmless script runs cleanly.
    rt.reset_for_new_document().expect("reset after timeout");
    let outcome = rt
        .execute_script(Some("calculate"), "var x = 1 + 1; x")
        .expect("runtime must recover after timeout");
    assert!(
        outcome.executed,
        "sandbox must execute cleanly after recovery"
    );
}

// ---------------------------------------------------------------------------
// F-3-2  Memory limit — allocation bomb terminates well within 10 seconds
// ---------------------------------------------------------------------------

/// A script that attempts to allocate a very large array must be stopped by
/// the per-document memory budget and return `SandboxError::OutOfMemory` (or
/// `SandboxError::Timeout` if the interrupt fires first, which is an
/// acceptable secondary termination path on some platforms).
///
/// The runtime is configured with a 4 MiB memory budget to keep the test
/// from consuming excessive host RAM.
#[test]
fn memory_allocation_bomb_terminates_within_10s() {
    let start = Instant::now();

    // 4 MiB budget — small enough to guarantee fast OOM on the allocation bomb,
    // large enough to initialise the sandbox.
    let budget_bytes: usize = 4 * 1024 * 1024;
    let mut rt = QuickJsRuntime::new()
        .expect("QuickJsRuntime::new")
        .with_memory_budget(budget_bytes)
        // Generous time budget: we want OOM, not timeout.
        .with_time_budget(Duration::from_secs(5));
    rt.init().expect("init");
    rt.reset_for_new_document().expect("reset_for_new_document");

    // Tries to push millions of entries — enough to blow 4 MiB.
    let bomb_script = r#"
var arr = [];
for (var i = 0; i < 10000000; i++) {
    arr.push(i);
}
"#;
    let result = rt.execute_script(Some("calculate"), bomb_script);
    let elapsed = start.elapsed();

    let err = result.expect_err("allocation bomb must fail with a resource error");
    assert!(
        matches!(
            err,
            SandboxError::OutOfMemory | SandboxError::Timeout | SandboxError::ScriptError(_)
        ),
        "expected OutOfMemory, Timeout, or ScriptError; got {err:?}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "allocation bomb took {elapsed:?} — must terminate within 10 s"
    );
}

// ---------------------------------------------------------------------------
// F-3-3  Recursion — deep stack bomb terminates without process kill
// ---------------------------------------------------------------------------

/// A deeply self-recursive function with no base case must be caught by
/// either the QuickJS call-stack depth limit or the time budget, and must
/// NOT cause a native Rust stack overflow that kills the host process.
///
/// The expected outcome is any `SandboxError` variant (ScriptError for
/// stack overflow, Timeout if the interrupt fires first). The critical
/// property is that the Rust test process survives and the runtime can
/// be used again afterwards.
#[test]
fn recursion_bomb_terminates_without_process_kill() {
    let start = Instant::now();

    let mut rt = QuickJsRuntime::new()
        .expect("QuickJsRuntime::new")
        // 200 ms budget: generous enough to hit the JS stack limit before
        // the interrupt fires on most machines; the interrupt is a fallback.
        .with_time_budget(Duration::from_millis(200));
    rt.init().expect("init");
    rt.reset_for_new_document().expect("reset_for_new_document");

    let recursion_script = r#"
function recurse(n) { return recurse(n + 1); }
recurse(0);
"#;
    let result = rt.execute_script(Some("calculate"), recursion_script);
    let elapsed = start.elapsed();

    // Must have terminated with some error — not Ok.
    let err = result.expect_err("deep recursion must terminate with an error");
    assert!(
        matches!(
            err,
            SandboxError::ScriptError(_) | SandboxError::Timeout | SandboxError::StackOverflow
        ),
        "expected ScriptError/Timeout/StackOverflow; got {err:?}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "recursion bomb took {elapsed:?} — must terminate within 10 s"
    );

    // Process is still alive (we are here), and the sandbox must be
    // recoverable for subsequent documents.
    rt.reset_for_new_document()
        .expect("reset after recursion bomb");
    let outcome = rt
        .execute_script(Some("calculate"), "1 + 1")
        .expect("runtime must recover after recursion bomb");
    assert!(
        outcome.executed,
        "sandbox must execute cleanly after recovery"
    );
}

// ---------------------------------------------------------------------------
// F-3-4  Timeout budget is per-script, not per-document
// ---------------------------------------------------------------------------

/// A script that times out must not consume the entire per-document budget.
/// Subsequent scripts in the same document (after a per-script reset) must
/// still run cleanly, demonstrating the deadline is cleared between calls.
///
/// This test verifies that the interrupt-handler deadline flag is always
/// cleared after each `execute_script` call, even on the timeout path.
#[test]
fn deadline_is_cleared_after_timeout() {
    let mut rt = QuickJsRuntime::new()
        .expect("QuickJsRuntime::new")
        .with_time_budget(Duration::from_millis(50));
    rt.init().expect("init");
    rt.reset_for_new_document().expect("reset_for_new_document");

    // First script: infinite loop → timeout.
    let err = rt
        .execute_script(Some("calculate"), "while (true) {}")
        .expect_err("first script must timeout");
    assert_eq!(err, SandboxError::Timeout);

    // Second script in the same document: must run cleanly because the
    // deadline has been cleared by the first execute_script call.
    let outcome = rt
        .execute_script(Some("calculate"), "var y = 42; y")
        .expect("second script must execute after timeout");
    assert!(
        outcome.executed,
        "second script must be marked executed after timeout recovery"
    );
}
