## Summary

Round 2 reduces the original 300-document output from **301,546,650 to 252,469,778 bytes**, saving **49,076,872 bytes (16.28%)**. Output/input is **122.98%**, compared with 146.89% in round 1 and 265.43% before either change. PDF/A-2b conformance improves from **288/300 to 300/300**. Original visual-threshold flags fall from **36 to 18**. These measurements do not establish commercial SDK parity or perfect fidelity for arbitrary PDFs.

This is the internal implementation and evidence handoff for independent review. Commercial comparison remains **unmeasured**: Apryse SDK/license and a licensed local Nutrient Document Engine are unavailable. Safe subsetting currently covers simple TrueType; unsupported CFF/CID and other cases retain their programs with recorded reasons. Remaining source ambiguity, font substitution and color differences are documented below. The issue should remain open for review and the outstanding comparison.

Baseline: `ca312ba638e811594ea5611efb7d3b9cc16582fc`. Measured implementation: **`3aa9a7001661fed44f96b26414dc8d1fa79325b7`**. Frozen converter SHA-256: `ccec5fd31c7e1e40b588678321b4e5fb0fa9ff75722344922a19fee345a18ff9`. The later evidence commit does not alter converter behavior. The shipping route is `pdfa::convert_bytes_with_report` (also used by `convert_bytes`) through `pdfa_convert_real`, PDF/A-2b, with the existing Free Tier watermark intact.

The original round-1 report and JSON remain unchanged. This report is accompanied by `pdfa_round2_results.json` (all four sets, hashes, diagnostics, attribution, tests and timing), `pdfa_round2_causes.json` (108 original tail cases), and `pdfa_round2_all_pages.json` (every original page).

## Byte attribution before

Encoded stream payload categories are exclusive; non-payload bytes include dictionaries and object delimiters. Columns sum to file bytes. The pre-round-1 column is retained to distinguish the initial storage repair from this broader quality iteration. Increased ICC/content storage can be necessary for correctness and must not be counted as an optimization regression without inspecting content.

| Class | Source | Before round 1 | Round 1 | Round 2 |
|---|---:|---:|---:|---:|
| cmap | 335,373 | 8,561,874 | 3,442,842 | 982,875 |
| content | 36,802,046 | 54,493,928 | 41,976,340 | 39,777,567 |
| fonts | 16,683,537 | 298,913,056 | 97,646,162 | 75,865,573 |
| icc | 755,297 | 1,241,261 | 953,868 | 8,096,222 |
| images | 113,179,725 | 112,304,854 | 108,220,925 | 108,220,925 |
| metadata | 1,887,812 | 1,133,025 | 1,133,025 | 277,581 |
| object_xref_overhead | 30,150,885 | 56,330,986 | 44,448,654 | 13,199,114 |
| other_streams | 3,638,264 | 6,538,089 | 3,724,834 | 3,638,617 |
| unreachable | 1,842,146 | 5,398,341 | 0 | 0 |
| xref_object_streams | 17,133 | 0 | 0 | 2,411,304 |
| **Total** | **205,292,218** | **544,915,414** | **301,546,650** | **252,469,778** |

The full attribution utility still emits per-stream/font/image details locally. The committed JSON retains class totals, per-document diagnostics and per-font program hashes/names/page usage rather than duplicating approximately 70 MB of full stream inventory. Input `002_002400.pdf` requires the existing loader repair. No lossy image recompression or downsampling was introduced.

## Causes with code paths

The matrix covers the union of all 12 original validator failures, 36 original visual flags, and 83 measured word-retention tails: **108 documents**. It records original validator clauses/pages, source font census, extraction exit codes and replacement characters, final conversion diagnostics, and every page's comparative metrics. Resource evidence and unverified hypotheses are explicitly separate; a missing embedded font alone does not prove the cause of every changed pixel.

Verified defect families and owned regression coverage:

