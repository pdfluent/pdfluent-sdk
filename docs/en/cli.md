# PDFluent CLI

> **Status: scaffolded, not yet published.** Build from source for now
> (`cargo build -p pdfluent-cli`). The binary is currently named `pdfluent-cli`;
> the public `pdfluent` binary name is a separate, planned migration. This CLI is
> a companion to — not a replacement for — the PDFluent SDK.

## Commands

```bash
pdfluent-cli info sample.pdf
pdfluent-cli inspect sample.pdf            # = info --json
pdfluent-cli extract-text sample.pdf --out text.txt
pdfluent-cli validate sample.pdf
pdfluent-cli doctor
pdfluent-cli completions bash
```

| Command | Status | Notes |
|---------|--------|-------|
| `info` / `inspect` | implemented | page count, PDF version, file size |
| `extract-text` | implemented | `--out <path>`, `--json` |
| `validate` | implemented | parse + page-count check — **not** a PDF/A or veraPDF conformance check |
| `doctor` | implemented | environment/build info; no network, no telemetry |
| `completions` | implemented | shell completions |
| `xfa flatten` | **stub (experimental)** | not implemented in the public CLI yet |

## Exit codes
`0` success · `1` generic · `2` usage · `3` input not found / IO · `4` invalid/corrupt PDF ·
`5` unsupported · `6` not implemented · `7` license/commercial · `70` internal.

## JSON envelope
Success:
```json
{ "ok": true, "command": "info", "version": "1.0.0-beta.8", "data": { "page_count": 1 }, "warnings": [] }
```
Error:
```json
{ "ok": false, "command": "extract-text", "version": "1.0.0-beta.8",
  "error": { "code": "INVALID_PDF", "message": "…", "exit_code": 4 }, "warnings": [] }
```

## XFA caveat
XFA support is **experimental and feature-gated** — not production-supported, no Adobe Reader
parity claim. The `fresh-merge` policy is **opt-in and never the default** (requires `--experimental`).

## License
PDFluent Commercial License; evaluation use permitted. The CLI is not a replacement for the SDK.
