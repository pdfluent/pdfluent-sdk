# Memory Management Design — Issue #464

## Problem Statement

Processing large PDFs through the XFA SDK causes peak RSS of 1–2 GB per lopdf
`Document` for files in the MOZILLA, GHOSTSCRIPT, and PDFBOX test suites.  With
14 parallel workers on the 32 GB VPS, peak process RSS reaches 28 GB, triggering
the Linux OOM killer.  The root cause is lopdf's eager full-document load: every
indirect object, every stream byte, and every compressed object stream is
parsed, decompressed, and retained in a single in-process `BTreeMap` for the
lifetime of the `Document`.  Enterprise customers who need to process 100 MB+
PDFs reliably cannot use the current pipeline without hitting these limits.

---

## Root Cause

### lopdf's load path (`Document::load_mem`)

```
load_mem(buffer: &[u8])
  └── Reader::read()
        ├── 1. Read entire xref table into BTreeMap<ObjectId, XrefEntry>
        ├── 2. For each XrefEntry::Normal → parse + store in Document::objects
        │       Stream::content = buffer[start..end].to_vec()   ← copy of compressed bytes
        ├── 3. For each XrefEntry::Compressed (ObjStm)
        │       ObjectStream::new(stream) → stream.decompress()  ← DECOMPRESS NOW
        │       → expand N objects → store all in Document::objects
        └── 4. Document owns all parsed Objects forever
```

**Three compounding amplifiers:**

| Amplifier | Effect |
|---|---|
| **Compressed streams stored verbatim** | Every stream's compressed bytes are copied from the caller's buffer into `Stream::content: Vec<u8>` — 1× stream size stays in memory even though the document never decompresses most streams |
| **Object streams (ObjStm) decompressed eagerly** | PDF 1.5+ ObjStm packs many small objects into a single zlib-compressed stream. lopdf decompresses the entire ObjStm during load and adds each contained object to `Document::objects`. The compressed stream bytes stay in memory (1×) AND all N decompressed objects stay in memory (N×), where N can be in the thousands |
| **BTreeMap + IndexMap overhead per object** | Each `Dictionary` is an `IndexMap<Vec<u8>, Object>` with ~64–128 bytes of overhead per entry plus `BTreeMap` node overhead (~40 bytes per object). A PDF with 43 000 objects accumulates 5–10 MB of pointer/node overhead alone |

### Why large government PDFs are extreme

Files like `sf86.pdf` (SF-86 security clearance form, 7.8 MB on disk) contain
43 742 indirect objects, most of them packed in ObjStm.  The compressed ObjStm
streams expand 8–12×, and all expanded objects are retained simultaneously.
Measurement on the local corpus confirms this pattern.

---

## Measurements

Measured on macOS (Apple M-series, release build) using `task_info` RSS deltas.
Linux RSS values on the VPS will differ ±20% due to allocator and page size
differences.  `load_mem` was called sequentially; prior documents were dropped
before each measurement.

| PDF | Disk size | RSS delta | Ratio | Objects |
|---|---|---|---|---|
| `f8844.pdf` | 46 KB | 1.4 MB | 30× | 165 |
| `sf181.pdf` | 63 KB | 840 KB | 13× | 176 |
| `f8908.pdf` | 138 KB | 613 KB | 4.4× | 2 460 |
| `f1042.pdf` | 142 KB | 912 KB | 6.4× | 2 460 |
| `f4136.pdf` | 245 KB | 820 KB | 3.4× | 4 927 |
| `f720.pdf` | 379 KB | 3.9 MB | 10× | 8 435 |
| `f990pf.pdf` | 557 KB | 7.7 MB | 14× | 11 138 |
| `f3800.pdf` | 561 KB | 6.1 MB | 11× | 14 824 |
| `f709.pdf` | 752 KB | 8.6 MB | 11× | 13 880 |
| `i-485.pdf` | 1.17 MB | 8.4 MB | 7.2× | 10 293 |
| `i-129.pdf` | 2.24 MB | 13 MB | 5.9× | 11 569 |
| `sf85p.pdf` | 5.69 MB | 100 MB | **18×** | 30 217 |
| `sf86.pdf` | 7.80 MB | 148 MB | **19×** | 43 742 |

**Average ratio across 230-PDF corpus: 8×**
**Worst case: 19× (sf86.pdf)**

