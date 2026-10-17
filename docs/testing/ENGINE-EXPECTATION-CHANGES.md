# Engine expectation changes

One entry per changed engine test expectation: what the test used to pin,
what it pins now, and why the old pin no longer holds. The newest entry
goes on top. A regenerated golden is not an expectation change -- it keeps
pinning the same behaviour on new bytes -- and is recorded in its own
commit message instead.

## f1040 default goldens re-recorded after the R7 product fixes (D35-e34)

- Tests: `crates/pdf-engine/tests/the_default_output_stays_byte_identical.rs`
  (`e1e2-f1040.json/.md`) and
  `crates/pdf-engine/tests/e34_span_fonts.rs::default_output_on_f1040_is_byte_identical_to_cc2f37b`
  (`f1040_default_cc2f37b.json`, `f1040_default_metrics_cc2f37b.txt`)
- Commit: `test(engine): re-record f1040 default goldens after the R7 fixes (#636)`
  (this commit)
- Date: 2026-10-12

### ORIGINAL expectation (kept verbatim)

174 blocks (126 tableCell, 47 paragraph, 1 listItem) as re-pinned in
fcd3f0d6a: slashes displaced (`MM DD YYYY/ /`), stray and redistributed
leader dots (`total . income`, `Wages . . from`, markers ahead of their
label), the `d`/`7a` rows and the `b Check if` option row as paragraphs
outside the Income table, and `27a` ahead of `you may need to`.

### NEW expectation

178 blocks (132 tableCell, 45 paragraph, 1 listItem): the same output with
the R7 fixes applied. 144 blocks are byte-identical (text, kind, bbox and
spans); 29 same-bbox pairs change text or kind; one paragraph splits into
5 cells (+4 blocks); nothing is removed.

### Reason

D35-e34 regenerates the f1040 goldens only after the product fixes, with
every difference explained. The fixes, all on this branch: N5 slash order
(89519ee3b), leader-dot reorder (d7afe782a), N88/N89 edge-row reclaim
(efaefd0cf), N125 27a overlap join (01820b013), and the sentence-period
guard that stopped the previous regen (bfbe9e4a7 -- the old run read
`youIf`, this one reads `you. If`, matching `pdftotext -layout` line 134).

Before/after counts per class (old HEAD golden vs new; aligned by page and
bbox overlap or text containment): N5 1, leader dots 26 (same bbox/kind,
text-only: markers move after their label, stray mid-label dots deleted),
N88/N89 reclaim 2 groups (the merged `d`+`7a` paragraph splits into the `d`
marker, the `d` stub, the `7a` marker, the `7a` stub and the `7a` amount
cells; the `b Check if` option row flips paragraph to tableCell on the same
bbox), N125 1 (`27a` moves after `you may need to`), OTHER 0. The metrics
projection gains exactly the 4 split-out `/` spans of block 5 and no
(name, metrics) value changes anywhere (same 4 distinct pairs).

### Content check

Against system `pdftotext -layout` and pypdf 6.8.0 (no competitor package
anywhere; `fitz` import verified absent): the only gold token missing from
pdftotext is the pre-existing `1 a` row-header split (unchanged since the
re-pin), and every remaining residual pairs with a pypdf line-join artifact
(`2025U`, `Form1040`, `blindStandard`, `YYYY/`). No word is lost or added by
this change -- the displaced-slash residuals of the old golden are gone. The
`d` stub, the `7a` stub plus amount column, and the `b Check if` option row
are `<tr>` rows of the Income table in the markdown, and `27a` reads after
`you may need to`.

### Red proof

Flipping one byte of the new `e1e2-f1040.json` fails
`the_default_json_is_pinned`; the golden was restored byte-identical
(sha256 `188ca791fca453a32e41c6b4e38e32efe82f0d967a3f34f58c2516ca1ca6b39a`).

## actual_text-alone close-words pin moves to the operator-built table (D35-E)

- Test: `crates/pdf-engine/tests/cell_path_words_choice.rs::actual_text_alone_leaves_cell_words_default`
  (second half only; structured.pdf first half unchanged)
- Commit: `test(engine): move the actualtext-alone close-words pin off the phantom cell (#636)`
  (this commit)
- Date: 2026-10-12

### ORIGINAL expectation (kept verbatim)

```rust
    let bytes =
        std::fs::read(fixture_path("doclaynet/arxiv_1001_0809_prose.pdf")).expect("read fixture");
    let blocks = blocks_of(&bytes, &options_actual_text());
    assert_eq!(
        cell_text(&blocks, 36, 0),
        "panels (b) and (d)], in Markovian approximation \u{3b4}k = 0max"
    );
```

### NEW expectation

```rust
    let pdf = close_words_table_pdf();
    let blocks = blocks_of(&pdf, &options_actual_text());
    assert_eq!(cell_text(&blocks, 1, 1), "0max");
```

With only `actual_text` on, the close cell words stay at their default
(joined) on row 1 column 1 of the operator-built ruled table from eb4201d1a.
Exact string, through the public path; no weaker assertion.

### Reason

D35-E retired the arxiv pin: commit 89b0f49f0 made that prose page a
negative test (no table) and eb4201d1a moved the close-cell-words pin to the
operator-built table. This test was missed and still read row 36 column 0 of
the phantom cell, so it fails at `cell_text` with `no cell at row 36 column
0`. The new half keeps the same intent (ActualText alone never moves the
word rules) on the table that now owns the rule.

### Red proof

With `cell_path_words = true` added to `options_actual_text`, the new half
fails with `left: "0 max", right: "0max"`; restored, the whole
`cell_path_words_choice` binary is 8/8 green.

## Compat pixel tests use the plain decode route (proposal 5, D33)

- Tests: `bindings/java/src/test/java/com/pdfluent/KnownImagesTest.java`
  (`pixels`), `bindings/dotnet/test/PDFluent.Tests/PdfDocumentTests.cs`
  (`ExtractImages`), `crates/pdf-node/tests/known_images.test.js` (`pixels`)
- Commit: `test(bindings): read known-image pixels through the plain decode route (#636)`
  (this commit)
- Date: 2026-10-12

### Contract (our own spec and code)

The compat `extract_image`/`extractImage` returns the ENCODED image file
(png/jpeg); decoded pixels come from the plain route
(`decode_image_pixels` / `decodeImagePixels` / `DecodeImagePixels` /
`pdf_image_decode_pixels`).

### ORIGINAL expectation (kept verbatim)

Each of the three tests asked the compat route for decoded pixels and
carried a STOP for the encoded-vs-pixels mismatch:

Java (`KnownImagesTest::pixels`):

```java
                ImageData data = doc.extractImage(r.getXref());
                byte[] bytes = data.getData();
                int expectLen = w.width * w.height * w.components;
                assumeTrue(bytes.length == expectLen,
                    "STOP: encoded bytes only (" + bytes.length + " != " + expectLen + "), no decode call");
```

.NET (`PdfDocumentTests::ExtractImages`, skipped at the runner):

```csharp
        [Fact(Skip = "TODO: Image extraction not yet exposed in .NET binding")]
```

```csharp
                ImageData data = doc.ExtractImage(refs[i].Xref);
                ...
                if (data.Data.Length != expectLen)
                {
                    throw new InvalidOperationException(
                        "STOP: encoded bytes only (" + data.Data.Length + " != " + expectLen + "), no decode call");
                }
```

Node (`known_images.test.js::pixels`):

```js
        if (!data) {
          throw new Error('STOP: extractImage has no pixel bytes');
        }
        if (data.length !== expectLen) {
          throw new Error(`STOP: encoded bytes only (${data.length} != ${expectLen}), no decode call`);
        }
```

### NEW expectation

Each test obtains pixels through the plain route (`pageImages` listing plus
`decodeImagePixels`/`DecodeImagePixels`) and compares width, height,
components and the pixel sha256 against
`fixtures/known-images.expected.json`. No skip and no STOP remains; every
test RUNS.

### Reason

The old expectation pinned behaviour the contract never promised: decoded
samples out of an entry point specified to return the encoded file. The new
expectation pins the agreed split -- encoded bytes from the compat call,
pixels from the plain call.

### Red proof

Each rewritten test compares the sha256 of the returned pixel bytes
directly, with no skip or STOP path left: a wrong expectation or a
corrupted decode fails the comparison (`assertEquals(w.sha256, ...)` /
`Assert.Equal(want[i].Sha, digest)` / `expect(...).toBe(w.sha256)`).
Liveness is verified build-free where possible (Java test-compile green;
.NET test project builds with 0 warnings/errors); green runs are BLOCKED
on the disk stop line -- see `meas-out/tests-5-11/REPORT.md`.

## The facade Document exposes the D16 image calls (proposal 11, D34)

- Tests: `crates/pdf-python/tests/test_known_images.py`
  (`test_facade_stop_no_image_call` replaced by
  `test_facade_decodes_known_image_pixels`)
- Commit: `test(python): assert the D16 facade image behaviour (#636)`
  (this commit)
- Date: 2026-10-12

### Contract and D16 evidence

D16 (owner decisions 1.1.0, `meas-in/OWNER-DECISIONS-1.1.0.md`):

```text
| D16 | Image extraction via the plain PdfDocument API | build in 1.1.0 | 10-10 |
```

Shipped API (`crates/pdfluent/src/images.rs` on `PdfDocument`):

```rust
pub fn page_images(&self, page: usize) -> Result<Vec<ImageInfo>>
pub fn extract_image_data(&self, id: ImageId) -> Result<ExtractedImage>
pub fn decode_image_pixels(&self, id: ImageId) -> Result<DecodedImage>
```

