# PDFluent Diagnostic Catalogue

> **This is not the error catalogue.** `docs/error_catalogue.md` lists the
> variants of the `Error` enum — the things that make a call *fail*, each with
> an `E-` prefix. This file lists `Diagnostic` codes, which report what happened
> to a call that **succeeded**. They are a separate taxonomy and they do not
> overlap: `STREAM_TOO_LARGE` appears here and its error counterpart
> `E-BUDGET-RESOURCE-LIMIT` appears there, because that one condition can do
> both.
>
> **Append-only, same as the error catalogue.** Codes are frozen once assigned.
> Add new ones; never rename, remove or reassign an existing one — a caller may
> be matching on the string.
>
> **Why it matters that these are documented at all.** A diagnostic is how the
> SDK reports *silent* degradation: the render succeeded, the page came back,
> and something is missing from it. A caller who does not know
> `doc.diagnostics()` exists has no way to tell a complete page from an
> incomplete one. That is the whole reason for this file (#324).

**How to read them**

```rust
let doc = PdfDocument::open_with(path, OpenOptions::new())?;
let _ = doc.render_page(1, 150, ImageFormat::Png);
for d in doc.diagnostics() {
    eprintln!("{} [{:?}/{:?}] {}", d.code, d.severity, d.category, d.message);
}
```

`diagnostics()` reads the buffer; `take_diagnostics()` drains it.

**Severity** — `Info` (something was recovered, output is intact), `Warning`
(output is intact but incomplete or substituted), `Error` (the operation was
stopped).

**Total diagnostic codes:** 15

| Code | Severity | Category | What happened | What a caller should do |
|------|----------|----------|---------------|-------------------------|
| `FONT_UNSUPPORTED` | Warning | Font | An unsupported font was encountered and a fallback was substituted. | Text renders, but metrics and glyph shapes may differ from the source. Reject the output if visual fidelity is contractual. |
| `IMAGE_DECODE_FAILED` | Warning | Image | An image could not be decoded and was omitted. | The page is missing that image entirely. Treat as content loss for archival. |
| `STREAM_TOO_LARGE` | Error | Limit | A stream's decompressed size exceeded `max_stream_bytes` and the operation stopped. | Raise `ProcessingLimits::max_stream_bytes` if the document is trusted; otherwise treat the file as hostile. Also surfaces as `E-BUDGET-RESOURCE-LIMIT`. |
| `NESTING_TOO_DEEP` | Warning | Limit | Content-stream nesting hit the interpreter's depth limit; the nested content was not drawn. | **Paint is missing from the page** while the render still reports success. Reject the output for archival; a document reaching this is either malformed or hostile. |
| `XREF_REBUILT` | Warning | Repair | The cross-reference table was invalid and objects were recovered by scanning. | Object numbering may differ from the source. Usually harmless for rendering; relevant if you round-trip object references. |
| `PAGE_TREE_REBUILT` | Warning | Repair | The page tree was invalid and pages were recovered by a brute-force scan. | **Page order may differ from the source.** Verify order before printing or splitting. |
| `INDIRECT_CYCLE` | Warning | Repair | An indirect reference cycle was cut. | A value resolved to nothing. Expect a missing attribute rather than a missing page. |
| `INDIRECT_DEPTH_EXCEEDED` | Warning | Repair | Indirect-reference resolution hit its depth limit. | Same shape as above: something resolved to nothing rather than looping. |
| `FLATE_BROKEN_FALLBACK` | Warning | Decode | A flate stream failed the strict decoder and was decoded by the lenient fallback. | Output is usually complete. A file that needs this is malformed. |
| `FLATE_BAD_BLOCK` | Warning | Decode | A bad block header was found in a flate stream. | Data after the bad block may be missing. |
| `LZW_PREMATURE_EOF` | Warning | Decode | An LZW stream ended before its end marker. | The tail of that stream is missing. |
| `LZW_INVALID_CODE` | Warning | Decode | An invalid code was found in an LZW stream. | Decoding stopped at that point; the remainder is missing. |
| `ASCII85_LENIENT_PARTIAL` | Info | Decode | A one-character terminal group in ASCII-85 was accepted leniently. | Informational. The spec forbids it; the data is intact. |
| `CCITT_PARTIAL_DECODE` | Warning | Decode | A CCITT image decoded only partially. | Part of the image is missing, typically the lower rows. |
| `STREAM_PARSE_FALLBACK` | Info | Decode | The manual stream parser was used because the declared `/Length` was wrong. | Informational. The stream was recovered. |

Severity for the `Decode` and cycle codes is carried by the underlying
`LeniencyEvent` rather than fixed at the mapping site, so a code listed here as
`Warning` can arrive as `Info` when the event was raised as informational. The
code and category are fixed; the severity is the event's.

## Keeping this file true

`scripts/ci/every_diagnostic_code_is_documented.py` compares the `CODE_*`
constants in `crates/pdfluent/src/diagnostics.rs` with the codes in the table
above and fails on any difference in either direction. Adding a code without a
row fails; leaving a row behind after removing a code fails too, because a
catalogue that only grows describes a product that no longer exists.
