# OCR in the structured route: plan

- Date: 2026-09-29
- Status: plan only. Nothing below is implemented except the per-page
  detection (`OcrNeed` / `page_needs_ocr` in `pdf-engine`), which this plan
  takes as its input.
- Scope: how an `off` / `auto` / `force` mode would feed recognized words
  into the same block pipeline that born-digital pages go through. The
  recognizers themselves, and any new option or API, are future work.

## 1. Where the words enter

The structured route today, per page: interpret the content stream into a
`TextExtractionDevice`, consume it with `into_paragraph_blocks_and_rules`,
then -- over the whole document -- furniture marking, heading
classification, reading-order arrangement, learned refinement,
cross-page table stitching, and windowing.

OCR joins that route at the earliest point that keeps everything downstream
unchanged: recognized words become synthetic `TextSpan`s appended to the
device's spans before it is consumed. The paragraph merge, the list/table
passes inside it, furniture, headings, reading order and the learned models
then run exactly as they do on born-digital text, because as far as they can
see, that is what they are reading. No second pipeline, no OCR-flavoured
block kinds, no fork in the serializers.

That needs one small seam that does not exist yet: a way to push spans into
a `TextExtractionDevice` (or to construct one from spans plus the rules the
page drew). Everything else -- `TextBlock`, `Block`, markdown, JSON,
Docling -- is untouched.

Per-word span synthesis, from one OCR word with a box:

- `x`, `y`, `width`, `height`: the word box mapped into extraction device
  space (section 3), with `y` the box bottom.
- `font_size`: the box height. The merge reads size only comparatively
  (same paragraph, heading statistics), and a word's height is the honest
  estimate of the size it was set in.
- `text`: the word; `char_bounds`: the word box evenly divided per
  character, since detectors report word boxes, not glyph boxes.
- `width_source`: `Estimate`; `render_mode`: `None` (nothing drew it);
  `transform`: `None`; font name/style/weight: `None`/defaults. An OCR word
  claims geometry and text and nothing else.
- One span per word, not per line: line grouping is the merge's job, and
  handing it pre-grouped lines would smuggle a second line segmenter into
  the route.

## 2. The three modes

`off` is today: no detection runs, no recognizer is called, extraction is
byte-identical to now, and no OCR dependency is linked. It stays the
default, because OCR pulls in models or network calls that a text
extraction library must never take silently.

`auto` runs detection first, then acts per page:

- `OcrNeed::None`: nothing happens. Born-digital pages cost one survey
  pass and no recognition.
- `OcrNeed::Full`: the page is rendered at the configured DPI, recognized
  whole, and the resulting spans are the page's spans (section 4 says what
  happens to any pre-existing invisible layer).
- `OcrNeed::Partial`: only the large bare image regions are recognized --
  rendered as crops or masked out of a full-page render -- and the
  resulting spans are appended beside the spans the interpreter already
  collected.

`force` skips detection: every page is recognized whole (`Full`
treatment), and every pre-existing invisible text layer is dropped in
favour of the fresh recognition. It exists for re-OCRing searchable scans
whose layer is stale, wrong-language, or corrupt -- the case detection
deliberately reports as `Full` rather than trusting.

Detection is shape, mode is policy: `OcrNeed` never says "skip this
already-searchable page", it says "this page is image-dominant without
visible text". Whether `auto` additionally skips pages whose invisible
layer already explains the images is an open policy refinement (section 6).

## 3. Coordinate transforms

Three frames meet here, and rotation makes hand-rolled mapping wrong:

1. **Render pixels**: top-left origin, `dpi` pixels per inch. The render
   path maps user space through
   `scale(dpi / 72) * page.initial_transform(true)`, where the `true`
   requests the y-flip to top-down rows.
2. **PDF user space**: points, y-up from the CropBox origin, before page
   rotation is applied.
3. **Extraction device space**: what `TextSpan.x/y` are in. The extraction
   path maps user space through `page.initial_transform(false)` -- same
   call, no y-flip, and a *different* rotation branch for 90/270-degree
   pages than the render path takes.

The rule: never convert pixels to device units with arithmetic. Compose
the affines the two paths already use:

- `user = inverse(render_transform) * pixel`, with
  `render_transform = scale(dpi/72) * initial_transform(true)`;
- `device = initial_transform(false) * user`.

For the common unrotated full-page case that reduces to the closed form
`x_dev = px / s`, `y_dev = base_h - py / s` with `s = dpi / 72`, but the
wiring should implement the composition, which stays correct under
rotation, CropBox offsets, and non-identity MediaBox origins. Region crops
for `Partial` carry their crop offset back through the same composition.

Two word shapes exist today and the wiring must normalize once:
`pdf_ocr::OcrWord` (`bbox_px: [x0, y0, x1, y1]` in top-down pixels, plus
the image dimensions on the page result) and `pdf_engine::ocr::OcrWord`
(`bbox: [x, y, width, height]`). The structured route should consume the
`pdf-ocr` trait output -- it is the shape with the image dimensions
attached, which the mapping needs -- and convert at the boundary.

## 4. How duplicate text is avoided

Four sources of doubling, each with its rule:

1. **Stale invisible layer under `Full`.** A searchable scan's old layer
   and the fresh recognition describe the same words. Rule: under `force`,
   and under `auto` on a `Full` page, spans synthesized from OCR replace
   the page's invisible spans rather than joining them. (Dropping them
   from the device input is enough; the PDF itself is not rewritten.)
2. **Real text beside OCR regions under `Partial`.** Rule: OCR spans are
   suppressed where the interpreter already found text. Concretely, a
   recognized word whose box overlaps an existing span's box above an IoU
   threshold (0.5 is the starting proposal) is dropped; below it, it
   stands. The threshold wants measuring against captioned figures, where
   a caption legitimately touches its figure's edge.
3. **Overlapping regions.** Where two `Partial` regions overlap (they
   should not, but crops are rectangles and figures are not), the same
   word can be recognized twice. Rule: de-duplicate OCR spans against
   each other with the same IoU test before merging.
4. **Low-confidence ghosts.** Rule: words under a confidence floor are
   dropped before synthesis. The floor is recognizer-relative and belongs
   in configuration, not in the merge.

What is deliberately *not* proposed: trusting an invisible layer's words
as already-correct blocks. An invisible layer carries no provenance -- it
may be a five-year-old pass in another language -- so the route either
replaces it (modes above) or, in the `auto`-skip refinement of section 6,
keeps it whole. Mixing kept and fresh words on one page is how a paragraph
ends up half in each.

## 5. Model choices

All three candidates are permissive-licensed; the table notes code
licence, and each integration must re-check the *weights* it ships or
fetches, because checkpoint provenance is per-artifact, not per-project.

| Engine | Licence | Shape in this repo | Notes |
|---|---|---|---|
| Tesseract 4/5 | Apache-2.0 | already behind `pdf-ocr`'s `tesseract` feature (via `leptess`; needs system libraries) | Word boxes plus confidences out of the box; the baseline every other engine is measured against. LSTM line recognizer; no learned detector, so dense or skewed layouts are its weak end. |
| PaddleOCR (PP-OCR) | Apache-2.0 | already behind `pdf-ocr`'s `paddle` feature (DBNet detection + SVTR recognition over ONNX, offline after the first weight fetch) | Detector plus recognizer plus angle classifier; the strongest of the three on layout-heavy pages and on CJK. Weight downloads must stay opt-in with pinned digests, as the crate does today. |
| docTR | Apache-2.0 | not integrated | PyTorch/TensorFlow document OCR with its own detection (LinkNet/DBNet variants) and recognition (CRNN/MASTER/SAR/ViTSTR). No Rust binding and no ONNX-first story in this repo, so it would arrive as a service-backed `OcrEngine` (self-hosted endpoint) rather than in-process. |

Recommendation: the structured route stays engine-agnostic behind the
existing `pdf-ocr` `OcrEngine` trait, and the first local default behind
it is PaddleOCR over ONNX -- in-process, offline-capable, and already in
the tree -- with Tesseract as the fallback where system libraries are
acceptable and a service adapter (docTR or cloud) where they are not. The
engine crate's own `OcrBackend` trait overlaps this seam; before wiring,
one of the two traits should become the single entry point (proposal: the
`pdf-ocr` one, for the image dimensions in section 3) with a thin adapter
for the other, rather than growing a third.

Existing cloud adapters (Mistral, Google Vision, Textract, Azure Document
Intelligence) already implement the engine-side trait; they ride along
whatever the trait decision is, unchanged.

## 6. Open questions

1. **Already-searchable skip.** Should `auto` skip a `Full` page whose
   invisible layer densely covers the images (right words unknown, but
   present and aligned)? It saves the common re-run; it also trusts the
   layer detection deliberately distrusts. Needs a coverage rule and a
   corpus measurement before it becomes behaviour.
2. **Region rendering for `Partial`.** Whole-page render plus masked
   recognition, or per-region crops? Crops are cheaper to recognize but
   lose line context at their edges; masks keep context but pay full-page
   recognition. Measure both on captioned-figure pages.
3. **DPI and cost.** Recognition quality and latency both ride on render
   resolution. 300 DPI is the `pdf-ocr` default; the structured route
   should confirm it against 150/200 on the benchmark set rather than
   inherit it.
4. **Provenance on blocks.** `Block` has no "this came from OCR" flag, so
   a caller cannot weight OCR text below native text. Adding one is a
   public-API change with binding fallout in six languages -- wanted, but
   a separate decision from the wiring.
5. **Table/heading interplay.** OCR spans flow into the merge, so tables
   and headings over recognized text come out of the existing passes --
   which other teams own. The wiring must not tune those passes for OCR;
   mis-grouped OCR paragraphs are evidence for their owners, not knobs
   for this work.
6. **Benchmarks before defaults.** The IoU suppression threshold, the
   confidence floor, and the engine default above are starting proposals.
   Each wants a number from the structured benchmark set, and the plan
   stays a plan until those runs exist.
7. **WASM.** In-process recognition (Paddle/ONNX, Tesseract system
   libraries) does not travel to `wasm32`. The structured route on WASM
   is `off` plus service-backed engines until someone proves otherwise.