exposed on the Python facade as `Document.page_images`,
`Document.extract_image_data` and `Document.decode_image_pixels`
(`crates/pdf-python/src/door_images.rs`,
`crates/pdf-python/python/pdfluent/_native.pyi`).

### ORIGINAL expectation (kept verbatim)

```python
def test_facade_stop_no_image_call() -> None:
    native = getattr(pdfluent, "Document", None)
    assert native is not None
    names = [n for n in dir(native) if "image" in n.lower()]
    assert names == [], "facade gained image calls: %s" % names
    pytest.skip("STOP: facade Document has no public image-extraction call")
```

### NEW expectation

The test asserts the agreed image behaviour: `Document` exposes
`page_images`, `extract_image_data` and `decode_image_pixels`, and
`decode_image_pixels` over `fixtures/known-images.pdf` returns the width,
height, components and pixel sha256 from
`fixtures/known-images.expected.json`. The test is neither removed nor
skipped; it RUNS.

### Reason

The sentinel pinned the pre-D16 surface (no image calls on the facade).
D16 put image extraction in the plain `PdfDocument` API in 1.1.0 and that
API has shipped, so the sentinel is outdated: run green it could only ever
skip, and on the shipped surface its `names == []` assertion fails.

### Red proof

Executed red run in `meas-out/tests-5-11/REPORT.md`: against a facade
without the image calls the test fails on `facade Document lacks
page_images`. A wrong pixel sha fails the direct sha comparison the same
way; green runs are BLOCKED on the disk stop line (see the report).

## The 4 balanced parity references move to joined spacing (D36, G)

- Test: `test_balanced_bindings.py::test_balanced_markdown_matches_the_cli_reference`
  (references `crates/pdf-python/tests/data/balanced/{structured,uksi_20200121_p50,
  nasdaq_wwd_2002_p7,pmc2438475_p1_table_notes}.json`). No test file changed.
- Contract: owner decision D36-G, after the SPACING fix on the balanced tier:
  re-record the 4 CLI references with each file's recording argv, with an
  independent per-block content check and a red-proof.
- Independent reference (venv with no PyMuPDF/fitz): `pdftotext` (plain and
  `-layout`) and pypdf 6.8.0 `extract_text`, checked both directions per file:
  0 words of the new markdown outside both references (after tag/markup-aware
  tokenising; hyphen splits and `<sup>` joins fall back to a whitespace-free
  substring check); the only pdftotext tokens missing from the new markdown
  are furniture: the `•` bullet marker (lives in the spans per the
  `BlockKind::ListItem` contract), page folios (`51` on uksi, `6` on nasdaq),
  and the journal running head/footer on pmc (`microRNA and Cervical Cancer`
  running head -- absent from pypdf too -- plus `PLoS`, `www.plosone.org`,
  `July`, `2008`, `Volume`, `e2557`); `miR193b` x2 is pdftotext's own
  line-break hyphen gluing of the `miR-193b` the markdown carries; `C2` is
  pdftotext's gluing of the page's `C-2`, which pypdf and the markdown both
  read with the hyphen (`C- 2` in the table caption). Tables: every pipe-grid
  cell (12 structured, 98 pmc) and every uksi `<table>` cell is in the
  `-layout` rows; the old letter-spaced runs are gone everywhere
  (16/56/40/209 -> 0).
- Targeted result: the binding suite 9/9 green; the binding output equals the
  CLI byte for byte on all 4 fixtures.

### ORIGINAL expectation (kept verbatim, excerpts)

`structured.md`: `## S a f e t y r u l e s`, `- W e a r t h e i s s u e d
b o o t s o n t h e fl o o r .`, `| Standing | none | 18 . 90 |` followed by
the cell texts repeated as spaced paragraphs. `uksi`: `## S C H E D U L E 3`,
`Articles 1 4 a n d 1 8`, spaced `<table>` header cells. `nasdaq`:
`E M I S S I O N S R E L I A B I L I T Y ...`, `**Power Ge...**` (paragraph).
`pmc`: `c y t o pl a s m b y e x p o r t i n 5 [ 1 5 , 1 6 ] , ...`.

### Difference classes (old -> new)

- markdown sha12: structured d4bd3768421b -> 4cd5f7d7a86b; uksi fcb1fda6f11c
  -> f5f94101a495; nasdaq 286c59d75291 -> 132c002fdfa5; pmc 8c2907e33d40
  -> 66de088b71cf.
- Kinds: structured 17 blocks (1 heading, 3 list_item, 13 paragraph incl. the
  repeated cell echoes) -> 5 (1 heading, 3 list_item, 1 pipe-table paragraph);
  uksi unchanged shape (6 blocks, 1 table of 14 rows); nasdaq 29 blocks with
  `**Power Generation**`-style bolds as paragraphs -> 29 blocks with `##`
  headings; pmc 15 blocks (2 stray headings) -> 11 blocks (3 headings:
  Results + 2 miRNA sections).
- Spacing joins: all letter-spaced runs joined (`S a f e t y` -> `Safety`,
  `1 8 . 9 0` -> `18.90`, `E M I S S I O N S` -> `EMISSIONS`); hyphenated page
  tokens keep their hyphens (`Back-Up`, `Rolls-Royce`, `miR-193b`).

### Reason

The old references froze the SPACING bug's per-glyph output on the balanced
tier. The new bytes are the fixed tier; the two-direction reference check
shows no lost or added text, only joined spacing.

### Red-proof

`Safety rules` -> `Safety rulez` in the recorded structured markdown fails
`test_balanced_markdown_matches_the_cli_reference[structured.json]`; restored
is green.

### Review input

Proposal G (12-10 11:40): parity holds (binding == CLI), only the recordings
are outdated; re-record from the r7 CLI with per-block pdftotext/pypdf
comparison, kinds/headings/grids before and after, one reviewed test commit
with red-proof.

### Addendum 13-10: the addon's copies of the same 4 references

- Test: `crates/pdf-node/tests/balanced_bindings.test.js` "balanced markdown
  matches the CLI reference" (references `crates/pdf-node/tests/data/balanced/`,
  same 4 file names). No test file changed.
- Contract: the same D36-G renewal. Before 1292312ff the addon's 4 files were
  byte-identical to the wheel's (sha12 structured e6b5811e1961, nasdaq
  93b868b113ba, pmc 3f322905bf3f, uksi 71c967a2cea8); 1292312ff renewed only
  the wheel's. The addon's files are now byte-identical to the renewed wheel
  files, so the independent per-block content check above covers them as is.
- Targeted result (r7 tree, macOS arm64, release addon with ort-layout and
  ort-tables): old references 4 of 9 failed, renewed references 9/9.
- Red-proof: `Safety rules` -> `Safety rulez` in the addon's structured.json
  fails 1 of 9; restored is 9/9.
- Why it was not caught: binding-suites.yml is dispatch-only and the trial
  gate does not run balbind; the release gate for r8 runs in full mode.

## The f1040 default golden moves to merged spans (D35, e34)

- Test: `e34_span_fonts::default_output_on_f1040_is_byte_identical_to_cc2f37b`
  (goldens `f1040_default_cc2f37b.json`, `f1040_default_metrics_cc2f37b.txt`,
  plus `e1e2-f1040.json/.md`, which pin the same default output and carry
  an identical block text sequence after regen). No test file changed.
- Contract: owner decision D35-e34, after the font-switch fix 9aa692d12:
  regenerate from the candidate with a per-block reference check.
- Independent reference (venv with no PyMuPDF/fitz): `pdftotext -layout
  corpus/f1040.pdf` and pypdf 6.8.0 `extract_text`, checked both
  directions: 0 new-block words outside both references; 0 pdftotext tokens
  missing from the new blocks+spans; the 4 pypdf-only tokens (`Form1040`,
  `2025U.S.`, `blindStandard`, `instructions):If`) are pypdf line-join
  artifacts whose separate words are all present.
- Targeted result: the `e34_span_fonts` binary 5/5 green, including the
  unchanged font-name and descriptor-metric requirements; the
  `the_default_output_stays_byte_identical` binary 2/2 green.

### Difference classes (old -> new)

- Span granularity: 7889 -> 610 spans; 1,424,577 -> 182,186 JSON bytes.
- Block count: 123 -> 174. Kinds: paragraph 20 -> 47, tableCell 102 -> 126,
  listItem 1 -> 1. No headings in either version.
- Segmentation: 83 new blocks split out of old blocks (long checkbox rows
  cut per box); 39 old blocks merged into new ones (headers, dependent
  rows). Same-text kind flips: 2, both tableCell -> paragraph.
- Spacing joins: 44 stray `.` word-tokens joined (`Jan . 1` -> `Jan.`,
  `P . O .` -> `P.O.`, `301 .9100-2` -> `301.9100-2`); total words
  2295 -> 2230 with zero losses per the reference check.
- Within-block glyph order follows draw order per the `Block.spans`
  contract: `Deceased MM DD YYYY/ /` matches pypdf verbatim, while
  pdftotext's layout pass reads `MM / DD / YYYY`. Same words, no addition.
- Fonts: the same four families before and after (Helvetica 6765 -> 386
  spans, Helvetica-Bold 1052 -> 221, Helvetica-Oblique 38 -> 2,
  ITCFranklinGothicStd-Demi 34 -> 1), so the font-switch fix keeps both
  faces of mixed-font runs across the merge.

### Reason

The old golden froze the SPACING bug's per-glyph spans. The new bytes are
the fixed default path; the two-direction reference check shows no lost or
added text.

### Red-proof

The old golden fails on the candidate at byte 1159 (observed in-session);
a corrupted byte (`DigitalXAssets`) fails the new pin at byte 17926,
restored is green.

## The e1e2 default goldens move to merged spans (D35, DEFAULT-OUTPUT)

