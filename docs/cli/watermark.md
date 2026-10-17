# `pdfluent watermark`

> Generated from `pdfluent watermark --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Add a text or image watermark across all pages

Usage: pdfluent watermark [OPTIONS] --out <OUT> <INPUT>

Arguments:
  <INPUT>
          Input PDF, or `-` for stdin

Options:
      --json
          Machine-readable JSON envelope on stdout

  -o, --out <OUT>
          Output PDF, or `-` for stdout

  -q, --quiet
          Quiet: human mode prints data only (no warnings or progress notes)

      --text <TEXT>
          Watermark text (exactly one of --text/--image)

      --image <FILE>
          Watermark image (PNG or JPEG; exactly one of --text/--image)

  -v, --verbose...
          Verbose stderr diagnostics (repeat for more)

      --position <POSITION>
          Placement on the page

          Possible values:
          - center:       Centred (default)
          - top-left:     Top-left corner
          - top-right:    Top-right corner
          - bottom-left:  Bottom-left corner
          - bottom-right: Bottom-right corner
          - exact:        Exact coordinates (needs `--at X,Y`)
          
          [default: center]

      --at <X,Y>
          Coordinates in PDF points: inset from the corner for corner placements (default `36,36`), absolute for `exact` (required)

      --rotate <ROTATE>
          Rotation in degrees (text only)
          
          [default: 0]

      --opacity <OPACITY>
          Opacity from 0 (invisible) to 1 (opaque)
          
          [default: 0.5]

      --layer <LAYER>
          Foreground or background placement (text only)

          Possible values:
          - foreground: In front of the content
          - background: Behind the content
          
          [default: foreground]

      --font-size <FONT_SIZE>
          Font size in points (text only)
          
          [default: 48]

      --color <COLOR>
          Text colour as RRGGBB hex (text only)
          
          [default: 999999]

      --width <WIDTH>
          Image display width in points (image only; default scales the image to half the page width, keeping its aspect)

      --height <HEIGHT>
          Image display height in points (image only; default keeps aspect)

      --force
          Overwrite the existing output file

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
