#![cfg(feature = "xfa-js-sandboxed")]

//! G-W3A — Wave 3-A REDOS-01 mitigation: adversarial regression tests.
//!
//! Wave 2-E recorded the REDOS-01 finding: QuickJS's `RegExp` engine is a
//! backtracking NFA, and the per-script interrupt handler is polled at JS
//! opcode boundaries only — it never fires inside a single regex C call.
//! A pattern like `/(a+)+$/.test("a".repeat(25)+"!")` therefore ran for
//! **41 seconds** without termination, well past the 100 ms time budget.
//!
//! Wave 3-A implements the static heuristic guard from
//! `crates/pdf-xfa/src/js_runtime/regex_guard.rs`: every script body is
//! scanned for known catastrophic-backtracking shapes before being handed
//! to QuickJS. Matching bodies are rejected with `SandboxError::RegexRejected`.
//!
//! These tests assert:
//!
//! 1. The exact W2-E proof pattern is now rejected in **well under 1 s**.
//! 2. Other dangerous shapes (`(a*)*$`, `(.+)+`, `(a|aa)+`) are also rejected.
//! 3. Real-world XFA format masks (`^\d{4}$`, ISO date, e-mail, etc.) still
//!    execute cleanly — no false positives.
//! 4. The guard is also applied to `new RegExp(...)` constructor calls.
//! 5. The runtime is fully recoverable after a rejection; the host process
//!    survives without panic.
//!
//! Reference: WAVE3_EXECUTION_PLAN.md §W3-A; W2_E_REPORT.md REDOS-01.

use std::time::{Duration, Instant};

use pdf_xfa::js_runtime::{QuickJsRuntime, SandboxError, XfaJsRuntime};

/// Build a fresh runtime with the production default 100 ms budget. With
/// the W3-A guard in place every test below MUST complete well under that
/// budget because the rejection happens BEFORE the QuickJS call.
fn fresh_rt() -> QuickJsRuntime {
    let mut rt = QuickJsRuntime::new()
        .expect("QuickJsRuntime::new")
        .with_time_budget(Duration::from_millis(100));
    rt.init().expect("init");
    rt.reset_for_new_document().expect("reset_for_new_document");
    rt
}

// =====================================================================
// W3-A-1 — The W2-E proof pattern is mitigated.
// =====================================================================

/// W3-A-1: The exact `(a+)+$` regression from W2-E REDOS-01. Pre-W3-A this
/// ran 41 seconds on a 25-character input. Post-W3-A the guard rejects the
/// body in microseconds, well under the 100 ms time budget.
#[test]
fn w2e_redos_01_proof_pattern_now_rejected_in_microseconds() {
    let mut rt = fresh_rt();
    let start = Instant::now();
    let result = rt.execute_script(
        Some("calculate"),
        r#"var matched = /(a+)+$/.test("aaaaaaaaaaaaaaaaaaaaaaaaa!");"#,
    );
    let elapsed = start.elapsed();

    // Critical assertion: total cost (including guard scan + error
    // construction) is bounded by 1 s on the slowest CI runner.
    // Reality: it is well under 1 ms.
    assert!(
        elapsed < Duration::from_secs(1),
        "ReDoS guard reject must be sub-second; elapsed: {elapsed:?}"
    );

    match result {
        Err(SandboxError::RegexRejected(reason)) => {
            assert!(
                reason.contains("nested quantifier"),
                "rejection reason should name the shape; got: {reason}"
            );
        }
        other => panic!("Expected RegexRejected, got {other:?}"),
    }

    // The host runtime must remain fully usable after rejection.
    let recovery = rt
        .execute_script(Some("calculate"), "var x = 1 + 1; x")
        .expect("runtime must remain usable after RegexRejected");
    assert!(recovery.executed);
}

/// W3-A-1b: An even longer input — pre-W3-A this would have been
/// minutes. With the guard the elapsed time is unchanged regardless of
/// input length, because the rejection happens before regex execution.
#[test]
fn very_long_input_still_rejected_fast() {
    let mut rt = fresh_rt();
    let payload = format!(
        r#"var matched = /(a+)+$/.test("{}!");"#,
        "a".repeat(40) // would be ~hours of backtracking pre-W3-A
    );
    let start = Instant::now();
    let result = rt.execute_script(Some("calculate"), &payload);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(1),
        "guard must be O(script.len), not O(2^n); elapsed: {elapsed:?}"
    );
    assert!(matches!(result, Err(SandboxError::RegexRejected(_))));
}

// =====================================================================
// W3-A-2 — Other dangerous shapes are caught.
// =====================================================================

/// W3-A-2-a: `(a*)*` — star/star ReDoS shape.
#[test]
fn star_star_shape_rejected() {
    let mut rt = fresh_rt();
    let result = rt.execute_script(
        Some("calculate"),
        r#"var m = /(a*)*$/.test("aaaaaaaaaaaaaaaaaaaa");"#,
    );
    assert!(matches!(result, Err(SandboxError::RegexRejected(_))));
}

