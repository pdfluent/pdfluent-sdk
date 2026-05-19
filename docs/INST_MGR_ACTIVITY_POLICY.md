# XFA instanceManager / JS Runtime Activity Policy

**Version:** v3 (consolidated)
**Status:** Active — describes CURRENT engine behaviour. No behaviour change introduced by this document.
**Sprint:** XFA Product Quality Wave 1, Track C (JS Runtime Semantics)
**Owner:** XFA JS runtime + layout engine
**Date:** 2026-05-19

## Scope

This document consolidates the contract between

- the XFA JavaScript dispatch path (`crates/pdf-xfa/src/dynamic.rs`),
- the sandboxed JS runtime (`crates/pdf-xfa/src/js_runtime/`),
- the host bindings (`crates/pdf-xfa/src/js_runtime/host.rs`), and
- the layout engine (`crates/xfa-layout-engine/`)

for **all script-bearing activities** during static flatten. It supersedes
`benchmarks/INST_MGR_ACTIVITY_POLICY.md` v1 (scope: instanceManager × five
lifecycle activities) and incorporates the operator-decision matrix from
`benchmarks/runs/xfa_enterprise_plan/sprint2_batchB/JS2_02_EVENT_POLICY_MATRIX.md`.

This document is **prescriptive** for the current contract and **descriptive**
for the operator-pending decisions. Any change to the allowlist requires:

1. Operator approval of the controversial-activity decisions in §6.
2. An architectural review and update of this document.
3. Tests pinning the new contract (see §8).

The defence-in-depth duplication between the dispatch layer and the
host-binding layer is intentional.

## Versioning

| Version | Date       | Scope                                                              | Status      |
|---------|------------|--------------------------------------------------------------------|-------------|
| v1      | 2026-05-17 | `instanceManager.{add,remove,set}Instance` × 5 lifecycle events    | Superseded  |
| v2      | (skipped)  | reserved for first operator decision on D1–D5 in JS2_02 matrix     | Pending     |
| **v3**  | 2026-05-19 | Consolidated policy + cross-ref to JS2-02 matrix; no behaviour change | **Active** |

The intentional gap at v2 mirrors the JS2-02 matrix's "v2 = operator-decision
committed". When the operator commits decisions D1–D5 (see §6), the policy
must be re-issued as v4 with the chosen options inlined.

## 1. Current state — allowlist & denylist (descriptive)

The dispatch layer routes JavaScript event scripts through
`activity_allowed_for_sandbox` (defined at
`crates/pdf-xfa/src/js_runtime/mod.rs::SANDBOX_ACTIVITY_ALLOWLIST`).

### 1.1 Allowlist (executed during flatten)

| Activity     | Allowed | Rationale                                                                                                  |
|--------------|---------|------------------------------------------------------------------------------------------------------------|
| `initialize` | yes     | XFA 3.3 §9.3: runs on form load; required for data-driven page composition (e.g. PIT-37 NIP/PESEL toggle). |
| `calculate`  | yes     | XFA 3.3 §9.3: iterates until stable; needed for derived values and conditional instance counts.            |
| `validate`   | yes     | Runs after calculate; instance count adjustments based on validation outcomes are legitimate.              |
| `docReady`   | yes     | Adobe runs this once the form DOM is ready; corpus uses it for cross-section consistency fixes.            |
| `layoutReady`| yes     | Final hook before layout; corpus uses it for last-mile structural adjustments.                             |

### 1.2 Denylist (silently skipped during flatten)

| Activity         | Allowed | Rationale                                                                                  |
|------------------|---------|--------------------------------------------------------------------------------------------|
| `preSave`        | no      | **Operator-decision pending** (JS2-02 D1). Today: deny. See §6.1.                          |
| `preSubmit`      | no      | Transport action. Submission is not a flatten concern. JS2-02 D2 confirms status quo.      |
| `postSave`       | no      | Post-write hook; circular for flatten. JS2-02 R08.                                         |
| `postSubmit`     | no      | Transport action.                                                                          |
| `click`          | no      | UI event; never fires during flatten. JS2-02 D3 confirms status quo.                       |
| `change`         | no      | UI event; depends on `xfa.event.newText` / user input.                                     |
| `enter`          | no      | UI event (field focus).                                                                    |
| `exit`           | no      | UI event (field blur).                                                                     |
| `mouseEnter`     | no      | UI event.                                                                                  |
| `mouseExit`      | no      | UI event.                                                                                  |
| `mouseDown`      | no      | UI event.                                                                                  |
| `mouseUp`        | no      | UI event.                                                                                  |
| `prePrint`       | no      | Print-time hook; flatten produces a print-ready PDF but does not simulate print.           |
| `postPrint`      | no      | Print-time hook.                                                                           |
| `preOpen`        | no      | Adobe's pre-render hook; corpus scripts here typically do UI-only work.                    |
| `ready`          | no      | Adobe's `form:ready` / `layout:ready` variants — covered by `layoutReady` / `docReady`.    |
| `full`           | no      | Fires when subform `occur.max` reached; mutations would loop.                              |
| (other / `None`) | no      | Default-deny. Scripts without an explicit `activity` attribute never run.                  |

