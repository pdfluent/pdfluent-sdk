# Performance SLA — PDFluent SDK

This document defines the formal performance SLA targets for the PDFluent SDK.

> ## UNCALIBRATED
>
> **Every absolute number below is uncalibrated. Do not quote them, and do not
> treat a run that clears them as a pass.**
>
> They were measured in April 2026 on a Xeon E-2176G @ 3.70 GHz, 6C/12T, 62 GB
> RAM, Ubuntu 22.04. That machine was decommissioned on 24-08-2026. No number
> here has been re-measured since, and no number here has been changed —
> changing them without measuring would only move the fiction.
>
> Comparing against them fails in the flattering direction: a real regression
> measured on faster hardware still clears a threshold set on slower hardware
> and reports a pass. `scripts/check_benchmark_sla.py` therefore refuses to
> judge a result at all while no machine class is calibrated. It exits 3 with
> `SKIPPED (not a pass): …` rather than returning a verdict it cannot support.
>
> **What would make them real again**, in order:
>
> 1. A machine class that can hold the corpus. `.github/workflows/bench.yml`
>    wants a runner labelled `xfa-corpus`; none is registered, and the workflow
>    is parked for that reason (#276). The corpus disk on the persistent runner
>    is currently unreadable, so the run cannot be done there either.
> 2. `BENCH_MACHINE_CLASS=<class> scripts/run_benchmarks.sh --corpus-dir …`
>    on that class. The script refuses to write a result whose class does not
>    match the cores actually present.
> 3. Set `calibrated = true` and `calibrated_on` for that class in
>    `benchmarks/BASELINE_HARDWARE.toml`, replace the numbers below with what
>    was measured, raise `CALIBRATED_FLOOR` in
>    `scripts/ci/a_baseline_names_the_machine.py`, and delete this block.
>
> Steps 2 and 3 are guarded and will fail until all of them are done. Which
> class to calibrate against is an owner decision: an ephemeral instance costs
> money per run and cannot hold the corpus, and the persistent runner shares a
> workstation with other work.
>
> Registry: `benchmarks/BASELINE_HARDWARE.toml`. Issue: #283.

---

## Throughput SLA (p95 latency per operation)

| Operation | Category | Target (p95) | Notes |
|---|---|---|---|
| Render A4 page — text-only | Native | **< 50 ms** | 150 DPI, RGBA |
| Render A4 page — mixed (text + images) | Native | **< 150 ms** | 150 DPI, RGBA |
| Render A4 page — image-heavy | Native | **< 300 ms** | 150 DPI, JPEG image content |
| Text extraction — 10-page doc | Native | **< 20 ms** | Structured text with positions |
| Text extraction — 100-page doc | Native | **< 200 ms** | Structured text |
| XFA form flatten — simple (1 page) | Native | **< 100 ms** | Basic form, no scripts |
| XFA form flatten — complex (10 pages) | Native | **< 500 ms** | Multi-page with data binding |
| XFA form flatten — complex (10 pages) | WASM | **< 1000 ms** | Chrome/Chromium, same form |
| Render A4 page — text-only | WASM | **< 200 ms** | 96 DPI, Chromium |
| WASM cold init (Node 22, M-class CPU) | WASM | **≤ 20 ms p90** | New in B3 — `-O3` measured 12.4 ms p90 |
| WASM `PdfDoc.open` on ≤ 5 MiB doc | WASM | **≤ 60 ms p90** | New in B3 — `-O3` measured 44 ms p90 on 5.7 MiB |
| WASM `renderPage` on f1040 (2p, scale 1) | WASM | **≤ 700 ms p90** | New in B3 — `-O3` measured 552 ms p90 |
| WASM `validatePdfA('2b')` on 5.7 MiB / 95p | WASM | **≤ 18,000 ms p90** | New in B3 — `-O3` measured 13.7 s; PdfA is currently the worst-case op |

## Memory SLA

| Operation | Target | Notes |
|---|---|---|
| Peak memory: A4 text-only page render | **< 50 MB** | Including PDF parse |
| Peak memory: A4 image-heavy page render | **< 200 MB** | Including image decode |
| Peak memory: 10-page XFA form flatten | **< 256 MB** | Including XFA DOM |
| WASM heap peak: single page render | **< 64 MB** | Browser `performance.memory` |

## Concurrency SLA

| Metric | Target | Notes |
|---|---|---|
| Throughput: 12-thread batch render | **> 8 pages/sec** | A4 text-only, 150 DPI |
| Throughput: 12-thread batch render | **> 3 pages/sec** | Mixed content |

## Stability SLA

| Metric | Target |
|---|---|
| Panics/crashes on 5000-PDF corpus | **0** |
| OOM crashes on 5000-PDF corpus | **0** |
| Timeout (30s) rate on 5000-PDF corpus | **< 0.1%** |

---

## Measurement Procedure

Run on a machine class registered in `benchmarks/BASELINE_HARDWARE.toml`.
`BENCH_MACHINE_CLASS` is required: it is checked against the cores actually
present, and it names the result file, so a run cannot claim a machine it was
not on.

```bash
# Build release binary
cargo build --release -p xfa-cli

# Run benchmark suite
BENCH_MACHINE_CLASS=<class> ./scripts/run_benchmarks.sh \
  --corpus-dir /opt/xfa-corpus/curated-1k \
  --warmup-seconds 2 \
  --measure-seconds 10

# Check against SLA. Exits 3 with `SKIPPED (not a pass): …` if the class this
# result came from has no calibrated baseline — which is the case for every
# class today.
python3 scripts/check_benchmark_sla.py \
  --suite-json benchmarks/results/<class>-$(date +%Y-%m-%d).json \
  --sla BENCHMARKS_SLA.md
```

## Stress Fixture Baselines

These measurements are baselines only. They do **not** set SLA targets; the
target threshold will be derived from repeated M2 memory/performance data.
Generated stress PDFs are intentionally not committed.

Generate the standard local/PR-CI fixture:

```bash
python3 scripts/generate_stress_fixtures.py --size 100M
```

Run the stress parse bench with Cargo's bench-argument separator:

```bash
cargo bench -p pdf-bench --bench pdf_parse stress -- --sample-size 3
```

500 MiB and 1 GiB fixtures are nightly/local-only. Generate them explicitly and
set `BENCHMARK_STRESS_LARGE=1` when including them in a bench run.

| Date | Environment | Fixture | Parser | Runs | Mean time | Peak time | Peak RSS |
|---|---|---|---|---:|---:|---:|---:|
| 2026-04-25 | macOS 26.3.1, Apple M1 Pro, 32 GiB RAM | `stress-100m.pdf` (100 MiB) | `lopdf_load_mem` | 3 | 3220.774 ms | 3435.760 ms | 214256 KB (209.2 MiB) |
| 2026-04-25 | macOS 26.3.1, Apple M1 Pro, 32 GiB RAM | `stress-100m.pdf` (100 MiB) | `pdf_syntax_new` | 3 | 40.235 ms | 44.373 ms | 108836 KB (106.3 MiB) |

## SLA History

| Date | Render text p95 | Render mixed p95 | XFA simple p95 | Notes |
|---|---|---|---|---|
| 2026-04-18 | GATE #60: 948/974 = 97.3% pass (1k corpus) | r19 branch post CMYK/shading/tiling fixes | — | First baseline; 5k corpus: 91.7% (pre-r19) |
| 2026-05-16 | (WASM track only) | (WASM track only) | (WASM track only) | B3: enable `wasm-opt = ["-O3", …]`. Cold init 13.66 → 11.85 ms (-13.3%). Median key-op runtime -7.9%. Raw .wasm -7.0%. Brotli/gzip wire +~4% (-O3 trade-off). Golden gate 34/34 byte-identical. |

---

## Enforcement

- The benchmark suite runs automatically on merge to `master` via `.github/workflows/benchmarks.yml`
- A regression of >20% on any SLA target triggers an alert (not a block — performance varies with CPU state)
- Hard block: any test timeout (>30s) or OOM in the benchmark suite
- No comparison happens against an uncalibrated machine class.
  `scripts/check_benchmark_sla.py` exits 3 and says so on stderr; a percentage
  threshold against numbers from other hardware measures nothing, and a gate
  that reports a pass it cannot support is worse than no gate.
  `scripts/ci/a_baseline_names_the_machine.py` keeps this document and the
  registry in step, in both directions.