- `pdfa_cleanup.rs` / `pdfa_fixups.rs`: structural `/S` attributes were treated as action subtypes. Positive action recognition preserves legitimate structure attributes.
- `pdfa_fonts.rs` / `cff_append.rs`: low-byte Type1 repairs could blank text or corrupt binary streams. Native CFF encodings are captured before later repairs; mathematical names receive Unicode mappings. Visible CFF `.notdef` shapes used at code zero are duplicated into legal glyph/CID slots without changing their outlines, widths, FD/private data or Unicode meaning. Symbolic TrueType ToUnicode is generated before removing its encoding. `001_001370.pdf` improves from approximately 1.5% word retention to 100%.
- `inline_image.rs` / `content_editor.rs`: generic parsing could consume filtered inline-image bytes as PDF tokens. The editor now frames supported image formats and preserves complete filter/predictor dictionaries and binary payloads. Unsupported framing declines the edit with diagnostics. Both legacy long-string repair paths now modify parsed string operands, avoiding binary-image truncation. `002_002489.pdf` regains full word/character retention and its lost page content.
- `pdfa.rs::join_content_fragments` and `pdfa_fixups.rs::fix_content_stream_operator_spacing`: a page's `/Contents` array can split a dictionary or text array across streams. Join incomplete fragments before stream-local repairs; do not invent MCID dictionaries based on line breaks. Shared originals remain intact and compression preferences survive. Seven targeted documents no longer acquire converter-generated syntax errors. A source structure-tree warning remains in `002_002406.pdf`.
- `pdfa.rs` serialization: normalize the binary header before computing offsets, remove stale xref stream parameters from the trailer, and select classic PDF 1.4 for PDF/A-1 versus modern PDF 1.7 storage for PDF/A-2/3. Explicitly reject inputs from which no pages can be recovered, including unsupported protected files, instead of manufacturing a blank page.
- `pdfa_colorspace.rs`: replace an incomplete CMYK ICC placeholder with the existing complete, licensed profile; use a legal RGB output intent. Correct sRGB's piecewise transfer curve and D50 adapted white point. The ICC fixture's maximum RGB error against the owned sRGB reference falls from 10 to 1 channel levels; mean error falls from 0.616533 to 0.002959. The changed baseline PNG is supported by this reference comparison.

Source/output fidelity is still not perfect. Unembedded fonts may require substitution; unprofiled DeviceCMYK requires an explicit characterization assumption. For `000_000314.pdf`, disabling color management largely removes the observed difference, supporting color characterization rather than geometry as the cause. This does not prove the chosen profile recovers an unknown author's intended colors.

`002_002202.pdf` is newly measurable after reader repairs: exact word retention is about 92.69%, while character retention is about 99.18%. Source extraction contains 608 replacement characters; output contains 320. Differences include ambiguous Type3/replacement tokens and canonically equivalent Ω/Ω. NFKC plus removal of replacement markers gives about 99.90% word retention as a **supplement only**; original metrics remain unchanged. `002_002385.pdf` and `002_002934.pdf` retain pre-existing sub-95% word tails with source replacement characters and font/spacing ambiguity. These are explicit unresolved limits, not claims that every difference is harmless.

## What was changed

Unknown symbolic glyph names no longer fall back to their raw byte as Unicode. The final 500-file check exposed this in `pdfjs__test_pdfs_issue6127.pdf`: an intermediate candidate changed 15 Wingdings replacement characters into controls. The final fix leaves unknown character meaning unresolved while retaining the glyph program, and restores the original extraction score. An owned test covers both low codes and overrides of printable base letters.

The public conversion report exposes repair warnings, font embedding/subsetting diagnostics, and failures of required steps. Warnings do not replace external conformance validation. The embedding count includes generated resources such as the watermark and is labeled accordingly.

`pdfa_subset.rs` conservatively subsets the full addressable 256-code repertoire of supported simple TrueType users, retaining GIDs, cmaps, metrics, hints and composite closure. Shared font users contribute a union of addressable glyphs, including symbolic mappings. This is broader than actual-used-text subsetting, deliberately retaining glyphs that may still be referenced. It respects embedding restrictions and `allows_compression`, uses valid subset names, and accepts only a positive encoded saving after overhead. The optional existing dependency is reused; no Cargo dependency was added.

`pdfa.rs::compact_storage` shares font programs, ToUnicode streams and output profiles only after complete stream dictionaries and payloads match. Existing codecs/DecodeParms and caller compression preferences survive. PDF/A-2/3 metadata may be compressed; PDF/A-1 metadata stays unfiltered. Reachability cleanup removes orphaned objects. Structural repairs, glyph preservation and corrected color profiles can increase individual document sizes. **65/300** outputs grow relative to round 1; all names and byte deltas remain in the per-document results.

Generated visual fixtures travel with this change: `pdfa-converted.pdf`, `image-icc.pdf`, and the reference-supported ICC PNG. All six visual-regression tests pass without changing tolerances. No source corpus PDF, proprietary font, workflow, or licensing-policy change is included.

## The four axes after

Original 300, unchanged `compare.mjs` definitions: visual flags are **grayscale MAE > 0.02 on the first three pages at 50 dpi**, not SSIM. Text is the existing whitespace-token multiset metric. Failed extraction is recorded separately, never converted into a perfect score.

