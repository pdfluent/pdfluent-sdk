# `pdfluent unprotect`

> Generated from `pdfluent unprotect --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Remove password protection

Usage: pdfluent unprotect [OPTIONS] --out <OUT> <INPUT>

Arguments:
  <INPUT>  Input PDF, or `-` for stdin

Options:
      --json                  Machine-readable JSON envelope on stdout
  -o, --out <OUT>             Output PDF, or `-` for stdout
      --password-file <FILE>  Password file (line 1 is the current password). Without it, PDFLUENT_PASSWORD, then an interactive prompt
  -q, --quiet                 Quiet: human mode prints data only (no warnings or progress notes)
      --force                 Overwrite the existing output file
  -v, --verbose...            Verbose stderr diagnostics (repeat for more)
  -h, --help                  Print help
  -V, --version               Print version
```
