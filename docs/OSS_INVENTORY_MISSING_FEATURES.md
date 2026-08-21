# Open source for the three unbuilt capabilities

**Date:** 2026-08-18 · **Question:** what can we use for OCR, HTML→PDF and XFA
barcodes, given that PDFluent is proprietary and the architecture rule is *pure
Rust, no C/C++ dependencies* (`CLAUDE.md`)?

Every licence below was read from crates.io metadata and the project's own
repository, not from memory. Where a licence could not be established, that is
stated rather than assumed — which is the whole point of the exercise.

## What "usable" means here

The desktop app is free but the **SDK is licensed commercially**, so we
distribute binaries without source. That rules out copyleft that reaches the
whole work:

| licence | usable | why |
|---|---|---|
| MIT, Apache-2.0, BSD | ✅ | attribution only |
| MPL-2.0 | ✅ with care | file-level copyleft: fine unless we modify those files, and then only those files must be published |
| LGPL | ⚠️ | dynamic linking only; static linking in Rust makes this impractical |
| GPL, AGPL | ❌ | would force us to publish the SDK |
| **no licence at all** | ❌ | default is all rights reserved — worse than a bad licence, because there is nothing to comply with |

---

## 1. XFA barcodes — straightforward, do this first

Everything except the encoder already exists: `<ui><barcode>` parses to
`FieldKind::Barcode`, layout honours the no-split rule (XFA 3.3 §8.7), and the
fields are correctly excluded from the fillable count. Only the step that turns
data into bars is missing.

| crate | version | licence | purity | covers |
|---|---|---|---|---|
| `barcoders` | 2.0.0 | MIT OR Apache-2.0 | pure Rust, 1 dependency | EAN-13/8, UPC-A, Code11, Code39, Code93, Code128, ITF |
| `rxing` | 0.9.2 | Apache-2.0 | pure Rust, 19 deps | the ZXing port: the 1D set plus PDF417, DataMatrix, QR, Aztec |

**Recommendation: `barcoders` for 1D, `rxing` for 2D.** Both permissive, both
pure Rust. `barcoders` has one dependency, which is about as low-risk as a
dependency gets. `rxing` pulls `chrono` (and through it the only `-sys` crate in
the whole set, `core-foundation-sys` — Apple's own frameworks, nothing to build
or ship).

Neither emits PDF; they produce module patterns, and we draw the rectangles into
the content stream ourselves. That is a good split: the geometry is ours, which
is where our precision already lives.

**Estimated shape:** one crate, `pdf-barcode`, mapping the XFA `<barcode type>`
attribute to a symbology, emitting a module vector, and a drawing routine in the
XFA flatten path. Bounded work, no research.

---

## 2. OCR — the code is fine, the models are the problem

The existing `pdf-ocr` crate offers two engines and **both are off by default**,
which is why the facade does not depend on it:

- `tesseract` → `leptess`, which binds Tesseract and Leptonica (C++)
- `paddle` → `ort`, the ONNX Runtime (C++, loaded dynamically)

Either would break the pure-Rust rule. So the interesting question is whether a
Rust-native OCR engine exists.

| crate | version | licence | purity | note |
|---|---|---|---|---|
| `ocrs` | 0.12.2 | MIT OR Apache-2.0 | pure Rust, 7 deps | detection + recognition pipeline |
| `rten` | 0.25.0 | MIT OR Apache-2.0 | pure Rust, 20 deps | the inference engine underneath; runs ONNX-derived models |

The code is exactly what we need and the licences are clean.

### The trap

**The trained models are a separate work from the code, and their licence is not
established.**

- `robertknight/ocrs-models` has **no LICENSE file**; GitHub's own licence
  detection reports none. Default position: all rights reserved.
- The models are trained on **HierText, which is CC-BY-SA 4.0** — share-alike.
  Whether trained weights are a derivative of their training data is legally
  unsettled, and "unsettled" is not a basis for shipping a commercial SDK.

This is precisely the case where the code licence tells you nothing about what
you may ship.

### Three ways forward

1. **Ask the author to state a licence** on the model weights. Cheapest by far.
   If he intends MIT/Apache, one file settles it.
2. **Train our own models** with `ocrs-models`' tooling on datasets we have
   cleared. Real work, and the CC-BY-SA question comes back unless the dataset
   is chosen for it.
3. **Keep OCR out of the facade** and stop advertising it as an SDK capability
   until 1 or 2 lands.

**Recommendation: 1, then 3 as the honest interim.** Until the weights have a
licence, shipping them in a commercial SDK is a risk that is not ours to take
quietly.

---

## 3. HTML → PDF — no ready-made pure-Rust option exists

There is no crate that does this. What exists:

| approach | licence | purity | verdict |
|---|---|---|---|
| `chromiumoxide` — drive a real Chrome | MIT/Apache | ❌ needs a browser (~150 MB) | what wkhtmltopdf, WeasyPrint and Puppeteer all do, and the reason they are heavy |
| Build on the Servo pieces | see below | ✅ | a browser engine is a multi-month project |
| `typst` | Apache-2.0 | ✅ pure Rust | excellent, but it renders *Typst markup*, not HTML |

Building blocks, if we go that way:

| crate | licence | role |
|---|---|---|
| `html5ever` | MIT OR Apache-2.0 | HTML parsing |
| `cssparser` | **MPL-2.0** | CSS parsing |
| `selectors` | **MPL-2.0** | selector matching |
| `taffy` | MIT | flexbox/grid layout |

All usable. The two MPL-2.0 crates are fine as long as we do not modify them; if
we do, only those files must be published.

**Recommendation: decide what the promise means before building anything.**
"HTML → PDF" as customers understand it means *any* web page, which is a browser
engine — nobody does that without Chromium. A defensible subset ("HTML and CSS
for documents: text, tables, images, page breaks; no JavaScript, no floats") is
buildable on the four crates above and is genuinely useful for invoices and
reports, which is what most such requests actually are.

Of the three, this is the only one where I would question the promise rather than
plan the work. It is currently an empty feature flag with **no crate behind it at
all** — the only capability we advertise where nothing exists.

---

## Summary

| capability | can we build it | licence risk | effort |
|---|---|---|---|
| XFA barcodes | **yes, now** | none | small, bounded |
| OCR | code yes, **models blocked** | **model weights unlicensed** | small once cleared |
| HTML → PDF | only as a defined subset | none for the subset | large |

Nothing here requires a licence we cannot live with, except the one thing that
carries no licence at all.
