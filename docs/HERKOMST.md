# Provenance per crate

**Generated** by `scripts/ci/herkomsttabel.py`. Do not edit by hand:
a table you overwrite is out of date on the day of publication.

This answers one question: *who holds the rights, and what shows it?*
Without that answer, relicensing gives away what may not be yours.

## What is established

**Own code is clean.** No external human contributor appears in the history of
this repository, so relicensing the own code needs nobody's permission.

**The Foxit fonts are in order.** `crates/pdf-interpret/assets/*.pfb` carries
BSD-3-Clause from the PDFium Authors and is fully redistributable. That is the
classic stumbling block in open source; here it is already settled.

## Own crates

45 crates, 581 `.rs` files, of which **581**
carry the PDFluent licence header.

| crate | licence | .rs | with header |
|---|---|---:|---:|
| `docx-extract` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 4 | 4 |
| `epub-extract` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 8 | 8 |
| `formcalc-interpreter` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 10 | 10 |
| `markup-extract` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 5 | 5 |
| `pdf-annot` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 16 | 16 |
| `pdf-bench` | — | 7 | 7 |
| `pdf-capi` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 13 | 13 |
| `pdf-compliance` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 11 | 11 |
| `pdf-content-stream` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 12 | 12 |
| `pdf-desktop` | — | 5 | 5 |
| `pdf-diff` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 4 | 4 |
| `pdf-docx` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 4 | 4 |
| `pdf-engine` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 111 | 111 |
| `pdf-extract` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 6 | 6 |
| `pdf-forms` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 15 | 15 |
| `pdf-invoice` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 9 | 9 |
| `pdf-java` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 1 | 1 |
| `pdf-learned` | — | 6 | 6 |
| `pdf-manip` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 39 | 39 |
| `pdf-node` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 16 | 16 |
| `pdf-ocr` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 15 | 15 |
| `pdf-pptx` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 3 | 3 |
| `pdf-python` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 20 | 20 |
| `pdf-redact` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 5 | 5 |
| `pdf-sign` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 18 | 18 |
| `pdf-standard-fonts` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 1 | 1 |
| `pdf-substitute-fonts` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 1 | 1 |
| `pdf-text-format` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 1 | 1 |
| `pdf-xfa` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 25 | 25 |
| `pdf-xlsx` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 3 | 3 |
| `pdfluent` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 31 | 31 |
| `pdfluent-cli` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 20 | 20 |
| `pptx-extract` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 3 | 3 |
| `test-skip` | — | 1 | 1 |
| `visual-regression` | — | 5 | 5 |
| `xfa-api-server` | — | 4 | 4 |
| `xfa-cli` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 23 | 23 |
| `xfa-dom-resolver` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 6 | 6 |
| `xfa-js-sandboxed` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 5 | 5 |
| `xfa-json` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 6 | 6 |
| `xfa-layout-engine` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 7 | 7 |
| `xfa-pdfrest-compare` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 1 | 1 |
| `xfa-test-runner` | — | 65 | 65 |
| `xfa-wasm` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 7 | 7 |
| `xlsx-extract` | AGPL-3.0-only OR LicenseRef-PDFluent-Commercial | 3 | 3 |

## Other people's work

All permissive and attributed in `NOTICE` and `THIRD_PARTY_LICENSES.txt`.
**These crates must not receive our header.** A sweep that adds it would
claim other people's work.

| crate | upstream | licence | .rs | with header |
|---|---|---|---:|---:|
| `cff-parser` | ttf-parser / pdf.js (Reizner, Muizelaar) | MIT OR Apache-2.0 | 10 | 0 |
| `hayro-ccitt` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 4 | 0 |
| `hayro-jbig2` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 22 | 0 |
| `hayro-jpeg2000` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 26 | 0 |
| `lopdf` | lopdf | MIT | 38 | 0 |
| `pdf-font` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 36 | 0 |
| `pdf-interpret` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 44 | 2 (WARNING) |
| `pdf-render` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 3 | 0 |
| `pdf-syntax` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 47 | 0 |

## Copied fragments, by name

| location | source | licence |
|---|---|---|
| `pdfluent-jpeg2000/src/lib.rs` | OpenJPEG | BSD-2-Clause |
| `pdf-interpret/src/color.rs:551` | [pdf.js `colorspace.js#L846`](https://github.com/mozilla/pdf.js/blob/06f44916/src/core/colorspace.js#L846) | Apache-2.0 |

The second one was once listed in #213 as having no name. It does have one:
the line above it carries the full pdf.js permalink.

## Own work, not a fork

`pdf-content-stream` and `pdf-diff` were listed in #213 as "own choice or fork?".
Checked in the history: both start with a commit that says "New crate" and
describes an original design. Not forks.

## Attention

These forked crates carry our header: `pdf-interpret`.
That is a claim on other people's work and must be reverted before LC10.
