# Performance SLA — PDFluent SDK

This document defines the formal performance SLA targets for the PDFluent SDK.
All targets are measured on the benchmark hardware: **Hetzner EX42 — Xeon E-2176G @ 3.70 GHz, 6C/12T, 62 GB RAM, Ubuntu 22.04**.

> **Status**: Initial baseline targets — to be confirmed by running `scripts/run_benchmarks.sh`.

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

Run on VPS:

```bash
# Build release binary
cargo build --release -p pdf-engine -p pdfluent

# Run benchmark suite
./scripts/run_benchmarks.sh \
  --corpus-dir /opt/xfa-corpus/curated-1k \
  --hardware-profile benchmarks/hardware/hetzner-e2176g.json \
  --warmup-seconds 2 \
  --measure-seconds 10

# Check against SLA
python3 scripts/check_benchmark_sla.py \
  --suite-json benchmarks/results/hetzner-e2176g-$(date +%Y-%m-%d).suite.json \
  --sla BENCHMARKS_SLA.md
```

## SLA History

| Date | Render text p95 | Render mixed p95 | XFA simple p95 | Notes |
|---|---|---|---|---|
| TBD | TBD | TBD | TBD | First baseline run needed |

---

## Enforcement

- The benchmark suite runs automatically on merge to `master` via `.github/workflows/benchmarks.yml`
- A regression of >20% on any SLA target triggers an alert (not a block — performance varies with CPU state)
- Hard block: any test timeout (>30s) or OOM in the benchmark suite
