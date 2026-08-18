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
| B2 | **OCR cloud integration** | none for OCR itself; `ureq` or `reqwest` for HTTP | a trait plus one adapter per provider. The work is three different request shapes, three auth schemes and three result formats — not recognition. No models, no licence question |
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
| B3 | **HTML → PDF** | two tracks, sized separately |

---

## HTML → PDF: two tracks is the right answer

Your instinct matches what the market actually does, and the split is not a
compromise — the two serve different customers.

**Track 1 — Chromium, as an optional dependency (M).**
`chromiumoxide` (MIT/Apache) drives a browser over the DevTools protocol and
asks it to print to PDF. Perfect fidelity, because it *is* a browser: JavaScript,
web fonts, flexbox, everything. We do not ship or bundle Chromium; the customer
points us at the browser they already have. That keeps the pure-Rust rule intact
for the default build — it becomes an opt-in feature with an external runtime
requirement, stated plainly.

This is what wkhtmltopdf, Puppeteer and every serious HTML-to-PDF service do, and
the reason they all weigh hundreds of megabytes. Anyone converting real web pages
needs this and will not accept a subset.

**Track 2 — a pure-Rust document subset (L to XL).**

Fair question: if the crates exist, why is this weeks rather than days?

Because of what they do *not* cover. The four crates give you:

| crate | gives you |
|---|---|
| `html5ever` | a DOM tree from HTML bytes |
| `cssparser` | CSS text into parsed rules |
| `selectors` | which rules match which element |
| `taffy` | box positions, given a tree of boxes with known sizes |

Between "rules that match" and "boxes with known sizes" sits everything that
makes a browser a browser, and none of it is in those crates:

- **The cascade.** Turning matched rules into one computed value per property per
  element: specificity, inheritance, initial values, units, `em` relative to
  which parent. Servo has a whole crate for this (`stylo`); it is not small.
- **Text layout.** Line breaking, font fallback, shaping, baselines. We have
  pieces of this from `pdf-render` and the text engine, which helps — but it must
  be driven from computed CSS rather than from PDF text runs.
- **Pagination.** `taffy` lays out one continuous area. It has no concept of a
  page, so page breaks, widows and orphans, repeated table headers and content
  that overflows onto the next page are all ours. For a *document* renderer this
  is the central problem, not an edge case.
- **Painting.** Turning the laid-out boxes into PDF operators: backgrounds,
  borders, images, clipping, z-order.

So the crates carry parsing and box layout — real work, genuinely saved — and we
build the cascade, pagination and paint. That is the L to XL.

Scope it honestly and it shrinks: text, tables, images, page breaks, basic CSS;
no JavaScript, no floats, no grid edge cases. That covers invoices, reports and
letters — most of the actual demand — with no external runtime, no browser to
install, and deterministic output.

**Recommendation:** build track 1 first. It is smaller, it is what the market
expects, and it lets you keep the promise now. Track 2 then becomes the
differentiator rather than the excuse — "runs anywhere, no browser required" is
a real selling point precisely because competitors cannot say it.

---

## Total

| category | items | size |
|---|---|---|
| wiring (crate exists, facade does not expose it) | 7 | ~1 day each |
| small builds | 4 | ~1 day each |
| medium builds | 6 | a few days each |
| large builds | 1 (PDF/UA + PDF/X) | 1–2 weeks |
| HTML → PDF track 1 (Chromium) | 1 | a few days |
| HTML → PDF track 2 (pure Rust) | 1 | 1–2 weeks, or more |
| deferred stubs to implement or remove | 3 | varies |

Roughly six to eight weeks of focused work for everything, and the first two
weeks would close most of the credibility gap because the wiring and the small
builds are where the advertised-but-unreachable claims are concentrated.

**Nothing is done until it has a test in the pipeline** and
`scripts/ci/feature_promises.py` shows it both tested and reachable from the
facade — coverage without reachability was already reporting 11/11 while two of
the eleven could not be called at all.
