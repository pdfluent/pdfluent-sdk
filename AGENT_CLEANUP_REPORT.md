# Agent D — Cleanup Report

**Agent:** D — Visual QA Existing Oracles
**Branch:** `xfa/product-visual-qa-existing-oracles`
**Baseline:** `743bb4acc278af457056b215e7c58b8fb95a6a85`
**Date:** 2026-05-19

---

## Work Summary

Re-analysed the 5 T2 `ambiguous` docs (SSIM 0.82–0.84) via:
1. Downloaded heatmap PNGs from VPS `/opt/xfa-runs/t2-visual-140/heatmaps_140/` (14-day window)
2. Visual inspection of all 5 ours/oracle/diff panels
3. Extended `scripts/xfa_oracle_fault_classifier.py` with fine-category subcategories
4. Applied new font_metric classification rule to resolve the ambiguous bucket

**Result:** 5 ambiguous → 5 `oracle_mismatch_rendering / font_metric`. 0 engine bugs.

---

## Disk Impact

| Location | Action | Size |
|----------|--------|------|
| `/tmp/d-visual-qa-heatmaps/` | Downloaded heatmaps (temp, not committed) | ~2.5 MB |
| `/tmp/d-visual-qa-patched-ssim/` | Patched SSIM JSONs with local paths (temp) | ~15 KB |
| `/tmp/d-visual-qa-reclassification/` | Classifier output (temp, content in JSON report) | ~20 KB |
| `benchmarks/runs/.../product_quality_track/` | New report files (committed) | ~20 KB |
| `scripts/xfa_oracle_fault_classifier.py` | Extended (committed) | +~120 lines |

Temporary directories `/tmp/d-visual-qa-*` are machine-local and will be cleaned
up automatically on system restart. No VPS files created or modified.

---

## VPS Status

- `/opt/xfa-corpus/xfa-forms/` — 140 PDFs, read-only, UNCHANGED
- `/opt/xfa-reference/pdfrest/` — 140 oracle PNGs, read-only, UNCHANGED
- `/opt/xfa-runs/t2-visual-140/heatmaps_140/` — 107 MB, read-only, UNCHANGED
  (retention until 2026-06-02; no deletions performed)

---

## Protected Files

| File | SHA256 | Status |
|------|--------|--------|
| `benchmarks/pdfrest_quota_log.json` | `ac414ecfc327f7e247e4f575538883e473056482fcbbc4fbf0de123f01d70b4c` | BYTE-IDENTICAL |
| `benchmarks/runs/xfa_enterprise_plan/XFA_RELEASE_CLAIMS_EXPORT.json` | `2963c162a46e37f46ceeff2675b0a5d7dfba218ed0aaa4a6ef0fdad609d04403` | BYTE-IDENTICAL |

---

## Universal Gates

| Gate | Result |
|------|--------|
| `cargo check -p pdf-xfa` | PASS |
| `cargo test -p pdf-xfa --features xfa-js-sandboxed` | PASS — 564 tests |
| `cargo clippy -p pdf-xfa --features xfa-js-sandboxed -- -D warnings` | PASS — 0 warnings |
| `cargo fmt --all --check` | PASS — exit 0 (pre-existing xfa-wasm drift not in scope) |
| `scripts/check_no_private_paths.sh` | PASS — 0 private paths |
| `pdfrest_quota_log.json` byte-identical | PASS |
| `XFA_RELEASE_CLAIMS_EXPORT.json` byte-identical | PASS |
| Engine code unchanged (`git diff -- crates/`) | PASS |
| Paid API calls | 0 |

---

## Branch Hygiene

```
Branch: xfa/product-visual-qa-existing-oracles
Baseline reset to: 743bb4acc278af457056b215e7c58b8fb95a6a85
Foreign commits in range: 0
```

Commits in this branch (743bb4acc..HEAD):
- `scripts/xfa_oracle_fault_classifier.py` — extended with fine-category subcategories
- `benchmarks/runs/.../product_quality_track/D_VISUAL_QA_RECLASSIFICATION.json`
- `benchmarks/runs/.../product_quality_track/D_VISUAL_QA_REPORT.md`
- `AGENT_CLEANUP_REPORT.md`

---

## Remaining Work

None. D-01 and conditional D-02 (no engine bugs found, so no follow-up issues) are both
complete. Branch is ready for push.

---

## Verdict

`XFA_PRODUCT_VISUAL_QA_READY`
