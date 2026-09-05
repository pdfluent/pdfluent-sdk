## Summary

PDF/A output on the original 300-document tuning set falls from **544,915,414 to 301,546,650 bytes**, saving **243,368,764 bytes (44.66%)**. Output/input falls from **265.43% to 146.89%**. Conversion still uses `pdfa::convert_bytes` through `pdfa_convert_real`.

Fresh baseline: `master` at `4ed3cecbd`, PDF/A-2b, veraPDF 1.28.2, MuPDF 1.27.2. All 300 input names and sizes match the committed comparison; the report records their SHA-256 hashes. The historical output total was 185 bytes larger than this fresh baseline. Its conformance, retention and visual results reproduce exactly.

The ticket names three different measurement sets. The original 265%/36 measurement is `benchmarks/pdfa/govdocs_sample_300.txt` with `benchmarks/pdfa/reproduce/compare.mjs`; its visual metric is grayscale mean absolute pixel difference on the first three pages at 50 dpi, **not SSIM**. This report repeats that measurement and separately checks the 50 checksum-pinned files in `corpus/SSIM_GATE_MANIFEST.json` with the existing SSIM function and gate. The separate 500-file `GATE_CORPUS_MANIFEST.json` was fetched and checksum-verified, but its results are not substituted for the original 300.

Per-document bytes, attribution, hashes, quality results and the 50 SSIM comparisons are in `crates/pdf-manip/examples/pdfa_size_results.json`. The tool emits the full per-font/per-image detail locally; no corpus PDFs or font assets are added.

## Byte attribution before

Encoded stream payloads are exclusive categories. The final row includes dictionaries, numbers/arrays, stream/object delimiters, xref, trailer and other non-payload bytes; the columns sum exactly to file sizes. Unreachable stream payload is separate, not also counted as fonts/images. Input object-stream payload remains in its own category. One damaged input, `002_002400.pdf`, requires the shipping loader's repair; the tool flags this explicitly.

| Class | Input bytes | Baseline output bytes |
|---|---:|---:|
| Font programs | 16,683,537 | 298,913,056 |
| Images | 113,179,725 | 112,304,854 |
| Page/Form content | 36,802,046 | 54,493,928 |
| ToUnicode CMaps | 335,373 | 8,561,874 |
| Other streams | 3,638,264 | 6,538,089 |
| Unreachable stream payload | 1,842,146 | 5,398,341 |
| ICC profiles | 755,297 | 1,241,261 |
| XMP/metadata streams | 1,887,812 | 1,133,025 |
| Xref/object-stream payload | 17,133 | 0 |
| Object/xref/non-payload overhead | 30,150,885 | 56,330,986 |
| **Total** | **205,292,218** | **544,915,414** |

## Causes with code paths

The three largest increases over input are:

1. **Font programs: +282,229,519 bytes.** `pdfa_fonts.rs::embed_font_on_target` allocates an uncompressed program for each embedding target. Some repair paths, including `fix_symbolic_truetype_cmap`, also write plain font bytes. `pdfa.rs::convert_bytes` previously saved without final storage cleanup. Of the 298,913,056 font bytes, **284,624,394 are uncompressed**; compressing those payloads saves 119,922,700 bytes before deduplication. **146,955,775 bytes are exact duplicate font payloads with equal stream dictionaries**. These diagnostic savings overlap and must not be added. There are 3,305 baseline font streams. The worst ratio example, `000_000336.pdf`, has 70,597,081 font bytes in a 71,364,532-byte output from a 130,717-byte input; the fixed output is **1,007,019 bytes**.
2. **Object/xref overhead: +26,180,101 bytes.** `pdfa.rs::convert_bytes` calls `Document::save_to`, which writes the expanded dictionaries and all surviving objects. `convert_document` previously did not prune the graph after cleanup/fixups. The baseline has **82,703 unreachable objects**, versus zero after cleanup. On `001_001131.pdf` alone, 62,686 baseline objects are unreachable and non-payload overhead falls by 10,181,679 bytes. Existing action cleanup/fixups can disconnect structural attribute dictionaries; this change removes already-unreachable storage and does not change those repair decisions. Live dictionary/xref overhead remains a separate follow-up.
3. **Page/Form content: +17,691,882 bytes.** Paths such as `pdfa_cleanup.rs::fix_unbalanced_emc` and `pdfa_fonts.rs::fix_type1_subset_missing_glyphs` reserialize modified content with `set_plain_content` or new uncompressed streams. **17,914,647 baseline content bytes are uncompressed**; final lossless compression recovers **12,517,588 payload bytes**. The decoded tokens are unchanged by the new step.

Other measured savings include 5,119,032 CMap payload bytes, 4,083,929 image payload bytes, 2,813,255 other-stream payload bytes and 287,393 ICC payload bytes from lossless compression. ICC insertion paths such as `pdfa_colorspace.rs::add_srgb_output_intent` create raw streams. All five images whose stored bytes change retain identical decoded bytes, dimensions and color-space entries; existing filtered image codecs are not recompressed or downsampled.

## What was changed

