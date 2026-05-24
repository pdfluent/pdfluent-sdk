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

## Installing shell completions
`completions` writes the script to stdout. Install per shell:
```bash
pdfluent-cli completions bash | sudo tee /etc/bash_completion.d/pdfluent-cli   # bash
pdfluent-cli completions zsh  > ~/.zfunc/_pdfluent-cli                          # zsh (ensure ~/.zfunc on $fpath)
pdfluent-cli completions fish > ~/.config/fish/completions/pdfluent-cli.fish    # fish
pdfluent-cli completions powershell >> $PROFILE                                 # PowerShell
```
> Known limitation: generated completions currently use the program name `pdfluent`
> (clap `name`), while the shipped binary is `pdfluent-cli`. Until the planned
> `pdfluent` binary-takeover, invoke completions for the name your binary uses.

## CI / automation examples
JSON mode gives a stable, parseable contract; exit codes drive control flow.
```bash
# Fail a pipeline on an unreadable PDF (exit 4), parse fields with jq.
pages=$(pdfluent-cli info "$f" --json | jq -r '.data.page_count') || exit $?

# Validate a batch; collect failures by exit code (3=missing, 4=invalid).
for f in *.pdf; do
  pdfluent-cli validate "$f" --json >/dev/null || echo "FAIL($?): $f"
done

# Extract text to a file deterministically.
pdfluent-cli extract-text "$f" --out "${f%.pdf}.txt"
```
- stdout carries data (text / JSON); stderr carries human error lines. In `--json`
  mode the envelope (success or error) goes to stdout as a single JSON document.
- No network, no telemetry, no temp-file surprises (`extract-text --out` writes only the path you give).

## Troubleshooting
| Symptom | Exit | Cause / fix |
|---|---|---|
| `[FILE_NOT_FOUND] no such file` | 3 | path wrong / not readable |
| `[INVALID_PDF] could not open PDF` | 4 | corrupt/encrypted/non-PDF input |
| `[NOT_IMPLEMENTED] xfa flatten …` | 6 | XFA flatten is a stub in the public CLI; use the SDK/internal tooling |
| `[EXPERIMENTAL_OPT_IN_REQUIRED]` | 5 | `--policy fresh-merge` needs `--experimental` |
| usage error / unknown command | 2 | run `--help` |

## Limitations (current public CLI)
- `validate` is a parse + page-count check, **not** PDF/A or veraPDF conformance.
- `xfa flatten` is an experimental **stub** (`NOT_IMPLEMENTED`); no Adobe parity claim.
- Encrypted/password PDFs are reported as `INVALID_PDF` (no decryption in the CLI).
- No rendering/rasterization, merge/split, or form-fill commands yet (SDK has these).
- Not published; build from source (`cargo build -p pdfluent-cli`).

## Binary coexistence (`pdfluent-cli` vs internal `pdfluent`)
The public CLI binary is **`pdfluent-cli`**. A separate internal crate (`xfa-cli`) builds a
binary named **`pdfluent`** used by XFA runners and D13/QM tooling. They do **not** collide
(distinct names). Taking over the `pdfluent` name for the public CLI is a **separate, deferred
milestone** that must first repoint every internal consumer of `target/release/pdfluent`.
