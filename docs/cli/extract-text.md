# `pdfluent extract-text`

> Generated from `pdfluent extract-text --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Extract plain text (kept for compatibility; prefer `extract`)

Usage: pdfluent extract-text [OPTIONS] <INPUT>

Arguments:
  <INPUT>
          Input PDF file, or `-` for stdin

Options:
      --json
          Machine-readable JSON envelope on stdout

      --out <OUT>
          Write extracted text to this path instead of stdout

      --no-page-breaks
          Join pages with a blank line instead of ending each with a form feed (U+000C).
          
          Page boundaries are lost; for a consumer that chunks or cites by page they are the point. Here for pipelines written against the pre-1.0 output.

  -q, --quiet
          Quiet: human mode prints data only (no warnings or progress notes)

      --force
          Overwrite the existing `--out` file

  -v, --verbose...
          Verbose stderr diagnostics (repeat for more)

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