**Code reference:** `crates/pdf-xfa/src/js_runtime/mod.rs`:

```rust
pub const SANDBOX_ACTIVITY_ALLOWLIST: &[&str] = &[
    "initialize",
    "calculate",
    "validate",
    "docReady",
    "layoutReady",
];
```

## 2. Defence-in-depth

The activity gate is enforced at two independent layers:

- **Dispatch (`apply_dynamic_scripts_with_runtime`).** Filters JavaScript
  scripts by activity *before* asking the runtime to execute them. Scripts
  outside the allowlist count as `js_skipped` and never reach QuickJS.
- **Host (`HostBindings::write_activity_allowed`).** Refuses any mutating
  host call (`instance_add` / `instance_remove` / `instance_set` /
  `set_raw_value` / `list_clear` / `list_add`) whose current activity is
  outside the allowlist, returning `Err(())` and bumping `binding_errors`.

Both checks are exercised by
`crates/pdf-xfa/tests/inst_mgr_real_mutations.rs::click_activity_instance_mutation_is_blocked_at_host_layer`
and reinforced by Track C tests (see §8).

## 3. End-to-end mutation guarantee

When a script under an **allowed** activity calls `addInstance()`,
`removeInstance(idx)`, or `setInstances(n)`, the implementation guarantees:

1. The host-binding layer (`HostBindings::instance_{add,remove,set}_inner`)
   mutates `FormTree.nodes` and the parent's `children` vec **before**
   returning to the script.
2. The `js_instance_writes` counter increments **only** when the form tree
   was actually mutated.
3. The layout engine (`LayoutEngine::layout`) is invoked **after** all
   scripts have run, so every successful instance mutation is visible to
   the layout DOM.
4. The rollback heuristic in `dynamic.rs::should_rollback` is structurally
   symmetric: when a pass is rejected, runtime-created clones are truncated
   and the children lists are restored to their pre-script shape.

## 4. Host object surface (descriptive)

The sandboxed runtime exposes the following host objects to script context.
All bindings are pre-existing; none are introduced by this document.

| Object              | Kind                        | Origin            | Tests pin                                                |
|---------------------|-----------------------------|-------------------|----------------------------------------------------------|
| `xfa`               | namespace                   | Phase D-γ         | `m3b_phasePQ_host_object_semantics.rs`                   |
| `xfa.form`          | Proxy + chainable sentinel  | JS2-01            | `m3b_phaseE_js2_01.rs`, `m3b_phasePQ_host_object_semantics.rs` |
| `form`              | identity-equal alias        | JS2-01            | `m3b_phaseE_js2_01.rs`                                   |
| `xfa.host`          | namespace (read-only props) | Phase D-γ + JS2-01| `m3b_phaseE_host_stubs.rs`, `m3b_phasePQ_host_object_semantics.rs` |
| `xfa.host.messageBox` | thunk → returns `0`       | Phase E           | `m3b_phaseE_host_stubs.rs`, `m3b_phasePQ_host_object_semantics.rs` |
| `xfa.host.closeDoc` | thunk → undefined           | JS2-01            | `m3b_phaseE_js2_01.rs`                                   |
| `xfa.resolveNode`   | function → `null` on miss   | Phase D-γ         | `m3b_phasePQ_host_object_semantics.rs`                   |
| `xfa.resolveNodes`  | function → frozen `[]`      | Phase D-γ         | `m3b_phasePQ_host_object_semantics.rs`                   |
| `app.alert`         | thunk → returns `0`         | Phase E           | `m3b_phaseE_host_stubs.rs`                               |
| `app.messageBox`    | alias of `app.alert`        | JS2-01            | `m3b_phaseE_js2_01.rs`                                   |
| `app.closeDoc`      | alias of `xfa.host.closeDoc`| JS2-01            | `m3b_phaseE_js2_01.rs`                                   |
| `util`              | frozen `{printd, printx, scand}` | Phase D-γ    | `m3b_phaseE_data2_02.rs`, `m3b_phasePQ_host_object_semantics.rs` |
| `console`           | `{log, warn, error, info, debug, trace}` capture-only | Phase C-α | `m3b_phaseE_data2_02.rs`, `m3b_phasePQ_host_object_semantics.rs` |
| `event`             | scalar surface with safe defaults | JS2-01      | `m3b_phaseE_js2_01.rs`, `m3b_phasePQ_host_object_semantics.rs` |