`pdfa.rs::compact_storage` runs at the end of `convert_document`, after every font/color/XMP repair, so both public conversion entry points receive it:

- Remove objects unreachable from the trailer, including unreachable cycles.
- Flate-compress eligible unfiltered streams only when the saving exceeds filter overhead. Preserve XMP as unfiltered for every conformance level. Preserve existing filters, external-file streams and unfiltered streams carrying DecodeParms.
- Share **only font programs**, and only after both content and complete stream dictionaries compare equal. SHA-256 selects candidates; byte/dictionary equality decides sharing. Descriptors, widths, encodings and ToUnicode remain independent. A final reachability pass preserves any remaining non-font reference to a duplicate.

No glyph subsetting, image quality changes, resolution changes, external dependencies in Cargo, workflow edits or license-policy changes. The sets of decoded font-program hashes match for every one of the 300 documents, with no undecodable font streams. The storage step itself is idempotent. Whole Free Tier conversion still adds another watermark on each call; that pre-existing behavior is not changed here.

## The four axes after

Same 300 inputs, same shipping route, same commands and validator profile:

| Axis | Fresh baseline | Fixed |
|---|---:|---:|
| Conversions completed | 300/300 | 300/300 |
| veraPDF conformant, PDF/A-2b | 288/300 | 288/300 |
| Word retention, median | 100.0% | 100.0% |
| Word retention, 5th percentile | 98.56% | 98.56% |
| Character multiset retention, median | 100.0% | 100.0% |
| Visibly changed documents, original pixel-difference metric | 36 | 36 |
| Output/input, total | 265.43% | **146.89%** |
| Output/input, median per document | 221.46% | **174.33%** |

**Every per-document conformance, word-retention, character-retention and original render result is equal**, not just the aggregate. The 100.0% figure is a median: pre-existing tail failures remain. Full extracted output text and extractor exit codes also match for all 300 documents. `text_retention_gate.py` reports 298 text-bearing documents measured and zero regressions. The regex-word metric in `scripts/pdfa/text_fidelity.py` separately gives the same 100.0% median and 99.6% fifth percentile on 298 text-bearing documents; it uses a different tokenizer from `compare.mjs`.

Separate pinned SSIM set, first page at 150 dpi, existing `run_gate_ssim.py::compute_ssim` and `check_ssim_gate.py`:

| Metric | Before | After |
|---|---:|---:|
| Documents checked | 50 | 50 |
| Mean source/output SSIM | 0.9951 | 0.9951 |
| Documents below 0.95 | 0 | 0 |
| Gate | pass | pass |
| Before/after pixel-identical | — | **50/50** |
| Before/after extracted text identical | — | **50/50** |

Existing renderer warning exit codes are recorded rather than hidden. A produced image must be readable; missing images are errors. Eleven documents return matching warning codes on both conversions. This check makes no claim about unrendered pages.

| Fixed output class | Bytes |
|---|---:|
| Font programs | 97,646,162 |
| Images | 108,220,925 |
| Page/Form content | 41,976,340 |
| ToUnicode CMaps | 3,442,842 |
| Other streams | 3,724,834 |
| ICC profiles | 953,868 |
| XMP/metadata streams | 1,133,025 |
| Unreachable stream payload | 0 |
| Object/xref/non-payload overhead | 44,448,654 |
| **Total** | **301,546,650** |

## Tests added and what they fail on

Seven deterministic tests use generated PDFs and the existing synthetic TrueType builder. No downloaded or corpus fixture is used. Four conversion tests fail when the final storage step is disabled, restoring the previous pipeline behavior: duplicate embedded fonts remain separate; font/ICC streams remain raw; page content remains raw; unreachable cycles survive. The other three protect decoded font bytes, independent descriptor metrics, storage idempotence, dictionary-sensitive sharing and existing codecs/DecodeParms/XMP.

Two additional tests exercise the attribution example: exact byte accounting with unreachable payload, and dictionary-sensitive duplicate detection.

Required checks pass: `cargo fmt --all -- --check`; `cargo clippy -p pdf-manip --all-targets -- -D warnings`; `cargo test -p pdf-manip` (**397 passed, 76 existing ignored tests**). The report example has **2 passed** with its separate test command. With the storage step disabled, the PDF/A unit run reports **145 passed, 4 failed**; restoring it returns the tests to green.

## Follow-ups with recovered size

- Exact duplicate non-font payload remaining: **2,487,160 CMap bytes**, **769,292 content bytes**, **105,016 ICC bytes**, **127,047 other-stream bytes**, and **23,285 image bytes**: **3,511,800 bytes total**, excluding object overhead. This is measured duplicate payload, not a validated additional fix. Sharing these classes needs its own semantics/validator checks.
- Remaining font payload is **97,646,162 bytes**. Subsetting full substitute fonts could reduce this, but no safe recoverable amount is established here. Existing glyph/encoding repair complexity makes quoting the entire pool as a saving misleading.
- Remaining dictionary/xref overhead is **44,448,654 bytes**. Object-stream serialization for PDF/A-2/3 could reduce it; the recoverable amount is unmeasured, and PDF/A-1 needs a separate policy.
- Compressing XMP could recover **855,444 payload bytes** on this set. It is intentionally left unfiltered; a level-specific policy would need separate review.
- Existing action repair rewrites structural `/S` values to `GoTo` in `001_001131.pdf`; disconnected attribute storage explains much of that document's cleanup saving. Correcting the structural repair is outside this size-only change. It should not be described as an additional size win.
- The existing 12 conformance failures and 36 visibly changed documents are unchanged. Further font substitution/color/watermark work belongs to its separate tickets.