| Axis | Round 1 | Round 2 |
|---|---:|---:|
| Converted | 300/300 | 300/300 |
| PDF/A-2b conformant | 288/300 | 300/300 |
| Word retention median | 100.00% | 100.00% |
| Word retention fifth percentile | 98.56% | 98.56% |
| Word retention below 95% / below 50% | 3 / 1 | 3 / 0 |
| Original visual flags | 36 | 18 |
| Total output bytes | 301,546,650 | 252,469,778 |
| Output/input | 146.89% | 122.98% |

Per-document increases and newly measurable text are retained in the JSON, including **0** exact word-score decreases and **1** sampled MAE increases over 0.001. Threshold counts alone are not a no-regression proof.

The separate checksum-pinned **50-file SSIM gate passes 50/50**, mean SSIM 0.9951; the per-document comparison reports 0 SSIM regressions. Extracted text is byte-identical for 38/50. The twelve changed texts were compared against their sources: word/character retention stays at 100% in eleven and improves to 100% in `f8233.pdf`, restoring the original triangle symbol. Eleven prior extractor syntax-error exits become successful. Byte-identity differences are reported rather than hidden.

All-page supplement: **8293 pages, 300 documents, 0 page-count/render-completeness errors**. Nonzero renderer exits, source/round1/round2: **1/11/0**. 139 pages retain identical round1/round2 RGB pixels; 284 documents retain identical full extracted text. 114 pages have RGB MAE increases greater than 0.001 and 0 documents have decreases in the approximate ordered-token score; their exact values remain visible in the all-page JSON. These are diagnostic measurements, not a human layout review.

All-page rendering uses 72 dpi RGB MAE and grayscale SSIM. 298 document measurements were reused only after source/prior-output hashes and the complete decoded Root/Info object graph matched; 2 were freshly measured. All 298 final reused PDFs are also byte-identical to their immediate prior measured outputs. Reuse includes diagnostic exit codes and warning hashes from the prior run, and records a provenance chain. Identity includes metadata, strings, filters, IDs and stream bytes, ignoring only physical stream length/storage packing. The reproduction command below performs a fresh run without reuse. Approximate ordered retention uses SequenceMatcher with its popular-token heuristic; it is not edit distance or a proof of reading order.

Across all four sets, no previously measurable word-retention score decreases and no successfully converted document becomes newly nonconformant. Explicit rejections remain visible below.

Additional sets remain separate:

| Set | Conformant before → after | Conversion failures | Visual flags | Word <95% | Output bytes |
|---|---:|---:|---:|---:|---:|
| Original 300 | 288 → 300 | 0 → 0 | 36 → 18 | 3 → 3 | 301,546,650 → 252,469,778 |
| Existing gate 500 | 475 → 472 | 3 → 10 | 40 → 35 | 60 → 60 | 62,374,383 → 60,237,041 |
| Development 93 | 87 → 86 | 1 → 4 | 11 → 9 | 16 → 16 | 9,578,204 → 8,927,611 |
| Confirmation 21 | 18 → 18 | 0 → 1 | 0 → 0 | 3 → 3 | 2,365,391 → 2,043,605 |

Converted outputs without a usable sampled render comparison (before → after): original300: 0 → 0; existing500: 10 → 4; development93: 4 → 1; confirmation21: 1 → 0. These are not visual passes.

The 500-file gate was already used historically and has one hash overlap with the 300; it is not independent. The 93-file set (50 pdf.js, 43 PDFBox) was used to repair writer defects and is a development set. The hash-disjoint 21-file confirmation set (20 pdf.js, one PDFBox) was frozen later; availability limited the target of 40 and it was re-evaluated after general repairs. It is not an untouched final blind test. Manifests preserve source commits, input hashes and repository provenance.

New rejections are visible in both conversion and conformance denominators:

- original300: none
- existing500: `pdfbox__examples_src_test_resources_org_apache_pdfbox_examples_signature_sign_me_protected.pdf`, `pdfbox__pdfbox_src_test_resources_org_apache_pdfbox_encryption_AES256ExposedMeta.pdf`, `pdfbox__pdfbox_src_test_resources_org_apache_pdfbox_encryption_AESkeylength128.pdf`, `pdfbox__pdfbox_src_test_resources_org_apache_pdfbox_encryption_PasswordSample-256bit.pdf`, `pdfjs__test_pdfs_bug1539074.1.pdf`, `pdfjs__test_pdfs_issue9105_other.pdf`, `pdfjs__test_pdfs_poppler-85140-0.pdf`
- development93: `pdfjs__test_pdfs_poppler-937-0-fuzzed.pdf`, `pdfbox__pdfbox_src_test_resources_org_apache_pdfbox_encryption_AESkeylength256.pdf`, `pdfbox__pdfbox_src_test_resources_org_apache_pdfbox_encryption_AES128ExposedMeta.pdf`
- confirmation21: `pdfjs__test_pdfs_issue19484_2.pdf`

