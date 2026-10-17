# PDFluent CLI (`pdfluent`)

The public command-line interface: every advertised SDK capability as a
subcommand, thin over the `pdfluent` facade. Anything the facade does not
expose is reported as a facade gap, never re-implemented in the CLI.

The binary is `pdfluent`; `pdfluent-cli` runs the same program as a
compatibility alias. Before the 30-09-2026 takeover the internal XFA tooling
owned the `pdfluent` name; it is `xfa-runner` now, and the names do not collide.

Install channels are listed in the crate README
(`crates/pdfluent-cli/README.md`): `cargo install`, Homebrew, release
binaries, `uvx`, and `npx`.

## Commands

| Command | Purpose |
|---|---|
| `extract <inputs...>` | PDF to Markdown (`md`), block JSON (`json`), Docling chunks (`chunks`), or plain text (`text`). Inputs are files, globs, directories, or `-` for stdin (alone). `--pages 1-3,5` selects; `-o <file\|dir\|->` routes output (`--recursive`, `--keep-going` + `--jobs N` for batches; default stops at the first error). `--quality balanced` runs the layout model tier (below); `best` fails until its models ship. |
| `models install layout tables` / `models status` | Install the layout and table-structure models (several names in one call) plus the platform's ONNX Runtime into the per-user model dir (the only command that ever downloads weights), and report what is installed with pin states. |
| `merge <a.pdf> <b.pdf>... -o out.pdf` | Combine in order (`--bookmarks concat`; `flatten`/`discard` are refused as a facade gap, below). One input may be `-`. |
| `split <in.pdf> (--pages 1-3,5 \| --every N) -o <dir>` | One file per `--pages` item, or chunks of N pages. |
| `pages <in.pdf> (--rotate DEG \| --delete SPEC \| --reorder SPEC \| --extract SPEC) -o out.pdf` | Page surgery. `--rotate` takes 90/180/270 (negatives allowed) over `--pages` or all pages; `--reorder` must list every page exactly once. |
| `compress <in.pdf> -o out.pdf [--level strict\|lossy\|archival]` | Reduce file size (default `strict`). |
| `protect <in.pdf> -o out.pdf` | Set a user password (and owner password, algorithm, permissions). |
| `unprotect <in.pdf> -o out.pdf` | Remove protection. Owner restrictions lift; removing a *user* password is a facade gap (below). |
| `watermark <in.pdf> (--text T \| --image F) -o out.pdf` | Text or PNG/JPEG across all pages (`--position`, `--at`, `--rotate`, `--opacity`, `--layer`, `--font-size`, `--color`, `--width`, `--height`). |
| `convert <in.pdf> --to docx\|xlsx\|pptx\|png\|jpeg\|pdfa -o <out>` | Office, image (`-o <dir>`, `--dpi`, `--pages` single range), or archival PDF (`--profile a1b\|a2b\|a3b`). |
| `redact <in.pdf> (--search TEXT \| --regex RE \| --boxes FILE) -o out.pdf` | Permanent removal (`--case-sensitive`, `--pages` for text modes). Boxes file: `[{"page": 1, "rect": [x0, y0, x1, y1]}]` in points, pages from 1. |
| `sign <in.pdf> --p12 FILE -o out.pdf` | PAdES B-B signature (`--reason`, `--location`, `--contact-info`, `--field`, `--visible PAGE,X0,Y0,X1,Y1`). `--tsa` and `--profile t` are a facade gap (below). |
| `validate <in.pdf> [--pdfa] [--profile a1b\|a2b\|a3b]` | Bare: parse + page-count check. `--pdfa`: conformance verdict with violations. `--pdfua` is a facade gap (below). |
| `info`, `inspect`, `extract-text`, `doctor`, `completions`, `xfa flatten` | Kept commands (below). |

Per-command flags: `pdfluent <command> --help`, generated to `docs/cli/`.

## Global flags

- `--json`: one machine-readable envelope on stdout (shapes below). Accepts
  position before or after the subcommand.
- `-q`: human mode prints data only — no warnings, no `Wrote …` notes.
  JSON output is unaffected (warnings there are data).
- `-v`: stderr diagnostics; repeat for more.

## The `--json` envelopes

Success:

```json
{
  "ok": true,
  "command": "info",
  "version": "1.0.0",
  "data": { "page_count": 1 },
  "warnings": []
}
```

Failure:

```json
{
  "ok": false,
  "command": "merge",
  "version": "1.0.0",
  "error": { "code": "FILE_NOT_FOUND", "message": "…", "exit_code": 2 },
  "warnings": []
}
```

