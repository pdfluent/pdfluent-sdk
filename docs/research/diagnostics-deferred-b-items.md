# Diagnostics & Repair Reporting — status

Engineering note for the public diagnostics layer (`pdfluent::diagnostics`).
Tracks which recovery/degradation signals are now observable and which remain
deferred. Recovery behaviour is unchanged throughout — these items add
*observability*, never strictness.

## Implemented

Public model `Diagnostic { severity, category, code, message, page, object,
source }` + `Severity` / `DiagnosticCategory`; collected via
`PdfDocument::diagnostics()` / `take_diagnostics()` (poison-tolerant sink).

| Code | Category | Source | Status |
|---|---|---|---|
| `FONT_UNSUPPORTED` | Font | pd-interpret font fallback (TrueType/Type1/CID) emits at the substitution sites | **done** |
| `IMAGE_DECODE_FAILED` | Image | image decode error path | **done** |
| `STREAM_TOO_LARGE` | Limit | `max_stream_bytes` cap | **done** |
| `XREF_REBUILT` | Repair | `Pdf::load_recovery().xref_rebuilt` (top-level xref fallback) | **done** |
| `PAGE_TREE_REBUILT` | Repair | `Pdf::load_recovery().page_tree_rebuilt` (`CachedPages` brute-force) | **done** |

Signal plumbing: pd-syntax `LoadRecovery` → `pd_engine::PdfDocument::load_recovery()`
→ seeded into the pdfluent collector once at open.

## Deferred (B)

### B-decode — Content-stream decode-leniency reporting
- **Finding:** `Stream::decoded()` returns `Err(DecodeFailure)` only for hard
  failures; the **filter** decoders (e.g. Flate) are lenient and return partial
  `Ok` bytes on malformed input (verified: a bad zlib header still yields
  `Some` from `page_stream`). So there is no signal that a content stream was
  decoded leniently.
- **Why B:** surfacing it needs a `recovered`/`lenient` flag produced by the
  filter decoders and threaded through `decoded()`/`decoded_image()` to its
  **18 call sites** — and the leniency must be *preserved* (going strict is a
  forbidden behaviour change). Broad fork change, not a bounded patch.
- **Would map to:** `CONTENT_DECODE_DEGRADED` (category `Decode`).

### B-xref-detail — Granular xref / object repair counts
- `XREF_REBUILT` reports *that* the xref was rebuilt, not which objects were
  recovered/lost, and the deeper in-stream "broken xref, attempting to repair"
  paths are not itemised. A per-object `XRefRepairInfo { recovered, lost }`
  needs more return channels out of the fork's xref builder. Low marginal value
  over the boolean already shipped.
- **Note:** `XREF_REBUILT` is surfaced via pdfluent only when **lopdf also**
  loads the document (pdfluent loads pd-syntax *and* lopdf, and lopdf's reader
  is stricter on broken xrefs). The fork signal itself is validated at the
  pd-engine level regardless (`load_recovery().xref_rebuilt`).

### B-object-attr — Object-id attribution
- Diagnostics carry an `object: Option<u32>` field, currently always `None`.
  Populating it needs the warning variants to carry the object id, which breaks
  `InterpreterWarning`'s `Copy` derive (relied on by the limit-collector), i.e.
  a variant redesign. Deferred.

## Rejected (C)

### Page-number attribution
- Attaching a page index to interpreter warnings is **unreliable under the
  current architecture**: `render_page` / `extract_text` take `&self`, so the
  same document can be rendered concurrently, and any per-operation "current
  page" context shared through the collector would race across threads. Per the
  project rule (avoid attribution that is unreliable), the `page` field is left
  `None` rather than populated incorrectly. Revisit only if the render API gains
  a per-call diagnostic context.
