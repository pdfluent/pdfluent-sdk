# `pdfluent compress`

> Generated from `pdfluent compress --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Reduce file size

Usage: pdfluent compress [OPTIONS] --out <OUT> <INPUT>

Arguments:
  <INPUT>
          Input PDF, or `-` for stdin

Options:
      --json
          Machine-readable JSON envelope on stdout

  -o, --out <OUT>
          Output PDF, or `-` for stdout

      --level <LEVEL>
          Compression preset

          Possible values:
          - strict:   Full stack, no lossy passes
          - lossy:    Everything strict enables, plus future lossy passes
          - archival: Maximally safe: keeps unused objects incremental updates may need
          
          [default: strict]

  -q, --quiet
          Quiet: human mode prints data only (no warnings or progress notes)

      --force
          Overwrite the existing output file

  -v, --verbose...
          Verbose stderr diagnostics (repeat for more)

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
