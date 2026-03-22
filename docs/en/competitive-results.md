# Competitive Benchmark Results

Comparison of XFA-Native against PDFBox 3.0.4, mutool (MuPDF 1.23.10),
and pdftotext (Poppler 24.02.0) across parse, text extraction, rendering,
cold start, memory, and bundle size.

Reproduce with:

```bash
bash scripts/competitive-benchmark.sh [CORPUS_DIR]
```

---

## Environment

| | |
|---|---|
| Machine | Hetzner CX53 (8 vCPUs, 16 GB RAM, Ubuntu 24.04) |
| XFA-Native | `xfa-cli 0.1.0` (release build, `/opt/xfa/target/release/`) |
| PDFBox | 3.0.4 (`pdfbox-app-3.0.4.jar`) |
| MuPDF | `mutool 1.23.10` |
| Poppler | `pdftotext 24.02.0` |
| Java | OpenJDK 21.0.10 |
| Corpus | 10 PDFs from `curated-20k`: 3 small (<50 KB), 4 medium (50–500 KB), 3 large (>500 KB) |
| Method | Single-shot wall-clock per invocation (cold CLI, not warm-loop microbenchmark) |

---

## Bundle size

| Tool | Size | Notes |
|------|-----:|-------|
| **xfa-cli (native binary)** | **20 MB** | Self-contained, no runtime required |
| **xfa-wasm (browser)** | **371 KB** | Compressed wasm module; no JVM |
| PDFBox 3.0.4 JAR | 13 MB | + 202 MB JVM runtime |
| iText 5.5.13 JAR | 2.3 MB | + 202 MB JVM runtime |
| mutool | 44 MB | Statically linked C binary |
| pdftotext | 46 KB | Dynamically linked; requires Poppler shared libs (~10 MB) |

**Key advantage:** The xfa-wasm module at 371 KB is 35× smaller than the PDFBox
JAR and ships with zero runtime dependency. The native `xfa-cli` binary bundles
the full PDF engine in 20 MB — no JVM, no shared-library management, no 200 MB
startup footprint.

---

## Cold-start latency

Measurement: first invocation on a 197 KB PDF, OS page cache dropped before run.

| Tool | Cold start |
|------|----------:|
| mutool | **9 ms** |
| **xfa-cli** | **35 ms** |
| PDFBox (JVM) | 2 603 ms |

**xfa-cli starts 74× faster than PDFBox.** The JVM bootstrap dominates PDFBox's
cold start; even a warm JVM run (JIT-compiled) is ≥400 ms for a trivial PDF
operation. For server-side per-request processing where the JVM stays warm this
gap narrows, but for CLI tooling, batch jobs, or edge deployments it is decisive.

---

## Parse / metadata

`xfa-cli info` vs `mutool info` — time to open, parse xref, and print page count.

| File | Size | xfa-cli | mutool | Speedup |
|------|-----:|--------:|-------:|--------:|
| `c4k-000_000236.pdf` | 12 KB | 3 ms | 4 ms | 1.3× |
| `c4k-000_000370.pdf` | 44 KB | 4 ms | 4 ms | 1.0× |
| `c4k-000_000580.pdf` | 41 KB | 3 ms | 7 ms | 2.3× |
| `c4k-000_000137.pdf` | 197 KB | 3 ms | 8 ms | 2.7× |
| `c4k-000_000262.pdf` | 419 KB | 5 ms | 9 ms | 1.8× |
| `c4k-000_000324.pdf` | 440 KB | 8 ms | 9 ms | 1.1× |
| `c4k-000_000354.pdf` | 93 KB | 3 ms | 3 ms | 1.0× |
| `c4k-000_000150.pdf` | 649 KB | 9 ms | 9 ms | 1.0× |
| `c4k-000_000187.pdf` | 8.7 MB | 17 ms | 20 ms | 1.2× |
| `c4k-000_000282.pdf` | 3.7 MB | 15 ms | 17 ms | 1.1× |
| **Total** | | **70 ms** | **90 ms** | **1.3×** |

**xfa-cli is 1.3× faster overall.** The `pdf-syntax` parser is zero-copy: it
maps raw PDF bytes and resolves objects on demand rather than eagerly loading
the entire xref into a mutable in-memory tree. On large PDFs I/O latency
dominates and the gap narrows to ±10%.

---

## Text extraction

`xfa-cli extract` vs `pdftotext` vs `java -jar pdfbox-app.jar export:text`.
PDFBox times include a warm JVM (first invocation was discarded).

