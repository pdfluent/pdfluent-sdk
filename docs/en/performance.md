# Performance

End-to-end benchmarks comparing XFA SDK against MuPDF and Poppler on a
real-world corpus. Micro-benchmarks of individual APIs are in
`crates/pdf-bench/` and can be run with `cargo bench`.

## Environment

| | |
|---|---|
| Machine | Hetzner CX53 (8 vCPUs, 16 GB RAM, Ubuntu 22.04) |
| XFA SDK | `xfa-cli 0.1.0` (`/opt/xfa/target/release/`) |
| MuPDF | `mutool 1.23.10` |
| Poppler | `pdftotext 24.02.0` |
| Corpus | 10 PDFs sampled from `curated-20k` (4 KB – 2.3 MB) |
| Method | Median of 5 runs per PDF per tool, tools run sequentially |

The corpus sample spans a wide range of PDF types: small test-suite PDFs,
signed PDFs, form-bearing PDFs, scanned content, and large multi-page
documents. All times are in milliseconds and include process startup overhead
(single-shot CLI invocations, not warm-loop microbenchmarks).

Run the comparison yourself with:

```bash
./scripts/benchmark-comparison.sh [XFA_CLI_PATH] [CORPUS_DIR]
```

---

## Parse / metadata (`xfa-cli info` vs `mutool info`)

| File | Size | XFA SDK (ms) | mutool (ms) | Speedup |
|------|-----:|-------------:|------------:|--------:|
| `c4k-veraPDF-6-2-9-t04-fail-c.pdf` | 4 KB | 3 | 9 | 3.0× |
| `gen-600_600867.pdf` | 4 KB | 5 | 5 | 1.0× |
| `forms-cmp_rotatedFieldsOnRotatedPages.pdf` | 12 KB | 3 | 7 | 2.3× |
| `signed-signedPAdES-T.pdf` | 27 KB | 6 | 9 | 1.5× |
| `cs-veraPDF-6-1-12-t02-pass-i.pdf` | 49 KB | 4 | 8 | 2.0× |
| `r3-499_499063.pdf` | 83 KB | 4 | 6 | 1.5× |
| `c4k-841_841258.pdf` | 155 KB | 6 | 9 | 1.5× |
| `c4k-011_011737.pdf` | 315 KB | 7 | 9 | 1.2× |
| `gen-593_593128.pdf` | 705 KB | 6 | 7 | 1.1× |
| `gen-196_196069.pdf` | 2.3 MB | 12 | 9 | 0.7× |
| **Total (10 PDFs)** | | **56** | **78** | **1.4×** |

**XFA SDK is 1.4× faster in aggregate.** The `pdf-syntax` parser is
zero-copy and read-only: it maps raw PDF bytes and resolves objects on
demand instead of eagerly loading the entire xref into a mutable graph.
On very large PDFs (2+ MB) the difference narrows because I/O dominates.

---

## Text extraction (`xfa-cli extract` vs `pdftotext`)

| File | Size | XFA SDK (ms) | pdftotext (ms) | Speedup |
|------|-----:|-------------:|---------------:|--------:|
| `c4k-veraPDF-6-2-9-t04-fail-c.pdf` | 4 KB | 5 | 14 | 2.8× |
| `gen-600_600867.pdf` | 4 KB | 10 | 17 | 1.7× |
| `forms-cmp_rotatedFieldsOnRotatedPages.pdf` | 12 KB | 7 | 25 | 3.5× |
| `signed-signedPAdES-T.pdf` | 27 KB | 6 | 19 | 3.1× |
| `cs-veraPDF-6-1-12-t02-pass-i.pdf` | 49 KB | 4 | 33 | 8.2× |
| `r3-499_499063.pdf` | 83 KB | 149 | 42 | 0.28× |
| `c4k-841_841258.pdf` | 155 KB | 105 | 37 | 0.35× |
| `c4k-011_011737.pdf` | 315 KB | 135 | 103 | 0.77× |
| `gen-593_593128.pdf` | 705 KB | 26 | 34 | 1.3× |
| `gen-196_196069.pdf` | 2.3 MB | 261 | 1690 | 6.4× |
| **Total (10 PDFs)** | | **708** | **2014** | **2.8×** |

