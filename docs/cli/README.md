# `pdfluent`

> Generated from `pdfluent --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
PDFluent — PDF toolkit: extract, merge, split, compress, protect, watermark, convert, redact, sign, validate. AGPL-3.0 or a commercial licence, at your option.

Usage: pdfluent [OPTIONS] <COMMAND>

Commands:
  extract       Extract a document as Markdown, JSON, chunks, or text
  merge         Combine several PDFs into one, in the order given
  split         Divide a document into separate files
  pages         Rotate, delete, reorder, or extract pages
  compress      Reduce file size
  protect       Set a password and restrict permissions
  unprotect     Remove password protection
  watermark     Add a text or image watermark across all pages
  convert       Convert to Word, Excel, PowerPoint, image, or PDF/A
  redact        Permanently remove text or regions, not just cover them
  sign          Apply a PAdES signature with a PKCS#12 identity
  validate      Validate: parse check, PDF/A conformance, or (unexposed) PDF/UA
  info          Show document information (page count, PDF version, file size)
  inspect       Inspect a document and emit JSON (alias of `info --json`)
  extract-text  Extract plain text (kept for compatibility; prefer `extract`)
  doctor        Check environment / installation and print build info
  models        Install and inspect the opt-in model tiers' files
  completions   Generate shell completions
  xfa           Experimental XFA tooling (feature-gated, not production-supported)
  help          Print this message or the help of the given subcommand(s)

Options:
      --json        Machine-readable JSON envelope on stdout
  -q, --quiet       Quiet: human mode prints data only (no warnings or progress notes)
  -v, --verbose...  Verbose stderr diagnostics (repeat for more)
  -h, --help        Print help
  -V, --version     Print version
```

## Commands

- [`pdfluent extract`](extract.md)
- [`pdfluent merge`](merge.md)
- [`pdfluent split`](split.md)
- [`pdfluent pages`](pages.md)
- [`pdfluent compress`](compress.md)
- [`pdfluent protect`](protect.md)
- [`pdfluent unprotect`](unprotect.md)
- [`pdfluent watermark`](watermark.md)
- [`pdfluent convert`](convert.md)
- [`pdfluent redact`](redact.md)
- [`pdfluent sign`](sign.md)
- [`pdfluent validate`](validate.md)
- [`pdfluent info`](info.md)
- [`pdfluent inspect`](inspect.md)
- [`pdfluent extract-text`](extract-text.md)
- [`pdfluent doctor`](doctor.md)
- [`pdfluent models`](models.md)
- [`pdfluent completions`](completions.md)
- [`pdfluent xfa`](xfa.md)
- [`pdfluent xfa flatten`](xfa-flatten.md)