Verdicts (a document that fails the asked standard) add a `data` object with
the findings next to `error`. Stdout holds exactly one JSON document; human
diagnostics stay on stderr in both modes.

## Exit codes and error codes

| Exit | Meaning | Codes |
|---|---|---|
| 0 | ok | — |
| 1 | usage | `BAD_USAGE`, `BAD_POLICY`, `EXPERIMENTAL_OPT_IN_REQUIRED` — unknown command/flag, missing argument, malformed value, exclusive options, the missing `--experimental` |
| 2 | input error | `FILE_NOT_FOUND`, `UNREADABLE_INPUT`, `INVALID_PDF`, `UNSUPPORTED_VERSION`, `PASSWORD_REQUIRED`, `BAD_PASSWORD`, `PASSWORD_MISMATCH`, `INVALID_IDENTITY`, `UNSUPPORTED_INPUT`, `PAGE_RANGE`, `EMPTY_RESULT`, `NOT_COMPLIANT` |
| 3 | processing error | everything else — `IO`, `FAILED`, `NOT_IMPLEMENTED`, `MODEL_NOT_AVAILABLE`, and the gap codes: `MODEL_NOT_SHIPPED`, `TSA_NOT_SUPPORTED`, `PDFUA_NOT_IN_FACADE`, `DECRYPT_NOT_SUPPORTED`, `BOOKMARKS_NOT_SUPPORTED` |

A wrong password is an input error (2); a right password the facade cannot
act on is a processing error (3). Non-compliance (`NOT_COMPLIANT`) is an
input error: the document fails the asked standard.

## Passwords

Never argv values (`ps` shows argv to every user on the machine). In order:

1. `--password-file <path>` — line 1 is the password (`protect` reads the
   owner password from line 2 unless `--owner-password-file` says otherwise;
   `PDFLUENT_OWNER_PASSWORD` applies only when line 2 is absent);
2. `PDFLUENT_PASSWORD` (user / current / P12) and `PDFLUENT_OWNER_PASSWORD`;
3. an interactive prompt — no-echo on a Unix TTY with confirmation, a single
   piped line otherwise.

Only trailing newlines are stripped: surrounding spaces are part of the
password. A missing password where one is required fails with
`PASSWORD_REQUIRED`, never with a hang.

## stdin and stdout

`-` reads stdin / writes stdout wherever a single document fits: every
command's input, and every single-file output. Binary outputs (`merge`,
`pages`, `protect`, `convert` office/PDF-A, `redact`, `sign`, …) refuse to
share stdout with `--json` (`BAD_USAGE`); text extraction wraps in the
envelope instead. Multi-file outputs (`split`, image conversion, batch
`extract`) need `-o <dir>`.

## Overwriting

File outputs are never clobbered silently: an existing output file is refused
with `BAD_USAGE` unless the command is passed `--force`, and an output that
names the input file is refused always — in-place rewriting would eat the
source, so pick another `-o`. Every file is written atomically, through a
temp file beside the target plus a rename, so a failed run leaves the
previous output untouched instead of half-written.

## Batch extraction

`extract` with several inputs stops at the first error by default: inputs
render through a pool of `--jobs` workers (default 1) but commit in input
order, a window at a time, so memory stays bounded no matter the batch
size. Outputs written before the failure are kept, later inputs are never
written, inputs that have not started are cancelled, and a re-run after
fixing the failed input needs `--force` only for what was written before
it.

`--keep-going` attempts and commits every input instead: later successes
are still written and every error is reported — human mode prints one
error line per failed input plus the `Wrote` notes, JSON keeps the first
error in input order as the envelope error with all successes under
`results` and all failures under `errors` — and the exit code is the
first error's. A re-run then needs `--force` for everything that was
written.

Two inputs that share an output name (`2024/invoice.pdf` and
`2025/invoice.pdf` both become `out/invoice.md`) are refused with
`BAD_USAGE` before anything is written — extract them in separate runs.
Names compare NFC-normalised and casefolded, so the refusal also holds
where the filesystem folds them: `A/Invoice.pdf` vs `B/invoice.pdf` on
macOS and Windows, composed vs decomposed `café` on macOS.

## Page counting and text joins

- Pages count from 1 everywhere (`--pages 1-3,5`); `0` and reversed ranges
  are usage errors, past-the-end pages are input errors.
- `extract-text` and `extract --format text` end every page with a form feed
  (U+000C), the `pdftotext`/`mutool` convention — `--no-page-breaks` (kept
  command only) joins with a blank line instead.
- JSON text larger than 64 KiB inlines a 2000-character preview plus a
  warning; `-o <file>` always writes the whole text.

