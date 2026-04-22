# PDFluent — Performance policy (1.0)

**Status:** Active for 1.0 GA. Referenced from
[`STABILITY.md`](STABILITY.md) §11.
**Last updated:** 2026-04-22 (Epic 4 / #1234).

---

## 1. Scope

This document defines **what performance claims we make** for 1.0
and **how they're enforced**.

It is deliberately lightweight: no absolute millisecond claims, no
per-hardware commitments. We make **relative** guarantees that
hold across contributor machines, CI runners, and end-user
deployments.

## 2. What we promise

### 2.1 Open / parse

- `PdfDocument::open` and `from_bytes` are **linear in input size**
  for well-formed PDFs (no accidental quadratic blowup on large
  object tables).
- A 10 MB PDF parses in comparable time to 10×(a 1 MB PDF). If a
  future change causes a 50-page document to suddenly take 10×
  longer than before, the regression is a bug, not a tradeoff.

### 2.2 Save / serialise

- `save_with` / `to_bytes` run in linear time on document size.
- Round-trip (`from_bytes` → `to_bytes`) produces output **no more
  than 1.5×** the input size on our fixtures, barring intentional
  transformations (encryption, subsetting).

### 2.3 Text extraction

- `text()` and `text_with_layout()` complete in sub-linear time
  relative to binary size (they walk the content stream, not the
  raw bytes). A 100-page document with 10 MB of embedded imagery
  does not take 10× the time of a 100-page text-only document.

### 2.4 Compression stack

- `compress(CompressOptions::strict())` is **idempotent** — running
  it twice on the same input produces byte-identical output on
  the second pass (up to PDF writer nondeterminism, which is
  separately pinned by [STABILITY.md §8] round-trip rules).
- `subset_fonts` never **increases** font stream size. If it
  can't reduce, it leaves the stream untouched.

### 2.5 Image export

- `to_images` rendering time is approximately linear in
  `dpi²` × page-area. Doubling DPI quadruples work, matching the
  pixel-count growth.

### 2.6 Signing

- `sign` is O(document size) for the digest pass plus a constant
  cryptographic cost independent of document size.
- `verify_signatures` is O(document size) for hashing, bounded
  per-signature.

## 3. What we do **not** promise

- Absolute millisecond numbers. Hardware variance (Apple M-series,
  x86 server, ARM CI, Windows laptop) is wider than any number
  we could publish without misleading.
- Worst-case bounds on adversarial input. A malformed PDF may
  reach the memory budget (set via `OpenOptions::strict_memory_limit`)
  and error; it may not hang indefinitely.
- Performance of unstable APIs or Deferred items (STABILITY.md
  §3.3) — once those promote to Stable, the policy above
  applies.

## 4. Baseline benchmarks

`crates/pdfluent/benches/facade.rs` exercises the eight most
common operations on the shipped `tests/fixtures/sample.pdf`:

| Benchmark | What it measures |
|---|---|
| `open_from_bytes` | `from_bytes` on a pre-loaded buffer |
| `page_count` | cheap accessor baseline |
| `text` | content-stream text extraction |
| `form_fields` | read-side AcroForm walk |
| `to_bytes` | round-trip serialisation |
| `compress_strict` | full stack (subset + streams + dedup + unused) |
| `subset_fonts` | font-subsetting in isolation |
| `extract_pages_all` | full-document page extraction |

Run with:

```bash
cargo bench -p pdfluent --bench facade
```

## 5. Regression guard

The benchmarks are **informational** in 1.0 CI — they don't block
merge. The intent is that maintainers run them locally before a
release cut (see [RELEASE_RUNBOOK.md §3](RELEASE_RUNBOOK.md)) and
eyeball the numbers against the last release.

A **perf regression** that triples any bench on the same hardware
without a corresponding functional change is a bug and must block
the release. There's no automated threshold — maintainer judgment
is the gate.

Promotion path: a future MINOR may land an automated perf regression
check with a documented threshold (e.g. "fail CI if any bench
regresses by >30% vs the last tag"). That's a 1.1 candidate.

## 6. What's coming post-1.0

- Per-operation absolute SLAs for a documented reference machine
  (post-1.0 product work, not this runbook).
- Streaming serialisation for documents > 200 MB (see §6 note in
  `crates/pdfluent/src/document.rs::to_bytes`).
- Parallel page-extraction in `extract_pages`.

None of the above are in 1.0 Stable. This document will be updated
when they land.
