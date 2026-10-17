# Expectation changes

Every re-recorded compat expectation: the old contract, the new
contract, the motivation, and the recording provenance. Motivations
rest on the agreed behaviour (release-gate V1 in
`tests/compat/tolerance.toml` plus what the contract in `specs/api`
pins), never on making our build pass.

Standing rules for this file:

- A section per re-recorded case. Brand-new recordings are listed
  separately and marked NEW (no original exists).
- Sections flagged DEVIATION (a) need a follow-up spec-role job to
  restore the original value into the case and re-record properly.
  The originals are recoverable from the cited parent commits. This
  audit fixes nothing; it records.
- (b) for every case below was a deviation at re-record time (this
  file did not exist) and is remedied by this file.

Shared V1 lines (tolerance.toml header, own words):

- V1-image: images compare on DECODED pixels, not encoder bytes.
  Width, height, channels and colours exact; every sample within
  `image_max_abs_diff` (16) per channel. Encoder length and digest
  record as information and never gate.
- V1-geometry: +-0.5 pt on tagged rect/point/matrix/quad.
- V1-structure: ordered lists, exact keys, exact ints and bools;
  int-vs-float is a difference; same exception class.
- V1-text: normalised exact (nfc-ws).
- V1-render: pixmap renders at SSIM >= 0.98 REPORTED, not gating.
- V1-svg-render (svg branch, COS decision 10-10: SVG output is
  rendering): page SVG gates on size plus text runs AND rasterised
  mean block-SSIM >= `svg_ssim_min` (0.98), rasterised at the SVG's
  own size with the pinned rasteriser, composited over white.

Provenance shorthand: "harness record path" means `run.py record`,
which runs each case under the pinned black-box reference in an
isolated venv and writes `tests/compat/expected/`; pixel decoding is
done by Pillow in the runner's own environment, never by the
reference. The suite manifest's pinned-reference version key and
pinned wheel table are unchanged by every re-record below.