## Balanced quality (`--quality balanced`)

`fast` is the shipping engine and the default. `balanced` regions each page
with the layout model (Heron-ft ep03) and fills the regions with the engine's
own text; it reads Markdown only. Two things must be in place:

1. a `pdfluent` built with the `ort-layout` cargo feature (`cargo install
   pdfluent-cli --features ort-layout`, or the matching release binary);
   the default build stays pure Rust and answers `MODEL_NOT_AVAILABLE`
   naming the feature;
2. the weights and the runtime beside it, installed once by an explicit
   `pdfluent models install layout tables` -- nothing is downloaded by
   default, ever. The tier runs ONNX Runtime 1.23.2 under
   `ort` =2.0.0-rc.13, loaded dynamically and never linked.

`models status` reports what the per-user model dir holds and whether the
pins verify; `--model-dir` (or `PDFLUENT_MODEL_DIR`) points `extract` at a
directory outside the default. Without the files the failure is
`MODEL_NOT_AVAILABLE` naming the path looked at and the install command.

`--table-model-dir DIR` structures the kept table regions with the TATR
table model (`tatr-v1.1-all/tatr-v1.1-all.onnx` under DIR, installed by
`pdfluent models install tables`) before the engine grid runs; it needs
a build with the `ort-tables` cargo feature, and without it extraction
refuses. Unset, `balanced` runs as it always has.

The tier comparison — what each tier needs, where it runs, preview or
not — is [Quality tiers](../STRUCTURED_EXTRACTION.md#quality-tiers).

## Kept commands

- `extract-text` stays byte-compatible for the extraction rig; new work
  should prefer `extract --format text`.
- `info` / `inspect` (always JSON), `doctor` (bare or with a file for
  decode-leniency events), `completions <shell>`.
- `xfa flatten` keeps its stub contract: `saved-state` answers
  `NOT_IMPLEMENTED`; `fresh-merge` additionally requires `--experimental`
  and is never the default. XFA is experimental and feature-gated — no
  Adobe Reader parity claim.

## Facade gaps (reported, not re-implemented)

- `unprotect` on a user-password file: the facade cannot serialise a
  password-opened document back to valid bytes (`DECRYPT_NOT_SUPPORTED`).
  Owner-restricted files that open without a password work.
- `sign --tsa` / `--profile t`: no TSA exchange in the facade
  (`TSA_NOT_SUPPORTED`). Only PAdES B-B signs.
- `validate --pdfua`: `pdf-compliance` has it, the facade does not expose it
  (`PDFUA_NOT_IN_FACADE`).
- `extract` of DOCX/XLSX/PPTX/EPUB/HTML/Markdown/text: extractors exist in
  the workspace but not on the facade (`UNSUPPORTED_INPUT`, per input).
- `extract --quality best`: models have not shipped
  (`MODEL_NOT_SHIPPED`). `balanced` is no gap: it runs where the
  `ort-layout` build and the layout model are installed (below).
- `merge --bookmarks flatten|discard`: the facade accepts the strategies but
  renders them concat-like (`BOOKMARKS_NOT_SUPPORTED`). Only `concat` merges.

Each fails with its code and names the gap; nothing writes a corrupt or
guessed output.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `BAD_USAGE: multiple inputs need -o <dir>` | batch to stdout | add `-o <dir>` (created when missing) |
| `PASSWORD_REQUIRED` on a `protect` | no password source | `--password-file`, `PDFLUENT_PASSWORD`, or a TTY |
| `DECRYPT_NOT_SUPPORTED` | user-password removal gap | owner-restricted files work; user passwords wait on the facade |
| `PAGE_RANGE` | spec past the last page | `info` the document first |
| `NOT_COMPLIANT` | not PDF/A | `--json` lists `data.violations` |
| `MODEL_NOT_SHIPPED` | `--quality best` | use `fast`, or `balanced` with the layout model installed |
| `MODEL_NOT_AVAILABLE` | `--quality balanced` without the tier | build with `--features ort-layout`, then `pdfluent models install layout` (add `tables` for table structure) |

## Binary coexistence (`pdfluent` vs internal `xfa-runner`)

The public CLI binary is **`pdfluent`** (with `pdfluent-cli` kept as a
compatibility alias). A separate internal crate (`xfa-cli`) builds a binary
named **`xfa-runner`** used by XFA runners and D13/QM tooling. They do **not**
collide (distinct names). Before the 30-09-2026 takeover (#636) the internal
binary was also called `pdfluent`; every in-repo consumer was repointed to
`xfa-runner` in that change.
