# Build plan for 1.0 — size, complexity, and what open source can carry

**Date:** 2026-08-18 · Sizes are for one developer, implementation plus the tests
the Definition of Done requires. Every crate named was checked on crates.io for
licence and for C/C++ dependencies; anything pulling `cc` or a `-sys` crate is
excluded by the pure-Rust rule in `CLAUDE.md`.

**S** = under a day · **M** = a few days · **L** = one to two weeks · **XL** = a month or more

---

## First: two things that turned out to be wiring, not building

The recount keeps shrinking the build list.

| capability | reality |
|---|---|
| **e-invoicing** (ZUGFeRD, Factur-X, XRechnung, EN16931) | `pdf-invoice` implements embedding *and* validation. The facade simply does not depend on it. **Wiring, S.** |
| PDF to Excel / PowerPoint | wired 2026-08-18 |

Together with the six wiring jobs already listed (delete/reorder/insert/crop
pages, extract images, extract tables), that is seven capabilities reachable in
about a day each, mostly test-writing.

---

## Build work

### Small — a day or less each

| # | capability | open source | why small |
|---|---|---|---|
| B4 | image watermark | `image` (MIT/Apache, already a dependency) | `add_watermark` exists for text; this reuses the same placement code with an XObject instead of a text run |
| B6 | XMP metadata | **`xmp-writer`** 0.3 (MIT/Apache, pure Rust, from the Typst project) | writes the packet; we attach it to the catalog. Note: `xmp_toolkit` is **excluded** — it builds Adobe's C++ toolkit via `cc` |
| B7 | add attachment | none needed | embedded-file streams are plain PDF structure; `attachments()` already reads them |
| B8 | read-side accessors (permissions, linearisation status, font inventory) | none needed | the data is already parsed; these are getters that were never surfaced |

### Medium — a few days each

| # | capability | open source | complexity sits in |
|---|---|---|---|
| B1 | **XFA barcodes** | **`barcoders`** 2.0 (MIT/Apache, 1 dependency) for 1D, **`rxing`** 0.9 (Apache-2.0) for PDF417/DataMatrix/QR | mapping the XFA `<barcode type>` attribute to a symbology, and drawing modules into the content stream. Everything else — parsing, layout, no-split rule — already exists. Neither crate emits PDF, which suits us: the geometry stays ours |
| B2 | ~~OCR cloud integration~~ | — | **Reduced to documentation, 19-08.** No adapter per provider: the `OcrEngine` trait and `make_searchable` already are the integration, so what was missing was an honest description of them, not code. See the OCR section below |
| B5 | page numbers, header/footer | none needed | `PageDecoration` currently models watermarks only; this generalises it to positioned content with page-relative placement |
| B9 | greyscale, N-up, resize, split-by-bookmark | `image` for colour | four independent page-level operations. Greyscale needs colour-space handling for images *and* content-stream colour operators, which is the fiddly part |
| B11 | timestamp / TSA | **`rasn-cms`** 0.28 (MIT/Apache, pure Rust) — or extend what we have | **not a new stack**: `pdf-sign` already uses `der`, `rsa`, `p256`, `sha2`. RFC 3161 is a CMS structure fetched over HTTP and embedded in the signature dictionary |
| B12 | annotation authoring | none needed | `pdf-annot` reads every ISO 32000-2 type and has styling setters; it has no `add`. Appearance-stream generation is the real work |

### Correction: PDF/UA and PDF/X are mostly built

I sized these at one to two weeks. That was wrong, and Jasper's recollection that
we already had something was right.

`pdf-compliance` carries dedicated `pdfua.rs`, `pdfx.rs`, `pdfx_gen.rs`,
`tagged.rs`, `tagged_gen.rs` and `xmp.rs` — around 29,000 lines — with
`validate_pdfua()` and `validate_pdfx(level)` as public entry points, and the
generation side present too: `add_heading`, `add_paragraph`, `add_table`,
`add_list`, `add_figure`, `add_role_mapping` and `bdc_operator` for tagged
structure, `add_output_intent`, `add_bleed_boxes`, `add_trim_boxes` and
`add_pdfx_xmp` for PDF/X.