Extrapolating to the MOZILLA/GHOSTSCRIPT/PDFBOX files (estimated 50–200 MB on
disk at similar object densities): expected RSS delta per document **500 MB –
2 GB**, consistent with the observed OOM.  At 14 parallel workers that is 7–28
GB of live document memory alone.

---

## How Other PDF Libraries Handle This

### PDFium (Chromium/Google)
Page-level object isolation via `FPDF_LoadPage` / `FPDF_ClosePage`.  The
underlying C++ `CPDF_Document` loads the xref table lazily; individual indirect
objects are parsed on first access.  Pages are loaded and released independently.
Stream content is decompressed on demand by the renderer, not at load time.
**Result**: only the current page's objects are fully materialised in memory.

### Apache PDFBox (Java)
`COSDocument` stores a `COSStream` with a backing `RandomAccessRead`.  Streams
are not copied into heap on open; they remain as byte ranges in the source file.
Decompression happens via `createRawInputStream()` / `createInputStream()` on
demand.  The PDF file is memory-mapped where possible.
**Result**: stream bytes never live in Java heap until explicitly read; GC can
collect parsed objects that are no longer referenced.

### MuPDF (Artifex)
`fz_document` + per-page `fz_page` arena allocator.  Objects reachable from a
page are loaded into a page-scoped arena and freed when the page is closed.
The xref table is read once; object bytes are fetched from the source
(file or buffer) on demand.  Image and stream data is kept compressed in a
tile cache with an LRU eviction policy.
**Result**: memory consumption is proportional to the number of open pages, not
the total object count.

---

## Options

### Option 1 — Per-document memory limit (simple)

**Description**
Add a `DocumentLoadOptions { max_rss_mb: Option<u64> }` parameter to
`Document::load` / `load_mem`.  Before loading, sample the process RSS
(via `/proc/self/status` on Linux, `task_info` on macOS).  After the xref table
is parsed and the approximate object count is known, estimate the expected
memory footprint: `object_count × 500 bytes + xref_size × 3`.  If the estimate
exceeds the limit, return `Err(ManipError::DocumentTooLarge)` before allocating
the object graph.  The xfa-test-runner worker pool passes `max_rss_mb` based on
`(total_ram / worker_count) * 0.8`.

**Pros**
- Zero changes to lopdf internals
- Protects the worker pool from OOM in < 2 days of work
- Graceful error propagation; worker skips the PDF and logs a warning

**Cons**
- Estimation is heuristic (±50%); some large PDFs will still slip through
- Does not reduce memory usage — only rejects oversized documents
- Does not help if the customer needs to process those specific large PDFs
- Process-wide RSS sampling has race conditions with concurrent workers

**Effort**: S — ~1–2 days

---

### Option 2 — Lazy ObjStm decompression (medium)

**Description**
The single biggest amplifier is eager ObjStm decompression in
`Reader::load_objects_raw` (reader.rs:988–1004).  Currently, every ObjStm is
decompressed during `Document::load_mem`, expanding N objects into
`Document::objects` while the compressed stream bytes remain in memory too.

The fix: instead of decompressing ObjStm at load time, keep the ObjStm
`Stream` in `Document::objects` and update the `reference_table` entries for
its contained objects to remain as `XrefEntry::Compressed`.  Decompress a
specific ObjStm only when one of its contained objects is first accessed via
`Document::get_object(id)`.  Cache the decompressed result in the ObjStm's
`Stream::content` so subsequent accesses within the same document are free.

Additionally: replace `stream.set_content(buffer[start..end].to_vec())` with a
`stream.start_position = Some(start); stream.end_position = Some(end);` approach
that keeps non-stream-type streams (content streams, XMP, etc.) as byte-range
references into the original buffer.  Store the buffer in the `Document` as an
`Arc<Vec<u8>>` so it stays alive as long as any stream references it.

```rust
// Proposed Stream representation:
pub enum StreamContent {
    Inline(Vec<u8>),                    // current behaviour — small streams
    Slice(Arc<Vec<u8>>, Range<usize>),  // zero-copy slice of source buffer
    Unresolved(ObjectId),               // ObjStm not yet decompressed
}
```

**Pros**
- Eliminates the ObjStm double-memory problem (the dominant amplifier)
- For typical PDF reading workloads (compliance, text extraction), most streams
  are never accessed → memory proportional to accessed objects, not total count