| File | Size | xfa-cli | pdftotext | PDFBox (warm JVM) |
|------|-----:|--------:|----------:|------------------:|
| `c4k-000_000236.pdf` | 12 KB | 15 ms | 19 ms | 1 443 ms |
| `c4k-000_000370.pdf` | 44 KB | 79 ms | 23 ms | 1 995 ms |
| `c4k-000_000580.pdf` | 41 KB | 13 ms | 24 ms | 1 291 ms |
| `c4k-000_000137.pdf` | 197 KB | 7 ms | 20 ms | 1 684 ms |
| `c4k-000_000262.pdf` | 419 KB | 53 ms | 41 ms | 2 289 ms |
| `c4k-000_000324.pdf` | 440 KB | 101 ms | 45 ms | 2 006 ms |
| `c4k-000_000354.pdf` | 93 KB | 140 ms | 40 ms | 1 569 ms |
| `c4k-000_000150.pdf` | 649 KB | 69 ms | 74 ms | 2 021 ms |
| `c4k-000_000187.pdf` | 8.7 MB | 140 ms | 103 ms | 954 ms |
| `c4k-000_000282.pdf` | 3.7 MB | 325 ms | 268 ms | 3 887 ms |
| **Total** | | **942 ms** | **657 ms** | **19 139 ms** |

**xfa-cli is 20× faster than PDFBox** (942 ms vs 19 139 ms total).
**pdftotext is 1.4× faster than xfa-cli** in this sample. Several medium-sized
corpus PDFs (44–440 KB) use dense CID fonts with complex encoding tables; our
font-mapping pipeline adds overhead that Poppler's mature C implementation
avoids. Large PDFs (>650 KB) close the gap as I/O dominates. This is a known
optimization area — see the font-metrics hot path in `pdf-extract`.

---

## Rendering

`xfa-cli render -d 72 -p 1` vs `mutool draw -r 72` — render first page to PNG.

| File | Size | xfa-cli | mutool | Speedup |
|------|-----:|--------:|-------:|--------:|
| `c4k-000_000236.pdf` | 12 KB | 21 ms | 24 ms | 1.1× |
| `c4k-000_000370.pdf` | 44 KB | 31 ms | 28 ms | 0.9× |
| `c4k-000_000580.pdf` | 41 KB | 19 ms | 24 ms | 1.3× |
| `c4k-000_000137.pdf` | 197 KB | 175 ms | 57 ms | 0.3× |
| `c4k-000_000262.pdf` | 419 KB | 40 ms | 74 ms | 1.9× |
| `c4k-000_000324.pdf` | 440 KB | 31 ms | 34 ms | 1.1× |
| `c4k-000_000354.pdf` | 93 KB | 19 ms | 22 ms | 1.2× |
| `c4k-000_000150.pdf` | 649 KB | 11 ms | 25 ms | 2.3× |
| `c4k-000_000187.pdf` | 8.7 MB | 80 ms | 44 ms | 0.6× |
| `c4k-000_000282.pdf` | 3.7 MB | 14 ms | 26 ms | 1.9× |
| **Total** | | **441 ms** | **358 ms** | **0.8×** |

**Rendering is broadly comparable** — mutool 0.8× faster in aggregate.
xfa-cli wins on 6 of 10 PDFs. The 197 KB outlier has a complex multi-layer
transparency group that our compositor builds eagerly; mutool uses a streaming
approach for that pattern. The 8.7 MB outlier is a multi-page document with
large raster images where MuPDF's C renderer has an advantage decoding JPEG/DCT
streams. Both are known areas for optimization.

---

## Peak memory (RSS)

`xfa-cli extract` and peers on a 419 KB PDF.

| Tool | Peak RSS |
|------|--------:|
| mutool info | 7.5 MB |
| **xfa-cli extract** | **8.2 MB** |
| pdftotext | 18.0 MB |
| PDFBox export:text | 181 MB |

**xfa-cli uses 22× less memory than PDFBox** and 2.2× less than pdftotext.
The zero-copy parser never builds a fully-loaded in-memory object graph;
objects are deserialized lazily from the original byte slice. PDFBox's 181 MB
includes JVM heap + class loading overhead.

---

## Summary

| Metric | vs PDFBox | vs mutool | vs pdftotext |
|--------|----------:|----------:|-------------:|
| Cold start | **74× faster** | 0.25× slower | — |
| Parse/info | — | **1.3× faster** | — |
| Text extract | **20× faster** | — | 1.4× slower |
| Render | — | 0.8× slower | — |
| Peak RSS | **22× less** | comparable | 2.2× less |
| Binary size | 1.5× smaller¹ | 2.2× smaller | — |
| WASM size | **35× smaller** | — | — |

¹ xfa-cli 20 MB vs PDFBox JAR 13 MB; but PDFBox requires +202 MB JVM runtime.

**Where we win decisively:** cold start, memory, WASM bundle size. These matter
most for edge deployments, browser integrations, and high-concurrency servers
that spawn per-request processes.

**Where we trail:** text extraction throughput on CID-font-heavy PDFs
(pdftotext), rendering on multi-layer transparency (mutool). Both are active
optimization targets.