**Cluster C contract (load-bearing):** `xfa.resolveNode("DoesNotExist")` and
`xfa.form.resolveNode("DoesNotExist")` both return JS `null`, **not** a
chainable sentinel. The dispatch site and downstream tests rely on the null
signal to surface real schema mismatches.

**Capture-only contract for console:** `console.{log, warn, error, info,
debug, trace}` are silent no-ops. They MUST NOT bump
`js_unsupported_host_calls`, because they have no host effect.

**Interactive-thunk counter contract:** Every call to an interactive thunk
(`xfa.host.messageBox`, `xfa.host.closeDoc`, `app.alert`, `app.messageBox`,
`app.closeDoc`, etc.) bumps `js_unsupported_host_calls` exactly once.

## 5. Cross-document isolation

The runtime adapter (`QuickJsRuntime`) is reusable across documents. Each
`flatten` call issues:

1. `runtime.reset_for_new_document()` — clears per-document state, bumps the
   host generation counter, resets all counters including
   `js_unsupported_host_calls`.
2. `runtime.set_form_handle()` — installs the current FormTree.

Stale node handles from a previous document are rejected via the
`generation` comparison. Pinned by
`m3b_phasePQ_host_object_semantics::host_object_state_does_not_leak_across_documents`.

## 6. Operator-decision matrix (cross-reference)

The JS2-02 event-policy matrix
(`benchmarks/runs/xfa_enterprise_plan/sprint2_batchB/JS2_02_EVENT_POLICY_MATRIX.md`)
captures five operator decisions:

### 6.1 D1 — preSave during flatten

- **Status quo (current):** deny.
- **Recommended:** D1.B (gated allow + feature flag + per-class host policy).
- **Risk delta if approved:** +1 feature flag in public Cargo surface, +18
  tests; no semantic change unless flag flipped; reversible.

**Until D1 is committed, `preSave` remains in the denylist.** Track C
(this sprint) does NOT change behaviour. See JS2-02 §6.1 for the option
comparison.

### 6.2 D2 — preSubmit during flatten

- **Status quo (current):** deny.
- **Recommended:** D2.A (confirm deny). Flatten is not transport.
- **Action:** no change.

### 6.3 D3 — click / UI events during flatten

- **Status quo (current):** deny.
- **Recommended:** D3.A (confirm deny). Non-reproducible click ordering;
  exclusive-state collapse.
- **Action:** no change.

### 6.4 D4 — C7 host-call typed-error policy

- **Status quo (current):** silent `unsupported_host_calls` bump on every
  `xfa.host.*` interactive call; return safe defaults (`messageBox → 0`,
  `closeDoc → undefined`, etc.).
- **Recommended:** D4.A (replace silent no-op with catch-able
  `XfaActivityError`). Independent of D1–D3.
- **Risk delta if approved:** scripts that today silently no-op
  (e.g. `xfa.host.messageBox`) would see a catch-able error; observable in
  `js_runtime_errors`; corpus replay required.

**Until D4 is committed, the current silent-no-op + counter-bump contract
holds.**

### 6.5 D5 — Policy doc cadence

This document (v3) reflects the **current** state. The operator should
re-issue as **v4** after committing D1 and/or D4.

## 7. Verification (current corpus baselines)

Per-document baselines, release build, `XFA_JS_EXECUTION_MODE=sandboxed`:

