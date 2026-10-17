# `pdfluent sign`

> Generated from `pdfluent sign --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Apply a PAdES signature with a PKCS#12 identity

Usage: pdfluent sign [OPTIONS] --out <OUT> --p12 <FILE> <INPUT>

Arguments:
  <INPUT>
          Input PDF, or `-` for stdin

Options:
      --json
          Machine-readable JSON envelope on stdout

  -o, --out <OUT>
          Output PDF, or `-` for stdout

      --p12 <FILE>
          PKCS#12 identity (`.p12`/`.pfx`)

  -q, --quiet
          Quiet: human mode prints data only (no warnings or progress notes)

      --password-file <FILE>
          Password file for the identity (line 1). Without it, PDFLUENT_PASSWORD, then an interactive prompt

  -v, --verbose...
          Verbose stderr diagnostics (repeat for more)

      --tsa <URL>
          Timestamp authority URL. Accepted and reported: fetching a token needs a TSA exchange the facade does not expose

      --profile <PROFILE>
          PAdES profile (only `basic` signs today)

          Possible values:
          - basic: PAdES B-B: basic signature, no timestamp
          - t:     PAdES B-T: refused — needs `--tsa`, which the facade cannot serve
          
          [default: basic]

      --reason <REASON>
          Human-readable reason for signing

      --location <LOCATION>
          Location of signing

      --contact-info <CONTACT_INFO>
          Signer contact information

      --field <FIELD>
          Signature form field name (default `Signature1`)

      --visible <PAGE,X0,Y0,X1,Y1>
          Visible signature rectangle: `PAGE,X0,Y0,X1,Y1` in points

      --force
          Overwrite the existing output file

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
