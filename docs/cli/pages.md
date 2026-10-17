# `pdfluent pages`

> Generated from `pdfluent pages --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Rotate, delete, reorder, or extract pages

Usage: pdfluent pages [OPTIONS] --out <OUT> <--rotate <DEG>|--delete <SPEC>|--reorder <SPEC>|--extract <SPEC>> <INPUT>

Arguments:
  <INPUT>  Input PDF, or `-` for stdin

Options:
      --json            Machine-readable JSON envelope on stdout
      --rotate <DEG>    Rotate pages by degrees: 90, 180, 270 (negatives allowed). Applies to `--pages`, or every page without it
      --delete <SPEC>   Delete pages, e.g. `2,5-7`
  -q, --quiet           Quiet: human mode prints data only (no warnings or progress notes)
      --reorder <SPEC>  New page order, e.g. `3,1,2`: every page exactly once
  -v, --verbose...      Verbose stderr diagnostics (repeat for more)
      --extract <SPEC>  Keep only these pages, e.g. `1-3,5`
      --pages <SPEC>    Pages `--rotate` applies to. Default: every page
  -o, --out <OUT>       Output PDF, or `-` for stdout
      --force           Overwrite the existing output file
  -h, --help            Print help
  -V, --version         Print version
```