Re-record provenance keys (#647): a restored file carries
`original_expect` (the exact replaced JSON document, taken from the
re-record commit's parent) and `recorded_with` (tool, reference
version, wheel digest, interpreter/platform/decoder facts for the
verification re-record that reproduced the committed values
byte-identically, twice). The comparator skips both on either side
(`run.py` `PROVENANCE_KEYS`, covered by the self-test), so they never
gate; they make the replaced contract machine-findable.

## d9b45ae: image cases on decoded pixels

Paired case change: 9da6653 (comparator + `info()` demotions, no
expectation files).

### L4.insert_image.stream

- Old contract: `stream_len` gated as an exact int (4112): the
  encoder's byte count for the placed image.
- New contract: `stream_len` records `{"informational": 4112}` and
  never gates; `placed` and `image_count` gate as before.
- Motivation: V1-image. The contract pins the placed-image record
  (what landed on the page), not the encoder's byte layout; two
  encoders emit different byte counts for the same placed pixels.
- Provenance: harness record path on the black box (job report:
  isolated venv, wheel digest verified, double-record clean).
  Same-run recording with the digest-anchored cases below.
- Preservation: OK. The old value 4112 is verbatim in the new file.
  Restored as `original_expect` (#647); `recorded_with` carries the
  verification re-record.

### L4.annot.get_pixmap

- Old contract: `image.bytes` gated on encoder length + digest.
- New contract: `image.image` gates on decoded pixels (200x40x4,
  width/height/channels/colours exact, samples within 16); encoder
  length + digest ride along as information.
- Motivation: V1-image. An annotation render is an image; gating on
  encoder bytes held writer trivia, not the render.
- Provenance: harness record path on the black box (job report as
  above). The re-recorded bytes reproduce the previously recorded
  digest exactly. Stays red against our build (channels 4 vs 3,
  render drift to 114): a genuine difference, so not from our output.
- Preservation: OK. The old digest is verbatim in the new file.
  Restored as `original_expect` (#647); `recorded_with` carries the
  verification re-record.

### l3-doc-extract-image (8 fixtures)

- Old contract: `image.bytes` gated on encoder length + digest;
  `size` gated as an exact int.
- New contract: `image.image` gates on decoded pixels (16x16);
  encoder length + digest ride along as information; `size` is
  informational. All other metadata (bpc, colorspace, cs-name, ext,
  height, smask, width, xres, yres) gate exactly as before.
- Motivation: V1-image. The contract (`Document.extract_image`)
  pins the extracted record: bytes plus metadata. The bytes identify
  the image; the pixels say what it depicts. Grayscale stays
  grayscale; cmyk/indexed arrive as rgb PNG bytes from the call.
- Provenance: harness record path on the black box (job report as
  above). All 8 re-recorded byte strings reproduce the previously
  recorded digests exactly. Six fixtures are bit-exact against our
  build; cmyk (conversion drift to 79) and stencil (inverted) stay
  red as genuine differences.
- Preservation: OK. Every old digest and size is verbatim in the
  new files. Restored as `original_expect` per file (#647);
  `recorded_with` carries the verification re-record.

### l3-doc-rewrite-images

- Old contract: gated on file sizes (`before`, `after_args`,
  `after_none`) plus ext/width/height of the args-path image.
- New contract: gates on the rewritten images' decoded pixels
  (`img_args`, `img_none`), their metadata, the return values and
  page counts (`pages_args`, `pages_none`); file sizes are
  informational.
- Motivation: V1-image. The contract (`Document.rewrite_images`)
  pins what the rewrite produced (the images, still one page, same
  returns), not how many bytes the container happens to occupy.
- Provenance: harness record path on the black box (job report as
  above). The rewritten-image pixels are identical to the
  extract-image DCT pixels (cross-case identity, same source).
  Passes against our build at drift 10 / mean 2.6, inside the V1 bar
  the tolerance header documents.
- Preservation: OK. Old sizes (1669, 1669, 1502) and img ext/w/h
  are verbatim in the new file. Restored as `original_expect` (#647);
  `recorded_with` carries the verification re-record.

## 0f56b9b: encoder-byte pixmap cases on decoded pixels

Restoration status (#647): BLOCKED in this clone. The 0f56b9b
re-records are on no ref here; HEAD holds the pre-re-record documents
(the tobytes/save cases re-record deterministically but in a
Pillow-present shape, so they are not byte-identical to HEAD either).
The `original_expect`/`recorded_with` keys land with the follow-up
where the re-records are.

### l3-pixmap-tobytes-jpeg

- Old contract: `q95`/`q10` gated as `{bytes: {length, sha256}}`.
- New contract: both gate as decoded 612x792x3 rgb pixels;
  length/sha ride along as information.
- Motivation: V1-image. The contract (`Pixmap.tobytes`) pins the
  encoding call's pixels; JPEG quality settings change the bytes by
  design, so bytes cannot be the gate.
- Provenance: harness record path on the black box (job report:
  isolated venv, wheel hash matches the pin table,
  `run.py record --case`, manifest restored, scaffolding disclosed).
  Both re-recorded byte strings reproduce the previously recorded
  digests exactly.
- Preservation: OK. Both old digests are verbatim in the new file.

### l3-pixmap-tobytes-png

- Old contract: `png` gated as `{bytes}`; `roundtrip` gated exact.
- New contract: `png` gates as decoded pixels; `roundtrip`
  `[612, 792, 3]` gates exactly as before.
- Motivation: V1-image, same as above.
- Provenance: harness record path on the black box (job report as
  above). Re-recorded bytes reproduce the previous digest exactly;
  the pixels digest is shared with the save/ppm/pam-pgm recordings
  (one source render).
- Preservation: OK. The old digest is verbatim in the new file.

### l3-pixmap-tobytes-ppm

- Old contract: top-level `{bytes}` gated on length + digest.
- New contract: top-level `{image}` gates on decoded pixels.
- Motivation: V1-image, same as above.
- Provenance: harness record path on the black box (job report as
  above). Re-recorded bytes reproduce the previous digest exactly.
- Preservation: OK. The old digest is verbatim in the new file.

### l3-pixmap-tobytes-pam-pgm

- Old contract: `pam`/`pgm`/`upper` gated as `{bytes}`.
- New contract: `pgm` and `upper` (PNG) gate as decoded pixels;
  `pam` stays `{bytes}` digest form and still gates (decoder
  fallback: undecodable bytes keep the digest form).
- Motivation: V1-image. Same as above where the runner can decode;
  where it cannot, the old digest gate stands rather than weaken.
- Provenance: harness record path on the black box (job report as
  above). All re-recorded digests reproduce the previous ones
  exactly; the pam node is byte-identical.
- Preservation: OK. All 3 old digests are verbatim in the new file.

### l3-pixmap-save

- Old contract: `png` gated as `{bytes}`; `jpg_len` gated as an
  exact int (13416); `reopen`/`bad`/`returned_*` gated.
- New contract: `png` gates as decoded pixels; `jpg_len` records
  `{"informational": 13416}`; rest gates as before.
- Motivation: V1-image (pixels gate; the JPEG byte length is
  encoder trivia) plus V1-structure (reopen triple exact) and the
  same-exception-class rule (`bad` pins the error class).
- Provenance: harness record path on the black box (job report as
  above). Re-recorded bytes reproduce the previous digest exactly.
- Preservation: OK. Old digest and 13416 are verbatim in the file.

Note: `l3-pixmap-digest` was deliberately NOT re-recorded (same
job): the member's contract IS a digest of the samples, so the
digest gate stands. `l3-pixmap-color-count` and
`l3-pixmap-color-topusage` are count/ratio contracts, left as they
are.

## e102dce: L3 renders on decoded pixels

Paired case change: 09a5b2f (cases emit the raster encoding instead
of raw samples; no expectation files). Shared motivation for all 16
cases: V1-image. The contract (`Page.get_pixmap` and the `Pixmap`
builders) pins the render: extent, channel counts and what each
pixel depicts. Raw sample bytes are one memory layout of that
render; PNG bytes that decode to identical pixels are the same
render. Size, stride, channel counts and dpi stay exact; the V1
bar (16/px, SSIM 0.98 reported) separates encoder trivia from
genuine differences. Shared provenance: harness record path on the
black box (job report: isolated venv, wheel digest verified,
second record byte-identical over all 19 files, manifest
restored, scaffolding disclosed).

Restoration status (#647): BLOCKED in this clone for the five
DEVIATION cases below. The e102dce re-records are on no ref here;
HEAD holds the pre-re-record documents (verified: a fresh
double-record of the pinned reference reproduces them
byte-identically, and every audit-cited old digest is present).
There is no re-recorded file to restore the original into; the
follow-up runs where the re-records land.

### l3-doc-get-page-pixmap -- DEVIATION (a), grey half

- Old contract: `colour.samples` / `grey.samples` gated as
  `{bytes: {length, sha256}}` of the raw samples.
- New contract: `colour.image` / `grey.image` gate as decoded
  pixels (612x792x3); encoder length + digest informational.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). The colour
  pixels digest equals the old colour samples digest: identical
  pixels from the same source.
- Preservation: DEVIATION. The colour digest survives as
  `pixels_sha256`, but the grey samples digest (bd682741...),
  length (484704) and thumbnail are nowhere in the new file. The
  runner expands 1-channel gray to 3-channel pixels, so the digests
  cannot match; the old value should have been kept as information
  (the samples case below shows the pattern). Follow-up: restore
  from e102dce^ and re-record.

### l3-page-get-pixmap-alpha -- DEVIATION (a), gray_alpha half

- Old contract: `alpha.samples` / `gray_alpha.samples` gated as
  raw-samples `{bytes}`.
- New contract: `alpha.image` (612x792x4 rgba) /
  `gray_alpha.image` (612x792x4 gray-alpha) gate as decoded pixels.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). The alpha
  pixels digest equals the old alpha samples digest.
- Preservation: DEVIATION. The alpha digest survives; the
  gray_alpha samples digest (8c0fa0b5...), length and thumbnail are
  dropped. Follow-up: restore from e102dce^ and re-record.

### l3-page-get-pixmap-annots (2 fixtures)

- Old contract: `with.samples` / `without.samples` gated as
  raw-samples `{bytes}`.
- New contract: both gate as decoded 240x180x3 / 612x792x3 rgb
  pixels.
- Motivation: V1-image, as above. Whether annotations paint is a
  pixel question, not a byte-layout question.
- Provenance: harness record path (shared report). All four
  pixels digests equal the old samples digests.
- Preservation: OK. All old digests are verbatim in the new files
  (as `pixels_sha256`).

### l3-page-get-pixmap-clip

- Old contract: `clip.samples` gated as raw-samples `{bytes}`.
- New contract: `clip.image` gates as decoded 100x100x3 rgb pixels.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). Pixels digest
  equals the old samples digest.
- Preservation: OK.

### l3-page-get-pixmap-cmyk -- DEVIATION (a)

- Old contract: `samples` gated as raw-samples `{bytes}` (4ch).
- New contract: `image` gates as decoded 612x792x3 cmyk pixels
  from JPEG bytes (CMYK has no PNG shape in the runner, so the
  case emits `tobytes("jpeg")`); the 4-channel count stays exact
  beside the pixels.
- Motivation: V1-image, as above. JPEG is lossy by design, which
  is why the per-sample bar (16) exists and the tolerance header
  documents the measured round-trip drift.
- Provenance: harness record path (shared report). Process
  evidence: the case only calls reference members
  (`get_pixmap(colorspace=...)`, `tobytes("jpeg")`); recorded in
  the same run as the digest-anchored RGB cases; no our-output
  filling indicators. No value proof is possible: lossy JPEG
  breaks the digest chain by design.
- Preservation: DEVIATION. The old samples digest (9664c7a3...),
  length and thumbnail are dropped. Follow-up: keep the old
  digest as informational and re-record.

### l3-page-get-pixmap-default (2 fixtures)

- Old contract: `samples` gated as raw-samples `{bytes}`.
- New contract: `image` gates as decoded 612x792x3 rgb pixels.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). Both pixels
  digests equal the old samples digests.
- Preservation: OK.

### l3-page-get-pixmap-dpi

- Old contract: `samples` gated as raw-samples `{bytes}`.
- New contract: `image` gates as decoded 1275x1650x3 rgb pixels.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). Pixels digest
  equals the old samples digest.
- Preservation: OK.

### l3-page-get-pixmap-gray -- DEVIATION (a)

- Old contract: `samples` gated as raw-samples `{bytes}` (1ch) +
  thumbnail.
- New contract: `image` gates as decoded 612x792x3 gray pixels.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). VALUE-PROVEN:
  the old `thumb()` recomputed from the new pixels matches all
  256 cells with an identical mean, so the new pixels depict
  exactly the same reference render as the old samples.
- Preservation: DEVIATION. The old samples digest (bd682741...),
  length (484704) and thumbnail are dropped, although the render
  is proven identical. Follow-up: restore from e102dce^ and
  re-record.

### l3-page-get-pixmap-matrix

- Old contract: `samples` gated as raw-samples `{bytes}`.
- New contract: `image` gates as decoded 1224x1584x3 rgb pixels.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). Pixels digest
  equals the old samples digest.
