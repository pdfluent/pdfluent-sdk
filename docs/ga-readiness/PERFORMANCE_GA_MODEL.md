# PDFluent SDK — Performance GA Model

Defines what **Performance 100%** means for the non-XFA SDK. "Performance
100%" = each core operation has a measured baseline, a per-operation
budget, and a CI-enforced regression gate, across the Rust core and every
binding — with memory and startup costs measured, not assumed.

Status vocabulary: see [SDK_GA_READINESS_TAXONOMY.md](SDK_GA_READINESS_TAXONOMY.md).

Current harness reality:
- `scripts/core_pdf/core_pdf_perf_gate.py` + `core_pdf_perf_budget.json` —
  one coarse gate: load + page_count + version + metadata + first-page text,
  ceiling `5.0s/op`, observed ~12ms on the dev host (CI-safe dry-run by default).
- `crates/pdf-bench/benches/{comparison,pdf_benchmarks,pdf_parse,sdk_operations}.rs`
  — criterion micro-benches (not wired to enforced budgets / CI regression).
- WASM perf rounds under `benchmarks/runs/wasm_sdk_dx/` (referenced).

So today: a **coarse anti-hang gate exists**; **per-op budgets and
regression enforcement do not**. That framing drives the statuses below.

---

## PF-1 · Open / parse latency

- **Current evidence:** coarse gate (load+text ≈12ms on simple/50p/acroform/pdfa/scanned).
- **Missing:** isolated open/parse timing (separate from text), percentiles, size-bucketed inputs.
- **Proposed harness:** criterion `open_parse` bench over size buckets (1p / 50p / 500p / large), p50/p95 captured.
- **Budget strategy:** per-bucket p95 budget; fail on >X% regression.
- **Release-blocking:** NO (anti-hang covered); budget is enterprise-ready, not GA-blocking.

## PF-2 · Save / write latency

- **Current evidence:** roundtrip exercised functionally; not timed in isolation.
- **Missing:** save-only timing incl. objstm/xref-stream writing, compression on/off.
- **Proposed harness:** `save` bench over buckets + compression variants.
- **Budget strategy:** per-bucket p95 budget.
- **Release-blocking:** NO; enterprise-ready.

## PF-3 · Text extraction throughput

- **Current evidence:** included in the coarse gate (first-page only).
- **Missing:** full-document chars/sec; layout-mode vs raw; multi-page throughput.
- **Proposed harness:** `extract_text` bench (full doc) reporting chars/sec.
- **Budget strategy:** chars/sec floor per bucket.
- **Release-blocking:** NO; enterprise-ready.

## PF-4 · Rendering / thumbnail latency

- **Current evidence:** render exists (`render_page`, `to_images`); no enforced timing.
- **Missing:** ms/page at fixed DPI; thumbnail latency; cold vs warm font cache.
- **Proposed harness:** `render` bench at 72/150 DPI; thumbnail bench.
- **Budget strategy:** ms/page p95 budget per DPI.
- **Release-blocking:** NO; enterprise-ready (render is a headline feature → strongly recommended).

## PF-5 · Split / merge / rotate throughput

- **Current evidence:** functional tests (`merge.rs` 14, split/rotate via parity).
- **Missing:** pages/sec timing; large-merge memory.
- **Proposed harness:** `split`/`merge`/`rotate` benches over buckets.
- **Budget strategy:** pages/sec floor.
- **Release-blocking:** NO; enterprise-ready.

## PF-6 · Memory peak / RSS

- **Current evidence:** none (resource-limit caps exist but peak RSS is unmeasured).
- **Missing:** peak RSS per op per bucket; leak-over-iterations check.
- **Proposed harness:** RSS sampler around each bench op (e.g. max-rss via getrusage / `/usr/bin/time -v`).
- **Budget strategy:** peak-RSS ceiling per bucket; flat RSS across N iterations (no leak).
- **Release-blocking:** YES for the **no-leak** assertion (overlaps QR-9); peak-RSS budget is enterprise-ready.

## PF-7 · WASM binary size

- **Current evidence:** WASM builds + SHA-256 in editor handoff; size not budgeted.
- **Missing:** tracked `.wasm` (and gzip/brotli) size with a ceiling.
- **Proposed harness:** size check in `transform-wasm-pkg.sh` output; record br/gz.
- **Budget strategy:** size ceiling; fail on growth >X%.
- **Release-blocking:** NO for binary; YES **claim-blocking** if "small/fast WASM" is advertised.

