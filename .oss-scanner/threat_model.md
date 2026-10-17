# Threat model: PDFluent SDK

Read with `SECURITY.md`, which has the reporting route and the severity table
this file follows.

## What this project does and where untrusted input enters

PDFluent is a PDF library written in Rust. Bindings for Python, Node.js, .NET,
Java, C and WebAssembly call the same Rust core. It opens, renders, extracts
text and structure from, edits, fills, signs, redacts and validates PDF files.

Treat every input PDF as untrusted. Applications hand the library documents
from uploads, email and crawlers. Untrusted bytes enter through document
loading (`crates/pdfluent`, `crates/pdf-engine`) and reach:

- the lexer, object parser, cross-reference tables and streams, object streams,
  and the repair path for broken files (`crates/pdf-syntax`, `crates/lopdf`);
- stream filters: Flate, LZW, ASCII85, ASCIIHex, RunLength, predictors, CCITT
  (`crates/hayro-ccitt`), DCT, JPEG 2000 (`crates/hayro-jpeg2000`) and JBIG2
  (`crates/hayro-jbig2`);
- decryption of encrypted documents;
- fonts: Type 1, CFF (`crates/cff-parser`), TrueType, Type 3, CMaps and
  ToUnicode maps (`crates/pdf-font`);
- content streams, images, colour spaces, shadings and transparency
  (`crates/pdf-interpret`, `crates/pdf-render`, `crates/pdf-content-stream`);
- XFA forms: the XML template and data DOM, SOM path resolution, layout,
  FormCalc and the script sandbox (`crates/pdf-xfa`, `crates/xfa-dom-resolver`,
  `crates/xfa-layout-engine`, `crates/formcalc-interpreter`,
  `crates/xfa-js-sandboxed`);
- AcroForm fields, annotations, and CMS/PKCS#7 signature parsing and
  verification (`crates/pdf-forms`, `crates/pdf-annot`, `crates/pdf-sign`);
- redaction (`crates/pdf-redact`): content the API reports as removed must not
  be recoverable from the output file;
- text, table and structure extraction (`crates/pdf-extract`);
- merging, splitting, encryption, watermarking and compression
  (`crates/pdf-manip`);
- PDF/A conversion and validation (`crates/pdf-compliance`).

The caller's API arguments, configuration and font directories are trusted.

## Components that matter most / least

- Most important: everything reachable from loading, parsing, rendering,
  extracting, filling, redacting and saving a document, and from signature
  verification.
- In scope, although they started as forks: `pdf-syntax`, `pdf-interpret`,
  `pdf-font`, `pdf-render`, `hayro-jbig2`, `hayro-jpeg2000`, `hayro-ccitt`,
  `lopdf` and `cff-parser`. They are maintained and modified in this
  repository.
- Lower priority: building documents from trusted caller data; the Office
  conversions (`pdf-docx`, `pdf-xlsx`, `pdf-pptx`); OCR (`pdf-ocr`); binding
  glue (`pdf-capi`, `pdf-node`, `pdf-java`, `pdf-python`, `xfa-wasm`) unless
  the defect is in the shared core.
- Out of scope: `pdf-bench`, `xfa-test-runner`, `visual-regression`,
  `xfa-pdfrest-compare`, `test-skip`, `xfa-api-server`, `pdf-desktop`,
  `benchmarks/`, `scripts/` and `tools/`. These are test, benchmark and
  internal tooling, not shipped library code.

## How to exercise it

- `target/release/pdfluent` is the command-line driver (crate `xfa-cli`).
  `pdfluent --help` lists its commands.
- `fuzz/fuzz_targets/` holds the libFuzzer targets; the built binaries are in
  `fuzz/target/<triple>/release/` and seed inputs in `fuzz/corpus/<target>/`.
  Run one with
  `cd fuzz && cargo +nightly-2026-10-01 fuzz run <target> -- -max_total_time=60`.
- `cargo test --release -p <crate>` runs a crate's tests. The test binaries of
  `pdf-syntax`, `pdfluent-lopdf`, `xfa-dom-resolver`, `formcalc-interpreter`
  and `pdfluent` are already compiled.
- `fixtures/` and `tests/corpus-mini/` hold sample PDFs, including XFA forms.

## How we rate severity

This follows the table in `SECURITY.md`:

- Critical: code execution from a crafted document, an escape from the XFA or
  FormCalc sandbox, or a document that makes the library read or write files or
  open network connections.
- High: memory corruption reachable from a crafted document without a
  demonstrated path to code execution; redacted content that is still
  recoverable from the output; a signature reported valid on a modified or
  forged document; an infinite loop or unbounded memory growth from a small
  input; disclosure of memory or of data from another document.
- Medium: a panic or abort on crafted input in library code; resource use that
  is bounded but far out of proportion to the input.
- Low: a minor information leak or an edge case with no practical exploit.

Panics, crashes and leaks in test, benchmark or tooling code are not findings.

## Anything to leave alone

- Rendering or extraction output that is merely inaccurate, with no security
  effect.
- The out-of-scope directories listed above.
- Missing features of the PDF specification, unless their absence makes the
  library accept or report something unsafe.

## What a good report looks like

A minimal input file, the command or test that triggers it, the backtrace, and
a patch against the affected crate with a regression test.
