# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 1.0.0-beta.x | Yes |
| < 1.0.0-beta.1 | No |

## Reporting a Vulnerability

**Email:** security@pdfluent.com

That address is a role address and it is the whole of the reporting route.
**Do not open a public GitHub issue for a vulnerability**: an issue publishes the
flaw to everyone who has not yet updated, including the people who will not read
it until the fix exists.

### What to include

- Description of the vulnerability
- Steps to reproduce (ideally with a minimal PDF file)
- Impact assessment (what an attacker could achieve)
- Affected crate(s) and version(s)

### What you can expect back

| Step | Aim |
|------|-----|
| Acknowledgment | 2 working days |
| First assessment, with a severity | 5 working days |
| Fix | as fast as the severity below argues for |
| Public disclosure | after the fix is released, crediting you unless you ask otherwise |

These are aims and not a service level. PDFluent is maintained by a small team
and a report that arrives on a Friday evening is read on Monday; a report that
turns out to need a change in a font or a decompression path can take longer
than the row below suggests. If you have heard nothing after five working days,
send the message again — the likeliest explanation is that it never arrived.

### How urgently a fix is attempted

| Severity | Description | Aim |
|----------|-------------|-----|
| **Critical** | Arbitrary code execution, sandbox escape | days |
| **High** | Denial of service (OOM, infinite loop), information disclosure | 1–2 weeks |
| **Medium** | Crash on crafted input (parser panic), resource exhaustion | a few weeks |
| **Low** | Minor information leak, non-exploitable edge case | next release |

### There is no bug bounty

No payment, no reward, and no prize is offered for a report, and none has ever
been paid. This is stated because the alternative is worse than saying nothing:
a policy that stays quiet about it collects reports written in the expectation
of money, and the disappointment lands on the person who did the work. What is
offered is credit in the release notes and in the advisory, unless you prefer
not to be named.

Nor is there an authorisation to test somebody else's deployment. Test against
your own copy of the library — a report obtained by attacking a running service
that is not yours is not a report we can act on.

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

Three runs are scheduled over them: a build of all 20 targets whenever `fuzz/`
or `crates/` changes, so a target cannot rot unnoticed; a 60-second smoke run
nightly over the seven core targets (`fuzz_pdf_parser`, `fuzz_content_stream`,
`fuzz_xref`, `fuzz_filters`, `fuzz_data_dom`, `fuzz_formcalc`, `fuzz_som_path`);
and a 120-second run weekly over all 20. Any crash fails the run and its
reproducer is kept.

Run a target yourself with `cargo +nightly fuzz run <target>`; `fuzz/README.md`
describes what to do with a crash it finds.

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