The seven newly rejected 500-set inputs all report that no pages could be recovered. This includes protected files; it does not assert that those source PDFs intrinsically contain zero pages. These explicit rejections replace old fabricated blank output. Existing fuzz timeouts and unsupported/encrypted inputs are retained, not excluded. To avoid counting rejected files as size savings, jointly converted totals are also recorded:

- original300: 300 jointly converted documents, 301,546,650 → 252,469,778 bytes.
- existing500: 490 jointly converted documents, 62,244,944 → 60,237,041 bytes.
- development93: 89 jointly converted documents, 9,503,731 → 8,927,611 bytes.
- confirmation21: 20 jointly converted documents, 2,340,171 → 2,043,605 bytes.

Secondary timing: 30 deterministically selected original inputs, fresh converter process, `/usr/bin/time` peak RSS on macOS arm64. Both rounds share the input hashes. Host load was uncontrolled and substantial; these are engineering observations, not latency promises or a controlled performance comparison.

| Binary | Successful | Median seconds | Max seconds | Median peak MiB | Max peak MiB |
|---|---:|---:|---:|---:|---:|
| round1 | 30/30 | 2.194 | 9.747 | 17.1 | 127.7 |
| round2 | 30/30 | 0.617 | 6.816 | 20.9 | 132.7 |

## Tests added and what they fail on

Final package tests: **430 passed, 0 failed, 76 existing ignored**; library tests: 328 passed. Strict package clippy (all targets), formatting, PDF/A without subsetting (296 tests), PDF/A example without JSON, the two attribution-example tests, and six visual-regression tests pass. Python harness syntax and strict-identity positive/negative controls pass.

Owned generated tests cover action/structure discrimination, native CFF and symbolic Unicode, zero-code outlines, subset repertoire/composites/shared users, compression flags, modern xref offsets, inline binary payloads/predictors, long-string repairs and split content instructions. The following **combined negative controls** restored old defects or disabled corresponding guards; sources were restored and the library suite returned to green after each group:

- Simultaneously reinstate old Type1/action/CMYK defects and disable each new mapping/subset/inline/marker guard. Independent named regression failures are recorded; this is a combined negative control, not isolated per-mutant scoring. **9 tests failed**, with no compilation error.
- Combined negative control: old sRGB profile; compression opt-out guards removed from compaction, subsetting and XMP repair **5 tests failed**, with no compilation error.
- Restore the complete round-1 content editor, with only the depth helper visibility adjusted for the new parser module. This reinstates filtered-image loss and incomplete dictionary encoding. **4 tests failed**, with no compilation error.
- Combined negative control: restore both raw long-string scanners, disable native CFF encoding capture, remove mathematical Unicode aliases. **4 tests failed**, with no compilation error.
- Combined negative control: disable content-fragment joining and restore line-based MCID dictionary invention. **2 tests failed**, with no compilation error.
- Restore byte-code identity fallback for unknown symbolic Differences names. **1 test failed**, with no compilation error.

These are grouped controls, not a claim of isolated mutation testing for every line. Named failing tests and restored source hashes are in the results JSON. An extra dependency-wide clippy run with default features disabled reports nine pre-existing `needless_range_loop` findings in the JPEG 2000 dependency; library-only strict clippy for that configuration passes. Broader no-default-feature invocations also hit examples/integration tests that require omitted features; the correctly scoped PDF/A library and no-JSON example checks pass. An initial broad repository gate used a Python lacking PyYAML; the repeat uses the prepared Python environment. That concurrent run then hit a repository scanner race with a deleted temporary render directory; publication runs after measurement scratch activity ends. The final publication comment records the normal push-gate outcome separately. Publication uses a clean checkout of the same final commit so measurement scratch directories cannot race the source scan.

## Follow-ups with recovered size