## Commands to reproduce

Use the unchanged source set from `benchmarks/pdfa/govdocs_sample_300.txt` in `target/pdfa-size/govdocs`, and checksum-matching SSIM files in `target/pdfa-size/ssim-corpus`. Preserve the baseline binary before applying the fix:

```bash
# In a worktree at baseline 4ed3cecbd:
cargo build -p pdf-manip --release --example pdfa_convert_real
mkdir -p target/pdfa-size
cp target/release/examples/pdfa_convert_real target/pdfa-size/before-convert

# After applying the fix in that worktree:
cargo build -p pdf-manip --release --example pdfa_convert_real --example pdfa_size_report
node benchmarks/pdfa/reproduce/compare.mjs --corpus-dir target/pdfa-size/govdocs --list benchmarks/pdfa/govdocs_sample_300.txt --converter 'target/pdfa-size/before-convert {in} {out}' --name baseline-4ed3cecbd --workdir target/pdfa-size/before
node benchmarks/pdfa/reproduce/compare.mjs --corpus-dir target/pdfa-size/govdocs --list benchmarks/pdfa/govdocs_sample_300.txt --converter 'target/release/examples/pdfa_convert_real {in} {out}' --name compact-storage --workdir target/pdfa-size/after
cargo run -p pdf-manip --release --example pdfa_size_report -- target/pdfa-size/govdocs target/pdfa-size/before benchmarks/pdfa/govdocs_sample_300.txt --json target/pdfa-size/attribution-before.json > target/pdfa-size/attribution-before.txt
cargo run -p pdf-manip --release --example pdfa_size_report -- target/pdfa-size/govdocs target/pdfa-size/after benchmarks/pdfa/govdocs_sample_300.txt --json target/pdfa-size/attribution-after.json > target/pdfa-size/attribution-after.txt
python3 crates/pdf-manip/examples/pdfa_size_verify.py compare --before target/pdfa-size/before --after target/pdfa-size/after --out target/pdfa-size/quality-comparison.json

# Same measurement dependencies used by the existing SSIM runner:
uv venv target/pdfa-size/venv
uv pip install --python target/pdfa-size/venv/bin/python numpy pillow scikit-image pyyaml
target/pdfa-size/venv/bin/python crates/pdf-manip/examples/pdfa_size_verify.py ssim --corpus target/pdfa-size/ssim-corpus --before target/pdfa-size/before-convert --after target/release/examples/pdfa_convert_real --out target/pdfa-size/ssim --workers 2
# The command above calls check_ssim_gate.py for both result files.

PDFA_SIZE_OUTPUT=target/pdfa-size/before RETENTION_CACHE_DIR=target/pdfa-size/text-cache python3 scripts/pdfa/text_retention_gate.py --corpus-dir target/pdfa-size/govdocs --binary crates/pdf-manip/examples/pdfa_size_verify.py --baseline target/pdfa-size/retention-baseline.json --update-baseline --word-fidelity
PDFA_SIZE_OUTPUT=target/pdfa-size/after RETENTION_CACHE_DIR=target/pdfa-size/text-cache python3 scripts/pdfa/text_retention_gate.py --corpus-dir target/pdfa-size/govdocs --binary crates/pdf-manip/examples/pdfa_size_verify.py --baseline target/pdfa-size/retention-baseline.json --word-fidelity
# Adapter only copies saved pdfa_convert_real outputs into the gate's requested path.

python3 - <<'PY'
from pathlib import Path
names = Path('benchmarks/pdfa/govdocs_sample_300.txt').read_text().splitlines()
for stage in ('before', 'after'):
    Path(f'target/pdfa-size/{stage}-pairs.tsv').write_text(''.join(
        f'{n}\ttarget/pdfa-size/govdocs/{n}\ttarget/pdfa-size/{stage}/{n}\n' for n in names))
PY
python3 scripts/pdfa/text_fidelity.py --pairs target/pdfa-size/before-pairs.tsv
python3 scripts/pdfa/text_fidelity.py --pairs target/pdfa-size/after-pairs.tsv

# Separate gate corpus: fetch/identity verification, not the 300-document baseline.
python3 scripts/ci/fetch_gate_corpus.py --manifest corpus/GATE_CORPUS_MANIFEST.json --out target/pdfa-size/gate-corpus

cargo fmt --all -- --check
cargo clippy -p pdf-manip --all-targets -- -D warnings
cargo test -p pdf-manip
cargo test -p pdf-manip --example pdfa_size_report
```
