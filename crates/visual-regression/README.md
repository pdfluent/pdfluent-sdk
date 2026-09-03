# Visual regression

This suite compares the current public `pdf-engine::PdfDocument` renderer with
committed images of its own output. It guards change; it does not establish
agreement with an independent reference renderer or prove PDF conformance.

All 42 single-page PDFs are created by Rust in `src/fixtures.rs`, at 240 by 180
points before rotation/cropping. There are no input PDFs. Object allocation,
trailer IDs, font outlines, image pixels, encryption inputs and metadata are
fixed. The TrueType font is an original polygon A/B/C face generated in
`src/fonts.rs`; embedded CFF data comes from the workspace's
`pdf-standard-fonts` crate, with its existing font licensing. The JPEG is encoded
from generated pixels in Rust. No system font or external command is used.

`cargo test -p visual-regression` regenerates every document in memory and
asserts byte equality with `fixtures/*.pdf`. Every page is opened with an empty
password and rendered at 96 dpi with annotations enabled, the default quality
pipeline and an opaque white background. Pages are saved as lossless RGBA8 PNG
with the `png` crate's best compression. Fixtures plus baselines must total less
than 8 MiB; the comparison test asserts this after any baseline update.

## Tolerances

`tolerances.toml` is the only tolerance configuration. `channel = 2` means that
a pixel differs if **any** RGBA channel changes by more than 2 of 255.
`fraction = 0.0` permits zero differing pixels; fractions are numbers in [0, 1],
not percentages. Dimension changes always fail, regardless of the fraction.
Unknown fixture names and invalid fractions fail. There are no overrides.
Every future non-default fixture entry must include a comment explaining the
measured cause, rather than merely suppressing a failure.

The tests also shift a real rendered page by one pixel and separately change
one channel by 3. Both must fail. Additional checks cover the allowed delta of
2, alpha changes, exact fraction boundaries, dimension mismatches, bounding
boxes and the three diagnostic PNGs.

## Update and review

```sh
# After deliberately changing generator source:
cargo run -p visual-regression --features generate-fixtures --bin generate

# First verify generator/render determinism, then update and compare:
cargo test -p visual-regression five_run_determinism -- --nocapture
UPDATE_VISUAL_BASELINE=1 cargo test -p visual-regression

# The exact CI command:
cargo test -p visual-regression --release
```

The update path writes baseline PNGs and then reads and compares them in the
same test. It removes obsolete PNGs so deliberate page-count changes can be
accepted. It never updates source PDFs implicitly. Without the environment
variable, a missing fixture, extra fixture, missing/extra baseline page,
unreadable PNG, failed render or unexpected page count fails the suite.

A pull request touching `baseline/` must explain **each changed image**.
Reviewers look at the images, not at the hashes. Inspect expected, actual and
diff together before accepting a change. The baseline deliberately preserves
current renderer behavior, including any current limitations.

## Read a failure

Pixel mismatches write these files under
`target/visual-regression/<fixture>-p<page>/` (page numbers start at one):

- `expected.png`: committed reference.
- `actual.png`: current rendered page.
- `diff.png`: changed pixels in red over a dimmed reference. Dimension changes
  use a canvas large enough for both images, marking unmatched pixels red.

The test reports the differing pixel count, percentage and inclusive
`[left, top, right, bottom]` bounding box, and both dimensions. It collects all
page mismatches before failing so one run shows their full extent. A missing
reference has only `actual.png`; there is no expected image to copy. Render
errors and inventory failures are reported explicitly instead of inventing
reference pixels. Previous diagnostics for successfully rendered pages are
cleared on each comparison. CI uploads the failure directory for 14 days and
does not allow test failure.

## Determinism

`five_run_determinism` renders the whole set three times in its own process,
then launches a fresh copy of the test executable with `RAYON_NUM_THREADS=1`,
then another uses the default thread count. The single-thread process also sets
`PDF_RENDER_THREADS=1`, because the rasterizer uses its own pool. All PDF and
PNG bytes and all inventories must match exactly, independent of pixel
tolerances. Temporary snapshots are retained on failure and removed on success.
The generator binary is gated behind `generate-fixtures` to keep tool and test
panic profiles from colliding in the engine's release library outputs.

RC4 revision 3 fills the unused half of its user-password record randomly in
the workspace encryption API. The fixture uses deterministic RC4-40 revision 2
instead. AES-256 revision 5 is assembled using fixed salts, keys and IVs solely
for test data; it must never be reused as a production encryption recipe.
No renderer determinism fix or tolerance was necessary for the initial set.
The conversion fixture preserves the default free-tier watermark. Run without
optional `PDFLUENT_LICENSE_FILE` / `PDFLUENT_LICENSE_KEY` settings; changing the
conversion license profile changes the generated document and fails equality.

## Fixture coverage