- Supported TrueType subsetting saves **3,687,840 encoded payload bytes across 36 programs**. This isolated counter is already included in total savings. It must not be added to class deltas again.
- Remaining font payload is **75,865,573 bytes** (see class table). Unsupported CFF/CID, signed/variable/color/restricted or otherwise unsafe programs remain intact with reason counts in JSON. Broader CFF/CID and actual-used-glyph subsetting remain future work; recoverable bytes are unmeasured.
- Xref/object-stream writing, ToUnicode/profile sharing and level-specific metadata compression reduce storage, while corrected ICC characterization adds necessary bytes. Class deltas are net effects of several interacting repairs, not independent additive optimization estimates.
- Residual visual/text tails need document-specific review, including source extraction ambiguity, substitute-font geometry and unprofiled CMYK. Conformance alone does not establish content preservation or product parity.
- Apryse and Nutrient results are **not measured**. `pdfa_vendor_adapter.py` and `pdfa_vendor_comparison.md` provide a local comparison route and explicit settings; syntax/missing-access behavior is checked, live SDK behavior remains unverified. No licensed evaluation was purchased and no corpus was sent to a hosted service.

## Commands to reproduce

Use veraPDF 1.28.2, MuPDF 1.27.2, Python 3.11 with numpy/Pillow/scikit-image/pypdf, and the same available font environment. Check all source hashes against the committed results/manifests. Build round 1 in a separate worktree at `ca312ba638e811594ea5611efb7d3b9cc16582fc` and retain its binary/output directory as `round1-convert` / `after`. Build this implementation at `3aa9a7001661fed44f96b26414dc8d1fa79325b7`; do not overwrite the baseline binary.

```sh
cargo build -p pdf-manip --release --example pdfa_convert_real --example pdfa_size_report
mkdir -p target/pdfa-size/round2
cp target/release/examples/pdfa_convert_real target/pdfa-size/round2/round2-convert
node benchmarks/pdfa/reproduce/compare.mjs --corpus-dir target/pdfa-size/govdocs --list benchmarks/pdfa/govdocs_sample_300.txt --converter 'target/pdfa-size/round2/round2-convert {in} {out} {out}.report.json' --name round2 --workdir target/pdfa-size/round2/reproduced-300
cargo run -p pdf-manip --release --example pdfa_size_report -- target/pdfa-size/govdocs target/pdfa-size/round2/reproduced-300 benchmarks/pdfa/govdocs_sample_300.txt --json target/pdfa-size/round2/reproduced-attribution.json
python3 crates/pdf-manip/examples/pdfa_size_verify.py ssim --corpus target/pdfa-size/ssim-corpus --before target/pdfa-size/round2/round1-convert --after target/pdfa-size/round2/round2-convert --out target/pdfa-size/round2/reproduced-ssim --workers 2
python3 crates/pdf-manip/examples/pdfa_round2_eval.py --source target/pdfa-size/govdocs --before target/pdfa-size/after --after target/pdfa-size/round2/reproduced-300 --list benchmarks/pdfa/govdocs_sample_300.txt --out target/pdfa-size/round2/reproduced-all-pages.json --workers 2
python3 crates/pdf-manip/examples/pdfa_round2_causes.py --baseline target/pdfa-size/after/compare.json --candidate target/pdfa-size/round2/reproduced-300/compare.json --source target/pdfa-size/govdocs --before target/pdfa-size/after --after target/pdfa-size/round2/reproduced-300 --out target/pdfa-size/round2/reproduced-causes.json
python3 crates/pdf-manip/examples/pdfa_round2_identity.py
cargo fmt --all -- --check
cargo clippy -p pdf-manip --all-targets -- -D warnings
cargo test -p pdf-manip
cargo test -p pdf-manip --example pdfa_size_report
cargo test -p pdf-manip --no-default-features --features pdfa-convert --lib
cargo build -p pdf-manip --no-default-features --features pdfa-convert --example pdfa_convert_real
cargo test -p visual-regression --test regression
```

`pdfa_size_verify.py ssim` deliberately returns a nonzero status for changed extracted text even when the SSIM gate passes; inspect `regressed`, both gate reports and the source-text supplement separately. Do not discard that diagnostic.

For the other sets, fetch `corpus/GATE_CORPUS_MANIFEST.json` with the existing `fetch_gate_corpus.py`; fetch each 93/21 manifest entry from its source's `raw_base` plus `path`, verify SHA-256, and save as `name`. Write the names in manifest order to a list. Run the same unchanged `compare.mjs` command for each corpus and each frozen binary in separate output directories. Conversion timeout remains 600 seconds. Run `pdfa_round2_timing.py` for both binaries with every tenth name from the original 300 list. Vendor commands and settings are in `pdfa_vendor_comparison.md`.
