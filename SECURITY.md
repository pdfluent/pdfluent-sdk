# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 1.0.0-beta.x | Yes |
| < 1.0.0-beta.1 | No |

## Reporting a Vulnerability

If you discover a security vulnerability in PDFluent, please report it responsibly.

**Email:** security@pdfluent.com

**Do NOT** open a public GitHub issue for security vulnerabilities.

### What to include

- Description of the vulnerability
- Steps to reproduce (ideally with a minimal PDF file)
- Impact assessment (what an attacker could achieve)
- Affected crate(s) and version(s)

### Response timeline

| Step | Timeframe |
|------|-----------|
| Acknowledgment | Within 48 hours |
| Initial assessment | Within 5 business days |
| Fix development | Depends on severity (see below) |
| Public disclosure | After fix is released |

### Severity classification

| Severity | Description | Fix target |
|----------|-------------|------------|
| **Critical** | Arbitrary code execution, sandbox escape | 24 hours |
| **High** | Denial of service (OOM, infinite loop), information disclosure | 7 days |
| **Medium** | Crash on crafted input (parser panic), resource exhaustion | 14 days |
| **Low** | Minor information leak, non-exploitable edge case | Next release |

## Security measures in place

### Input validation

- **MAX_OBJECTS:** 500,000 indirect objects per PDF (prevents combinatorial explosion)
- **MAX_PAGES:** 50,000 pages per document
- **Flate decompression cap:** 100 MB per stream (prevents zip bombs)
- **RLIMIT_AS:** 8 GB virtual memory cap per process in corpus runner
- **Stream length validation:** Declared vs actual byte count verification
- **Xref rebuild:** Automatic recovery from corrupt cross-reference tables

### Fuzzing

20 libfuzzer targets (declared in `fuzz/Cargo.toml`, sources in
`fuzz/fuzz_targets/`) cover the input-facing APIs:

- `fuzz_pdf_parser` — PDF parsing and xref resolution
- `fuzz_content_stream` — Content stream operator parsing
- `fuzz_xref` — Cross-reference table parsing
- `fuzz_filters` — Flate/LZW/ASCII85/RunLength decompression
- `fuzz_data_dom` — XFA data DOM construction
- `fuzz_formcalc` — FormCalc expression evaluation
- `fuzz_som_path` — SOM path resolution
- `fuzz_content_editor` — Content stream editing
- `fuzz_text_replace` — Text replacement in content streams
- `fuzz_redact` — Text and area redaction
- `fuzz_pdfa_convert` — PDF/A conversion pipeline
- `fuzz_pdfa_validate` — PDF/A validation
- `fuzz_lopdf_roundtrip` — lopdf load/save roundtrip
- `fuzz_form_fill` — AcroForm field writing
- `fuzz_annot_create` — Annotation creation
- `fuzz_sign` — Detached signature creation
- `fuzz_sign_document` — Document signing pipeline
- `fuzz_xfa_extract` — XFA template/data extraction
- `fuzz_g3_content_stream` — Content-stream tokenizer
- `fuzz_xfa_template` — XFA DOM data-XML parsing

The GitHub Actions workflow (`.github/workflows/fuzz.yml`) has three jobs:

- **build** — `cargo +nightly fuzz build` on every trigger; fails on bit-rot
  (so no declared target can silently stop compiling).
- **smoke** — a 60-second run over the seven core targets (`fuzz_pdf_parser`,
  `fuzz_content_stream`, `fuzz_xref`, `fuzz_filters`, `fuzz_data_dom`,
  `fuzz_formcalc`, `fuzz_som_path`) on a nightly schedule.
- **deep** — a 300-second run (configurable) over all 20 targets on a weekly
  schedule.

Both run jobs upload crash reproducers as artifacts (30-day retention) and fail
the job on any crash. The workflow can be dispatched manually
(`workflow_dispatch`) for any target and duration. Run a target locally with
`cargo +nightly fuzz run <target>`; see `fuzz/README.md` for the crash-triage
process.

### Dependency management

- `cargo-audit` runs in CI to detect known vulnerabilities in dependencies
- Minimal dependency tree — pure Rust implementations preferred over C bindings
- No OpenSSL dependency — TLS via rustls

### Code safety

- Zero `unsafe` in application crates (only in FFI boundary crates: pdf-capi, pdf-java, pdf-ocr)
- Core library crates (pdf-engine, pdf-compliance, pdf-manip): 9 `unsafe` blocks total, all for performance-critical byte operations with safety comments
- `#[deny(unsafe_code)]` on pure-logic crates (formcalc-interpreter, xfa-layout-engine, xfa-license)

## Threat model

PDFluent processes untrusted PDF input. The security boundary is:

- **Trusted:** SDK API callers, configuration, font directories
- **Untrusted:** PDF file contents, embedded fonts, JavaScript/FormCalc in XFA, XMP metadata, embedded file attachments

The SDK must never:
- Execute arbitrary code from PDF content
- Access files outside explicitly configured directories
- Leak memory contents between unrelated documents
- Enter infinite loops on crafted input
- Consume unbounded memory on crafted input