## PF-8 · WASM browser load / init time

- **Current evidence:** Node runtime proof exists; **no browser load/init timing**.
- **Missing:** instantiate + first-call time in a headless browser.
- **Proposed harness:** Playwright headless: measure fetch+instantiate+first op.
- **Budget strategy:** init-time ceiling (e.g. p95).
- **Release-blocking:** NO for binary; YES claim-blocking for any "loads in <Ns in browser" claim.

## PF-9 · Node / Python / .NET / Java binding overhead

- **Current evidence:** binding smokes compile/run; **overhead vs Rust not measured**.
- **Missing:** per-binding cost of one representative op vs native Rust (marshalling overhead).
- **Proposed harness:** per-binding micro-bench calling the same op; report overhead %.
- **Budget strategy:** overhead ceiling per binding.
- **Release-blocking:** NO; enterprise-ready.

## PF-10 · C ABI overhead

- **Current evidence:** strict C-ABI build; no timing.
- **Missing:** call overhead vs direct Rust.
- **Proposed harness:** C micro-bench around one op.
- **Budget strategy:** overhead ceiling.
- **Release-blocking:** NO; enterprise-ready.

## PF-11 · Large PDF performance

- **Current evidence:** resource limits prevent blow-ups; no large-file timing baseline (VPS corpus available).
- **Missing:** timing/memory on 100MB+ / 1000+ page docs.
- **Proposed harness:** large-file bench using curated big fixtures (off the VPS corpus, no private paths in repo).
- **Budget strategy:** time + RSS ceilings for the large bucket.
- **Release-blocking:** NO; enterprise-ready (and a common enterprise stressor → recommended).

## PF-12 · Object stream / xref stream impact

- **Current evidence:** functional roundtrip proven; perf delta unmeasured.
- **Missing:** parse-time delta classic-xref vs xref-stream vs objstm.
- **Proposed harness:** comparative bench across the three encodings of one doc.
- **Budget strategy:** informational baseline; no hard budget needed.
- **Release-blocking:** NO; post-GA improvement.

## PF-13 · Cold start

- **Current evidence:** coarse gate runs cold processes (~12ms total) — includes process start for Rust.
- **Missing:** per-binding cold start (Python import, .NET/JVM startup, WASM instantiate) isolated.
- **Proposed harness:** cold-start timing per binding (import/instantiate + first op).
- **Budget strategy:** cold-start ceiling per binding.
- **Release-blocking:** NO; enterprise-ready.

## PF-14 · Regression budgets

- **Current evidence:** the coarse 5.0s ceiling is the *only* enforced budget; criterion benches are not gated.
- **Missing:** per-op budgets wired to a CI gate that fails on regression beyond tolerance.
- **Proposed harness:** budget JSON per op/bucket + a gate comparing measured vs budget with tolerance; baseline committed.
- **Budget strategy:** committed baselines + % tolerance; update via explicit re-baseline.
- **Release-blocking:** NO for GA binary; **YES for enterprise-ready** (this is the durability mechanism for the whole perf axis).

## PF-15 · Perf reporting format

- **Current evidence:** `CORE_PDF_PERFORMANCE_BASELINE.{md,json}` format exists.
- **Missing:** a single standardized perf report schema covering all ops/bindings/buckets (p50/p95, RSS, sizes) consumable by the gate.
- **Proposed harness:** define `perf_report.schema.json`; all benches emit to it.
- **Budget strategy:** schema is the contract the budget gate reads.
- **Release-blocking:** NO; enabling work for PF-14.

---

## Performance axis summary

Performance is the **least-proven axis**: only a coarse anti-hang gate is
enforced today. Nothing here is a **GA binary** release-blocker (the SDK is
fast and bounded on the read path, and resource limits prevent blow-ups),
but the entire axis is **`missing_evidence` for enterprise-ready** until
PF-15 → PF-14 establish a standardized report + per-op budgets + a CI
regression gate, plus the no-leak assertion (PF-6, shared with QR-9). The
Performance milestone (C) is therefore *baseline + budgets + gate*, not
optimization.