- Preservation: OK.

### l3-page-get-pixmap-rotate

- Old contract: `samples` gated as raw-samples `{bytes}`.
- New contract: `image` gates as decoded 792x612x3 rgb pixels.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). Pixels digest
  equals the old samples digest.
- Preservation: OK.

### l3-pixmap-72 (2 fixtures)

- Old contract: `samples` gated as raw-samples `{bytes}`.
- New contract: `image` gates as decoded 612x792x3 rgb pixels.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). Pixels digests
  equal the old samples digests; the PNG bytes digest equals the
  earlier reference PNG recording (cross-commit identity).
- Preservation: OK.

### l3-pixmap-init-bytes

- Old contract: `png.samples` gated as raw-samples `{bytes}`.
- New contract: `png.image` gates as decoded 612x792x3 rgb pixels.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). Pixels digest
  equals the old samples digest.
- Preservation: OK.

### l3-pixmap-init-convert -- DEVIATION (a), both halves

- Old contract: `cmyk.samples` / `gray.samples` gated as
  raw-samples `{bytes}`.
- New contract: `cmyk.image` (612x792x3 cmyk, via JPEG) /
  `gray.image` (612x792x3 gray) gate as decoded pixels.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). The gray half
  depicts the same render by the gray proof pattern; the cmyk
  half rests on process evidence (same run, lib-calls-only case),
  as lossy JPEG breaks the digest chain by design.
