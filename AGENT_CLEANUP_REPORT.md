# Agent F Cleanup Report

**Agent:** F — Crash/Sandbox Hardening
**Branch:** `xfa/product-crash-sandbox-hardening`
**Date:** 2026-05-19
**Baseline:** `743bb4acc` (xfa/product-quality-track-plan)

## LOC Delta

| File | Change | LOC added |
|---|---|---|
| `crates/pdf-xfa/tests/m3b_phaseE_sandbox_forbidden_globals.rs` | new | 215 |
| `crates/pdf-xfa/tests/m3b_phaseF_resource_limits.rs` | new | 195 |
| `benchmarks/runs/…/product_quality_track/F_PANIC_AUDIT.md` | new | 100 |
| `benchmarks/runs/…/product_quality_track/F_SANDBOX_REGRESSION.md` | new | 130 |
| `AGENT_CLEANUP_REPORT.md` | new | (this file) |

**Total production code delta:** 0 LOC (no engine semantic changes)
**Test LOC added:** 410

## Files Touched

- `crates/pdf-xfa/tests/m3b_phaseE_sandbox_forbidden_globals.rs` (new)
- `crates/pdf-xfa/tests/m3b_phaseF_resource_limits.rs` (new)
- `benchmarks/runs/xfa_enterprise_plan/product_quality_track/F_PANIC_AUDIT.md` (new)
- `benchmarks/runs/xfa_enterprise_plan/product_quality_track/F_SANDBOX_REGRESSION.md` (new)

No existing source files were modified.

## Tests Added

| Test file | Tests | All pass |
|---|---|---|
| `m3b_phaseE_sandbox_forbidden_globals.rs` | 8 | yes |
| `m3b_phaseF_resource_limits.rs` | 4 | yes |
| **Total new** | **12** | **yes** |

## Gate Log

| Gate | Command | Result |
|---|---|---|
| cargo check | `cargo check -p pdf-xfa --features xfa-js-sandboxed` | OK |
| cargo test (total) | `cargo test -p pdf-xfa --features xfa-js-sandboxed` | **576 passed** (baseline 564, target ≥ 572) |
| cargo clippy | `cargo clippy -p pdf-xfa --features xfa-js-sandboxed -- -D warnings` | 0 warnings |
| cargo fmt | `cargo fmt --all --check` | exit 0 |
| private paths | `scripts/check_no_private_paths.sh` | OK, 0 private paths |

## Branch Hygiene Log

- Pre-flight: branch at `090c0a18c` (existing worktree) → reset to `743bb4acc`
  (baseline per plan brief: `743bb4acc = plan branch = 090c0a18c + 1 plan-docs commit`)
- All commits on this branch are authored by this agent
- Zero foreign commits between baseline and HEAD
- Push target: `git push origin HEAD:refs/heads/xfa/product-crash-sandbox-hardening`

## Sub-task Summary

### F-1 — Panic audit re-run

Scanned `crates/pdf-xfa/src/`, `crates/xfa-layout-engine/src/`,
`crates/xfa-dom-resolver/src/`, `crates/formcalc-interpreter/src/`.

- 638 raw regex hits (`.unwrap()` / `.expect()` / `panic!` / `unreachable!`)
- 598 in `#[cfg(test)]` / `#[test]` regions
- **40 non-test production sites**
- **0 unguarded sites** — all carry `// SAFETY:` comments with invariant proof or
  `// INFALLIBLE:` comments for compile-time-embedded data
- 30 `self.expect(…)` hits in `formcalc-interpreter/src/parser.rs` are
  false-positives: custom `fn expect(&mut self, …) -> Result<()>` method,
  all followed by `?`, never panics

Batch A steady-state: confirmed. No regression.

### F-2 — Sandbox forbidden-globals regression tests

8 tests in `m3b_phaseE_sandbox_forbidden_globals.rs`:

- Individual tests for each of: `fetch`, `XMLHttpRequest`, `process`,
  `require`, `Deno`, `Bun`
- `globalThis` escalation test (all 6 via `globalThis[name]`)
- Multi-document persistence test (all 6 absent after `reset_for_new_document`)

All 8 PASS.

### F-3 — Resource limit tests

4 tests in `m3b_phaseF_resource_limits.rs`:

- `timeout_infinite_loop_terminates_within_10s` — 50 ms budget, asserts `Timeout`
- `memory_allocation_bomb_terminates_within_10s` — 4 MiB budget, asserts OOM/Timeout/ScriptError
- `recursion_bomb_terminates_without_process_kill` — 200 ms budget, asserts
  ScriptError/Timeout/StackOverflow, process survives
- `deadline_is_cleared_after_timeout` — verifies deadline cleanup between scripts

All 4 PASS, all within 10 s.

## Verdict

`XFA_PRODUCT_CRASH_SANDBOX_READY`

- Panic audit published and steady-state confirmed (0 unguarded sites)
- 8 forbidden-global tests landed (all denial asserted PASS)
- 4 resource-limit tests landed (all terminated < 10 s)
- `cargo test` ≥ 572: **576 passed**
- Clippy clean, fmt clean, no private paths