**XFA SDK is 2.8× faster in aggregate.** Three PDFs (83–315 KB) show
pdftotext ahead: these are corpus PDFs with dense CID fonts and complex
encoding tables where our font-mapping path is not yet as optimized as
Poppler's mature codebase. The very large 2.3 MB document shows a 6.4×
advantage for XFA SDK — Poppler's pdftotext must linearly scan a large
xref whereas `pdf-syntax` resolves objects lazily.

---

## Rendering (`xfa-cli render --dpi 72 --pages 1` vs `mutool draw -r 72`)

| File | Size | XFA SDK (ms) | mutool draw (ms) | Speedup |
|------|-----:|-------------:|-----------------:|--------:|
| `c4k-veraPDF-6-2-9-t04-fail-c.pdf` | 4 KB | 11 | 25 | 2.2× |
| `gen-600_600867.pdf` | 4 KB | 13 | 29 | 2.2× |
| `forms-cmp_rotatedFieldsOnRotatedPages.pdf` | 12 KB | 13 | 27 | 2.0× |
| `signed-signedPAdES-T.pdf` | 27 KB | 11 | 18 | 1.6× |
| `cs-veraPDF-6-1-12-t02-pass-i.pdf` | 49 KB | 9 | 21 | 2.3× |
| `r3-499_499063.pdf` | 83 KB | 29 | 32 | 1.1× |
| `c4k-841_841258.pdf` | 155 KB | 33 | 63 | 1.9× |
| `c4k-011_011737.pdf` | 315 KB | 37 | 48 | 1.2× |
| `gen-593_593128.pdf` | 705 KB | 266 | 93 | 0.3× |
| `gen-196_196069.pdf` | 2.3 MB | 19 | 32 | 1.6× |
| **Total (10 PDFs)** | | **441** | **388** | **0.9×** |

**Rendering is roughly comparable overall**, with XFA SDK faster on 9 of
10 PDFs. The one outlier (`gen-593`, 705 KB) is a heavily-scanned PDF
with large raster images: mutool's C renderer streams the image data
without decoding it all into a Rust-owned pixel buffer first. This is a
known area for optimization in the rendering pipeline.

---

## Criterion micro-benchmarks

The numbers above measure cold CLI invocations (including process startup,
file I/O, and PNG encoding). For API-level hot-path benchmarks run the
Criterion suite:

```bash
cargo bench -p pdf-bench
```

Key groups and what they measure:

| Group | Crate | What is measured |
|-------|-------|-----------------|
| `parse` | `pdf-syntax` | `Pdf::new` — zero-copy reader init |
| `parse` | `lopdf` | `Document::load_mem` — full mutable load |
| `text_extract` | `pdf-extract` | All-page text + character counts |
| `search` | `pdf-extract` | Full-text search, case-{in}sensitive |
| `pages` | `pdf-manip` | Page extraction + document merge |
| `compliance` | `pdf-compliance` | PDF/A-2b + PDF/UA-1 validation |
| `forms` | `pdf-forms` | AcroForm parse + field iteration |
| `render` | `pdf-engine` | First-page render at 72 DPI |

Criterion HTML reports are written to `target/criterion/report/index.html`
after each run. Use `cargo bench -- --save-baseline foo` and
`cargo bench -- --baseline foo` to track regressions across commits.

---

## WASM footprint

The `xfa-wasm` crate compiles the core parse + compliance + text stack to
WebAssembly. Build size (release, with `wasm-opt -O3`):

```bash
wasm-pack build crates/xfa-wasm --target web --release
ls -lh crates/xfa-wasm/pkg/xfa_wasm_bg.wasm
```

| Configuration | Size |
|---|---|
| Release + wasm-opt -O3 | ~970 KB |
| Release (no opt) | ~1.4 MB |

By comparison, MuPDF's WASM build is ~5 MB and requires WebAssembly SIMD
extensions for full performance.