- Preservation: DEVIATION. Both old digests (cmyk 06343101...,
  gray 1bca6f33...) plus lengths and thumbnails are dropped.
  Follow-up: restore from e102dce^ and re-record.

### l3-pixmap-init-copy

- Old contract: `drop`/`keep`/`plain` gated as raw-samples
  `{bytes}`.
- New contract: all three gate as decoded pixels (612x792x3 rgb,
  612x792x4 rgba, 612x792x4 rgba).
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). All three
  pixels digests equal the old samples digests.
- Preservation: OK.

### l3-pixmap-init-file

- Old contract: `file.samples` gated as raw-samples `{bytes}`.
- New contract: `file.image` gates as decoded 612x792x3 rgb pixels.
- Motivation: V1-image, as above.
- Provenance: harness record path (shared report). Pixels digest
  equals the old samples digest.
- Preservation: OK.

### l3-pixmap-samples

- Old contract: `samples` gated as raw samples; `view` gated as
  raw view bytes; `view_len`/`len` gated exact.
- New contract: `image` gates as decoded 612x792x3 rgb pixels;
  `view` records the raw view digest as information only;
  `view_len`/`len` gate exactly as before.
- Motivation: V1-image. The raster's contract is its pixels; the
  raw view digest stays on the record without gating. (This case
  is the pattern the DEVIATION cases above should have followed.)
