# `pdfluent merge`

> Generated from `pdfluent merge --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Combine several PDFs into one, in the order given

Usage: pdfluent merge [OPTIONS] --out <OUT> <INPUTS>...

Arguments:
  <INPUTS>...
          Input PDFs in merge order (`-` reads one input from stdin)

Options:
      --json
          Machine-readable JSON envelope on stdout

  -o, --out <OUT>
          Output PDF, or `-` for stdout

      --bookmarks <BOOKMARKS>
          How to combine bookmarks

          Possible values:
          - concat:  Each source keeps its bookmarks under a top-level entry
          - flatten: All bookmarks flattened into one top-level sequence. Refused until the facade supports it (it renders concat-like today)
          - discard: Bookmarks dropped. Refused until the facade supports it
          
          [default: concat]

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