- Non-breaking for callers that use `Document::get_object` / `decompressed_content`
- Expected reduction: 60–80% for ObjStm-heavy PDFs (sf85p, sf86, MOZILLA corpus)

**Cons**
- Requires forking lopdf and maintaining the fork (we already vendor lopdf)
- Thread-safety: `get_object` must take `&mut self` or use `Mutex` for lazy init
- `Document: Clone` currently clones all stream bytes; clone semantics need review
- PDF/A conversion pipeline (pdfa_fonts, pdfa_cleanup) iterates all objects and
  may trigger full materialisation anyway, negating savings for those paths

**Effort**: M — ~2 weeks including lopdf changes, regression testing across the
4 K corpus, and the pdfa-convert pipeline review.

---

### Option 3 — Page-level loading (complex)

**Description**
Implement a `PageDocument` wrapper that loads only the objects reachable from a
given page tree node.  This mirrors PDFium's model.  The reachability walk starts
from the page's `Page` dict and follows all direct and indirect references
(Resources, Annots, XObject, Font, etc.) recursively.

```rust
pub struct PageDocument {
    source: Arc<Vec<u8>>,
    xref: Xref,
    trailer: Dictionary,
    page_objects: BTreeMap<ObjectId, Object>, // only this page's objects
}

impl PageDocument {
    pub fn open_page(source: Arc<Vec<u8>>, page_index: u32) -> Result<Self>;
}
```

Shared objects (fonts embedded in multiple pages, the document catalog, info
dict) are re-parsed per page.  For rendering and text extraction this is
acceptable; for PDF/A conversion (which must mutate the full document) this
approach does not apply — full-document loading remains necessary there.

**Pros**
- Memory truly proportional to page complexity, not document size
- Natural unit of work for multi-threaded rendering/extraction pipelines
- Matches how PDFium, MuPDF, and PDFBox are architecturally designed

**Cons**
- Requires a new, separate document type — significant API change
- PDF/A conversion, form processing, and bookmark rewriting require the full
  object graph; incompatible with page-level isolation
- Shared objects (embedded fonts, ICC profiles) are loaded multiple times →
  inter-page memory waste unless a shared object cache is added
- Incremental updates and cross-reference streams are complex to handle per-page
- Estimated 4–6 weeks + high regression risk

**Effort**: L — ~4–6 weeks, high risk

---

## Recommendation

**Implement Option 2 (lazy ObjStm decompression) as the primary fix, with Option 1
as an immediate safety valve.**

The measurements show that ObjStm decompression is the dominant amplifier
(10–19× ratios occur precisely for high-ObjStm PDFs like sf86, sf85p, f990pf).
Lazy ObjStm decompression eliminates the root cause while staying within the
existing lopdf API contract.  The 2-week effort is justified by the 60–80%
memory reduction it delivers for the worst-case VPS workloads.

Option 1 (size limit) should be shipped first as a 2-day protective measure
while Option 2 is developed — it prevents OOM crashes in production immediately.
Option 3 is the right long-term architecture for the rendering pipeline but is
too large a change to pursue before the OOM issue is resolved.

---

## API Impact

### Option 1 (no lopdf changes)
```rust
// pdf-manip/src/pages.rs or a new pdf-manip/src/load.rs
pub struct LoadOptions {
    pub max_memory_mb: Option<u64>,  // None = unlimited (current behaviour)
}

pub fn load_with_limit(data: &[u8], opts: &LoadOptions) -> Result<Document>;
```
Non-breaking: existing `Document::load_mem` calls continue to work unchanged.
The xfa-test-runner worker passes `LoadOptions { max_memory_mb: Some(ram / workers) }`.

### Option 2 (lopdf changes)
- `Stream::content: Vec<u8>` → `Stream::content: StreamContent` — **breaking for
  direct `stream.content` field access** (all callers in this repo use
  `stream.decompressed_content()` / `stream.set_content()` — no external API breakage)
- `Document::get_object` signature unchanged; lazy decompress is transparent
- `Document::clone()` must decide: clone resolved objects only (cheap) vs. force
  full materialisation first (safe but defeats the purpose) — proposed: clone
  resolved objects only, mark Unresolved entries as Unresolved in the clone
- New error variant: `Error::ObjStmDecompress` for lazy failures

### Option 3 (new type)
- `PageDocument` is additive; does not break `Document`
- High-level APIs (`pdf-engine`, `pdf-extract`) would need overloads