/// W3-A-2-b: `(.+)+` — wildcard with nested quantifier.
#[test]
fn dot_plus_plus_shape_rejected() {
    let mut rt = fresh_rt();
    let result = rt.execute_script(
        Some("calculate"),
        r#"var m = /(.+)+$/.test("abcdefghijklmnopqrstuvwxyz");"#,
    );
    assert!(matches!(result, Err(SandboxError::RegexRejected(_))));
}

/// W3-A-2-c: Overlapping alternation `(a|aa)+`.
#[test]
fn overlapping_alternation_rejected() {
    let mut rt = fresh_rt();
    let result = rt.execute_script(
        Some("calculate"),
        r#"var m = /(a|aa)+$/.test("aaaaaaaaaaaaaaaaa!");"#,
    );
    assert!(matches!(result, Err(SandboxError::RegexRejected(_))));
}

/// W3-A-2-d: `[a-z]` class inside nested quantifier — covers ReDoS shapes
/// that span more than a single literal char.
#[test]
fn char_class_nested_quantifier_rejected() {
    let mut rt = fresh_rt();
    let result = rt.execute_script(
        Some("calculate"),
        r#"var m = /([a-z]+)+$/.test("abcdef!");"#,
    );
    assert!(matches!(result, Err(SandboxError::RegexRejected(_))));
}

