# XFA Engine — Success Criteria

**Milestone #47 — Phase 1: Foundation (Issue #1088)**

This document defines the measurable success criteria for the XFA flattening
engine in `crates/pdf-xfa`.  All metrics apply to well-formed XFA PDFs from
the production corpus (see `crates/xfa-golden-tests/`).

---

## 1. Visual Fidelity

| Metric | Target |
|--------|--------|
| Structural Similarity (SSIM) vs. oracle | ≥ 0.95 per page |
| Oracle source | pdfRest `/flatten-pdf` endpoint (Adobe-compatible engine) |
| Measurement | Per-page SSIM averaged across the corpus gold set |

**Rationale**: SSIM ≥ 0.95 captures rendering that is visually equivalent to
the Adobe reference.  Values below 0.90 indicate layout or font regression.
Differences in anti-aliasing or sub-pixel hinting are acceptable (0.95–0.99).

Reference generation: `scripts/generate_xfa_reference.sh`.

---

## 2. Data Completeness

| Metric | Target |
|--------|--------|
| Bound field values present in output | 100% of non-empty fields |
| Detection method | Text extraction from flattened PDF, matched against `<xfa:datasets>` |
| Acceptable missing rate | 0% for required fields, ≤ 2% for optional/calculated fields |

**Rationale**: A flattened PDF that drops field values is functionally broken —
the user's data must survive the flatten round-trip.

---

## 3. Layout Consistency (Page Count)

| Form type | Target |
|-----------|--------|
| Static (XFAF — `baseProfile="interactiveForms"`) | Exact match (oracle ± 0 pages) |
| Dynamic (full XFA grammar) | Oracle ± 1 page |

**Rationale**: Static forms have a fixed layout determined by the PDF page
streams; the page count must be preserved exactly.  Dynamic forms reflow
content based on data, so a one-page tolerance is acceptable.

---

## 4. Crash Rate

| Metric | Target |
|--------|--------|
| Panics / aborts on well-formed XFA PDFs | 0 |
| Panics / aborts on malformed XFA PDFs | 0 (graceful Err return) |
| Memory safety violations | 0 |

**Rationale**: The engine must never crash.  Malformed documents should return
an appropriate `Err(XfaError::…)`.  The no-panic contract is enforced by the
test `flatten_xfa_to_pdf_does_not_panic_on_empty_input` in `flatten.rs`.

---

## 5. Performance

| Metric | Target |
|--------|--------|
| P95 flatten time | ≤ 5 seconds for documents up to 50 pages |
| Memory usage (peak RSS) | ≤ 512 MB per document |
| Measurement environment | Single thread, M-series MacBook / comparable VPS |

**Rationale**: Interactive use-cases (editor preview, on-demand flatten) require
sub-5-second response times.  Batch corpus runs can tolerate higher latency but
must not exhaust memory.

---

## 6. Regression Gate

All criteria above are enforced by the SSIM gate (`scripts/run_gate_ssim.py`).
A gate failure blocks merge.  The current gate index is tracked in
`PROJECT_STATUS.md`.

---

## Appendix — Criterion Ownership

| Criterion | Enforced by |
|-----------|-------------|
| Visual fidelity | `scripts/run_gate_ssim.py`, `scripts/oracle_pdfrest.py` |
| Data completeness | `crates/xfa-golden-tests/tests/xfa_flatten_tests.rs` |
| Page count | `crates/xfa-golden-tests/tests/xfa_flatten_tests.rs` |
| Crash rate | Unit tests in `crates/pdf-xfa/src/flatten.rs` + corpus fuzz |
| Performance | `scripts/run_benchmarks.sh`, `scripts/check_benchmark_sla.py` |
