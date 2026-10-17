# PDF/A fidelity implementation and measurement protocol

This follow-up improves the shipping `convert_bytes` / `convert_document` route.
Final measured totals, report hashes and residual cases accompany the pull
request and tracker handoff; this document describes the implementation and
reproduction contract.
It does not claim zero visual deviations or measured parity with Nutrient or
Apryse. The reference is clean AGPL master `2e321f018`; automatic licence
markings had already been removed by `f9ae3a1d7`. Earlier watermark-based
explanations do not apply to this comparison.

## Changes

- Subset signed TrueType programs by removing their now-invalid DSIG table,
  rebuilding SFNT directory fields and checksums, and preserving embedding and
  subsetting restrictions. A document-wide strict text census covers page
  contents, forms, patterns, appearances and Type3 charprocs. Ambiguous content
  or editable forms retain the full simple-font repertoire.
- Compact TrueType and simple/Identity-CID CFF programs without renumbering
  retained glyphs. Keep selected outlines, widths, names, instructions and
  composite components. CFF charstrings are replaced in place only after every
  retained outline and width agrees; offsets, private data and subroutines are
  unchanged. CFF savings are compressed bytes, not shorter decoded programs.
  Unsupported CMaps, sharing, variable/color programs and uncertain cases fall
  back to retaining the original program.
- Prefer an exact installed font over a guessed family and classify Century
  Gothic as sans serif. For unembedded substitutes on complete page streams,
  fit glyph widths explicitly while preserving character/word spacing, kerning
  and saved text state. Changed codes/resources and ambiguous states decline
  adjustment. Already embedded fonts with contradictory widths keep the existing
  repair route: width deltas alone were insufficient to preserve word boundaries.
- Deduplicate matching ICCBased streams, including their dictionaries and
  compression policy, and correct the gray profile's D50 white point. Preserve
  original output intents. Avoid inventing an RGB output device for opaque
  CalRGB registration-color content; transparency and inline-image cases retain the
  established output-intent path.
- Test repeated blank-page conversion for any injected drawing operations.
  Refresh the generated conversion fixture and stale licence documentation.
  All committed reference PNGs remain unchanged.
- Preserve image soft masks for PDF/A-2 and later. The old cleanup applied
  PDF/A-1's prohibition to every part and turned transparent image backgrounds
  black. A generated three-pixel opacity ramp survives two full PDF/A-2
  conversions with its mask samples intact; PDF/A-1 retains its existing policy.
  Keep the characterized blending space, backdrop and isolation flags of a
  soft-mask Form instead of forcing it to the RGB output intent.

The registration-color case `001_001029.pdf` had white panels turning black in
MuPDF with the baseline's added output intent. Removing that intent restores
white panels without rewriting the tint function or image pixels. Poppler did
not exhibit the baseline's black panels. This is a measured reader interaction,
not a claim that every viewer rendered black.

## Measurement contract

The original `compare.mjs` remains unchanged: veraPDF PDF/A-2b validation,
all-document MuPDF word/character retention, grayscale MAE on the first three
pages at 50 dpi, and serialized file size. A visual flag is document mean MAE
above 0.02. Failed/missing measurements stay visible. Size totals alone do not
establish preservation, and a word-score threshold is not a no-regression proof.

The original 300 and existing 500 sets are reported separately. Forty pinned
PDF.js inputs were selected before evaluation and checked against prior input
hashes. Their first candidate exposed an embedded-width case, so the final
results call these a confirmation set. Twenty additional inputs were frozen
after the font iteration, excluding prior names and exact hashes. The later
color and soft-mask corrections were motivated by the original 300, and the
same 20 are rerun; they are not described as an untouched final holdout.
The adjacent manifests identify all 60 inputs. No corpus PDFs, extracted text
or rendered corpus pages are committed.

Supplemental checks use the fixed 50-file SSIM gate, all pages at 72 dpi with RGB
MAE and grayscale SSIM, approximate ordered word retention, and the original
visual tail plus two font cases and the discovered CMYK registration case at
150 dpi in both MuPDF and Poppler. Dimensions
must match; images are never resized to improve a score. Supplemental measures
do not replace the original four axes.

## Interpretation and limits

