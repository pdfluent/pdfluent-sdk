# `pdfluent convert`

> Generated from `pdfluent convert --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Convert to Word, Excel, PowerPoint, image, or PDF/A

Usage: pdfluent convert [OPTIONS] --to <TO> --out <OUT> <INPUT>

Arguments:
  <INPUT>
          Input PDF, or `-` for stdin

Options:
      --json
          Machine-readable JSON envelope on stdout

      --to <TO>
          Conversion target

          Possible values:
          - docx: Word document (`.docx`)
          - xlsx: Excel workbook (`.xlsx`)
          - pptx: PowerPoint deck (`.pptx`)
          - png:  PNG images, one per page (needs `-o <dir>`)
          - jpeg: JPEG images, one per page (needs `-o <dir>`)
          - pdfa: Archival PDF (PDF/A)

  -o, --out <OUT>
          Output file, or a directory for images (created when missing). Single-file targets also accept `-` for stdout

  -q, --quiet
          Quiet: human mode prints data only (no warnings or progress notes)

      --dpi <DPI>
          Render resolution for images (default 150)

  -v, --verbose...
          Verbose stderr diagnostics (repeat for more)

      --profile <PROFILE>
          PDF/A profile for `--to pdfa` (default a2b)

          Possible values:
          - a1b: PDF/A-1b
          - a2b: PDF/A-2b (default)
          - a3b: PDF/A-3b

      --pages <PAGES>
          Page range for images, e.g. `1-3` (single range only)

      --raster <RASTER>
          Raster pipeline for images: `quality` (default, f32) or `speed` (u8, faster)

          Possible values:
          - quality: Higher-precision f32 pipeline (default, historical output)
          - speed:   Faster u8 pipeline (~1.4–1.6× on content-heavy pages)

      --force
          Overwrite existing output files

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