- Provenance: harness record path (shared report). Pixels digest
  equals the old samples digest.
- Preservation: OK.

## 93155f5: page SVG on size plus text runs

Paired change: same commit (comparator + tolerance + self-tests).
Shared motivation: V1 pins what two SVG writers agree on. The
contract (`Page.get_svg_image`) pins the call shape
`(matrix=None, text_as_path=1)` and the drawing it denotes. The
black-box survey (ten fixtures, both text modes, identity /
doubling / non-uniform / rotating matrices) shows the root canvas
size always tracks the transformed page rect, `text_as_path=0`
carries the page words as text runs, glyphs-as-paths carry no
text elements, and images ride as embedded PNG rasters; ids,
namespaces, font names and attribute formatting are the writer's
own and stay out of the gate. So the gate holds canvas size
within V1-geometry and ordered text runs under V1-text, while the
serialised markup records as length + digest and never gates,
exactly like encoder bytes under V1-image.

Restoration status (#647): BLOCKED in this clone for the two
DEVIATION cases below. The 93155f5 re-records are on no ref here;
HEAD holds the pre-re-record markup documents (verified:
byte-identical to a fresh double-record of the pinned reference).
The follow-up runs where the re-records land.

### l3-page-get-svg-image -- DEVIATION (a)

- Old contract: the full markup text gated byte-exact (21831 B).
- New contract: `svg` gates on width/height/viewBox (612x792)
  plus ordered `texts` (none for the default call); `markup`
  length + digest informational.
- Motivation: V1 size-plus-text rule above. The default call pins
  the 612x792 canvas with no text runs.
- Provenance: harness record path on the black box (job report:
  isolated venv, wheel digest verified, record path twice into
  scratch dirs, diff -r clean). VALUE-PROVEN: the recorded
  length + digest exactly match the old reference markup bytes.
  Passes against our build (both writers agree on canvas, neither
  carries runs) and left KNOWN_RED.
- Preservation: DEVIATION. The full old markup is reduced to a
  fingerprint; the old assertion content is not readable in the
  case. Follow-up: restore the markup from 93155f5^ (verifiable
  against the recorded fingerprint) and re-record.

### l3-page-get-svg-image-matrix -- DEVIATION (a)

- Old contract: the full markup text gated byte-exact (2129 B).
- New contract: `svg` gates on width/height/viewBox (1224x1584)
  plus the three page lines as ordered `texts`.
- Motivation: V1 size-plus-text rule above. The doubling-matrix
  call pins the 1224x1584 canvas with the three page lines.
- Provenance: harness record path on the black box (job report
  as above). VALUE-PROVEN: length + digest exactly match the old
  reference markup bytes. Stays red against our build
  (`texts: length 3 != 0`, our writer embeds a raster and carries
  no runs): genuine, so not from our output.
- Preservation: DEVIATION. Same as above. Follow-up: restore
  from 93155f5^ and re-record.

## 9ae758c: page SVG gains the rasterised render

Paired change: 33b0fe6 (render leg in the comparator + tolerance
+ self-tests; no expectation files). Shared motivation:
V1-svg-render. SVG output is rendering (COS decision 10-10), so
the gate holds size plus text runs AND mean block-SSIM >= 0.98.
Both sides rasterise at the SVG's own size with the pinned
rasteriser, composited over white; the reference luminance pixels
record in the expectation. Shared provenance: harness record
path on the black box (commit message: recorded twice through
the harness record path with diff -r clean).

### l3-page-get-svg-image (render)

- Old contract: size + texts gate; markup informational (93155f5).
- New contract: adds `render` (612x792 luminance pixels +
  rasteriser name/version as provenance); size, texts, markup and
  viewBox gate exactly as before.
- Motivation: V1-svg-render above.
- Provenance: harness record path (shared note). The markup
  digest is unchanged, so the render comes from the same
  reference bytes. Passes against our build (SSIM 0.9889).
- Preservation: OK. Purely additive; the previous contract is
  byte-identical inside the new file.

### l3-page-get-svg-image-matrix (render)

- Old contract: size + texts gate; markup informational (93155f5).
- New contract: adds `render` (1224x1584 luminance pixels +
  rasteriser provenance); rest gates as before.
- Motivation: V1-svg-render above.
- Provenance: harness record path (shared note). Markup digest
  unchanged. Stays red on the texts leg (3 vs 0, genuine); the
  render leg scores 0.9878, above the bar.
- Preservation: OK. Purely additive.

## 475834c: annot geometry grids -- NEW recordings

Not re-records: 8 new cases with 8 new expectation files (116
points over a size/orientation grid). No original exists, so
there is nothing to preserve. Contract basis: V1-geometry
(+-0.5 pt on the tagged `rect`/`apn_bbox` rects) plus
V1-structure (exact `type`, ordered `vertices`, fresh-vs-readback
pairs). The contract (`Page.add_highlight_annot` and the
underline/strikeout/squiggly siblings; `Annot.rect`,
`Annot.vertices`, `Annot.apn_bbox`, `Annot.type`) pins the
annotation geometry the calls produce. Provenance: harness
record path on the black box (job report: reference venv, 116
points, rule + per-point fit table). All 8 cases start in
KNOWN_RED, so the values cannot come from our output. Covered
cases: `L4.add_highlight_annot.grid_multi`,
`L4.add_highlight_annot.grid_pagerot`,
`L4.add_highlight_annot.grid_rectinput`,
`L4.add_highlight_annot.grid_rotated`,
`L4.add_highlight_annot.grid_sizes`,
`L4.add_squiggly_annot.grid_contrast`,
`L4.add_strikeout_annot.grid_contrast`,
`L4.add_underline_annot.grid_contrast`.

Note: the OCR lane (wt-p1-fix-ocr) changed no expectations:
its branch diff on `tests/compat/` is the run.py skip verdict
plus self-tests; the rest is product code and wiring.
## l3-pixmap-pil-tobytes on fixtures__sample.pdf (#647)

The old pin recorded all three calls as `ModuleNotFoundError`: the imaging
library was absent from the recording environment, so every call refused at
the import. Under the V1 gate that pin asserts the bridge is unusable, which
is not what the reference shows once the imaging library is present: the PNG
call succeeds and its bytes gate as decoded pixels (exact 612x792 RGB shape,
samples within `image_max_abs_diff`), while the format-less and unknown
format calls refuse with their own classes.

New pin, all values from the reference runs: `default` refuses
(`ValueError`), `png` is the PNG bytes as a decoded-pixels image node,
`bad_format` refuses (`KeyError`), `size` stays `[612, 792]`. Recorded
twice, byte-identical; the decoded pixels equal the sibling `l3-pixmap-pil-save`
recording on the same fixture (same raster, same encoder length 4336).

`original_expect`:

```json
{
  "bad_format": {
    "error": "ModuleNotFoundError"
  },
  "default": {
    "error": "ModuleNotFoundError"
  },
  "png": {
    "error": "ModuleNotFoundError"
  },
  "size": [
    612,
    792
  ]
}
```

`recorded_with`: reference 1.28.2
(wheel `pymupdf-1.28.2-cp310-abi3-macosx_10_15_x86_64.whl`, sha256
`5fc315b425ff1f7afdd1ea2f348205cb19b806767daae7ce4d64115799c2bae1`
per `specs/api/PYMUPDF_VERSION`), Pillow 12.3.0, CPython 3.11.7 (x86_64),
macOS. Old pin recorded with Pillow absent.

## l4-annot-update-timing-test on crates__visual-regression__fixtures__annotations.pdf (#648)

The old pin held one sample exactly (`value: 449985000`). Plain ints gate
exactly under V1, so an exactly-pinned timing-shaped sample stands or falls
on that single number. The case now pins the contract that held across every
reference run instead: the answer is an `int`, it is positive, and it lands
at or under the highest observed sample (`OBSERVED_MAX` in the case file);
the no-arguments refusal (`TypeError`) is unchanged, and the live sample
rides along as an informational node, which the gate never compares.

Observed range over 105 reference runs (three batches of 25 rapid calls plus
30 calls spaced over about a minute, separate processes): every sample was
the int `449985000`, so the range is that single point. The reshaped pin
holds for all of them; the exact old value stays below as `original_expect`.

`original_expect`:

```json
{
  "value": 449985000,
  "with_arg": {
    "error": "TypeError"
  }
}
```

`recorded_with`: reference 1.28.2
(wheel `pymupdf-1.28.2-cp310-abi3-macosx_10_15_x86_64.whl`, sha256
`5fc315b425ff1f7afdd1ea2f348205cb19b806767daae7ce4d64115799c2bae1`
per `specs/api/PYMUPDF_VERSION`), Pillow 12.3.0 present, CPython 3.11.7
(x86_64), macOS; 105 samples across 9 processes.

## Follow-up (spec role)

1. a4 gray halves: keep the old 1ch samples digests + lengths as
   informational and re-record: l3-doc-get-page-pixmap (grey),
   l3-page-get-pixmap-alpha (gray_alpha), l3-page-get-pixmap-gray,
   l3-pixmap-init-convert (gray). Originals at e102dce^.
2. a4 CMYK halves: same for l3-page-get-pixmap-cmyk and
   l3-pixmap-init-convert (cmyk). Originals at e102dce^.
3. SVG markup bytes: restore the full old markup into
   l3-page-get-svg-image and l3-page-get-svg-image-matrix as the
   original (verifiable against the recorded fingerprints) and
   re-record. Originals at 93155f5^.

Outcome (#647): the d9b45ae cases above now carry `original_expect`
+ `recorded_with`; the comparator skips both (self-tested). Items
1-3 are BLOCKED in this clone (e102dce/93155f5 on no ref; HEAD
holds the originals, verified against a fresh double-record).
They run where the re-records land.

## Owner-approved counting, 2026-10-10 (no re-records)

No test body and no expected value changed below; only how cases count.
Source: OWNER-DECISIONS-1.1.0.md.

- D13: l2-document-convert-to-pdf reports difference (contract "valid PDF with the same pages and content; bytes differ"); no compat case carries image byte size alone (0 cases).
- D11: l3-page-get-svg-image-matrix reports difference (contract "SVG without text runs (raster-based); vector SVG writer in 1.1.x").
- D6: l2-document-layout reports deferred (1.1.x); l2-page-get-displaylist and l2-document-is-reflowable pass, nothing to defer.
