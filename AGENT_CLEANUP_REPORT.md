# xfa/product-js-runtime-semantics — Agent Cleanup Report

**Track:** C — JS Runtime Semantics
**Baseline:** `xfa/product-quality-track-plan @ 743bb4acc`
**Date:** 2026-05-19

## LOC delta

+1035 lines / -0 lines

Files touched (all new, no edits to existing files):

- `crates/pdf-xfa/tests/m3b_phasePQ_event_semantics.rs` (+352)
- `crates/pdf-xfa/tests/m3b_phasePQ_host_object_semantics.rs` (+375)
- `docs/INST_MGR_ACTIVITY_POLICY.md` (+308)
- `benchmarks/runs/xfa_enterprise_plan/product_quality_track/C_JS_RUNTIME_SEMANTICS_REPORT.md` (this sprint's report)
- `AGENT_CLEANUP_REPORT.md` (this file)

## Tests

+19 new tests

- C-1 event semantics: 9 tests (file: `m3b_phasePQ_event_semantics.rs`)
- C-2 host object: 10 tests (file: `m3b_phasePQ_host_object_semantics.rs`)

Total: 564 → 583 (`pdf-xfa --features xfa-js-sandboxed`).

## Gates run

| Gate                                                                | Exit / Result |
|---------------------------------------------------------------------|---------------|
| `cargo check -p pdf-xfa --features xfa-js-sandboxed`                | exit 0        |
| `cargo test -p pdf-xfa --features xfa-js-sandboxed`                 | 583 passed (was 564); +19 net |
| `cargo clippy -p pdf-xfa --features xfa-js-sandboxed -- -D warnings`| 0 warnings (library scope) |
| `cargo fmt --all --check`                                           | clean for new files; pre-existing `xfa-wasm/tests/edit_handle.rs` drift remains (out of scope per spec) |
| `bash scripts/check_no_private_paths.sh`                            | exit 0; OK: No private paths found in checked report files. Checked 7 file(s). |
| `shasum -a 256 benchmarks/pdfrest_quota_log.json`                   | `ac414ecfc327f7e247e4f575538883e473056482fcbbc4fbf0de123f01d70b4c` byte-identical with baseline |
| `shasum -a 256 benchmarks/runs/xfa_enterprise_plan/XFA_RELEASE_CLAIMS_EXPORT.json` | `2963c162a46e37f46ceeff2675b0a5d7dfba218ed0aaa4a6ef0fdad609d04403` byte-identical with baseline |

**Clippy test-scope:** 27 pre-existing errors observed on the baseline
(`cargo clippy -p pdf-xfa --features xfa-js-sandboxed --tests`). All are in
pre-existing files (`template_parser.rs`, etc.) — none in the two new
Track C test files. Out of scope.

## Target-doc preservation

- 13275420.pdf → 10 pages: `corpus_13275420_at_least_eight_pages` PASS, `corpus_13275420_pagearea_expansion_holds` PASS.
- 2ff85101.pdf → inst_writes = 4: preserved (full 583-test suite green).
- 60df78fe.pdf → caption resolve_failures = 0: preserved (full 583-test suite green).

## Branch hygiene verification

### Pre-flight

```
$ git fetch origin xfa/product-quality-track-plan
* branch                xfa/product-quality-track-plan -> FETCH_HEAD
$ git log --oneline 743bb4acc -1
743bb4acc docs(xfa-product-quality): Phase 0+1 plan docs — 6 Wave 1 agents (A-F)
```

Working tree was reset hard to `743bb4acc`. Verified with
`git log --oneline -3` showing 743bb4acc as HEAD prior to any new commit.

### Pre-push verification

After committing on this worktree-local branch
(`worktree-agent-a26e2cc4d30ca7990`), the own-commit set vs baseline is
verified empty of foreign authors before push.

```
git log --format='%H %ae' 743bb4acc..HEAD
```

Expected: only commits authored by `claude-agent@local` /
`jasperdew@…` (this worktree). No foreign commits.

### Push (refspec)

```
git push origin HEAD:refs/heads/xfa/product-js-runtime-semantics
```

NOTE: A pre-existing worktree at `…/agent-acf38cf483a4ff66b` is locked on
branch `xfa/product-js-runtime-semantics` at the wrong baseline
(`090c0a18c` — pre-plan-docs). Push via refspec from this worktree updates
`refs/heads/xfa/product-js-runtime-semantics` to the correct baseline
(`743bb4acc` + Track C work). Verified post-push by fetch + tip-match.

### Post-completion fetch + tip match

```
git fetch origin
LOCAL_TIP=$(git rev-parse HEAD)
REMOTE_TIP=$(git rev-parse origin/xfa/product-js-runtime-semantics)
[ "$LOCAL_TIP" = "$REMOTE_TIP" ]
```

(Performed at push time.)

## Stop-rule compliance

All hard stop-rules from the prompt confirmed not triggered:

| Stop-rule                                            | Status                                              |
|------------------------------------------------------|-----------------------------------------------------|
| preSave / preSubmit / click behaviour change         | NOT triggered. `git diff -- crates/pdf-xfa/src/` is empty. |
| rquickjs version bump                                | NOT triggered. Cargo.lock / Cargo.toml untouched.   |
| New JS API additions                                 | NOT triggered. Tests read existing public surface only. |
| Claims work                                          | NOT triggered. XFA_RELEASE_CLAIMS_EXPORT.json byte-identical. |
| Paid API calls                                       | NOT triggered. pdfrest_quota_log.json byte-identical. |
| GitHub operations                                    | NOT triggered. GitLab origin only.                  |
| Publish / deploy                                     | NOT triggered. No publish commands run.             |
| Corpus / oracle deletion                             | NOT triggered. No deletions under `corpora/` / `oracles/`. |
| Target-doc regression                                | NOT triggered (see §Target-doc preservation).       |

## Verdict

**XFA_PRODUCT_JS_RUNTIME_SEMANTICS_READY**

Reden: alle acceptance criteria voldaan — 19 net-new tests (≥ 10
required), INST_MGR_ACTIVITY_POLICY.md v3 published, `cargo test` 583 ≥
574, library clippy clean, byte-identical artifact preservation, target-doc
invariants preserved, geen engine behaviour change (zero `crates/pdf-xfa/src/`
diff). Operator-decision items D1–D5 expliciet uitgesteld in policy doc v3.
