# PDFluent CLI — Binary Coexistence & Future Takeover Strategy

**Current state (DO NOT change in this milestone):**
- Public CLI binary = **`pdfluent-cli`** (crate `crates/pdfluent-cli`, `publish = false`).
- Internal binary = **`pdfluent`** (crate `crates/xfa-cli`), consumed by XFA/benchmark tooling
  via `target/release/pdfluent`.
- They **do not collide** (distinct names). No takeover is performed here.

## Known internal consumers of `target/release/pdfluent`
  - scripts/corpus_full_replay_benchmark.sh
  - scripts/corpus_js_runtime_benchmark.sh
  - scripts/m56_common.py
  - scripts/m59_vertical_profile.py
  - scripts/perf_regression_gate.sh
  - scripts/probe_javascript_policy_gate.py
  - scripts/qf2a_sf85p_measure.py
  - scripts/quality/pdf_repairability_scan.py
  - scripts/release/package_cabi.sh
  - scripts/render_pdfluent_candidates.py
  - scripts/run_benchmarks.sh
  - scripts/run_dual_oracle.py
  - scripts/run_gate_ssim.py
  - scripts/run_weekly_benchmark.sh
  - scripts/run_xfa_benchmark.py
  - scripts/run_xfa_corpus_gate_cron.sh
  - scripts/w3g_namedcap_measure.py
  - scripts/xfa/d13_preflight_check.sh
  - scripts/xfa_adobe_oracle_fidelity_compare.py
  - scripts/xfa_corpus_gate_runner.py
  - scripts/xfa_corpus_smoke_check.py
  - scripts/xfa_fidelity/run_fresh_merge_policy_comparison.py
  - scripts/xfa_fidelity/run_pdfluent_oracle_batch.py
  - scripts/xfa_formcalc_residual_scan.py
  - scripts/xfa_longform_perf_gate.py
  - scripts/xfa_unresolved_identifier_scan.py

(Plus any operator/CI invocation not in-repo — audit before takeover.)

## Why the public CLI is NOT named `pdfluent` yet
Renaming `pdfluent-cli`→`pdfluent` would shadow the internal binary and silently break the
consumers above (wrong binary, wrong CLI surface). The clap program name currently reports
`pdfluent` (a cosmetic mismatch) — also deferred to the takeover, NOT fixed by renaming.

## Future takeover sequencing (separate milestone)
1. **Inventory & freeze**: confirm the full consumer list (repo + CI + operator habits).
2. **Rename internal**: give `xfa-cli`'s binary a distinct internal name (e.g. `xfa-runner`);
   update all consumers above + docs in one commit; keep a transition symlink/alias.
3. **Promote public**: rename `pdfluent-cli`→`pdfluent` and align the clap `name`; update docs/completions.
4. **Transition window**: ship both names (alias) for one release; announce.
5. **Verify**: full XFA/benchmark gate run green against the renamed internal binary.

## Rollback
Each step is a single revertible commit; the transition alias means a bad step can be rolled
back without breaking consumers. No takeover step is started until the public CLI is
release-approved and every consumer is repointed in a dry-run branch first.

## Decision class
This is **future takeover work**, gated behind a **business decision** to publish the public
CLI under the `pdfluent` name. Not an engineering blocker for the current `pdfluent-cli`.