| Document     | js_executed | js_instance_writes | js_runtime_errors | Pages | Notes                                                                                       |
|--------------|-------------|--------------------|-------------------|-------|---------------------------------------------------------------------------------------------|
| 2ff85101.pdf | 159         | 4                  | 11                | 9     | setInstances on `_IDPESEL` / `_IDNIP` via `initialize` + `change`. Layout reflects writes.   |
| 3963b9b6.pdf | 29          | 0                  | 4                 | 3     | addInstance/removeInstance guarded by data conditions that don't fire with empty dataset.    |
| 13275420.pdf | 169         | 0                  | 30                | 5/10  | All `instanceManager` calls are click handlers; intentionally not executed during flatten.   |
| 60df78fe.pdf | (varies)    | 0                  | 0                 | (varies) | Caption resolve_failures = 0 (DATA2-02 cluster fix preserved).                          |

Target-doc preservation invariants enforced by every Wave 1 agent:

- `13275420.pdf` → 10 pages (`corpus_13275420_at_least_eight_pages` PASS).
- `2ff85101.pdf` → `inst_writes = 4`.
- `60df78fe.pdf` → caption `resolve_failures = 0`.

## 8. Tests pinning the contract

| Layer                  | File                                                        | Count |
|------------------------|-------------------------------------------------------------|-------|
| Sandbox unit tests     | `crates/pdf-xfa/src/js_runtime/mod.rs::tests`               | 4     |
| End-to-end allowlist   | `crates/pdf-xfa/tests/inst_mgr_real_mutations.rs`           | 12    |
| Host stub gap closure  | `crates/pdf-xfa/tests/m3b_phaseE_js2_01.rs`                 | 17    |
| DATA2-02 cluster fixes | `crates/pdf-xfa/tests/m3b_phaseE_data2_02.rs`               | 5+    |
| Host interactive thunks| `crates/pdf-xfa/tests/m3b_phaseE_host_stubs.rs`             | 13+   |
| **Track C event semantics** | `crates/pdf-xfa/tests/m3b_phasePQ_event_semantics.rs`  | **9** |
| **Track C host object** | `crates/pdf-xfa/tests/m3b_phasePQ_host_object_semantics.rs`| **10**|

The Track C suite (this sprint) adds 19 net-new tests that pin
- the canonical allowlist constant,
- every allowlist activity executing,
- every denylist activity silently skipping,
- the `activity_allowed_for_sandbox` helper,
- multi-activity execution on a single node,
- mixed allow / deny partitioning,
- mutation-observability for allowed activities,
- mutation-suppression for denied activities,
- the zero-script baseline,
- top-level host object exposure,
- arbitrary-depth chainable sentinels,
- the `xfa.host.messageBox → 0` cancelled sentinel,
- read-only `xfa.host.*` properties,
- the Cluster C null-on-missing contract,
- direct event-script visibility of `util` and `console`,
- per-document counter reset,
- the no-forbidden-globals invariant,
- and `xfa.event` deterministic defaults.

## 9. Stop-rules (this document)

This document MUST NOT be edited to change behaviour. Edits permitted:

- Adding new entries to §1 reflecting code-side allowlist changes (which
  themselves require operator approval per §6).
- Updating §6 cross-references when the JS2-02 matrix changes.
- Updating §7 baselines after corpus replay.
- Updating §8 when new tests land.

Edits forbidden without operator decision commit:

- Moving any activity from §1.2 (denylist) to §1.1 (allowlist).
- Removing any test pin in §8 without a follow-up doc update.
- Promising behaviour changes (this is a description of v3, not a roadmap).

## 10. References

- XFA 3.3 §9 (event model), §10 (data interchange), §11.6.4 (instanceManager),
  §27.10.4 (script execution order).
- `crates/pdf-xfa/src/js_runtime/mod.rs::SANDBOX_ACTIVITY_ALLOWLIST` —
  source of truth for §1.1.
- `crates/pdf-xfa/src/js_runtime/host.rs::write_activity_allowed` — host
  side of defence-in-depth.
- `crates/pdf-xfa/src/dynamic.rs::apply_dynamic_scripts_with_runtime` —
  dispatch entry point.
- `benchmarks/INST_MGR_ACTIVITY_POLICY.md` — v1 (superseded; preserved for
  history).
- `benchmarks/runs/xfa_enterprise_plan/sprint2_batchB/JS2_02_EVENT_POLICY_MATRIX.md`
  — operator-decision input for D1–D5.
- `benchmarks/runs/xfa_enterprise_plan/sprint2_batchB/JS2_01_HOST_STUB_GAP_REPORT.md`
  — host stub gap closure inventory.
- `benchmarks/JS_SANDBOX_SECURITY_AUDIT.md` — sandbox capability inventory.