The converter cannot reconstruct an absent original font from its family name.
Some existing repairs and reader-dependent source constructs still differ.
There is no new lossy image encoding, rasterization, or deletion of page content
in this iteration. Unknown mappings retain their font programs.

Ghostscript's historical approximately 98% ratio used policy 1 without an
explicit PDF/A definition file. Its official documentation describes the ICC
profile/definition-file setup and explains that policy 1 can elide unsupported
features. The exact recipe is retained for comparison; file size must be read
alongside conformance, text retention and rendering. Nutrient/Apryse SDKs are
not measured, and no commercial SDK access was purchased.

References: [PDF Association on PDF/A-2 transparency](https://pdfa.org/pdfa-faq/),
[Ghostscript PDF/A creation](https://ghostscript.readthedocs.io/en/latest/VectorDevices.html#creating-a-pdf-a-document),
[Microsoft DSIG table](https://learn.microsoft.com/en-us/typography/opentype/spec/dsig),
[W3C guidance on stale font signatures](https://www.w3.org/submissions/WOFF/),
[MuPDF substitute fonts](https://github.com/ArtifexSoftware/mupdf/blob/master/source/pdf/pdf-font.c).

## Reproduction

Use separate build directories for the clean baseline and the candidate. Sharing
one Cargo release directory across worktrees can reuse stale workspace artifacts.
Freeze the two `pdfa_convert_real` executables and record their SHA-256 before
conversion. Build each with `cargo build -p pdf-manip --release --example
pdfa_convert_real`; no alternate converter or external repair is used.

The original input list is `benchmarks/pdfa/govdocs_sample_300.txt`; acquire the
GovDocs files with `scripts/corpus-download-govdocs.sh` and check their hashes
against `pdfa_fidelity_inputs_300.json` in this directory. The 500 and SSIM sets are pinned by
`corpus/GATE_CORPUS_MANIFEST.json` and `corpus/SSIM_GATE_MANIFEST.json`. The adjacent
40/20 manifests include the selected PDF.js sources and hashes. All inputs stay
local. Set `PDFA_SOURCE`, `PDFA_LIST`, `PDFA_BEFORE`, `PDFA_AFTER`, `PDFA_BASE_BIN`
and `PDFA_NEW_BIN` to the selected input directory/list, two empty output
directories, and frozen binaries, respectively, then run from the repository:

```sh
node benchmarks/pdfa/reproduce/compare.mjs \
  --corpus-dir "$PDFA_SOURCE" --list "$PDFA_LIST" \
  --converter "$PDFA_BASE_BIN {in} {out}" --name baseline --workdir "$PDFA_BEFORE"
node benchmarks/pdfa/reproduce/compare.mjs \
  --corpus-dir "$PDFA_SOURCE" --list "$PDFA_LIST" \
  --converter "$PDFA_NEW_BIN {in} {out}" --name candidate --workdir "$PDFA_AFTER"
python crates/pdf-manip/examples/pdfa_fidelity_summary.py \
  --before "$PDFA_BEFORE/compare.json" --after "$PDFA_AFTER/compare.json" \
  --out "$PDFA_AFTER/summary.json"
python crates/pdf-manip/examples/pdfa_round2_eval.py \
  --source "$PDFA_SOURCE" --before "$PDFA_BEFORE" --after "$PDFA_AFTER" \
  --list "$PDFA_LIST" --dpi 72 --workers 4 --out "$PDFA_AFTER/all-pages.json"
```

Run the four-axis commands separately for 300/500/40/20; the all-page supplement
covers the original 300 only. For the dual-renderer supplement, invoke
`pdfa_fidelity_review.py` with the same source/before/after arguments, `--dpi 150`,
and the fixed visual-tail list from the measurement summary. The SSIM check is
`pdfa_size_verify.py ssim --corpus <pinned-50> --before <baseline-binary>
--after <candidate-binary> --out <empty-directory>`.

The optional all-page reuse mode accepts earlier measurements only after source
and baseline file hashes match and the entire candidate Root/Info object graph
is identical, including object identifiers, metadata, filters and decoded stream
bytes. Only physical stream Length is ignored. Parse uncertainty or a changed
object requires new rendering. This avoids rerendering identical outputs; it
never copies a measurement merely because the changed source file seems unrelated.
