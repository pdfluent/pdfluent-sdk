# `pdfluent redact`

> Generated from `pdfluent redact --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Permanently remove text or regions, not just cover them

Usage: pdfluent redact [OPTIONS] --out <OUT> <--search <TEXT>|--regex <RE>|--boxes <FILE>> <INPUT>

Arguments:
  <INPUT>  Input PDF, or `-` for stdin

Options:
      --json            Machine-readable JSON envelope on stdout
  -o, --out <OUT>       Output PDF, or `-` for stdout
  -q, --quiet           Quiet: human mode prints data only (no warnings or progress notes)
      --search <TEXT>   Literal text to redact everywhere it appears
      --regex <RE>      Regular expression to redact
  -v, --verbose...      Verbose stderr diagnostics (repeat for more)
      --boxes <FILE>    JSON boxes file: `[{"page": 1, "rect": [x0, y0, x1, y1]}]` (points, 1-based pages) or `{"boxes": [...]}`
      --case-sensitive  Case-sensitive text matching
      --pages <PAGES>   Limit text search to pages, e.g. `1-3,5`
      --force           Overwrite the existing output file
  -h, --help            Print help
  -V, --version         Print version
```
