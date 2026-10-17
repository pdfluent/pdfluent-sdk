# `pdfluent validate`

> Generated from `pdfluent validate --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Validate: parse check, PDF/A conformance, or (unexposed) PDF/UA

Usage: pdfluent validate [OPTIONS] <INPUT>

Arguments:
  <INPUT>
          Input PDF, or `-` for stdin

Options:
      --json
          Machine-readable JSON envelope on stdout

      --pdfa
          Validate PDF/A conformance (default profile a2b, see --profile)

      --pdfua
          Validate PDF/UA conformance. Accepted and reported: the facade does not expose it

  -q, --quiet
          Quiet: human mode prints data only (no warnings or progress notes)

      --profile <PROFILE>
          PDF/A profile for --pdfa

          Possible values:
          - a1b: PDF/A-1b
          - a2b: PDF/A-2b (default)
          - a3b: PDF/A-3b
          
          [default: a2b]

  -v, --verbose...
          Verbose stderr diagnostics (repeat for more)

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
