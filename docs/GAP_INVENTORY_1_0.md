# What we promise versus what the SDK does — recount for 1.0

**Date:** 2026-08-18 · **Method:** the website's `credibility_inventory.json`
(123 English pages, compilation-verified) re-checked against today's facade
(`crates/pdfluent`, 78 public methods), not against the `1.0.0-beta.8` it was
originally written for.

That re-check was the point. Four months of SDK work sit between the audit and
now, and an audit that feels like a fact while being stale is the same trap the
XFA limitations doc fell into.

---

## The website audit, as it stood

| bucket | meaning | pages |
|---|---|---|
| A | already correct | 4 |
| B | snippet wrong, capability exists | 63 — **all repaired** |
| C | **SDK cannot do it** | **52** |
| D | WASM/JS, separate audit | 4 |

Bucket C splits into C1 (exists in a published crate, not in the facade — wiring)
and C2 (genuinely absent — building).

---

## What the recount changed

**Now genuinely available**, contrary to the audit:

| audit said | today |
|---|---|
| "no PDF/A conversion API, validate only" | `convert_to_pdfa` ships |
| "no text-replacement API" | the whole TextEditor: Unicode, regex, reflow, fit policies |
| "no XFA data API" | `form_model`, `has_xfa_form` |
| "no search API, text() returns a String" | `find_text` with regex |
| PDF to Excel / PowerPoint | wired to the facade 2026-08-18 |

**Exists but does nothing** — the most dangerous category, because the method is
there and returns an error only at runtime:

- `flatten_forms()` → `Error::MissingDependency`
- `embed_font()` → `Error::MissingDependency`
- `linearize()` → `Error::MissingDependency`
- `add_decoration()` → header/footer deferred; only watermark works

**Empty feature flags** in the facade, same problem in a different place:
`ocr-tesseract`, `ocr-paddle`, `html-to-pdf`, `xfa-flatten`. Enabling one is a
no-op, which reads as consent.

---

## The build list for 1.0

### Wiring — the crate exists, the facade does not expose it

| # | capability | note |
|---|---|---|
| W1 | delete pages | `extract_pages` keeps a range; there is no delete |
| W2 | reorder pages | |
| W3 | insert pages at an index | merge only appends |
| W4 | crop / MediaBox | |
| W5 | extract images | insert exists, extract does not |
| W6 | extract tables | `pdf_xlsx::extract_tables` exists; not surfaced |

### Building — nothing exists yet

| # | capability | route |
|---|---|---|
| B1 | **XFA barcodes** | `barcoders` (MIT) 1D + `rxing` (Apache-2.0) 2D, both pure Rust; we draw the modules ourselves |
| B2 | **OCR — cloud integration** | decided: refer to AWS Textract / Azure / Google rather than ship models. So build the *integration surface*, not an engine — see below |
| B3 | **HTML → PDF** | decided: Chromium-based. Not pure Rust, so it needs an explicit exception to the architecture rule |
| B4 | image watermark | `add_watermark` is text-only |
| B5 | page numbers, header/footer | `add_decoration` covers watermark only |
| B6 | XMP metadata | Info dictionary only today |
| B7 | add attachment | `attachments()` is read-only |
| B8 | permissions / linearisation / font inventory accessors | read-side gaps |
| B9 | greyscale conversion, N-up, resize, split-by-bookmark | page-level operations |
| B10 | PDF/UA, PDF/X | PDF/A exists; the sister standards do not. MR !17 is open on PDF/UA |
| B11 | timestamp / TSA | signing exists, timestamping does not |
| B12 | annotation authoring | `annotations()` is read-only |
| B13 | e-invoicing (ZUGFeRD, XRechnung, Peppol) | `pdf-invoice` exists; check what it covers |

### Deferred stubs to resolve

`flatten_forms`, `embed_font`, `linearize` — each ships a method that fails at
runtime. Either implement them or remove the method; a method that always errors
is worse than an absent one, because the type system promised it.

---

## OCR: the decision, and why

Not shipping a local engine. `ocrs` + `rten` are MIT/Apache and pure Rust, and
the code is exactly right — but the **trained models carry no licence at all**
(`robertknight/ocrs-models` has no LICENSE file) and are trained on CC-BY-SA
data. The code licence says nothing about what we may redistribute.

So: **an integration surface, not an engine.** A trait the caller implements or
a thin adapter per provider, so a customer plugs in Textract, Azure Document
Intelligence or Google Vision with their own credentials. Nothing to license,
nothing to ship, and it matches what the three existing how-to pages already
describe.

`ocr-tesseract` and `ocr-paddle` should stop being empty flags: either they gate
the optional C++ engines honestly, or they go.

---

## HTML → PDF: needs an explicit decision

There is no pure-Rust option. Everyone who does this properly drives Chromium,
which conflicts with the architecture rule in `CLAUDE.md`. The realistic choices:

1. **Chromium, as an explicit documented exception** — heavy, and it is what the
   market expects.
2. **A defined subset** on `html5ever` + `taffy` + `cssparser` (all usable
   licences) — "HTML and CSS for documents", no JavaScript. Buildable, honest,
   and covers invoices and reports, which is most of the real demand.

This is the only capability where the promise itself should be settled before
any code is written.

---

## Definition of done applies to all of it

Nothing above counts as delivered until it has a test that runs in the pipeline
(`CLAUDE.md`), and until `scripts/ci/feature_promises.py` shows it both **tested**
and **reachable from the facade**. Coverage without reachability is not a kept
promise — that gate was reporting 11/11 while Excel and PowerPoint could not be
called at all.
