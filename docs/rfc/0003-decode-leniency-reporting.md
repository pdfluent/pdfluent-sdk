# RFC 0003 — Content-stream decode-leniency reporting

**Status:** Design spike (Phase A, milestone A5). **Not implemented in this
milestone.** The spike proves a bounded, additive, non-breaking, testable design
exists; it is deferred to the first post-editor task because it modifies the
fuzzed, crash-critical flate decoder across three crates and is not on the
editor's critical path. This RFC corrects the earlier "broad, ~76 call sites"
assessment.

## 1. Problem

`pd-syntax` filter decoders recover from malformed streams and return `Ok` with
partial/best-effort data, emitting only a `log::warn!`. A render can therefore
silently degrade with no machine-readable signal:

- `crates/pdf-syntax/src/filter/lzw_flate.rs:32,38` — primary inflate errors are
  swallowed with `.read_to_end(..).ok()`.
- `:44` — `warn!("flate stream is broken, decoding with fallback")` then a
  custom block-by-block decoder that warns-and-continues on bad blocks
  (`:201,218,232,...`).
- `crates/pdf-syntax/src/filter/mod.rs:129` — `warn!("failed to apply filter ..")`.

`pdfluent`'s public `DiagnosticCategory::Decode` (`diagnostics.rs:45`) is
reserved but **no code path ever fires it**. Enterprise consumers cannot
distinguish a clean render from a silently-degraded one; support cannot triage
"malformed PDF" vs "engine bug".

## 2. Re-verified architecture

- **Signal origin:** the filter decoders, inside `pd-syntax`.
- **Sink:** the `WarningSinkFn` / `InterpreterWarning` mechanism lives in
  `pd-interpret`; `pdfluent` translates it to `Diagnostic`.
- **They are decoupled by design:** `pd-syntax` does **not** depend on
  `pd-interpret` and has **no** sink concept (only `log`). This boundary is the
  whole difficulty — the signal must cross from pd-syntax's return values to
  pd-interpret's sink.

Decode entry points (`crates/pdf-syntax/src/object/stream.rs`):

- `decoded() -> Result<Vec<u8>, DecodeFailure>` (`:163`) — delegates to
  `decoded_image(&Default).map(|r| r.data)`. **74 call sites** workspace-wide.
- `decoded_image(&ImageDecodeParams) -> Result<FilterResult, DecodeFailure>`
  (`:170`) — **2 call sites**. Returns the **public** `FilterResult { data:
  Vec<u8>, image_data: Option<ImageData> }` (`:318`).

## 3. Why the earlier "broad / ~76 sites" assessment was wrong

The prior B-item note and the recon assumed reporting required adding a field to
`decoded()`'s return and threading it to all **76** `.decoded()`/`.decoded_image()`
callers — a **breaking** change. That is **not** necessary:

`decoded()` already discards everything except `.data`. We can carry the
recovery signal on the **already-returned** `FilterResult` and leave
`decoded()`'s signature (and its 74 callers) **untouched**. Only the handful of
sites that *want* the signal opt in by reading the `FilterResult`.

## 4. Proposed design (bounded, additive, non-breaking)

1. **`pd-syntax` — carry the signal (internal + one public field):**
   - Add `recovered: bool` to `FilterResult` (public struct; constructed only
     internally, callers only read `.data`/`.image_data`, so additive in
     practice). Mark `FilterResult` `#[non_exhaustive]` so future fields are
     clean.
   - Set the flag at the ~3–4 lenient branches in `lzw_flate.rs` (primary
     `.ok()` swallow; fallback entry at `:44`; warn-and-continue block paths)
     and propagate it (OR across filters) in `Filter::apply`
     (`filter/mod.rs`).
   - **`decoded()` is unchanged** — it keeps returning `Vec<u8>`; the 74 callers
     do not change.
2. **`pd-interpret` — surface it (3 sites):** at `util.rs:14` (content stream),
   `x_object.rs:421` (image), and `font/cid.rs:560` (font), read
   `FilterResult.recovered` (switching the two `.decoded()` sites to
   `decoded_image`) and emit a new `InterpreterWarning::ContentDecodeDegraded`.
   `InterpreterWarning` must first gain `#[non_exhaustive]` (it is currently
   `#[derive(Copy, Clone, Debug)]`, not non_exhaustive) so the new variant is
   not a breaking change.
3. **`pdfluent` — translate it:** map the new warning in
   `from_interpreter_warning` to a `Diagnostic` with
   `DiagnosticCategory::Decode` (already reserved) and a new stable code
   `CODE_CONTENT_DECODE_DEGRADED`, `Severity::Warning`.

**Blast radius:** ~6 files across pd-syntax (FilterResult, lzw_flate,
filter/mod.rs), pd-interpret (3 decode sites + enum), pdfluent (translation).
Not 76 sites. Additive and non-breaking given the two `#[non_exhaustive]`
markers; leniency behaviour is unchanged (we only *observe* it).

## 5. API impact

| Change | Kind | Note |
|---|---|---|
| `FilterResult.recovered: bool` + `#[non_exhaustive]` | additive | field-read callers unaffected; non_exhaustive future-proofs |
| `decoded()` / 74 callers | none | unchanged |
| `InterpreterWarning::ContentDecodeDegraded` + `#[non_exhaustive]` | additive (after marker) | requires the non_exhaustive marker to be non-breaking |
| `CODE_CONTENT_DECODE_DEGRADED`, `Category::Decode` | additive | `Diagnostic`/category already `#[non_exhaustive]`; lights a reserved category |

## 6. Why defer past the editor release

- It modifies the **flate decoder** — the most crash-critical, fuzzed code in
  the parser. Any change there must be re-fuzzed (the `fuzz_filters` /
  `fuzz_pdf_parser` targets) before shipping; that is a dedicated task, not a
  pre-release add-on.
- It spans **three crates** and lights a previously-dormant public diagnostic
  category — a feature, not a stabilization fix.
- The editor consumes only the read path (`render_page` / `extract_text*`);
  decode-leniency reporting is **not** on its critical path.

**Recommendation:** implement §4 as the **first post-editor task**, phased:
(1) pd-syntax flag + re-fuzz; (2) pd-interpret surfacing + enum marker;
(3) pdfluent translation; (4) tests (§7). It is now de-risked to a bounded,
additive change.

## 7. Test plan (for the implementation task)

- **Corrupt-stream fixtures** (deterministic, built in-test): truncated Flate,
  bad-Adler32 Flate, a stream that triggers the custom fallback decoder, a
  truncated LZW stream → each must yield **exactly one**
  `CONTENT_DECODE_DEGRADED` diagnostic (`Category::Decode`).
- **Clean false-positive controls:** a corpus of well-formed Flate/LZW streams
  must yield **zero** decode diagnostics; re-rendering the same document must
  not duplicate the diagnostic (one per degraded stream, no spam).
- **Leniency preserved:** the decoded `.data` for a recoverable stream is
  byte-for-byte identical before and after the change (we only add observation).
- **Fuzz:** re-run `fuzz_filters` and `fuzz_pdf_parser` after the decoder change
  (zero crashes) per `fuzz/README.md`.

## 8. Out of scope

Granular per-error reporting (which byte / which block failed) and any change to
the recovery *behaviour* itself. Only the boolean "this stream was decoded
leniently" signal is proposed.
