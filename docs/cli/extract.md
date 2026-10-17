# `pdfluent extract`

> Generated from `pdfluent extract --help` by `scripts/ci/generate_cli_docs.sh`. Do not edit; edit the clap definitions in `crates/pdfluent-cli` instead.

Version: `1.1.0`

## Reference

```text
Extract a document as Markdown, JSON, chunks, or text

Usage: pdfluent extract [OPTIONS] <INPUTS>...

Arguments:
  <INPUTS>...
          Input PDFs, globs, directories, or `-` for stdin (alone)

Options:
      --format <FORMAT>
          Output format

          Possible values:
          - md:     Markdown with headings, lists, and tables
          - json:   Block-model JSON (`to_json` over `extract_blocks`)
          - chunks: Docling JSON chunks for RAG ingestion
          - text:   Plain text, one form feed per page
          - words:  Every word with its box, one JSON object per line
          
          [default: md]

      --json
          Machine-readable JSON envelope on stdout

  -q, --quiet
          Quiet: human mode prints data only (no warnings or progress notes)

      --quality <QUALITY>
          Extraction quality. `balanced` needs the `ort-layout` build and the layout model (`pdfluent models install layout`)

          Possible values:
          - fast:     The shipping engine
          - balanced: The layout model tier; fails naming the install until it is there
          - best:     Needs unshipped models; errors until they land
          
          [default: fast]

      --model-dir <MODEL_DIR>
          Where the model tier looks for its weights. Default: the `PDFLUENT_MODEL_DIR` environment variable, else the per-user model directory. Read only with `--quality balanced`

  -v, --verbose...
          Verbose stderr diagnostics (repeat for more)

      --table-model-dir <TABLE_MODEL_DIR>
          Where the TATR table model lives: `tatr-v1.1-all/tatr-v1.1-all.onnx` under this directory structures the balanced tier's kept table regions first (track B, P4). Needs the `ort-tables` build; without it extraction refuses. Unset, `balanced` runs as it always has

      --pages <PAGES>
          Pages to extract, e.g. `1-3,5`. Default: the whole document

      --images <IMAGES>
          What the Markdown carries for figures: `none` writes no reference, `embed` inlines each figure's pixels as a base64 data URL, and `dir:<path>` writes one PNG per figure and references it. Unset, references are markers naming the figure. With `dir:`, each input's files land in `<path>/<stem>/`, so batch inputs never share names

  -o, --out <OUT>
          Output file, directory, or `-`/stdout. Directories are created. Multiple inputs need a directory

      --recursive
          Descend into subdirectories (symlinks are never followed)

      --jobs <JOBS>
          Parallel inputs when writing files (stdout stays sequential)
          
          [default: 1]

      --keep-going
          Keep going after a failed input. Default stops at the first error, keeping earlier outputs and cancelling the rest; with this every input is attempted, later successes are still written, and every error is reported (a re-run then needs --force for those)

      --force
          Overwrite existing output files

      --words
          Carry each block's word lines in the JSON output (`--format json`). Without the flag the JSON is exactly what it was: no word lines

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
