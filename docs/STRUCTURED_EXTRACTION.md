# Structured extraction: the block model and its three serialisations

What PDFluent hands a caller that wants a document's STRUCTURE rather than its
characters, what each of the two forms is for, and how to ask for it from each
of the six languages.

`PdfDocument::extract_text` gives a wall of text. `extract_text_blocks` gives
the page's lines with their geometry. Neither says which line is a heading,
which run of lines is one paragraph, which cell stands under which column, or
which line is the running head the document repeats on every page — and those
are the questions anything that chunks, cites, embeds or converts a document
has to answer first.

`PdfDocument::extract_blocks` answers them. It returns `Vec<Block>` in the
document's reading order, and the two functions here write that vector out.

| Function | For | Carries |
|---|---|---|
| `pdf_engine::to_markdown` | a reader, a RAG pipeline, a converter | headings, lists, tables, page markers |
| `pdf_engine::to_json` | a caller that chunks the document itself | every block with kind, page, box, text, spans and reading-order index |
| `pdf_engine::to_docling_json` | a pipeline built on Docling | the same document as Docling's `DoclingDocument` (see [The Docling form](#the-docling-form)) |

```rust
use pdfluent::{to_json, to_markdown, ExtractOptions, PdfDocument};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = PdfDocument::open("report.pdf")?;
    let blocks = document.extract_blocks(&ExtractOptions::default())?;

    std::fs::write("report.md", to_markdown(&blocks))?;
    std::fs::write("report.json", to_json(&blocks))?;
    Ok(())
}
```

`extract_markdown` is the first two lines in one call, and is what most callers
want:

```rust
let markdown = document.extract_markdown(&ExtractOptions::default())?;
```

## Which pages: `ExtractOptions`

Both methods take the same options. The window of pages to hand back is
`first_page`, counted from zero, and `page_count`, which is `None` for the
rest of the document. `ExtractOptions::default()` is the whole document and
`ExtractOptions::window(10, 10)` is pages 10 to 19.