| Fixture | Rendering features |
|---|---|
| text-truetype | Embedded original TrueType A/B/C polygon outlines |
| text-cff | Embedded CFF Latin text; workspace standard-font program |
| text-standard14 | All 14 standard fonts using Type1 font dictionaries |
| text-type3 | Original Type3 glyph stream |
| text-render-modes | Text render modes 0–7: fill, stroke, invisible, and clipping |
| text-spacing | Character/word spacing, horizontal scaling, text rise, TJ adjustments |
| vector-fills | Nonzero versus even-odd fill and cubic curves |
| vector-joins | Miter, round and bevel line joins |
| vector-caps | Butt, round and projecting square line caps |
| vector-dashes | Dash arrays, nonzero phases and curved dashed strokes |
| clip-nested | Nested q/Q clips with nonzero and even-odd rules |
| vector-dash-caps | Dashed strokes with round and square caps and short gaps |
| vector-dash-transform | Nonuniformly transformed dashed cubic path and dash phase |
| shading-axial | Type 2 axial shading |
| shading-radial | Type 3 radial shading |
| shading-mesh | Type 4 free-form Gouraud triangle mesh |
| pattern-tiling | Colored tiling pattern |
| image-rgb | DeviceRGB Flate image |
| image-gray | DeviceGray Flate image |
| image-cmyk | DeviceCMYK Flate image |
| image-indexed | Indexed palette Flate image |
| image-icc | ICCBased sRGB image |
| image-dct | Rust-generated DCT JPEG image |
| image-smask | Image with grayscale SMask |
| image-stencil | One-bit stencil image mask |
| blend-multiply | Constant fill/stroke alpha and named blend mode |
| blend-screen | Constant fill/stroke alpha and named blend mode |
| blend-overlay | Constant fill/stroke alpha and named blend mode |
| blend-difference | Constant fill/stroke alpha and named blend mode |
| transparency-group | Isolated and knockout transparency group |
| page-rotate90 | Asymmetric colored paths and page rotation |
| page-rotate180 | Asymmetric colored paths and page rotation |
| page-rotate270 | Asymmetric colored paths and page rotation |
| page-crop | Offset CropBox smaller than MediaBox |
| forms-nested | Three levels of form XObjects with transforms |
| acroform-widget | AcroForm field and widget with explicit normal appearance |
| annotations | Stamp, square and free-text annotations with appearance streams |
| encrypted-rc4 | RC4-40 revision 2, empty user password |
| encrypted-aes256 | AES-256 revision 5, fixed test salts/IV, empty user password |
| watermark | pdf-manip text watermark applied to color-shapes |
| pdfa-converted | pdf-manip PDF/A conversion; pdf-compliance identification |
| color-shapes | Opaque asymmetric colored paths |

## API and repository integration limits

No requested rendering category is skipped. PDF/A conversion is implemented by
`pdf-manip::pdfa::convert_bytes` on this revision; `pdf-compliance` exposes
validation and identification, not a conversion API. The converted fixture uses
the actual conversion pipeline and checks PDF/A identification with
`pdf-compliance`. This visual suite does not claim independent PDF/A validation.

The internal crate is registered in `docs/licensing/boundary.toml` and the
capability register is regenerated using its existing tool. The following
constraints conflict with the current repository guards:

- `scripts/ci/corpus_herkomst.py` accepts generated PDF provenance only through
  its hard-coded `GEGENEREERD` mapping. Registering
  `crates/visual-regression/fixtures` with generator
  `crates/visual-regression/src/fixtures.rs` requires editing that protected
  script. The generator and byte-equality assertion establish provenance, but
  the existing document-registration guard cannot recognize it without that
  registration. No guard has been modified or bypassed.
- `scripts/ci/pr_code_stays_off_the_desktop.py` rejects unconditional
  `[self-hosted, xfa-fast]` jobs on `pull_request`. The workflow uses the runner
  and events required for this task; making it pass that guard requires a
  decision to change either the requested runner policy or the repository rule.

- The persistent-build guard requires its protected `EXPECTED_BUILD_JOBS`
  registration to increase from six to seven for this workflow. The workflow
  already invokes the required build-directory health check before testing.
- `scripts/ci/territories_do_not_overlap.py` rejects the required `codex/` branch
  for the CI and licensing paths. Its branch/ownership mechanism requires a
  reviewed change to `.claude/territories.toml`, which is also protected here.
- The DCO guard requires an identity-bearing `Signed-off-by` trailer, conflicting
  with the task's prohibition on names in commit messages. No trailer was added.

The publication attempt was rejected by the pre-push gate. It also encountered
an existing scanner issue: the connecting-script guard excludes every file when
an ancestor directory is named `.worktrees`. No hook or guard was bypassed.

The initial byte-identity evidence covers the local architecture and thread
counts; cross-architecture equality needs a runner measurement, not an assumed
tolerance. The PR records test timings, exact sizes and temporary mutation
results. All three renderer mutations are reverted before the final diff.

This crate supersedes `scripts/run-avrt.sh`, `scripts/avrt-report.sh` and
`avrt-config.json`; those files remain for removal in a separate change.

## Initial validation

All required formatting, strict crate linting, debug tests, release tests and
`cargo test --workspace --no-run` passed. The release suite took 4.397 seconds
wall-clock without compilation (3.50 seconds in the test harness). The 42 PDFs
occupy 109,167 bytes and the 42 PNGs 192,664 bytes: 301,831 bytes combined.

| Temporary renderer mutation | Failing pages | Examples |
|---|---:|---|
| Anti-aliasing disabled | 31 | text-truetype, vector-fills, vector-dashes |
| Red/blue fill channels swapped | 23 | text-truetype, blend-multiply, color-shapes |
| Dash arrays ignored | 3 | vector-dashes, vector-dash-caps, vector-dash-transform |

Each experiment ran the full debug suite and failed the visual test while the
other three tests passed. All mutations were reverted. The final debug and
release suites both passed the five-run byte-identity audit with no exceptions.