/// W3-A-2-e: Unbounded brace `(a{1,})+` — semantically equivalent to
/// `(a+)+`.
#[test]
fn unbounded_brace_nested_quantifier_rejected() {
    let mut rt = fresh_rt();
    let result = rt.execute_script(Some("calculate"), r#"var m = /(a{1,})+$/.test("aaaaaa!");"#);
    assert!(matches!(result, Err(SandboxError::RegexRejected(_))));
}

// =====================================================================
// W3-A-3 — `new RegExp(...)` constructor is also guarded.
// =====================================================================

/// W3-A-3-a: `new RegExp("(a+)+$")` — string-form construction.
#[test]
fn new_regexp_string_constructor_rejected() {
    let mut rt = fresh_rt();
    let result = rt.execute_script(
        Some("calculate"),
        r#"var re = new RegExp("(a+)+$"); re.test("aaaaaaaaaaaaaaaaaaaa!");"#,
    );
    match result {
        Err(SandboxError::RegexRejected(reason)) => {
            assert!(
                reason.contains("new RegExp"),
                "reason should identify the constructor call site; got: {reason}"
            );
        }
        other => panic!("Expected RegexRejected, got {other:?}"),
    }
}

/// W3-A-3-b: `new RegExp('...')` — single-quote variant.
#[test]
fn new_regexp_single_quote_rejected() {
    let mut rt = fresh_rt();
    let result = rt.execute_script(
        Some("calculate"),
        r#"var re = new RegExp('(.+)+$'); re.test('abc!');"#,
    );
    assert!(matches!(result, Err(SandboxError::RegexRejected(_))));
}

// =====================================================================
// W3-A-4 — Real-world XFA format masks must NOT trigger the guard.
//          Zero false positives.
// =====================================================================

/// W3-A-4-a: Dutch postal code (`1234 AB`). Common XFA mask.
#[test]
fn xfa_dutch_postal_code_accepted() {
    let mut rt = fresh_rt();
    let result = rt
        .execute_script(
            Some("calculate"),
            r#"var m = /^\d{4}\s?[A-Z]{2}$/.test("1234 AB");"#,
        )
        .expect("clean XFA postal-code mask must execute");
    assert!(result.executed);
}

/// W3-A-4-b: ISO date.
#[test]
fn xfa_iso_date_accepted() {
    let mut rt = fresh_rt();
    let result = rt
        .execute_script(
            Some("calculate"),
            r#"var m = /^\d{4}-\d{2}-\d{2}$/.test("2026-05-19");"#,
        )
        .expect("clean ISO-date mask must execute");
    assert!(result.executed);
}

/// W3-A-4-c: E-mail (typical XFA loose validation).
#[test]
fn xfa_email_accepted() {
    let mut rt = fresh_rt();
    let result = rt
        .execute_script(
            Some("calculate"),
            r#"var m = /^[^@\s]+@[^@\s]+\.[a-zA-Z]{2,}$/.test("a@b.co");"#,
        )
        .expect("clean e-mail mask must execute");
    assert!(result.executed);
}

/// W3-A-4-d: Phone number.
#[test]
fn xfa_phone_number_accepted() {
    let mut rt = fresh_rt();
    let result = rt
        .execute_script(
            Some("calculate"),
            r#"var m = /^\+?[0-9]{1,3}-[0-9]{3,12}$/.test("+31-205551234");"#,
        )
        .expect("clean phone-number mask must execute");
    assert!(result.executed);
}

/// W3-A-4-e: Bounded outer quantifier is NOT dangerous.
#[test]
fn bounded_outer_quantifier_accepted() {
    let mut rt = fresh_rt();
    let result = rt
        .execute_script(Some("calculate"), r#"var m = /(a+){1,3}$/.test("aaaaa");"#)
        .expect("bounded outer quantifier must not be flagged");
    assert!(result.executed);
}

/// W3-A-4-f: Bounded inner quantifier is NOT dangerous.
#[test]
fn bounded_inner_quantifier_accepted() {
    let mut rt = fresh_rt();
    let result = rt
        .execute_script(Some("calculate"), r#"var m = /(a{1,5})+$/.test("aaaaa");"#)
        .expect("bounded inner quantifier must not be flagged");
    assert!(result.executed);
}

// =====================================================================
// W3-A-5 — Comments / strings must NOT be misread as regex literals.
// =====================================================================

/// W3-A-5-a: A dangerous pattern in a `//` line comment is ignored.
#[test]
fn dangerous_shape_in_line_comment_accepted() {
    let mut rt = fresh_rt();
    let result = rt
        .execute_script(
            Some("calculate"),
            "// /(a+)+$/ is dangerous but commented out\nvar x = 1; x",
        )
        .expect("line-comment must not trip guard");
    assert!(result.executed);
}

/// W3-A-5-b: A dangerous pattern in a `/* */` block comment is ignored.
#[test]
fn dangerous_shape_in_block_comment_accepted() {
    let mut rt = fresh_rt();
    let result = rt
        .execute_script(Some("calculate"), r#"/* see /(a+)+$/ */ var x = 1; x"#)
        .expect("block-comment must not trip guard");
    assert!(result.executed);
}

/// W3-A-5-c: A dangerous pattern inside a string literal is ignored.
/// (The string is not executed as a regex; only `new RegExp(string)` is
/// flagged, and this test does not wrap it.)
#[test]
fn dangerous_shape_in_string_literal_accepted() {
    let mut rt = fresh_rt();
    let result = rt
        .execute_script(
            Some("calculate"),
            r#"var s = "/(a+)+$/ is the W2-E proof"; s.length"#,
        )
        .expect("plain string literal must not trip guard");
    assert!(result.executed);
}

// =====================================================================
// W3-A-6 — Host capability surface is unchanged.
//
//          Confirms that the W3-A guard adds NO new capabilities and
//          does not weaken any forbidden-global stripping. The classic
//          "is `process` exposed" probe must still report `undefined`.
// =====================================================================

/// W3-A-6: No sandbox capability leak from the guard. The forbidden-global
/// surface remains intact.
#[test]
fn no_sandbox_capability_leak_from_guard() {
    let mut rt = fresh_rt();
    let result = rt
        .execute_script(
            Some("calculate"),
            r#"
            var probe = (typeof process) + ',' +
                        (typeof require) + ',' +
                        (typeof fetch) + ',' +
                        (typeof XMLHttpRequest) + ',' +
                        (typeof Bun) + ',' +
                        (typeof Deno);
            if (probe.indexOf('function') !== -1 || probe.indexOf('object') !== -1) {
                throw new Error('capability leak: ' + probe);
            }
            "#,
        )
        .expect("forbidden-globals surface must be intact post-W3-A");
    assert!(result.executed);
}

// =====================================================================
// W3-A-7 — Recovery semantics: rejection is observable, recoverable,
//          and counted as a runtime_error in metadata.
// =====================================================================

/// W3-A-7: Rejection bumps the `runtime_errors` counter and the runtime
/// continues to function. Critical for dispatch-site observability and
/// to ensure the rejection is NOT silently masked.
#[test]
fn rejection_bumps_runtime_errors_and_runtime_recovers() {
    let mut rt = fresh_rt();
    let _ = rt.take_metadata(); // clear any startup counters

    let result = rt.execute_script(Some("calculate"), r#"var m = /(a+)+$/.test("aaaa!");"#);
    assert!(matches!(result, Err(SandboxError::RegexRejected(_))));

    let meta = rt.take_metadata();
    assert_eq!(meta.runtime_errors, 1, "rejection must bump runtime_errors");
    assert_eq!(meta.executed, 0, "rejection must NOT bump executed");
    assert_eq!(meta.timeouts, 0, "rejection is not a timeout");
    assert_eq!(meta.oom, 0, "rejection is not an OOM");

    // Same runtime continues to work for a clean script after the reject.
    let clean = rt
        .execute_script(Some("calculate"), "var x = 2 * 3; x")
        .expect("runtime must process clean scripts after a regex reject");
    assert!(clean.executed);
}

// =====================================================================
// W3-A-8 — Defence-in-depth: oversized pattern bound.
// =====================================================================

/// W3-A-8: Patterns over the 4 KiB safety bound are rejected even if they
/// do not contain a nested quantifier — they cannot reflect a real-world
/// XFA mask and might encode a future shape the heuristic does not see.
#[test]
fn oversized_pattern_rejected() {
    let mut rt = fresh_rt();
    let payload = format!(r#"var re = new RegExp("{}");"#, "a".repeat(5000));
    let result = rt.execute_script(Some("calculate"), &payload);
    match result {
        Err(SandboxError::RegexRejected(reason)) => {
            assert!(
                reason.contains("4 KiB"),
                "oversized pattern reason should mention bound; got: {reason}"
            );
        }
        other => panic!("Expected RegexRejected (oversize), got {other:?}"),
    }
}