The same struct carries the figure options, which only `extract_markdown`
reads: `images` (`Placeholder`, `None`, `Embed` or `Write(dir)`),
`image_format` (PNG or JPEG), `min_image_size`, and `image_stem` — see
[Figures in the markdown](#figures-in-the-markdown). The default is
`images: Placeholder`, which marks every figure where it stands;
`images: None` writes exactly what the markdown wrote before figures
existed. It also carries `table_output` (`Auto`, `Html` or `Markdown`),
which only `extract_markdown` reads: the default writes each table in the
form its shape fits, and the other two force every table to one form. And it
carries `code` (default true), which only `extract_markdown` reads:
monospace runs become inline code and qualifying all-monospace paragraphs
fenced blocks, and `false` restores the output from before code existed.

**The window is a window on the ANSWER, not on the work.** The document is read
whole whatever it asks for, because everything the model says about a block is
said against the rest of the document: a size is a heading against what the body
is set in, and a line is a running head because THREE pages repeat it. A window
that re-derived those from its own pages would call a different thing a heading
than the same document does. The cheap path for one page read on its own, with
that page's own statistics, is `extract_page_blocks`, which says so.

Two consequences worth knowing. `Block::index` keeps the DOCUMENT's numbering
rather than being counted again per window, so two windows of one document agree
about the same block. And `continues_previous` is cleared where the half a block
continues fell outside the window — there is no longer one above it to join onto.

A window past the last page is EMPTY rather than an error, and so is a count of
zero: a caller stepping through a long document a window at a time stops when the
answer is empty, and an error there would make the ordinary end of that loop
indistinguishable from a broken file.

## Quality tiers

Extraction runs in one of two tiers. `fast` is the default; `balanced`
is a preview in which the layout and table-structure models process
each page.

| Tier | What it is | Needs | Runs | Status |
|---|---|---|---|---|
| `fast` (default) | The engine alone: pure Rust, no models | Nothing | Everywhere, including WASM | Shipping |
| `balanced` | The layout model regions each page — headings, lists, tables, page furniture — and the engine's own text fills the regions; the table-structure model structures tables | The `ort-layout` build (`ort-tables` for table structure) plus installed models | Native only, never WASM | Preview |

`balanced` runs locally through ONNX Runtime 1.23.2, loaded at runtime
and never linked. It needs the installed model files
(`pdfluent models install layout tables`) and does more work per page
than `fast`. It is markdown-only: `extract_markdown` reads it,
`extract_blocks` refuses it. Balanced/tables: glibc Linux, macOS,
Windows; not on musl Linux (loading returns a typed ModelError).

On the FinTabNet and PubTables-1M benchmarks, the balanced tier's
table structure (TEDS) scores higher than Docling and pymupdf4llm
(claim A35, claim A36, both measured 2026-10-08; see
[../benchmarks/BENCHMARK_CLAIMS.md](../benchmarks/BENCHMARK_CLAIMS.md)).

### Enabling `balanced`

The models arrive by an explicit install — never at runtime, never
automatically:

```sh
pdfluent models install layout tables   # several names in one call; the platform's ONNX Runtime comes along
pdfluent models status                   # what is installed, and whether the pins verify
```

`install` is the only command that ever downloads a model or a runtime
library: it fetches the files from
`https://github.com/pdfluent/pdfluent-sdk/releases/download/models-v1/`
and verifies every file's size and SHA-256 before placing it. Either
name installs alone (`pdfluent models install layout`), and `install`
prints one line per model — name, licence, and the notice to read —
plus one for the runtime:

```text
installed heron_ep03.onnx (layout) — PDFluent SDK licence (AGPL-3.0 or commercial) — see MODELS-NOTICE.txt
```

The source override is `PDFLUENT_MODELS_BASE`, an `https://` or
`file:///dir/` URL for mirrors and offline installs; the pins are
enforced identically whatever the source. Point the tier at a directory
outside the default with `--model-dir` / `--table-model-dir` (CLI),
`model_dir=` / `table_model_dir=` (Python), `modelDir` /
`tableModelDir` (Node), or `ExtractOptions::model_dir` / `with_tatr`
(Rust); without them the tier reads `PDFLUENT_MODEL_DIR`, else the
per-user `<data dir>/pdfluent/models/`. Files already in place and
verifying are left alone: nothing is fetched when the files are already
there.

Without the cargo features the tier refuses: `MODEL_NOT_AVAILABLE`,
naming the feature to rebuild with (`ort-layout`, `ort-tables` for
table structure) or the path looked at, and the install command. Rust
callers enable the features explicitly; default builds stay pure Rust.

The layout model (`heron_ep03.onnx`) is distributed under the PDFluent
SDK licence — AGPL-3.0 or the PDFluent commercial licence — with
Apache-2.0 attribution for IBM's base model; the table-structure model
(`tatr-v1.1-all.onnx`) and ONNX Runtime are MIT. The models release
carries `LICENSE`, `LICENSE-COMMERCIAL`, `LICENSE-Apache-2.0.txt`,
`MODELS-NOTICE.txt`, and the ONNX Runtime `LICENSE` and
`ThirdPartyNotices`; `install` places `MODELS-NOTICE.txt` and
`LICENSE-Apache-2.0.txt` at the model root and
`onnxruntime-LICENSE.txt` plus `onnxruntime-ThirdPartyNotices.txt`
beside the library.

## The same two methods in the other five languages

Every binding exposes both, over the same block model and the same window. The
structured result crosses a boundary that cannot carry a `Vec<Block>` as JSON --
the form this document specifies -- and comes back as objects where the language
has a natural shape for them.

| Binding | Markdown | Blocks |
|---|---|---|
| Rust (`pdfluent`) | `doc.extract_markdown(&opts)?` | `doc.extract_blocks(&opts)?` → `Vec<Block>` |
| Python | `doc.extract_markdown(first_page=0, page_count=None)` | `doc.extract_blocks(...)` → `list[Block]` |
| Node | `doc.extractMarkdown({ firstPage, pageCount })` | `doc.extractBlocks(...)` → `BlockInfo[]` |
| WASM | `doc.extractMarkdown(firstPage, pageCount)` | `doc.extractBlocks(...)` → JSON string |
| Java | `doc.extractMarkdown(firstPage, pageCount)` | `doc.extractBlocks(...)` → `List<Block>` |
| .NET | `doc.ExtractMarkdown(firstPage, pageCount)` | `doc.ExtractBlocks(...)` → JSON string |
| C ABI | `pdf_document_extract_markdown(doc, first, count)` | `pdf_document_extract_blocks_json(...)` |

The Docling form has one call per binding too, each returning the
`DoclingDocument` JSON as a string and taking an optional `name`:
`extract_docling(name, &opts)` (Rust), `extract_docling(first_page, page_count,
name="document")` (Python), `extractDocling({ firstPage, pageCount }, name)`
(Node), `extractDocling(firstPage, pageCount, name)` (WASM),
`extractDocling(firstPage, pageCount, name)` (Java), `ExtractDocling(firstPage,
pageCount, name)` (.NET) and `pdf_document_extract_docling_json(doc, first,
count, name)` (C, `NULL` for the default name).

In C, .NET and WASM a negative or omitted `pageCount` means "to the end of the
document", which is what `None` means in Rust. In C and .NET both calls return a
string the caller frees with `pdf_string_free`; .NET frees it for you.

Every one of those suites reads `fixtures/structured.pdf` and asserts that a
heading, a list item and a table cell come back. That file is built and pinned
by `crates/pdf-engine/tests/the_api_over_the_block_model.rs`, so one document
answers for all six languages.

Both are deterministic: the same blocks produce the same bytes, on every run
and on every machine. That is what the golden tests in
`crates/pdf-engine/tests/serializers_over_five_documents.rs` rest on, and it is
what lets a caller store a hash of the output and mean something by it.

## The markdown form

Headings become `#` to `###`, list items `-` and `1.`, a table a pipe table or
an HTML one, and a change of page an HTML comment. A `handbook.pdf` of one
page comes back as:

```markdown
# Safety rules

- Wear the issued boots on the floor.
- Keep the gangways clear of stock.
  - Pallets are stacked two high, no more.
  - Drums are stacked one high, on a mat.
- Report a spill before you clean it.
1. Stop the line at the nearest pull cord.
2. Tell the shift supervisor at once.
3. Write the entry in the day book.
```

Three rules are worth knowing before you read the output of your own document.

**A table is HTML when markdown cannot hold it.** A markdown table is a
rectangle: every row the same number of cells, none reaching across two. A
table with a `colspan` in it is written as an HTML table instead, which every
markdown reader passes through untouched. This is the rule Docling follows, for
the same reason. A caller that wants one form for every table sets
`table_output`: `Html` writes each table as `<table>`, `Markdown` each as
pipes with merged cells repeated into every cell they cover.

**Monospace reads as code where it is a signal.** A run of spans set in a
monospace face inside a paragraph or a list item is written as an inline code
span, and an all-monospace paragraph over two lines or more as a fenced block
— but only where the block shows a code signal (punctuation, indentation, or
a short opening line), and never in a document set wholly in a typewriter
face, where the face is the typesetting. Tables, headings and digit blocks
never are. `code: false` switches the whole of it off.

**The running heads are not in it.** A line the document repeats at the top or
the foot of three or more pages is marked `BlockKind::Header` or
`BlockKind::Footer`, and the markdown does not write it: standing
between two paragraphs is exactly what makes a running head a defect for
anything that chunks the text. It is still in the block model, and it is in the
JSON form — see below. A caller that needs running titles or page numbers in
the text sets `ExtractOptions::include_headers` and
`ExtractOptions::include_footers` (`include_headers=True`,
`include_footers=True`; `includeHeaders: true`, `includeFooters: true`), and
they are written as plain paragraphs at their reading-order position — never
as headings, which they are not.

**A page change is a comment, and the number in it is 0-based.**

```markdown
Revenue for the quarter was ahead of the budget laid before the board.

<!-- page 2 -->

No further business was brought before the meeting.
```

An HTML comment because it is the only thing CommonMark carries through
untouched and shows to nobody: no renderer prints it, and it cannot be mistaken
for content the way a `---` rule can. The number is `Block::page`, the same
number the JSON form carries — so a caller holding both forms of one document
finds the same page in each. A reader's chrome counts pages from one; this does
not. A caller that wants no marker sets `ExtractOptions::page_separators` to
`PageSeparators::None` (`page_separators="none"`, `pageSeparators: "none"`),
and the paragraphs on either side stand separated by a blank line.

**A marker never stands inside a paragraph, but it is not lost either.** Where
a page break cut a sentence in two, the two halves are written as ONE paragraph
(`Block::continues_previous`) and no marker stands between them — one
there would cut the paragraph that the joining exists to keep whole. The marker
for the new page is deferred instead: the next block on that page opens with
it. The one case that leaves no marker at all is a paragraph whose second half
ends the document, because nothing follows it to attribute to the new page.

**The syntax is reserved.** A line that opens with `<` in the document's own
text is escaped, so a document that quotes an HTML comment — a manual, a style
guide, a page of markdown about markdown — cannot write something that reads as
a marker. A marker at the start of a line is always ours, which is what makes
it safe to match on.

## Figures in the markdown

The block model carries text and tables; figures reach only the markdown.
Each image drawn on the page is marked as its own paragraph at its
reading-order position: before the first body block on its page whose top
edge is below the figure's own middle, or after the page's last body block.
The marker is an HTML comment, `<!-- image -->`, so it shows in the source
a chunker reads and in no rendering; where the tagged structure tree gives
the figure an `/Alt` (only with `use_tags`) the marker carries it as
`<!-- image: <alt> -->`.

```rust
use pdfluent::{ExtractOptions, MarkdownImages, PdfDocument};

let document = PdfDocument::open("report.pdf")?;
// The default: every figure an <!-- image --> where it stands.
let markdown = document.extract_markdown(&ExtractOptions::default())?;
// The output from before figures existed: no markers.
let mut plain = ExtractOptions::default();
plain.images = MarkdownImages::None;
let unmarked = document.extract_markdown(&plain)?;
// Written to figures/report-p1-0.png, report-p1-1.png, … and linked.
let mut options = ExtractOptions::default();
options.images = MarkdownImages::Write("figures".into());
options.image_stem = "report".into();
let linked = document.extract_markdown(&options)?;
// Or embedded: ![](data:image/png;base64,…).
let mut embedded = ExtractOptions::default();
embedded.images = MarkdownImages::Embed;
let standalone = document.extract_markdown(&embedded)?;
```

Five things to know.

**A marker decodes nothing.** It costs one interpreter pass over the page
and no encode: no files, no data URIs. Only `Embed` and `Write` decode the
figures and link them as `![](link)`. `extract_blocks` never carries
figures in any mode; they are collected on the way to the markdown.

**Names are `<stem>-p<page>-<n>.<ext>`.** Page counted from 1, `n` from 0
per page in reading position (top to bottom, then left to right), `ext` is
`png`, `jpeg` or `webp`. The directory is created when it does not exist;
pass it relative for relative links. `image_format` chooses PNG (lossless,
default), JPEG (smaller for photographic figures) or WebP (lossless).

**Native pixels unless asked.** By default a linked figure is the image's
own decoded pixels at its own size. With `image_dpi` set, `Embed` and
`Write` instead cut each figure out of a render of its page region at that
resolution, so masks, clips and whatever the page draws over the figure
come out as seen on the page.

**Tiny images are skipped.** Anything narrower than `min_image_size` pixels
in either dimension — icons, bullets, tracking pixels — is left out, as are
stencil masks (which carry no colours of their own). The default minimum is
32; 0 keeps everything. (Linked figures additionally leave out images the
decoder cannot produce samples for; a marker needs no samples, so it marks
those too.)

**Small on the page is skipped too.** With `image_size_limit` set, a figure
under that fraction of the page width or height is left out, markers
included — beside `min_image_size`, and both apply. Where the minimum
counts the figure's own pixels, the limit counts its size on the page.
The limit must be finite and within 0..=1, and `image_dpi` must be at
least 1: anything else is an option error, not a silent default.

In Python the same options are keywords of `extract_markdown`:
`images="write"`, `image_dir="figures"`, `image_format="png"`,
`min_image_size=32`, `image_stem="image"`, `image_dpi=None`,
`image_size_limit=None`. In Node they are fields of `ExtractOpts`: `images`,
`imageDir`, `imageFormat`, `minImageSize`, `imageStem`, `imageDpi`,
`imageSizeLimit`. Both bindings default to `none` — no markers — until the
caller asks; the placeholder default above is the Rust API's.

## The JSON form

An object with a `blocks` array in it, one entry per block, written in reading
order. The first two blocks of a two-column notice:

```json
{
  "blocks": [
    {
      "index": 0,
      "kind": "heading",
      "level": 1,
      "page": 0,
      "bbox": [72, 680, 220.47999963378908, 696],
      "text": "Notice to occupiers",
      "continuesPrevious": false,
      "spans": [
        { "text": "Notice to occupiers", "x": 72, "y": 680, "width": 148.47999963378908, "height": 16, "fontSize": 16, "fontName": "Helvetica-Bold", "isBold": true, "isItalic": false }
      ]
    },
    {
      "index": 1,
      "kind": "paragraph",
      "page": 0,
      "bbox": [72, 614, 215.9599993801117, 660],
      "text": "# 14 and # 16 are served by the rear staircase while the lift is out of use.",
      "continuesPrevious": false,
      "spans": [
        { "text": "# 14 and # 16 are served by the", "x": 72, "y": 650, "width": 141.1999986743927, "height": 10, "fontSize": 10, "fontName": "Helvetica", "isBold": false, "isItalic": false }
      ]
    }
  ]
}
```

`crates/pdf-engine/tests/golden/*.json` are five whole ones.

**The keys.** `index` is the block's place in the document's reading order,
counted from the first block of the first page. `kind` is one of `paragraph`,
`heading`, `listItem`, `tableCell`, `header`, `footer`. `page` is the 0-based
page index. `bbox` is `[x0, y0, x1, y1]` in PDF user space, y up. `text` is the
block's text with the line breaks of the measure taken out and hyphenated words
put back together. `continuesPrevious` says the block opens the second half of
the paragraph above it. `spans` are the runs of glyphs the block is made of, in
the order the page DREW them.

**The kind's own keys sit beside it**, because each block has exactly one kind:

| Kind | Extra keys |
|---|---|
| `heading` | `level` (1, 2 or 3) |
| `listItem` | `level` (nesting depth, 0 for the outermost), `ordinal` (the number the document wrote, `null` for a bullet) |
| `tableCell` | `row`, `column`, `rowSpan`, `columnSpan`, `header` |

**The furniture IS in this form.** A caller reading the blocks is not reading a
flow, and the line the document repeats — the chapter, the revision of the
form, the page number — is often exactly what it wants. Header and footer
blocks carry the numbers after the last block of the body, which is where the
reading order puts them.

**A coordinate JSON cannot hold is `null`.** JSON has no NaN and no infinity.
Writing `NaN` would produce a file no parser accepts and writing `0` would
state a coordinate the document does not have, so a non-finite number comes out
as `null`. It is not expected: `Block::bbox` is finite or `[0, 0, 0, 0]` by the
time it leaves extraction.

**The spans are six numbers and a face, not the whole `TextSpan`.** The
per-glyph boxes on a span outweigh everything else in the file by an order of
magnitude and nothing that chunks a document reads them. The caller that wants
them asks `extract_text_blocks`, whose `TextSpanInfo` is the canonical wire form
of a span and carries all of it.

## The Docling form

`DoclingDocument` is the document model of [Docling](https://github.com/docling-project/docling),
defined in `docling-core` (MIT). A pipeline built on Docling reads its
documents through that model: it walks `body`, filters on `label`, cites
`prov` and asks a table for its `grid`. `to_docling_json` writes our blocks in
that model, so such a pipeline can swap the converter and keep every line
downstream of it:

```python
from docling_core.types.doc import DoclingDocument
from pdfluent import Document

with Document("report.pdf") as doc:
    document = DoclingDocument.model_validate_json(doc.extract_docling(name="report"))

markdown = document.export_to_markdown()
```

**The version is pinned**: schema `1.10.0` as `docling-core` 2.98.1 writes it
(`SCHEMA_VERSION` and `DOCLING_CORE_VERSION` in `crates/pdf-engine/src/docling.rs`).
`crates/pdf-python/tests/test_docling_document.py` installs exactly that
release, validates the five golden `*.docling.json` files and the binding's own
output against its JSON schema, and loads each through `DoclingDocument` and
dumps it back: what comes back must EQUAL what we wrote. docling-core is a test
dependency only; nothing we ship depends on it.

**What maps:**

| Block | Docling |
|---|---|
| `paragraph` | `text` item under `body` |
| `heading` | `section_header` with the block's `level` |
| `listItem` | `list_item` in a `list` group; `enumerated` where the document numbered it, `marker` `N.` or `-`. A deeper item opens a group under the item above it, and a numbered list straight after a bulleted one at the same depth is a second group |
| a run of `tableCell` | one `table`; its cells with `row_span`, `col_span`, offsets, `column_header` from the header row, and the derived `grid` |
| `header`, `footer` | `page_header`, `page_footer` under `furniture`, content layer `furniture` |
| `continuesPrevious` | no item of its own: the half is joined onto the paragraph it continues, which carries one `prov` entry per page, each with the `charspan` of its half |

**Provenance.** `page_no` counts from ONE, as Docling does -- the one place
this form disagrees with `page` in the JSON form and the page markers, which
count from zero. Boxes are `BOTTOMLEFT`: the page as displayed, `/Rotate`
applied, in points from the bottom left of the crop box, and `pages` gives each
page of the window at its size in that frame.

**What has no mapping:**

- `confidence`: a Docling item has no per-item confidence.
- `spans`: Docling items carry no runs, fonts or sizes; read the JSON form.
- The document's own list marker: the block keeps the number, so `c)` and `iv.`
  are written `3.` and `4.`.
- `title`, `caption`, `footnote`, `picture`, `formula`, `code`, key-value and
  form items: the block model has no such kind, so `pictures`,
  `key_value_items` and `form_items` are always empty and a document title is a
  level-1 `section_header`.
- `origin` (mimetype, file hash, file name) is `null`.

## Loaders for LangChain and LlamaIndex

`pdfluent.chunks.iter_chunks` (Python) turns the blocks into retrieval chunks:
one per paragraph, heading and list item, one per TABLE rather than per cell,
the running heads left out, each with `page`, `bbox`, `kind`, `section` and --
where a model decided the kind -- `confidence` in its metadata. Two packages
wrap it in their framework's own conventions:

| Package | Class | Path |
|---|---|---|
| `langchain-pdfluent` | `langchain_pdfluent.PDFluentLoader` (a `BaseLoader`) | `integrations/langchain-pdfluent/` |
| `llama-index-readers-pdfluent` | `llama_index.readers.pdfluent.PDFluentReader` (a `BaseReader`) | `integrations/llama-index-readers-pdfluent/` |

Their READMEs have the install instructions and the metadata keys; their tests
load `fixtures/structured.pdf` through each loader in `ci-ephemeral`, against
pinned `langchain-core` and `llama-index-core` releases (both MIT).

## Where the code is

| Thing | Path |
|---|---|
| The block model and `ExtractOptions` | `crates/pdf-engine/src/blocks.rs` |
| The two methods | `crates/pdf-engine/src/document.rs`, `crates/pdfluent/src/document.rs` |
| The API and the fixture every binding reads | `crates/pdf-engine/tests/the_api_over_the_block_model.rs` |
| Markdown | `crates/pdf-engine/src/markdown.rs` |
| JSON | `crates/pdf-engine/src/json.rs` |
| Docling | `crates/pdf-engine/src/docling.rs`, `crates/pdf-python/tests/test_docling_document.py` |
| RAG chunks and loaders | `crates/pdf-python/python/pdfluent/chunks.py`, `crates/pdf-node/chunks.js`, `integrations/` |
| Golden tests over all three, on five documents | `crates/pdf-engine/tests/serializers_over_five_documents.rs` |
| The golden files themselves | `crates/pdf-engine/tests/golden/` |

A golden is regenerated with `PDFLUENT_WRITE_GOLDEN=1 cargo test -p pdf-engine
--test serializers_over_five_documents`, and the diff is what the author reads
before committing it. A golden rewritten without reading the diff is a test
that agrees with whatever the code now does.