- Tests: `the_default_output_stays_byte_identical::{the_default_json_is_pinned,
  the_default_markdown_is_pinned}` on `e1e2-simple` and `e1e2-structured`
  (f1040 follows in the e34 commit on this branch).
- Contract: proposal 1, approved earlier (D33): the default output is
  re-pinned on r7 once the new output is proven right; owner decision D35
  orders the per-block reference check below.
- Independent reference (venv with no PyMuPDF/fitz): `pdftotext -layout`
  reads `Test page`, `Safety rules`, `Standing   none       18.90` (also
  24.31, 11.07); pypdf 6.8.0 reads the same words. Every word of every new
  block is in at least one reference, and no reference token is missing
  from the new blocks+spans (checked both directions; the `•` marker lives
  in the spans, not the block text, per the `BlockKind::ListItem` contract
  at `blocks.rs:345`).
- Targeted result: both tests green on simple+structured after regen
  (f1040 still red here, covered by e34).

### ORIGINAL expectation (kept verbatim, excerpts)

`e1e2-simple.md`: `T e s t   p a g e`; `e1e2-structured.md`:
`# Safety   rules`, `- • Wear   the  issued   boots   on   the   floor.`,
`| Standing | none | 18 . 90 |`. JSON blocks carried per-glyph spans
(simple block 0: 9 spans; structured blocks: 12/34/33/31 and 3-7 per cell).

### NEW expectation

Regenerated goldens from the candidate: one span per word/run (simple block
0: 1 span; structured: 1-2 spans per block), joined words, joined decimals.

### Changed blocks

simple: block 0 (paragraph `Test page`). structured: block 0 (heading),
blocks 1-3 (list items; bullet leaves the block text per contract, stays in
the spans), blocks 9/12/15 (rate cells `18.90`/`24.31`/`11.07`). All other
blocks byte-identical text; kinds and block counts unchanged (1 and 16).

### Reason

The old goldens froze the SPACING bug's per-glyph output plus the pre-fold
decimals. The new bytes are the fixed default path; the reference check
shows no lost or added text beyond that.

### Red-proof

The old goldens fail on the candidate (observed in-session); a corrupted
byte (`TestXpage`) fails the new `the_default_json_is_pinned`, restored is
green.

## The image-only page serialises to one figure (D35-D)

- Tests: `serializers_over_five_documents::an_empty_page_serialises_to_nothing`
  and `::an_image_only_page_serialises_to_one_figure`, split from
  `::an_empty_page_and_an_image_only_page_serialise_to_nothing`.
- Contract: owner decision D26 (commit 53a5c406a) plus the G66 figure
  contract: a textless scan gets a figure marker, never empty output;
  owner decision D35-D splits this pin accordingly.
- Independent reference: the page is operator-built and carries no text, so
  no pdftotext/pypdf text reference applies; the marker
  `![](figure-0001-01.png)` is the same G66 marker pinned by
  `an_ocr_layer_makes_a_scan_readable::auto_reads_a_scanned_page_and_never_does_not`.
- Targeted result: the whole `serializers_over_five_documents` binary green.
## The arxiv prose page pins no table instead of a phantom cell

- Test: `crates/pdf-engine/tests/cell_path_words_choice.rs:close_cell_words_part_with_the_option`
  becomes `no_table_is_detected_on_the_arxiv_prose_page`
- Commit: `test(engine): keep the arxiv prose page table-free and drop the phantom cell (#636)`
  (this commit)
- Date: 2026-10-12

### Contract

Section E of the 12-10 expectation proposals: the pinned cell (row 36,
column 0) is figure-caption prose on a one-page prose-and-figures arxiv
fixture, and the page's 51 rules are chart drawings, not table rulings. With
correct spans no table is detected there at all, which is the correct reading.
The positively-worded move the proposals asked for -- the close-words
assertion onto a real ruled-table fixture -- was searched for and does not
exist (see reference); the page keeps its regression value as a negative
test. Follow-up (2026-10-12, this commit): the move is now done -- see
Positive pin below. The unscripted 151-PDF search under INDEPENDENT reference
is not reproducible from the repo (no script was committed), so it is kept
as history and superseded by that pin.

### ORIGINAL expectation (kept verbatim)

```rust
fn an_empty_page_and_an_image_only_page_serialise_to_nothing() {
    let blank = Page::new(Vec::new());
    let mut picture = Page::new(Vec::new());
    picture.image = true;

    for pages in [vec![blank], vec![picture]] {
        let blocks = blocks_of(&pages);
        assert!(blocks.is_empty(), "a page with no glyph has no block");
        assert_eq!(to_markdown(&blocks), "");
        assert_eq!(to_json(&blocks), "{\n  \"blocks\": []\n}\n");
    }
}
```

### NEW expectation

The blank page keeps the old pin verbatim in
`an_empty_page_serialises_to_nothing`; the image-only page pins exactly one
`BlockKind::Figure` block with the `![](figure-0001-01.png)` markdown in
`an_image_only_page_serialises_to_one_figure`.

### Reason

Since 53a5c406a placed images merge on every page, so the image-only page
yields one figure block and the old "serialise to nothing" loop fails on
its second half. No feature is dropped: emptiness is still pinned for the
truly empty page.

### Red-proof

The pre-split pin fails on the candidate (`a page with no glyph has no
block` on the picture half, observed in-session); a wrong marker
(`figure-0001-02.png`) fails the new figure test with `left:
"![](figure-0001-01.png)\n"`.

## The folded dot keeps its box inside the merged span (D35-B)

- Test: `a_decimal_dot_abutting_its_digit_joins_it::a_folded_dot_keeps_its_own_box_after_its_digit`.
- Contract: owner decision D35-B keeps the intent (the folded dot keeps its
  own box after its digit) and asserts it at word/character level instead of
  on a span `8.`. The module doc's claim "glyphs arrive as spans of their
  own" described the SPACING bug and is corrected with it.
- Independent reference: the joined text `18.90` is the pdftotext/pypdf
  reading per entry D35-A above; the box assertions are geometry, checked
  against the span's own `char_bounds` (one box per glyph).
- Targeted result: both tests in the binary green.

### ORIGINAL expectation (kept verbatim)

```rust
let folded = spans.iter().find(|s| s.text == "8.").expect("the 8. span");
assert_eq!(folded.char_bounds.len(), 2, "one box per glyph: {folded:?}");
```

```rust
let (digit, dot) = (folded.char_bounds[0], folded.char_bounds[1]);
```

### NEW expectation

```rust
let folded = spans
    .iter()
    .find(|s| s.text == "18.90")
    .expect("the 18.90 span");
assert_eq!(folded.char_bounds.len(), 5, "one box per glyph: {folded:?}");
```

```rust
let (digit, dot) = (folded.char_bounds[1], folded.char_bounds[2]);
```

### Reason

After the SPACING fix one run arrives as one span, so no `8.` span exists;
the dot's own box lives at `char_bounds[2]`, right after the `8` box at
`[1]`. The remaining assertions (dot starts where the digit ends, dot has
extent, span covers the dot) are unchanged.

### Red-proof

Restoring the old `s.text == "8."` lookup panics with no `8.` span
(FAILED); the merged lookup is green. A fold that dropped the dot box
would leave 4 boxes and fail the 5-box assertion.

## The default decimal fold reads 18.90 joined (DECIMAL-FOLD, D35-A)

- Tests: `cell_path_words_choice::folded_decimal_dots_join_with_the_option`
  (structured.pdf rows 1-3 and the doclaynet `otc_sdxhf_2013_p360.pdf` cell),
  `cell_path_words_choice::actual_text_alone_leaves_cell_words_default`
  (structured.pdf rows 1-3; the arxiv tail is untouched, pending decision E).
- Contract: the decimal fold runs on the default path since 5c4cb43 (passed
  the variant rule 10-10); owner decision D35-A moves these pins to the
  reference reading.
- Independent reference (venv with no PyMuPDF/fitz): `pdftotext -layout
  fixtures/structured.pdf` reads `Standing   none       18.90` (also 24.31,
  11.07); pypdf 6.8.0 `extract_text` reads `Standing none 18.90`. On
  `otc_sdxhf_2013_p360.pdf` both read `3. Social data:`.
- Targeted result: `folded_decimal_dots_join_with_the_option` green;
  `to_json_of_a_real_document_is_byte_identical` green on the regenerated
  golden (see that commit); `actual_text_alone_leaves_cell_words_default`
  still red on the untouched arxiv tail (no cell at row 36 column 0),
  which belongs to decision E.
