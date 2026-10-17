# `pdfluent protect`

> Generated from `pdfluent protect --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Set a password and restrict permissions

Usage: pdfluent protect [OPTIONS] --out <OUT> <INPUT>

Arguments:
  <INPUT>
          Input PDF, or `-` for stdin

Options:
      --json
          Machine-readable JSON envelope on stdout

  -o, --out <OUT>
          Output PDF, or `-` for stdout

      --password-file <FILE>
          Password file: line 1 is the user password, line 2 (if present) the owner password. Without it, PDFLUENT_PASSWORD and PDFLUENT_OWNER_PASSWORD, then an interactive prompt

  -q, --quiet
          Quiet: human mode prints data only (no warnings or progress notes)

      --owner-password-file <FILE>
          Owner-password file (line 1). Overrides line 2 of --password-file

  -v, --verbose...
          Verbose stderr diagnostics (repeat for more)

      --algorithm <ALGORITHM>
          Encryption algorithm

          Possible values:
          - aes256: AES-256 (default)
          - aes128: AES-128, for readers older than PDF 1.7 ext
          
          [default: aes256]

      --permissions <PERMISSIONS>
          Reader permissions

          Possible values:
          - full:       Authorised readers may do anything
          - print-only: Printing only
          - read-only:  Reading only
          - annotate:   Reading plus annotation
          
          [default: full]

      --force
          Overwrite the existing output file

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