**The facade exposes none of it.** Same shape as Excel and PowerPoint: built,
tested, published, unreachable from the crate customers are told to use.

| # | capability | revised size |
|---|---|---|
| B10a | surface `validate_pdfua` / `validate_pdfx` on the facade | **S** — wiring |
| B10b | a conversion entry point (`convert_to_pdfua`) on top of the existing tagged generators | **M** |
| B10c | corpus rounds to find where conformant output actually breaks | **L**, and this is the part that took PDF/A four rounds |

The lesson is the one from the XFA limitations doc again: I sized a capability
from its absence in the facade instead of checking the crate behind it. The
`codebase-questions` skill exists for exactly this, and I did not follow it here.

### The special case

| # | capability | see below |
|---|---|---|
| B3 | **HTML → PDF** | **dropped 19-08** — not built, not offered, documented Chrome route |

---

## HTML to PDF: decided, and we are not building it

**Decision, Jasper, 2026-08-19: do not build it. Do not offer it. Support it.**

Both tracks below are dropped — the pure-Rust renderer and the Chromium wrapper
alike. The reasoning that made track 2 large applies to track 1 as well once you
follow it through: a renderer that is nearly right is worse than none, because the
output looks plausible and is wrong, and the customer finds out downstream.

What we say instead: use headless Chrome or Chromium for the conversion, then hand
the PDF to PDFluent for everything after it — merging, page operations,
compression, watermarks, encryption, signing, PDF/A, redaction. That is a real
answer to the actual need, and it is honest about which part is ours.

Recorded in `crates/pdfluent/README.md` and `STABILITY.md`. The `html-to-pdf`
feature flag said "Reserved for 1.1 (#1206 IronPDF parity)" and now says what is
true: not offered, not planned.

## OCR: decided, and the seam is what we ship

**Decision, Jasper, 2026-08-19: do not develop OCR. Be genuinely ready for cloud
providers, and say so. That is enough for 1.0.**

B2 shrinks from "an adapter per provider" to nothing but documentation, because
the preparation already exists and is real:

- `pdf_ocr::OcrEngine` — implement `recognize(rgb, width, height, dpi)` against
  Google Cloud Vision, AWS Textract, Azure Document Intelligence, or anything else
- `pdf_ocr::make_searchable(doc, engine, config, render_fn)` — renders each page,
  calls your recognizer, and writes the returned words and `bbox_px` back as an
  invisible text layer over the image

Recognition is the provider's; the PDF work is ours. Two local backends
(`tesseract`, `paddle`) exist behind their own flags but need C system libraries,
which is why the facade does not wire them and why a cloud recognizer is the
recommended path — it keeps the pure-Rust guarantee intact.

Correcting the documentation was the actual work here, and it was overdue: the
published crates.io README for `pdfluent` advertised "OCR via Tesseract" and "OCR
via PaddleOCR" as feature flags that are empty, and `pdf-ocr/README.md` pointed
readers straight at those no-ops.

## Total

| category | items | size |
|---|---|---|
| wiring (crate exists, facade does not expose it) | 7 | ~1 day each |
| small builds | 4 | ~1 day each |
| medium builds | 6 | a few days each |
| large builds | 1 (PDF/UA + PDF/X) | 1–2 weeks |
| HTML → PDF | **dropped 19-08** | not built, not offered; documented Chrome route |
| OCR | **reduced 19-08** | documentation only; the `OcrEngine` seam already exists |
| deferred stubs to implement or remove | 3 | varies |

Roughly six to eight weeks of focused work for everything, and the first two
weeks would close most of the credibility gap because the wiring and the small
builds are where the advertised-but-unreachable claims are concentrated.

**Nothing is done until it has a test in the pipeline** and
`scripts/ci/feature_promises.py` shows it both tested and reachable from the
facade — coverage without reachability was already reporting 11/11 while two of
the eleven could not be called at all.