fn close_cell_words_part_with_the_option() {
    let bytes =
        std::fs::read(fixture_path("doclaynet/arxiv_1001_0809_prose.pdf")).expect("read fixture");
    let off = blocks_of(&bytes, &ExtractOptions::default());
    let on = blocks_of(&bytes, &options_on());
    assert_eq!(
        cell_text(&off, 36, 0),
        "panels (b) and (d)], in Markovian approximation \u{3b4}k = 0max"
    );
    assert_eq!(
        cell_text(&on, 36, 0),
        "panels (b) and (d)], in Markovian approximation \u{3b4}k = 0 max"
    );
}
```

Pinned against per-glyph spans, this read a table cell where the corrected
span formation detects no table: at this base `cell_text` panics with `no
cell at row 36 column 0`, and the page extracts as 189 Paragraph/Figure/
Caption blocks with zero `TableCell`.

### NEW expectation

```rust
fn no_table_is_detected_on_the_arxiv_prose_page() {
    let bytes =
        std::fs::read(fixture_path("doclaynet/arxiv_1001_0809_prose.pdf")).expect("read fixture");
    for options in [ExtractOptions::default(), options_on()] {
        let blocks = blocks_of(&bytes, &options);
        assert!(
            blocks
                .iter()
                .all(|block| !matches!(&block.kind, BlockKind::TableCell { .. })),
            ...
        );
        for caption in ["FIG. 2", "Markovian approximation", "= 0"] {
            assert!(
                blocks.iter().any(|block| block.text.contains(caption)),
                ...
            );
        }
    }
}
```

No `TableCell` on the page with the option off or on, and the `FIG. 2`
caption the old pin quoted still reads as paragraph prose. The loose caption
substrings are the tokens all three readers share (see reference); the exact
micro-spacing (`0max` vs `0 max` vs `kmax = 0`) legitimately differs between
readers and is no longer pinned.

### Reason

The old pin's premise -- a table on this page -- was an artefact of the
spacing bug's per-glyph spans, not a reading any reference shares. Keeping
the cell assertions would pin a phantom table; weakening them to empty output
would drop the caption. The negative keeps the regression guard (no false
table) and the prose (the caption still extracts).

### INDEPENDENT reference

All from a python venv with no PyMuPDF/fitz (system python3, pypdf 6.8.0,
poppler pdftotext 26.02.0):

- `pdftotext -layout` lays the page out as two-column figure captions
  (`FIG. 2` left, `FIG. 4` right) under chart tick labels (`0.3`, `(b)`,
  `(d)`, `t(fs)`, `0 250 500 750 1000`): prose and figures, no table grid.
  The disputed line sits inside the `FIG. 2` caption.
- pypdf reads the same caption as `panels (b) and (d)], in Markovian
  approximation δkmax = 0`, twice (both columns), with `FIG. 2` present: the
  caption tokens the new test pins, with no table structure around them.
- pypdf content-stream census: 41,945 `l`, 14,426 `c`, 1,734 `re`, 9,555
  `S` -- dense chart vector graphics. The engine distils 51 horizontal
  rules from it, every one exactly 84.0 pt wide and confined to the two
  figure x-bands (92.5-260.5 left, 355.6-523.5 right): chart panel edges,
  never a spanning grid with text cells between the lines.
- Ruled-table search: every PDF under `fixtures/` and
  `crates/pdf-engine/tests/fixtures/` (151 files) was extracted with the
  option off and on through the public path. No file shows a word-level
  space-only text difference on a stable cell grid -- the only
  close-words-range instance left anywhere was the phantom cell itself, and
  the gaps that remain (e.g. the caption now reads `kmax = 0` identically
  both ways) sit off the quarter-em boundary. So no existing ruled-table
  fixture exercises the option at word level; the positive close-words pin
  has no observable instance to move to.

### Targeted test result and red-proof

`cargo test -p pdf-engine --test cell_path_words_choice
no_table_is_detected`: 1 passed, 0 failed. Red-proofs: the caption pin with
`FIG. 9` fails, and the same assertions pointed at the real ruled table
`doclaynet/otc_sdxhf_2013_p360.pdf` fail on the no-table assertion -- the
negative logic rejects a real table. Both restored, suite state unchanged.
(The suite's two remaining reds, `folded_decimal_dots_join_with_the_option`
and `actual_text_alone_leaves_cell_words_default`, pin `18 . 90` and belong
to the DECIMAL-FOLD section owned by another job.)

### Positive pin (this commit)

```rust
fn close_cell_words_part_with_the_option() {
    let pdf = close_words_table_pdf();
    let off = blocks_of(&pdf, &ExtractOptions::default());
    let on = blocks_of(&pdf, &options_on());
    assert_eq!(cell_text(&off, 1, 1), "0max");
    assert_eq!(cell_text(&on, 1, 1), "0 max");
}
```

`close_words_table_pdf` builds a stroked 3 x 3 lattice operator by operator
(header plus two rows, no corpus): row-1 cell holds `0` in Helvetica with a
bold `max` starting 1.5 pt inside its 5.56 pt advance -- closer than the
2.78 pt word space, past the tenth-em overlap the option parts on. The bold
face keeps the two runs as separate lattice fragments (same-face touching
runs merge at placement); the gap is what the option reads. Both
extractions detect the same 9-cell grid; only the pair's join differs.

`cargo test -p pdf-engine --test cell_path_words_choice
close_cell_words_part_with_the_option`: 1 passed, 0 failed. Red-proof: with
the overlap veto (`gap > -0.1 * em`) removed from the cell join, the
on-extraction reads `0max` and the test fails (`left: "0max", right:
"0 max"`); product file restored byte-identical afterwards. This pin
replaces the unscripted-scan argument above: the rule is covered by a
reproducible fixture instead.

## The ActualText word-box test opts in to the replacement

- Test: `crates/pdf-engine/tests/word_boxes_come_from_their_glyphs.rs:one_glyph_box_covering_two_letters_stays_whole`
- Commit: `test(engine): read the ActualText word boxes behind the actual_text option (#636)`
  (this commit)
- Date: 2026-10-12

### Contract

`fix(engine): read ActualText behind its own option (#663)` (629c95be4)
moved the replacement behind `ExtractOptions::actual_text`, default false,
"opt-in pending a variant measurement". The default was never agreed ON: the
struct still carries `actual_text: false`, the docs say "Default false", and
the default-off pins from that commit hold -- `the_actual_text_is_opt_in`
(`!ExtractOptions::default().actual_text`), the off leg of
`actual_text_applies_with_the_option` (`QQQ` without the option),
`cell_path_words_alone_ignores_actual_text` (the cell-path flag leaves `QQQ`),
and `actual_text_alone_leaves_cell_words_default` (the replacement flag
leaves the cell words spaced). No later commit agrees a default ON.

### ORIGINAL expectation (kept verbatim)

```rust
for (row, spaced, joined) in [
    (1, "18 . 90", "18.90"),
    (2, "24 . 31", "24.31"),
    (3, "11 . 07", "11.07"),
]
```

```rust
assert_eq!(cell_text(&off, 10, 0), "3 .");
```

```rust
for (row, spaced) in [(1, "18 . 90"), (2, "24 . 31"), (3, "11 . 07")] {
```

### NEW expectation

The default-path side reads joined, as the option side already did:

```rust
for (row, spaced, joined) in [
    (1, "18.90", "18.90"),
    (2, "24.31", "24.31"),
    (3, "11.07", "11.07"),
]
```

```rust
assert_eq!(cell_text(&off, 10, 0), "3.");
```

```rust
for (row, spaced) in [(1, "18.90"), (2, "24.31"), (3, "11.07")] {
```

### Reason

The old pins froze the pre-fold spacing (`18 . 90`, pin 55f05db #659).
The fold is default behaviour now, and both independent references read the
amounts joined. Expected values come from those references, not from our
output.

### Red-proof

Feeding the old spaced value back (`(1, "18 . 90", "18.90")`) fails with
`left: "18.90" | right: "18 . 90"`; restoring the joined value is green.
fn one_glyph_box_covering_two_letters_stays_whole() {
    let (blocks, words) = blocks_and_words(actual_text_document());
    assert_eq!(texts(&words), vec!["fi", "x"]);
```

The default options read the `/ActualText (fi)` replacement carried by the
drawn `A`. That held while the device applied replacements by default; under
the opt-in it reads the drawn glyph instead (`A`, `x`) and the test fails.

### NEW expectation

```rust
fn one_glyph_box_covering_two_letters_stays_whole() {
    // The replacement reads only with `actual_text`: the default leaves the
    // drawn glyph, exactly as without the option (#663).
    let mut options = ExtractOptions::default();
    options.actual_text = true;
    let (blocks, words) = blocks_and_words_with(&actual_text_document(), options);
    assert_eq!(texts(&words), vec!["fi", "x"]);
```

Every assertion on what is read is unchanged: words `fi`, `x`, the carrier
glyph's whole box on `fi`, span `fi x` over three glyph bounds. Only the
option selects the replacement now.

### Reason

The test was written for the replacement reading; the reviewed opt-in
decision moved that reading behind the flag without changing what the
replacement says. The default path is pinned off by the tests above, so the
test opts in instead of the default moving.

### INDEPENDENT reference

A byte-faithful mirror of `actual_text_document` (same operators: `BT Tf Td
BDC(/ActualText fi) Tj(A) EMC Tj(space) Tj(x) ET`, Courier): pdftotext
(poppler 26.02.0, no PyMuPDF involved) reads `fi x` -- the replacement --
while pypdf 6.8.0 reads `A x` -- the drawn glyphs. The new expectation follows
pdftotext; the default follows pypdf. Both references agree the replacement
is `fi`, never invented text.

### Targeted test result and red-proof

`cargo test -p pdf-engine --test word_boxes_come_from_their_glyphs`: 13
passed, 0 failed. Red-proof: the same test with `blocks_and_words` (default
options) fails on `assert_eq!(texts(&words), vec!["fi", "x"])` with left
`["A", "x"]`, then the opt-in is restored and the suite is green again.

## The TATR suites pin the stand-in Rect and never a reference library

- Tests: `scripts/extraction/tables_b/test_eval_zeroshot.py` (`test_vendored_pipeline`, `test_rect_units`, `test_tatr_adapter`) and `scripts/extraction/tables_b/test_make_tatr_rust_fixtures.py` (`test_synthetic_fixtures`)
- Commit: `test(tables-b): pin the stand-in Rect and never a reference library (#636)`
  (this commit)
- Date: 2026-10-11

### ORIGINAL expectation (kept verbatim)

`test_eval_zeroshot.py` hid the reference library and expected the stub to be seeded, then read both backends as equal:

```python
@contextlib.contextmanager
def without_fitz():
    """PyMuPDF hidden: `ensure()` must seed the stub, and state restored."""
```

```python
    with without_fitz():
        check("without PyMuPDF the stub is seeded",
              fitz_compat.ensure(), "stub")
        import fitz

        check("and it is ours", fitz.Rect is fitz_compat.Rect, True)
        stubbed = [run_vendored(*tatr_fixture()),
                   run_vendored(*tatr_fixture(spanning=True))]
    fitz_compat.ensure()
    real = [run_vendored(*tatr_fixture()),
            run_vendored(*tatr_fixture(spanning=True))]
    check("stub and real Rect agree, plain and spanning", stubbed, real,
          "a stand-in that reads differently is a second implementation")
```

```python
    try:
        import fitz  # noqa: F401
        have = True
    except ImportError:
        have = False
    check("ensure() names its Rect honestly", fitz_compat.ensure(),
          "pymupdf" if have else "stub")
```

```python
    check("the Rect backend is recorded",
          details["rect_backend"] in ("pymupdf", "stub"), True)
```

`test_make_tatr_rust_fixtures.py` (`test_synthetic_fixtures`) did the same for the committed fixtures:

```python
@contextlib.contextmanager
def without_fitz():
    """PyMuPDF hidden: `ensure()` must seed the stub, and state restored."""
```

```python
    with without_fitz():
        check("without PyMuPDF the stub is seeded",
              fitz_compat.ensure(), "stub")
        import tatr_postprocess as stubbed

        for name in names:
            record = json.loads((TATR_FIXTURES / name).read_text(
                encoding="utf-8"))
            got, _confidence = cells_of(record, stubbed)
            check(f"{name} reads the same on the stub Rect", rounded(got),
                  rounded(record["cells"]))
```

### NEW expectation

The helper places raising `fitz` / `pymupdf` fakes on `sys.path`; any import attempt fails the run. `test_eval_zeroshot.py`:

```python
    with with_fake_reference_library():
        try:
            import tatr_postprocess as guarded
        except RuntimeError as err:
            FAILURES.append("the post-processing imported a reference "
                            f"library: {err}")
            return
        check("Rect resolves to the stand-in",
              guarded.Rect is rect_standin.Rect, True)
        guarded_runs = [run_vendored(*tatr_fixture()),
                        run_vendored(*tatr_fixture(spanning=True))]
    plain_runs = [run_vendored(*tatr_fixture()),
                  run_vendored(*tatr_fixture(spanning=True))]
    check("a reference library on sys.path changes nothing",
          guarded_runs, plain_runs,
          "the stand-in is the only Rect the tooling knows")
```

```python
    check("ensure() always names the stand-in", rect_standin.ensure(),
          "stub",
          "the stand-in is the only Rect; nothing is preferred")
```

```python
    check("the Rect backend is recorded", details["rect_backend"], "stub")
```

`test_make_tatr_rust_fixtures.py` (`test_synthetic_fixtures`):

```python
    with with_fake_reference_library():
        try:
            import tatr_postprocess as guarded
        except RuntimeError as err:
            FAILURES.append("the post-processing imported a reference "
                            f"library: {err}")
            return
        check("Rect resolves to the stand-in",
              guarded.Rect is rect_standin.Rect, True)
        for name in names:
            record = json.loads((TATR_FIXTURES / name).read_text(
                encoding="utf-8"))
            got, _confidence = cells_of(record, guarded)
            check(f"{name} decodes with a reference library present",
                  rounded(got), rounded(record["cells"]))
```

The fakes raise `RuntimeError`, not `ImportError`, so an old import-or-fallback path cannot swallow them back into green.

### Why the old pin no longer holds

The clean-room policy forbids any path that imports or prefers the reference library: the `fitz` module name, the `sys.modules` seeding and the `"pymupdf"` preference are removed, and the stand-in is the only Rect the tooling knows. COS decision 11-10: the change to the existing test assertions goes as a separate commit with this entry. Byte-identity proof: all 33 committed fixtures (`pt_00..14`, `ftn_00..14`, `syn_*`) plus the plain and spanning 2x2 grids decode identically before and after, sha256 `6a4e091f3f55146c415eb9bc3e1fc1a0dc5d2d599f4ca3df44bc5f66b1ce3f11` before = after (`meas-out/rect-standin/before.json` vs `after.json`, `cmp` identical).

## A scan without OCR reads as its G66 figure marker

- Test: `crates/pdf-engine/tests/an_ocr_layer_makes_a_scan_readable.rs:auto_reads_a_scanned_page_and_never_does_not`
- Commit: `fix(engine): a textless page carries its image figures (#658)`
  (this commit)
- Date: 2026-10-11

### ORIGINAL expectation (kept verbatim)

```rust
assert_eq!(
    plain.trim(),
    "<!-- image -->",
    "without OCR the scan is one figure marker"
);
```

### NEW expectation

```rust
assert_eq!(
    plain.trim(),
    "![](figure-0001-01.png)",
    "without OCR the scan is one figure marker"
);
```

### Why the old pin no longer holds

G66 figure contract: markdown figures render as `![](figure-NNNN-NN.png)`
(commit `6d2644c` `feat(extract): figures in markdown, figure listing and
image options (#658)`, pinned by `markdown_carries_figures`). `"<!-- image
-->"` has had no producer since that commit, and the scan read `""` without
OCR because `merge_figures` withheld figures on pages with no text block.
Owner decision 11-10: a page whose ONLY content is an image (no text block)
MUST carry its figure marker, so the scan now reads its one marker and the
test pins that. Reviewed alignment of this expectation is part of the owner
decision.

## The release carries no models-package job and no models-v1 upload

- Test: `scripts/release/test_release_ort_variant.py:OrtVariantTests.test_models_package_uploads_one_models_v1_artefact`
  → `test_no_models_package_job_and_no_models_v1_upload`
- Commit: `ci(release): fetch models-v1 from the public release (#636)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

```python
uploads = [
    s for s in _steps(doc, "models-package")
    if s.get("uses", "").startswith("actions/upload-artifact")
]
self.assertEqual(len(uploads), 1)
self.assertEqual(uploads[0].get("with", {}).get("name"), "models-v1")
```

plus the assemble step naming all 14 `EXPECTED_MODELS_ASSETS`.

### NEW expectation

```python
self.assertNotIn("models-package", doc["jobs"])
```

and no `upload-artifact` step anywhere in the workflow names `models-v1`
(the remaining uploads are held non-empty, so the loop judges something).

### Why the old pin no longer holds

Owner D18 + A1, models-review REVIEW §4 If (i): the model pack is not
uploaded as a GitHub Actions artefact, and the public `models-v1` release
now exists, so the `models-package` job and its upload are deleted; both
consumers download from the release instead.

## The smoke job waits on package only

- Test: `scripts/release/test_release_ort_variant.py:OrtVariantTests.test_smoke_job_waits_on_package_and_models_package`
  → `test_smoke_job_waits_on_package`
- Commit: `ci(release): fetch models-v1 from the public release (#636)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

```python
needs = _doc(NATIVES)["jobs"]["smoke"]["needs"]
self.assertEqual(sorted(needs), ["models-package", "package"])
```

### NEW expectation

```python
needs = _doc(NATIVES)["jobs"]["smoke"]["needs"]
self.assertEqual(needs, ["package"])
```

### Why the old pin no longer holds

Owner D18 + A1, models-review REVIEW §4 If (i): `models-package` is gone,
so the smoke job's `needs` drops it; the models arrive per-leg from the
public release, not from an earlier job.

## The ort smokes download from the public release

- Test: `scripts/release/test_release_ort_variant.py:OrtVariantTests.test_every_non_musl_smoke_leg_has_a_models_install_smoke`
  (adjusted)
- Commit: `ci(release): fetch models-v1 from the public release (#636)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

```python
download = _step(doc, "smoke", "Download models-v1 for the ort smokes")
...
self.assertEqual(
    download.get("with", {}).get("name"), "models-v1",
    "the ort smokes download the same-run models-v1 artefact",
)
```

### NEW expectation

The step is `Download models-v1 from the public release`, still gated to
self-pool non-musl, and invokes the downloader that the byte case holds to
the release base and the tree pins:

```python
run = download.get("run", "")
self.assertIn("fetch_models_release.py", run)
self.assertIn("--out models-v1", run)
self.assertIn("--all", run)
```

The three channel-step assertions stay, plus `--models models-v1`: each
channel consumes the dir the download step staged.

### Why the old pin no longer holds

Owner D18 + A1, models-review REVIEW §4 If (i): no same-run artefact
exists to download. The self-pool half of the `if:` is kept per the
decision point (the github pool is not opened here).

## The ort-rust legs download from the public release

- Test: `scripts/release/test_release_ort_variant.py:OrtVariantTests.test_ort_rust_runs_the_tiered_suite_per_target`
  (adjusted)
- Commit: `ci(release): fetch models-v1 from the public release (#636)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

```python
job = doc["jobs"]["ort-rust"]
self.assertIn("models-package", job["needs"])
...
download = _step(doc, "ort-rust", "Download models-v1")
self.assertEqual(download.get("with", {}).get("name"), "models-v1")
```

### NEW expectation

`needs` no longer names `models-package`, and the download step is the
download-and-verify step:

```python
download = _step(doc, "ort-rust", "Download models-v1 from the public release")
self.assertIn("fetch_models_release.py", download.get("run", ""))
self.assertIn("--out models-v1", download.get("run", ""))
self.assertIn("matrix.ort-lib", download.get("run", ""))
```

The `cargo test ... --features ort-layout,ort-tables` / `ORT_DYLIB_PATH`
/ `matrix.ort-lib` assertions and the self-only job `if:` stay.

### Why the old pin no longer holds

Owner D18 + A1, models-review REVIEW §4 If (i): same route change as the
smoke legs; the leg fetches only its platform's library.

## The github pool skips the self-only model legs

- Test: `scripts/release/test_release_ort_variant.py:OrtVariantTests.test_github_pool_gets_a_skip_message_and_no_artefact`
  → `test_github_pool_skips_the_self_only_model_legs`
- Commit: `ci(release): fetch models-v1 from the public release (#636)`
## The runner_pool pin covers build-wheels.yml too

- Test: `scripts/release/test_release_workflows_route_through_runner_pool.py`
  (`test_runner_pool_input_declared`,
  `test_every_runs_on_routes_through_runner_pool`,
  `test_matrix_entries_carry_both_labels`,
  `test_plain_jobs_fall_back_to_github`)
- Commit: `feat(ci): route build-wheels through the runner_pool input (#636)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

```python
skip = _step(
    doc, "models-package",
    "The github pool cannot fetch the models (self pool is the release pool)",
)
self.assertEqual(skip.get("if"), "inputs.runner_pool != 'self'")
```

plus the fetch/stage/assemble steps gated to `inputs.runner_pool == 'self'`.

### NEW expectation

Re-targeted at what remains self-only: the ort-rust job-level `if:`, the
smoke download step, and the three channel steps all carry
`inputs.runner_pool == 'self'`.

### Why the old pin no longer holds

Owner D18 + A1, models-review REVIEW §4 If (i): the skip message lived in
the deleted job, and the github pool is not opened for the ort legs here,
so the case holds the gating that keeps them self-only instead.

## The no-secret ban covers the consumers and the artefact

- Test: `scripts/release/test_release_ort_variant.py:OrtVariantTests.test_no_step_uses_a_secret_for_gcs`
  (extended)
- Commit: `ci(release): fetch models-v1 from the public release (#636)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

```python
text = _job_text(_doc(NATIVES), "models-package")
for forbidden in ("secrets.", "gcloud auth", "GOOGLE_APPLICATION_CREDENTIALS"):
    self.assertNotIn(forbidden, text)
```

### NEW expectation

The same three tokens are forbidden in the `ort-rust` and `smoke` jobs
and in the downloader, the stager, and the smoke script -- and no step
may reference a `models-v1` Actions artefact at all.

### Why the old pin no longer holds

Owner D18 + A1, models-review REVIEW §4 If (i): the scanned job is gone,
so the ban moves to the jobs that fetch the models now, extended to the
artefact class D18 removes.

## The assemble cases are deleted with the assemble step

- Tests: `scripts/release/test_release_ort_variant.py:OrtVariantTests.test_assemble_step_pins_match_the_staged_texts`,
  `test_staging_step_refuses_a_modified_notice`,
  `test_models_v1_carries_no_per_asset_sidecars` (deleted);
  `MINIMUM_CASES` 14 → 19
- Commit: `ci(release): fetch models-v1 from the public release (#636)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

The assemble step's pinned-text table equals the staged digests; its
verification block, executed, refuses a tampered notice with REFUSED;
SHA256SUMS lists exactly the thirteen installables.

### NEW expectation

Deleted: nothing assembles the pack in the workflow anymore. Pin
enforcement moves to the downloader (new byte case below) plus the
existing `models install` / engine pin tests. The floor is recomputed
for the new case count: 20 − 3 + 2 = 19.

### Why the old pin no longer holds

Owner D18 + A1, models-review REVIEW §4 If (i): the assemble step lived
in the deleted job; the release is published once by the owner instead.

## New case: no step names a models-v1 Actions artefact

- Test: `scripts/release/test_release_ort_variant.py:OrtVariantTests.test_no_step_names_a_models_v1_actions_artefact`
  (new)
- Commit: `ci(release): fetch models-v1 from the public release (#636)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

None -- new case.

### NEW expectation

```python
self.assertEqual(_models_v1_artifact_steps(_doc(NATIVES)), [])
```

No `upload-artifact` or `download-artifact` step in the workflow carries
`models-v1` as its artefact name.

### Why this case exists

Owner D18 + A1: the ban on the models-v1 artefact class, held directly
instead of only through the deleted job's shape.

## New case: every downloaded byte is verified against the tree pins

- Test: `scripts/release/test_release_ort_variant.py:OrtVariantTests.test_every_downloaded_byte_is_verified_against_the_tree_pins`
  (new)
- Commit: `ci(release): fetch models-v1 from the public release (#636)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

None -- new case.

### NEW expectation

Both download steps invoke the downloader into `--out models-v1`; the
downloader reads `models_pins.toml` and `ort_pins.toml`, refuses with
REFUSED, and its base equals `MODELS_RELEASE_BASE` in `cmd_models.rs`.
Executed against a `file://` base: all thirteen installables are known
names, a bogus name refuses, a tampered tree text refuses leaving
nothing behind, and the genuine tree bytes verify and land identical.

### Why this case exists

Owner D18 + A1, models-review REVIEW §4 If (i): pin enforcement for the
public-release route, replacing the deleted assemble cases.

## The runner-pool map drops the deleted job

- Test: `scripts/release/test_release_workflows_route_through_runner_pool.py`
  (`EXPECTED_JOBS["release-natives.yml"]`)
- Commit: `ci(release): fetch models-v1 from the public release (#636)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

```python
"release-natives.yml": [
    "models-package",
    "natives",
    "ort-rust",
    "package",
    "smoke",
],
```

### NEW expectation

```python
"release-natives.yml": [
    "natives",
    "ort-rust",
    "package",
    "smoke",
],
Five workflows routed every runs-on through `runner_pool`:

```python
FIVE = [
    "release-natives.yml",
    "release-binaries.yml",
    "node-bindings.yml",
    "wasm.yml",
    "release-smoke.yml",
]
```

and every plain (matrix-less) job fell back to ubuntu-latest:

```python
                    self.assertIn("inputs.runner_pool == 'self'", runs_on)
                    self.assertIn(SELF_LINUX_X64, runs_on)
                    self.assertIn("|| 'ubuntu-latest'", runs_on)
```

### NEW expectation

Six workflows: build-wheels.yml joins the list (`SIX`), with
`EXPECTED_JOBS["build-wheels.yml"] == ["linux", "macos", "sdist",
"windows"]`. The plain-job case reads the hosted fallback label off the
runs-on and expects that label's release set from SELF_LABELS, instead of
demanding ubuntu-latest everywhere:

```python
                    fallbacks = [
                        label
                        for label in SELF_LABELS
                        if f"|| '{label}'" in runs_on
                    ]
                    self.assertEqual(
                        len(fallbacks), 1, f"one hosted fallback: {runs_on}"
                    )
                    self.assertIn(
                        json.dumps(SELF_LABELS[fallbacks[0]]), runs_on
                    )
```

### Why the old pin no longer holds

Owner D18 + A1, models-review REVIEW §4 If (i): the job-list pin follows
the reviewed deletion; the routing assertions on the remaining jobs are
unchanged. (`gate_requirements.toml` drops the matching `[release-job]`
table for the same reason; the JSON is regenerated from it.)
SPEC: this job -- build-wheels.yml gains the `runner_pool` input (#636)
and routes every job through it, the way release-natives.yml,
release-binaries.yml and node-bindings.yml already do. Its windows job is
the first plain job whose github pool is not ubuntu-latest: pinning
`|| 'ubuntu-latest'` there would move Windows wheels onto Linux. The
generalised case keeps pinning exactly one hosted fallback plus its
matching release set, and still passes the other five workflows
unchanged. The extended test fails on the unrouted workflow (7
failures, 1 error, all build-wheels) and passes with the routing.

## The mechanical import check knows fixtures/ tooling

- Test: `scripts/ci/test_gate_preflight.py:CoverageTests.test_declarations_cover_the_mechanical_requirements`
  (via `repo_has`)
- Commit: `test(ci): teach the mechanical check about fixtures/ tooling (#636)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

`repo_has` recognised repo-local modules under six top-level dirs only:

```python
                for base in ("scripts", "crates", "integrations", "tools", "tests", "benchmarks"):
```

### NEW expectation

`fixtures/` joins the recognised dirs:

```python
                # fixtures/ holds repo-local test tooling like the rest: the
                # known-images generator the genimgdet gate imports.
                for base in ("scripts", "crates", "integrations", "tools", "tests", "benchmarks",
                             "fixtures"):
```

### Why the old pin no longer holds

SPEC: test_gate_preflight.py's mechanical rule ("the declaration holds at
least what the lane line already says") against gate_preflight.py's module
rule ("importable right now (find_spec; nothing is imported)"). The genimgdet
gate imports `known_images`, the generator beside its test under
fixtures/gen/ -- repo-local tooling, found by no other base, so the old
six-base list read it as third-party and demanded it in python_modules,
where the real preflight would fail every lane on it as unimportable. The
two halves of the same test contradicted; the seven-base list resolves it.
Verified the widening masks nothing: the only gate script importing a
fixtures-named module is genimgdet's own test importing the generator.

## The suite-location pin searches binding-suites.yml too

- Test: `scripts/ci/test_orchestration_guard_runs_no_build.py:test_each_suite_still_runs_in_another_job`
- Commit: `test(ci): search binding-suites.yml for the compiling suites (#350)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim)

The test read ci.yml only and required each compiling suite to run in a
job of that file outside orchestration-guard:

```python
    def test_each_suite_still_runs_in_another_job(self):
        doc = _doc()
        for suite in HEAVY_SUITES:
            elsewhere = [
                name for name, job in doc["jobs"].items()
                if name != "orchestration-guard"
                and isinstance(job, dict)
                and any(
                    suite in str(s.get("run", ""))
                    for s in (job.get("steps") or [])
                    if isinstance(s, dict)
                )
            ]
```

### NEW expectation

The test reads ci.yml and binding-suites.yml and requires each suite to
run in a job of either file outside orchestration-guard:

```python
    def test_each_suite_still_runs_in_another_job(self):
        docs = [_doc(), yaml.safe_load(SUITES_WORKFLOW.read_text())]
        for suite in HEAVY_SUITES:
            elsewhere = [
                name for doc in docs for name, job in doc["jobs"].items()
                if name != "orchestration-guard"
                and isinstance(job, dict)
                and any(
                    suite in str(s.get("run", ""))
                    for s in (job.get("steps") or [])
                    if isinstance(s, dict)
                )
            ]
```

### Why the old pin no longer holds

SPEC: heavy_work_leaves_the_desktop.py ("nothing triggered by a push to
master compiles on the desktop") and orchestration_stays_hosted.py (the
heavy rule is trigger-independent), enforced on t5/r4-gate. Those rules
ordered the binding-suites job out of ci.yml's push path into the
dispatch-only binding-suites.yml; a ci.yml-only scan then fails on a tree
that satisfies every guard. The pin's intent -- the suites run somewhere
outside the guard, so the move cannot silently become a deletion -- is
unchanged, and the widened test fails when a suite is removed from the
new file (verified by deleting one suite path and watching it go red).
## `test_journal` temp path drops `P_tmpdir` for `TMPDIR`/`/tmp` (portability, no expectation change)

- Test: `crates/pdf-capi/tests/test_journal.c:test_save_load_round_trip` — `P_tmpdir` is undefined under glibc strict `-std=c99`/`-std=c11`, so the temp path now uses `getenv("TMPDIR")` with a `"/tmp"` fallback. No assertion changed.
## The ActualText choice test selects its own option

- Test: `crates/pdf-engine/tests/cell_path_words_choice.rs:actual_text_applies_with_the_option`
- Commit: `fix(engine): read ActualText behind its own option (#663)` (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim, from `git show 5d14e02:crates/pdf-engine/tests/cell_path_words_choice.rs`)

The `on` leg selected the cell-path words through the shared helper:

```rust
fn options_on() -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.cell_path_words = true;
    options
}
```

```rust
fn actual_text_applies_with_the_option() {
    let pdf = page_with_an_actual_text_run();
    let off = blocks_of(&pdf, &ExtractOptions::default());
    let on = blocks_of(&pdf, &options_on());
    assert_eq!(off.len(), 1);
    assert_eq!(on.len(), 1);
    assert_eq!(off[0].text, "QQQ");
    assert_eq!(on[0].text, "REPLACEMENT");
}
```

and the module docs pinned `/ActualText` as one of the three rules the
cell-path flag moves ("Three rules are pinned here ... and `/ActualText`
on a marked run").

### NEW expectation

The `on` leg selects `actual_text` alone:

```rust
fn options_actual_text() -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.actual_text = true;
    options
}
```

```rust
    let on = blocks_of(&pdf, &options_actual_text());
```

with the same pins (`QQQ` off, `REPLACEMENT` on); two new tests pin the
independence (each flag alone moves only its own behaviour) and one pins
the new default off.

### Reason

COS reviewed decision (t5/r4-actualtext-opt, 10-10): two features, two
flags. `cell_path_words` must no longer affect ActualText in any way, and
`actual_text` must not affect the four cell-path word rules. The changed
test is the choice-commit scaffolding written two hours earlier by
55f05db/5d14e02, not a spec test.

### SPEC line reference

The job: "the ActualText case of tests/cell_path_words_choice.rs moves to
selecting `actual_text` ... allowed here as the reviewed decision of the
COS, with an entry in docs/testing/ENGINE-EXPECTATION-CHANGES.md".

## The DLR C test pins the one-row header with full column names

- Test: `crates/pdf-engine/tests/dlr_stacked_figure_rows.rs:fused_figure_rows_read_as_body_under_c`
- Commit: `test(engine): align the DLR C test with the one-row header design (#574)`
  (this commit)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim, from REVIEW (a))

Doc comment + assertion block, per `meas-out/dlr-ctest-review/REVIEW.md`
§(a) (test file lines 290–292, 310–314):

```rust
/// The fused table opens with `<th>` body rows with C off, none under C:
// `86,090 (4,305)`, `— —`, `$—`, `$ —` and `$ 990` read as figures for the
/// header block, so only the head row stays header.
```

```rust
    let switched = to_markdown_with_switches(&blocks, switches_c());
    assert!(
        switched.contains("<th>Shares</th>"),
        "the head row stays header under C:\n{switched}"
    );
```

`<th>Shares</th>` pins the four-line header read as four rows; neither
revision produces it (REVIEW §(c): MISSING at r3 and at HEAD).

### NEW expectation

```rust
    let switched = to_markdown_with_switches(&blocks, switches_c());
    for cell in [
        "<th>Number of Common Shares</th>",
        "<th>Common Stock</th>",
        "<th>Additional Paid-in Capital</th>",
        "<th>Dividends In Excess of Earnings</th>",
        "<th>Accumulated Other Comprehensive Income, net</th>",
        "<th>Owner’s Equity</th>",
        "<th>Total</th>",
    ] {
        assert!(
            switched.contains(cell),
            "the head row stays header under C ({cell:?}):\n{switched}"
        );
    }
```

The seven cells are REVIEW §(c)'s produced header verbatim (HEAD C
markdown, all 22 header words preserved per `check_preserved.py`); every
other assertion in the test is unchanged.

### Reason

owner D17 10-10: one header row with full column names per the shipped
design (REVIEW (b))

### SPEC line reference

`meas-out/dlr-ctest-review/REVIEW.md`: §(b) design quotes (a header
row's cells that only a word space parts are one cell; ONE BLOCK PER
CELL with `header: bool`; head = leading rows above the first
mostly-figures row; a pipe table holds ONE header row) and §(c)
produced header (the seven `<th>` cells above, byte-identical to HEAD's
C markdown).
## Door OCR pins content recall, not exact bytes (new content test, owner D8 10-10)

- Test: `crates/pdf-python/tests/door/test_l6_ocr_content.py:test_door_ocr_content_recall` (new)
- Commit: `1e659e3` (`feat(door): wire default OCR through native path; text-strategy <br> grid`)
- Date: 2026-10-10

### ORIGINAL expectation

No content test existed: `test_g183_default_finds_words` and
`test_g133_ocr_words_not_listed` pin the recogniser's exact bytes
(`results/ocrwords/words-default-md.md`, `words-default-json.json`).
Both are unchanged and keep failing on exact parity.

### NEW expectation

The new test converts the scanned fixture through the default door
path and pins the content: the output is non-empty and the reference
words are present case-insensitively at or above the calibrated OCR
recall (`tests/compat/tolerance.toml` `[ocr]` `recall = 0.60`). It
skips (never passes) without a Tesseract binary and records the
Tesseract version in its output.

### Reason

Owner decision D8 (10-10): the door's default OCR is wired through
the native path, and the wiring is pinned with a content comparison
instead of exact bytes, because exact OCR bytes move with the
recogniser version (word joins, "tine" vs "line").

### SPEC line reference

Owner D8 10-10 (new content test, different comparison approved);
`tolerance.toml [ocr]` ("normalised bag-of-words recall >= `recall`").
## D7 door differences counted, 2026-10-10 (no expectation change)

- D7 (owner 2026-10-10, OWNER-DECISIONS-1.1.0.md): door tests test_g53_chunks_page_subset, test_g54_layout_metadata, test_g56_toc_per_page report difference (base-14 ink metrics); no test body or expected value changed, only counting. The other twelve candidates also fail on text, class, colour, structure or span splitting and stay red.

## `test_structured_extraction.py` gains its missing `PdfluentValidationError` import

- Test: `crates/pdf-python/tests/test_structured_extraction.py` (import block only)
- Commit: `77a3e4e` (`test(python): import PdfluentValidationError in the structured-extraction tests (#643)`)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim, from `git show 77a3e4e^:<file>`)

The file used `PdfluentValidationError` in
`test_a_page_list_beside_a_window_is_a_validation_error` without importing
it, so that test errored with `NameError`. The import block was:

```python
from pdfluent import Document
```

### NEW expectation

The import block gains one name, in the form the other pdf-python tests
use (`crates/pdf-python/tests/test_markdown_furniture.py`):

```python
from pdfluent import Document, PdfluentValidationError
```

No assertion changed; the test now runs instead of erroring on the name.

### Reason

Import missing since R0, no expectation change.

### SPEC line reference

Clean base `41fcb9d`: the file is unchanged since R0, which is where the
missing import comes from.

## `test_structured_extraction.py` gains its missing `Document` import

- Test: `crates/pdf-python/tests/test_structured_extraction.py` (import block only)
- Commit: `6de675a` (`test(python): import Document in the structured-extraction tests (#643)`)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim, from `git show 6de675a^:<file>`)

The file had no import: it used `Document(...)` in the `doc` fixture and
in `test_vision_advice_names_the_scanned_page_and_not_the_typeset_ones`
without importing it, so every test errored with `NameError`. The import
block was:

```python
import os

import pytest
```

### NEW expectation

The import block gains one line, in the form the other pdf-python tests
use:

```python
from pdfluent import Document
```

No assertion changed; the tests now run instead of erroring on the name.

### Reason

Import missing since R0, no expectation change.

### SPEC line reference

Clean base `41fcb9d`: the file is unchanged since R0, which is where the
missing import comes from.

## `ocr_failure_detail` only compiles where its premise holds

- Test: `crates/pdf-engine/tests/ocr_failure_detail.rs` (whole file)
- Commit: `d9df3d1` (`test: gate the no-engine OCR test on its premise (#660)`)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim, from `git show d9df3d1^:<file>`)

The test compiled in every build; its doc comment stated the premise in
words only:

```rust
//! Only the shape is pinned: the message names the attempted engine.
//! Unspecified; our behaviour: without an engine the front door stays
//! silent (G184) where we raise, so the least the raise can do is say what
//! it looked for. Run on the default build, without the OCR feature.

// Copyright (c) 2026 Innovation Trigger B.V.
```

### NEW expectation

The premise is compiled in: the file carries
`#![cfg(not(feature = "ocr-cli"))`, so the test exists exactly in builds
without an engine. The pinned shape (the raise names the attempted
engine) is unchanged.

### Reason

In a workspace-wide build Cargo unifies features: pdf-node enables
`ocr-cli`, so an engine IS compiled into the test's build, forced
recognition succeeds, and `expect_err` fails. Solo the binary has no
engine (pass), in the suite it has one (fail) -- the order/time
dependence was feature unification, not flakiness. The old pin held only
where the premise held by accident.

### SPEC line reference

The test's own contract, quoted above: "Run on the default build,
without the OCR feature." The gate makes the build match the sentence.

## Measure-once equivalence compares NaN-aware

- Tests: `crates/pdf-engine/src/learned/captions.rs:mod tests` (caption
  rows over shared heading rows), `crates/pdf-engine/src/learned/lines.rs:mod tests`
  (grouping the page once)
- Commit: `4138f30` (`test: compare measure-once vectors NaN-aware (#666)`)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim, from `git show 4138f30^:<file>`)

```rust
assert_eq!(
    page_caption_rows_on_rows(&page, &images, &rows),
    page_caption_rows(&page, &images)
);
```

and likewise `assert_eq!(line_vectors_on_lines(...), line_vectors(...))`:
two identical vectors compared unequal, because rows carry NaN for
missing context and NaN never equals itself.

### NEW expectation

Element-wise comparison with NaN equal to NaN
(`assert_caption_pages_equal`, `assert_line_vectors_equal`): same shape,
same values, same NaN placement -- a NaN against a number still fails.
No decision code touched.

### Reason

NaN for missing context (out-of-range neighbours, absent features) is by
design, so `assert_eq!` cannot express the equivalence the tests pin
(one measure pass equals per-block measures). The old pin failed on two
identical vectors; the new one fails only on a real divergence.

### SPEC line reference

The equivalence property the test doc comments pin ("Caption rows over
shared heading rows are caption rows", "Grouping the page once is
grouping each block"), unchanged; only the comparison method moved, and
it stays strict on values, shapes and NaN placement.

## The glyph-cache eviction test pins thrash counts

- Test: `crates/pdf-render/src/glyph_cache.rs:mod tests` (eviction order)
- Commit: `91156af` (`test: pin the LRU order the glyph cache actually keeps (#665)`)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim, from `git show 91156af^:<file>`)

```rust
// Key 2 was least-recently-used, so it rebuilds; 1 and 3 are stored.
cache.get_or_insert_with(2, || build(2));
cache.get_or_insert_with(1, || build(1));
cache.get_or_insert_with(3, || build(3));
assert_eq!(builds.get(), [0, 1, 2, 1]);
```

### NEW expectation

```rust
// Key 2 was least-recently-used, so it rebuilds -- and re-fetching
// three keys through two slots thrashes: each re-fetch evicts the
// next one asked for, so all three build twice. (FIFO would keep 2
// and LIFO would keep 1 here, so this still pins LRU order.)
cache.get_or_insert_with(2, || build(2));
cache.get_or_insert_with(1, || build(1));
cache.get_or_insert_with(3, || build(3));
assert_eq!(builds.get(), [0, 2, 2, 2]);
```

### Reason

The old count `[0, 1, 2, 1]` is impossible for any 2-slot LRU (T5 review
verdict, 10-10, KEEP): inserting the third key must evict a resident,
and fetching all three keys through two slots thrashes them all twice
(traced). The implementation evicts the true least-recently-used key;
FIFO and LIFO both fail the corrected counts, so the LRU order stays
pinned.

### SPEC line reference

The LRU contract the implementation keeps (evict the oldest live touch
pair), cited in the commit; the corrected comment names the FIFO/LIFO
rejections that keep the order pinned.

## Visual fixtures regen'd, sync patterns widened

- Tests: `crates/visual-regression` (4 fixture PDFs),
  `crates/pdf-node/tests/typed_error_layer.test.js` (ESM/CJS surface sync)
- Commit: `5d01476` (`fix: re-export the compat surface from the ESM wrapper (#643)`)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim, from `git show 5d01476^:<file>`)

Fixtures `encrypted-aes256.pdf`, `encrypted-rc4.pdf`,
`pdfa-converted.pdf`, `watermark.pdf` with the pre-E7 dictionary key
order; sync patterns reading only letters:

```js
[...cjs.matchAll(/module\.exports\.([A-Za-z]+)/g)].map((m) => m[1])
[...dts.matchAll(/^export declare (?:class|function) ([A-Za-z]+)/gm)].map((m) => m[1])
```

### NEW expectation

The four fixtures regen'd (same byte length each, dictionary keys
reordered only: Type/XRef after Size/Encrypt/Info); patterns widened to
`[A-Za-z0-9_]+`. Pixels unchanged; the full visual suite passes.

### Reason

The E7 xref-stream change reordered dictionary keys, moving the fixture
bytes without changing a pixel -- a byte-only regen, recorded here
because the job series names it. The compat lane added hundreds of napi
exports whose names carry digits and underscores, which the old
letter-only patterns could not see; widened, the sync test pins all 733
CJS names re-exported from `index.mjs` with zero undefined at runtime.

### SPEC line reference

The napi-regenerated `crates/pdf-node/index.js`/`index.d.ts` committed
in the same change: the sync test reads its names from those files, so
the widened patterns pin the surface the generator emits.

## e1e2 goldens pin the joined words

- Tests: `crates/pdf-engine/tests/` e1e2 default-output goldens
  (`e1e2-f1040`, `e1e2-simple`, `e1e2-structured`, json + md)
- Commit: `70e37fc` (`fix: resolve the default-output merge damage the W1 join caused (#668)`)
- Date: 2026-10-10

### ORIGINAL expectation (kept verbatim, from `git show 70e37fc^:<file>`)

Per-glyph spaced text, e.g. `e1e2-simple.md`:

```text
T e s t   p a g e
```

### NEW expectation

Goldens regenerated after reading the diff, e.g.:

```text
Test   page
```

Every move de-garbles per-glyph text, JSON moves text-only, all 8 f1040
bold runs survive at their positions, and closed dashes join
(`Treasury--Internal`, `1--Dec`) per the rule.

### Reason

b8de1e7 applied W1's shared word rule to the block rebuild, but a plain
revert re-breaks the W1 invariant that a block's words joined with
spaces reproduce its text; neither side alone satisfies both pins. The
fix keeps the shared rule (factored as `words_join::spans_join`) and
teaches the emphasis marks to find sub-word spans across the same
boundary (a late whole match used to jump the cursor and drop every
later mark). The old goldens pinned the garbled side of that merge
damage.

### SPEC line reference

`crates/pdf-engine/src/words_join.rs`: "One rule, three callers ... so
joining the words of a line with single spaces is always the line's own
text." Two unit tests pin the new mark placement and the hyphen guard;
both fail with their feature removed.

## `an_all_monospace_block_is_fenced_with_its_lines` now fences against prose

- Test: `crates/pdf-engine/tests/monospace_renders_as_code.rs:an_all_monospace_block_is_fenced_with_its_lines`
- Commit: `6977820` (`fix(extract): leave default block text alone where nothing is hidden (#655)`)
- Date: 2026-10-09

### ORIGINAL expectation (kept verbatim, from `git show 6977820^:<file>`)

Input: a single all-monospace block, fenced alone:

```rust
let block = paragraph(
    "let x = 1; let y = 2;",
    vec![
        mono("let x = 1;", 100.0, 700.0),
        mono("let y = 2;", 100.0, 688.0),
    ],
);
assert_eq!(
    pdf_engine::to_markdown(&[block]),
    "```\nlet x = 1;\nlet y = 2;\n```\n"
);
```

An all-monospace paragraph over two lines becomes a fenced block with its
lines -- on a page of nothing else.

### NEW expectation

Input: the same two-line monospace block, now standing after a proportional
prose paragraph:

```rust
let prose = paragraph(
    "A note about the pump.",
    vec![plain("A note about the pump.", 100.0, 724.0)],
);
let block = paragraph(
    "let x = 1; let y = 2;",
    vec![
        mono("let x = 1;", 100.0, 700.0),
        mono("let y = 2;", 100.0, 688.0),
    ],
);
assert_eq!(
    pdf_engine::to_markdown(&[prose, block]),
    "A note about the pump.\n\n```\nlet x = 1;\nlet y = 2;\n```\n"
);
```

The fence survives where the typewriter switch does not reach.

### Reason

The old test pinned the clean-base behaviour after R0 (#652) removed the
whole-document typewriter switch as tainted: with the switch gone, every
monospace paragraph read as code, so an all-monospace page fenced. Lane
L1's red spec (d17aadd, #654) re-specifies the switch: "a document set
wholly in monospace reads as prose, and fences only code standing out
against a proportional face". Under that rule the old input -- a document
of nothing but monospace -- reads as prose, and the old pin contradicts
the spec. The edited test keeps pinning the fence, but on the input the
rule still fences: code standing out against a proportional face.

### SPEC line reference

`crates/pdf-engine/tests/whole_document_typewriter_switch.rs`, lines 1-13
(verified against the working tree). The rule is stated at line 1:

```text
//! A document set wholly in monospace reads as prose (lane L1).
```

and restated at lines 6-7:

```text
//! The rebuilt engine treats a document set wholly in monospace as prose,
//! and fences only code standing out against a proportional face.
```
