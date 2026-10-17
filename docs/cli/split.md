# `pdfluent split`

> Generated from `pdfluent split --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Divide a document into separate files

Usage: pdfluent split [OPTIONS] --out <OUT> <--pages <PAGES>|--every <N>> <INPUT>

Arguments:
  <INPUT>  Input PDF, or `-` for stdin

Options:
      --json           Machine-readable JSON envelope on stdout
      --pages <PAGES>  Pages to split out, e.g. `1-3,5`: one file per item
      --every <N>      Split into chunks of N pages
  -q, --quiet          Quiet: human mode prints data only (no warnings or progress notes)
  -o, --out <OUT>      Output directory (created when missing)
  -v, --verbose...     Verbose stderr diagnostics (repeat for more)
      --force          Overwrite existing part files
  -h, --help           Print help
  -V, --version        Print version
```
