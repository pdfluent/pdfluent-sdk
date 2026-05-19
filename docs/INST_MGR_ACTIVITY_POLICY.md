# XFA instanceManager / JS Runtime Activity Policy

**Version:** v4 (Wave 3 closure)
**Status:** Active — describes CURRENT engine behaviour. **No behaviour change introduced by this document.** v4 promotes v3's "operator-pending" notes for D1/D2/D3 into an explicit closure decision (status quo confirmed pending operator commit), pins the deny contract with W3-B tests, and documents the gated-allow plan for D1.B that an operator may commit later WITHOUT a silent default change.
**Sprint:** XFA Product Quality Wave 3, Track B (Event Policy Closure)
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
| v3      | 2026-05-19 | Consolidated policy + cross-ref to JS2-02 matrix; no behaviour change | Superseded |
| **v4**  | 2026-05-19 | Wave 3 W3-B closure: explicit status-quo confirmation for D1/D2/D3, FINAL policy table (§1.3), W3-B regression tests (§8); **no behaviour change** | **Active** |

The intentional gap at v2 mirrors the JS2-02 matrix's "v2 = operator-decision
committed". v4 supersedes v3 by **explicitly closing** D1/D2/D3 against the
current engine semantics (status quo: deny during flatten). Closure means
the deny contract is now load-bearing and regression-pinned (§8). It does
NOT mean the operator has committed to a permanent decision: if the operator
later commits D1.B (gated allow for `preSave`) per §6.1, the policy must be
re-issued as v5 with the feature flag wired up; that change is **opt-in**
behind `XFA_PRESAVE_DURING_FLATTEN`, default off, never a silent flip.

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

### 1.3 FINAL policy table (v4 closure — current build)

This is the canonical allow / deny / sandboxed-host-call decision for every
event activity Adobe defines in XFA 3.3 §9.3, projected against the seven
host method classes from `JS2_02_EVENT_POLICY_MATRIX.md`. Cells reflect the
**current** engine. No code change is implied by this table; W3-B promotes
the status-quo into the regression suite (see §8).

Column legend mirrors `JS2_02_EVENT_POLICY_MATRIX.md` §2:
C1 instance mutations, C2 value reads, C3 value writes, C4 validation hooks,
C5 data binding (read), C6 SOM resolve, C7 host application calls.

Cell legend:
- **allow** — script body runs; host call performs effect.
- **deny** — dispatch layer skips the entire script body (`js_skipped`);
  no host call ever runs. Read columns marked allow are unreachable when
  the row is denied at dispatch.
- **sandboxed-host** (`sb-host`) — script body runs; the specific host call
  returns a safe default (`messageBox → 0`, `closeDoc → undefined`, …) and
  bumps `js_unsupported_host_calls`. Pending D4 commit, the behaviour is a
  silent no-op; once D4.A lands, the bump is a catch-able `XfaActivityError`.

| Activity     | Phase     | Flatten? | C1 inst mut | C2 reads | C3 val writes | C4 validate hook | C5 data bind | C6 SOM | C7 host calls | Policy doc cross-ref |
|--------------|-----------|---------|-------------|----------|---------------|------------------|--------------|--------|---------------|----------------------|
| initialize   | lifecycle | yes     | allow       | allow    | allow         | allow            | allow        | allow  | sb-host       | §1.1, §6 D0          |
| calculate    | lifecycle | yes     | allow       | allow    | allow         | allow            | allow        | allow  | sb-host       | §1.1, §6 D0          |
| validate     | lifecycle | yes     | allow       | allow    | allow         | allow            | allow        | allow  | sb-host       | §1.1, §6 D0          |
| docReady     | lifecycle | yes     | allow       | allow    | allow         | allow            | allow        | allow  | sb-host       | §1.1, §6 D0          |
| layoutReady  | lifecycle | yes     | allow       | allow    | allow         | allow            | allow        | allow  | sb-host       | §1.1, §6 D0          |
| **preSave**  | save      | **no**  | **deny**    | n/a      | **deny**      | n/a              | n/a          | n/a    | n/a           | §1.2, §6.1 (D1.A)    |
| **preSubmit**| submit    | **no**  | **deny**    | n/a      | **deny**      | n/a              | n/a          | n/a    | n/a           | §1.2, §6.2 (D2.A)    |
| postSave     | save      | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2, §6 R08         |
| postSubmit   | submit    | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| postOpen     | open      | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2 R09             |
| **click**    | UI        | **no**  | **deny**    | n/a      | **deny**      | n/a              | n/a          | n/a    | n/a           | §1.2, §6.3 (D3.A)    |
| change       | UI        | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| enter        | UI        | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| exit         | UI        | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| mouseEnter   | UI        | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| mouseExit    | UI        | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| mouseDown    | UI        | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| mouseUp      | UI        | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| prePrint     | print     | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| postPrint    | print     | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| preOpen      | open      | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| ready        | open      | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |
| full         | data      | no      | deny        | n/a      | deny          | n/a              | n/a          | n/a    | n/a           | §1.2                 |

**Bolded rows** are the W3-B closure trio (`preSave`, `preSubmit`, `click`).
W3-B does NOT change the cell values; it pins them with regression tests
(see §8) so a silent default flip is no longer possible at the dispatch
layer or the host-binding layer.

**Read-column entries marked `n/a` for denied rows.** A denied row never
executes its script body, so the read-column distinction (C2 / C4 / C5 / C6
/ C7) is unreachable in practice. The matrix retains the columns for
forward-compatibility with the gated-allow path described in §6.1 — if
D1.B is committed for `preSave`, the `n/a` cells become `allow` (reads) and
`sb-host` (C7) while C1 / C3 acquire the new per-method-class gate.

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

