# Diagnostics & Repair Reporting — Deferred (B) items

Engineering note accompanying the public diagnostics layer
(`pdfluent::diagnostics`). The A-slice — collecting and exposing the three
interpreter warnings that the engine already emits — is implemented. The items
below are deferred: they require a recovery/decode **signal that does not exist
yet** in the read path, which is a fork-level change, not a bounded patch.

## Implemented (A)
- Public `Diagnostic { severity, category, code, message, page, object, source }`
  with `Severity` / `DiagnosticCategory` enums and stable `CODE_*` strings.
- A collecting, poison-tolerant warning sink installed on the engine at open;
  `PdfDocument::diagnostics()` / `take_diagnostics()`.
- Codes: `FONT_UNSUPPORTED`, `IMAGE_DECODE_FAILED`, `STREAM_TOO_LARGE`.
- `OpenOptions::with_repair` clarified (recovery is always-on; flag advisory).

## Deferred (B)

### B1 — Page-tree brute-force recovery reporting
- **Problem:** `pdf-syntax` reconstructs a broken page tree via
  `Pages::new_brute_force` and only emits a `log::warn!` — no programmatic
  signal reaches `pdf-engine`/`pdfluent`. A caller cannot learn that page order
  was reconstructed.
- **Why B:** the loader (`CachedPages::new`) returns `Option<Pages>` with no
  recovery flag; reporting needs a `RepairReport { pages_brute_forced, … }`
  threaded out of `pdf-syntax` → `pdf-engine` → `pdfluent`. Fork-level signal +
  load-chain plumbing.
- **Maps to code:** future `DiagnosticCategory::Repair`, e.g. `PAGE_TREE_REBUILT`.

### B2 — Xref / object repair reporting
- **Problem:** `pdf-syntax` rebuilds an invalid xref by brute-force object scan
  (`xref::fallback`), again only `log::warn!`. Recovered/lost object counts are
  not surfaced (qpdf reports per-object repairs).
- **Why B:** same as B1 — the recovery is inside the fork loader with no return
  channel; needs `XRefRepairInfo { objects_recovered, objects_lost }` propagated
  through the load chain.
- **Maps to code:** `XREF_REBUILT`.

### B3 — Decode-leniency reporting
- **Problem:** `Stream::decoded()` is intentionally lenient — a corrupt content
  stream returns best-effort bytes (verified: `page_stream()` returns `Some`
  even on an invalid zlib header), so a *content*-stream decode failure leaves
  no signal. (`ImageDecodeFailure` exists because image decode is a separate,
  error-returning path — and is already surfaced here.)
- **Why B:** distinguishing a clean decode from a lenient one needs the decode
  path to return a `recovered` flag (or emit through a sink) without removing
  the leniency that handles real-world broken PDFs. Fork-level change to the
  decode API + every consumer.
- **Maps to code:** `CONTENT_DECODE_DEGRADED`.

### B4 — Comprehensive font-substitution reporting + per-page/object attribution
- **Problem:** `UnsupportedFont` is emitted for some cases (e.g. CID
  non-identity) but the descriptor-based fallback to a standard font is mostly
  silent, and warnings carry no page/object id (the `Diagnostic.page`/`object`
  fields are populated as `None`).
- **Why B:** richer emission requires adding warning calls at the silent
  fallback sites in `pdf-interpret` (fork), and per-page/object attribution
  requires per-operation context threading that is race-safe under the `&self`
  (shareable) render API — more than a bounded patch.
- **Maps to code:** extend `FONT_UNSUPPORTED` payload + `FONT_SUBSTITUTED`.

## Note
None of the B-items change recovery *behaviour* — recovery is and stays
always-on. They add **observability** of recovery that currently happens
silently. They are gated on fork-level signal plumbing, which is why they are
deferred rather than bundled into this bounded A-slice.
