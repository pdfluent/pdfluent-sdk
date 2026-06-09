# Differential testing — unification status

Engineering note for the differential-rendering test infrastructure (Phase A,
milestone A3). Scope of this slice: establish a single canonical comparison
primitive + gate and prove it can fail; the broader migration of the offline
Python oracles into the Rust harness is deferred (below).

## Canonical primitive (this slice)

SSIM and the differential gate now live in one place: the `pdf-diff` crate.

- `pdf_diff::ssim::compute_ssim(a, wa, ha, b, wb, hb) -> f64` — RGBA, 8×8
  windows, 50% overlap. Previously duplicated; `xfa-test-runner`'s
  `oracles::ssim` now re-exports it (single definition, no drift).
- `pdf_diff::gate` — the cross-engine match gate:
  - `GATE_SSIM_THRESHOLD = 0.75` (cross-engine tolerance, calibrated from the
    measured mutool-vs-pdftoppm SSIM distribution the render oracles use).
  - `DifferentialVerdict { Match, Regression, OracleUnavailable }`.
  - `classify(ssim)` / `compare(ours, reference)`.

Two distinct threshold models, kept separate on purpose:

| Model | Compares | Threshold | Where |
|---|---|---|---|
| Cross-engine match | our render vs a *different* engine (mutool/Poppler/Adobe) | `GATE_SSIM_THRESHOLD = 0.75` | `pdf_diff::gate` |
| Same-engine regression | our render vs our own frozen baseline | 0.005 SSIM drop | `gate-ci.yml` + `corpus/CI_BASELINE.json` |

## Pass / fail

- SSIM ≥ 0.75 → `Match`. SSIM < 0.75 → `Regression` (gate fails).
- A missing reference renderer (no `mutool` binary, no pdfRest key) →
  `OracleUnavailable`: a **skip**, never a pass or fail. This is what keeps the
  gate non-flaky and free of any required CI secret.

## Proof the gate can fail (planted regression)

`pdf_diff::gate` ships a deterministic self-test (`cargo test -p pdf-diff`),
with **no external renderer dependency**:

- `identical_renders_match` — identical buffers → SSIM ≈ 1.0 → `Match`.
- `divergent_render_is_a_regression` — solid black vs solid white → SSIM far
  below 0.75 → `Regression`. This is the planted regression: it proves the gate
  *fails* when output diverges.
- `classify_respects_the_threshold_boundary` — boundary behaviour at 0.75.

## Reference renderers (local-first, config-gated)

- **mutool / Poppler** — used by `xfa-test-runner` `render_mupdf_oracle` and
  `render_multi_oracle`; each **skips** if the binary is absent
  (`Command::new("mutool").arg("-v").output().is_err()`), so they never fail in
  an environment without the tool.
- **Adobe / pdfRest** — `xfa-pdfrest-compare`; keys are read from the user's
  config/HOME (`~/.config/pdfluent/pdfrest-keys.json`), never hard-coded, with a
  local PNG cache keyed by SHA-256. Absent key → skipped. CI must not require
  this secret; the gate degrades to `OracleUnavailable`.
- **iText** — `oracles/itext.rs`, used for XFA-flatten comparisons.

## Gate corpus + baseline

- `corpus/CI_CORPUS_MANIFEST.json` — 500-PDF stratified set (148 fail / 50
  near-miss / 302 pass) for the SSIM gate; falls back to the in-repo 2-PDF mini
  corpus where the curated set is unavailable.
- `corpus/CI_BASELINE.json` — frozen same-engine SSIM baseline; regressions
  beyond 0.005 fail `gate-ci.yml`.

## Deferred (follow-up, not in this slice)

- Migrate the offline Python oracles (`run_dual_oracle.py`, `oracle_pdfrest.py`,
  `compare_oracle_to_candidate.py`) into the Rust harness so a single test
  invocation compares against all available engines and records a unified
  verdict.
- Collapse the remaining inline SSIM copies (`render_llm_review.rs`,
  `xfa_flatten.rs`) onto `pdf_diff::ssim`.
- Add an Adobe/pdfRest dimension to the gated path (behind the existing
  key-gating + cache), surfacing an `adobe_outlier` verdict alongside the
  existing mutool/Poppler verdicts.
