# SDK GA Milestone — Prompt Template

Reusable `/goal`-style template for executing one GA-readiness milestone
(A–E) from [SDK_GA_READINESS_MILESTONE_ROADMAP.md](SDK_GA_READINESS_MILESTONE_ROADMAP.md).
Copy, fill the `<...>` placeholders, and run as a single autonomous goal.
One milestone per run; **do not start the next milestone.**

---

```
/goal

You are the non-XFA PDFluent SDK GA-readiness execution terminal.
Milestone: <A: DX 100% | B: Quality 100% | C: Performance baseline | D: Packaging RC dry-run | E: Claims/GA review>

GOAL
<one-sentence goal from the roadmap for this milestone>.
This run executes ONLY this milestone. Close every assigned matrix row to
green-with-evidence or label it explicitly (no fake GREEN).

NON-GOALS
- No XFA fidelity work (parallel track) except package-naming/claims checks.
- No publish, no deploy, no version bump, no public marketing-claim changes.
- No paid APIs. No GitHub. No private-path leakage. No commit of private artifacts.
- No scope creep into other milestones; out-of-milestone findings are
  recorded as backlog issues, not silently fixed.

INPUTS
- docs/ga-readiness/SDK_GA_READINESS_TAXONOMY.md (status vocabulary)
- docs/ga-readiness/<DEVELOPER_EXPERIENCE|QUALITY_RELIABILITY|PERFORMANCE>_GA_MODEL.md
- benchmarks/runs/ga_readiness_3d/sdk_ga_readiness_matrix.json (rows for this milestone)
- docs/ga-readiness/sdk_ga_readiness_issue_backlog.json (issues <GA-XX-NNN..>)
- prior milestone reports (if any).

PHASES
0. Fresh worktree off latest origin/enterprise/ga-hardening; record HEAD; run
   baseline gates (cargo metadata/check/fmt/clippy -D warnings,
   check_no_private_paths.sh, audit-all-packages.sh --dry-run, binding+license
   parity, core-pdf checker). Write PHASE0 state.
1..N. Execute the milestone's issues (<list GA-XX-NNN ids>). For each:
   implement the deliverable, add its gate, capture evidence, update the
   matrix row status with a referenceable artifact. Tiny measurement
   blockers may be fixed; larger behavior fixes are spun out as new issues.
GATES (must all pass for GREEN)
   - all milestone gates from the roadmap + the per-issue gates
   - cargo fmt/check/clippy -D warnings still green
   - check_no_private_paths.sh; audit-all-packages.sh --dry-run
   - binding parity; license E2E; core-pdf checker; docs/example checker
   - quota log unchanged; claims export unchanged
FINAL
   - update sdk_ga_readiness_matrix.json rows for this milestone
   - write benchmarks/runs/ga_readiness_3d/<milestone>/<MILESTONE>_REPORT.{md,json}

ACCEPTANCE CRITERIA
- Every assigned matrix row is green_proven (or green_but_needs_release_recheck
  with an explicit release-resolution note), OR explicitly labelled
  known_gap/release_blocker/explicit_v1_limitation with rationale + evidence
  pointer + expected developer behaviour. No "mostly green".

GATES (hard)
<list the milestone's gates from the roadmap>

STOP CONDITIONS
<list the milestone's stop conditions from the roadmap>
- Any fake-GREEN temptation: stop and label honestly.
- Any publish/deploy/version-bump need: stop and ask the operator.

FINAL VERDICT (exactly one)
- <SDK_GA_<MILESTONE>_100_PERCENT_GREEN>  (or the milestone's specific verdict)
- or <SDK_GA_<MILESTONE>_BLOCKED_<reason>>

INTEGRATION
- Branch: <quality/sdk-ga-...> (from the roadmap).
- Push to GitLab origin only (no GitHub).
- Merge to enterprise/ga-hardening only if: all gates pass; changes are
  docs/JSON/tests/harness (or milestone-appropriate); no behavior regression;
  no private artifacts. Else explain why not and leave for operator review.

DO NOT START THE NEXT MILESTONE. End with: exact verdict, what changed,
what is now green-with-evidence, what remains, and the recommended next
milestone (named only, not executed).
```

---

## Notes on use

- Keep one milestone per run so each has a clean verdict and a bounded diff.
- Always re-baseline in Phase 0 (the base branch moves; other tracks merge).
- Treat the scoring matrix as the single source of truth for status; every
  status change must cite an artifact.
- The template forbids starting the next milestone — chaining is an operator
  decision after reviewing the verdict.
