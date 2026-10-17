# pdfluent-cli

The PDFluent command-line interface: extract, merge, split, organise, compress,
protect, watermark, convert, redact, sign, and validate PDFs — a thin layer over
the [`pdfluent`](https://crates.io/crates/pdfluent) SDK facade.

Repository: <https://github.com/pdfluent/pdfluent-sdk> ·
Docs: <https://pdfluent.com/docs> ·
Pricing: <https://pdfluent.com/sdk/pricing>

## Install

```sh
cargo install pdfluent-cli
```

Further channels ship with the release (entries published by packaging):

```sh
brew install pdfluent/tap/pdfluent     # Homebrew
# Release binaries: https://github.com/pdfluent/pdfluent-sdk/releases
uvx pdfluent-extract                   # Python wrapper
npx @pdfluent/extract                  # Node wrapper
```

The binary is `pdfluent`; `pdfluent-cli` remains as a compatibility alias and
runs the same program.

## Quick start

```sh
pdfluent extract report.pdf -o report.md
pdfluent merge a.pdf b.pdf -o combined.pdf
pdfluent pages combined.pdf --delete 1 -o trimmed.pdf
pdfluent convert trimmed.pdf --to docx -o trimmed.docx
pdfluent validate trimmed.pdf --pdfa --profile a2b
```

## Commands

| Command | What it does |
|---|---|
| `extract` | PDF to Markdown, JSON, chunks, or text; files, globs, and directories |
| `merge` | Combine PDFs in order |
| `split` | Divide into files (`--pages` or `--every N`) |
| `pages` | Rotate, delete, reorder, or extract pages |
| `compress` | Reduce file size (`--level strict\|lossy\|archival`) |
| `protect` | Set a password and permissions |
| `unprotect` | Remove password protection |
| `watermark` | Text (`--text`) or image (`--image`) across all pages |
| `convert` | To `docx`, `xlsx`, `pptx`, `png`, `jpeg`, or `pdfa` |
| `redact` | Permanently remove text (`--search`, `--regex`) or regions (`--boxes`) |
| `sign` | PAdES signature with a PKCS#12 identity |
| `validate` | Parse check, or `--pdfa` conformance |
| `info`, `inspect` | Document facts, human or JSON |
| `extract-text` | Plain text (kept for compatibility; prefer `extract`) |
| `doctor` | Environment and build report |
| `completions` | Shell completions |
| `xfa flatten` | Experimental XFA flattening (not implemented yet) |

`pdfluent <command> --help` documents every flag. Generated per-command pages
live in `docs/cli` next to this crate's sources.

`extract` runs in one of two quality tiers — `fast` by default,
`balanced` as a preview over installed models; see
[Quality tiers](../../docs/STRUCTURED_EXTRACTION.md#quality-tiers).

## Output, errors, exit codes

- `--json` prints one machine-readable envelope on stdout; data goes to
  stdout, diagnostics to stderr, in both modes.
- `-q` silences warnings and progress notes; `-v` adds stderr diagnostics.
- Exit codes: `0` ok, `1` usage, `2` input error, `3` processing error.
- Commands take `-` for stdin/stdout where a single document fits.

## Passwords

Passwords arrive via `--password-file`, the environment (`PDFLUENT_PASSWORD`,
and `PDFLUENT_OWNER_PASSWORD` for `protect`), or an interactive prompt —
never as argv values, which are visible in `ps`.

## Honest limits

A few advertised restrictions come from the facade, not this CLI: `unprotect`
removes owner restrictions but not a user password, `sign --tsa` and
`--profile t` need a timestamp exchange the facade does not expose,
`validate --pdfua` is not exposed either, and `extract` takes PDF only (office
and ebook inputs fail loudly instead of guessing). Each reports its gap with a
clear error instead of re-implementing the missing piece.

## License

AGPL-3.0 or a commercial licence, at your option. The AGPL
build is the complete product: every command above works without paying
anything, and there is nothing to unlock. If the AGPL does not fit your use,
commercial terms are at <https://pdfluent.com/sdk/pricing>.
