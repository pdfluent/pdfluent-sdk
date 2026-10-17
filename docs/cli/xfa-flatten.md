# `pdfluent xfa flatten`

> Generated from `pdfluent xfa flatten --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Flatten an XFA/AcroForm document (not yet implemented in the public CLI).

XFA support is EXPERIMENTAL and feature-gated — no Adobe Reader parity claim. `--policy saved-state` (default) honors the document's saved form state; `--policy fresh-merge` is EXPERIMENTAL, opt-in, and never the default (requires `--experimental`).

Usage: pdfluent xfa flatten [OPTIONS] --out <OUT> <INPUT>

Arguments:
  <INPUT>
          Input PDF file

Options:
      --json
          Machine-readable JSON envelope on stdout

  -o, --out <OUT>
          Output PDF file

      --policy <POLICY>
          XFA rendering policy: `saved-state` (default) or `fresh-merge` (experimental, opt-in)
          
          [default: saved-state]

  -q, --quiet
          Quiet: human mode prints data only (no warnings or progress notes)

      --experimental
          Acknowledge that you are using an experimental, opt-in code path

  -v, --verbose...
          Verbose stderr diagnostics (repeat for more)

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