- **Status quo (current, v4 closed):** deny. **Pinned by W3-B tests** in
  `crates/pdf-xfa/tests/m3b_phasePQ_event_policy_closure_w3b.rs`
  (`w3b_closure_presave_script_is_skipped_at_dispatch`,
  `w3b_closure_host_layer_refuses_mutations_for_all_three_activities`).
- **Recommended:** D1.B (gated allow + feature flag + per-class host policy).
- **Operator commit status:** **not yet committed.** Until operator commits
  D1.B, the current deny contract is the FINAL contract.
- **Risk delta if approved later:** +1 feature flag in public Cargo surface
  (`XFA_PRESAVE_DURING_FLATTEN`, default OFF), +18 tests; no semantic change
  unless the flag is flipped; reversible by removing the flag.
- **Implementation plan (deferred — only if D1.B is committed):**
  1. Extend `SANDBOX_ACTIVITY_ALLOWLIST` behind a Cargo feature `presave-during-flatten`, default OFF.
  2. Generalise `write_activity_allowed` → `host_call_allowed(class)` so C7 keeps `sb-host` semantics even when C1/C3 are opened up.
  3. Add `xfa.event.target == "flatten"` marker so scripts can branch.
  4. Re-issue this document as v5 with the new row/column cells.
  5. Corpus replay required before flipping the flag default.

**Until D1.B is committed, `preSave` remains in the denylist.** v4 (this
document) does NOT change behaviour. See JS2-02 §6.1 for the option
comparison.

### 6.2 D2 — preSubmit during flatten

- **Status quo (current, v4 closed):** deny. **Pinned by W3-B tests**
  (`w3b_closure_presubmit_script_is_skipped_at_dispatch`,
  `w3b_closure_host_layer_refuses_mutations_for_all_three_activities`).
- **Recommended:** D2.A (confirm deny). Flatten is not transport.
- **Operator commit status:** D2.A treated as **permanent** in v4 — there
  is no corpus evidence for the alternative and Adobe's transport semantics
  are out of scope for static flatten. No re-issue planned unless evidence
  arrives.
- **Action:** no change.

### 6.3 D3 — click / UI events during flatten

- **Status quo (current, v4 closed):** deny. **Pinned by W3-B tests**
  (`w3b_closure_click_script_is_skipped_at_dispatch`,
  `w3b_closure_host_layer_refuses_mutations_for_all_three_activities`,
  `w3b_closure_activity_helper_denies_w3b_trio_under_all_casings`) and by
  the existing `inst_mgr_real_mutations::click_activity_instance_mutation_*`
  pair.
- **Recommended:** D3.A (confirm deny). Non-reproducible click ordering;
  exclusive-state collapse.
- **Operator commit status:** D3.A treated as **permanent** in v4 —
  `13275420.pdf` (10 pages with all click handlers skipped) is the
  load-bearing corpus evidence that the deny path produces correct output.
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
| Track C event semantics | `crates/pdf-xfa/tests/m3b_phasePQ_event_semantics.rs`      | 9     |
| Track C host object     | `crates/pdf-xfa/tests/m3b_phasePQ_host_object_semantics.rs`| 10    |
| **W3-B policy closure** | `crates/pdf-xfa/tests/m3b_phasePQ_event_policy_closure_w3b.rs` | **7** |

The Track C suite (Wave 1) adds 19 net-new tests that pin
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

The **W3-B closure suite** (Wave 3, this document's v4 promotion) adds 7
net-new tests that pin the FINAL policy table (§1.3) against silent
default-drift:

- `w3b_closure_presave_presubmit_click_are_not_in_allowlist` — pins the
  closure invariant: the three controversial activities are NOT in
  `SANDBOX_ACTIVITY_ALLOWLIST` and the helper rejects them.
- `w3b_closure_presave_script_is_skipped_at_dispatch` — pins D1.A deny at
  the dispatch gate (throw body never reaches QuickJS).
- `w3b_closure_presubmit_script_is_skipped_at_dispatch` — pins D2.A deny
  (mutation attempt never lands on the field).
- `w3b_closure_click_script_is_skipped_at_dispatch` — pins D3.A deny
  (mutation + throw body skipped together).
- `w3b_closure_host_layer_refuses_mutations_for_all_three_activities` —
  defence-in-depth: even bypassing dispatch, the host refuses
  `instance_add` / `instance_remove` / `instance_set` for preSave /
  preSubmit / click (and bumps `binding_errors`).
- `w3b_closure_mixed_three_denied_plus_one_allowed_partitions_correctly`
  — three denied + one allowed on the same node partitions cleanly; throws
  never count because denied bodies never execute.
- `w3b_closure_activity_helper_denies_w3b_trio_under_all_casings` — pins
  case-sensitivity of `activity_allowed_for_sandbox`; any "tolerant"
  rewrite (case-insensitive, trim, alias-table) would constitute a silent
  default change and trip this test first.

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
- `crates/pdf-xfa/tests/m3b_phasePQ_event_policy_closure_w3b.rs` — W3-B
  regression suite that pins the §1.3 FINAL policy table.
- `benchmarks/runs/xfa_enterprise_plan/product_quality_track/WAVE3_EXECUTION_PLAN.md`
  §W3-B — track brief.
- `benchmarks/runs/xfa_enterprise_plan/product_quality_track/W3_B_REPORT.md`
  — Wave 3 closure report (this document's promotion evidence).
