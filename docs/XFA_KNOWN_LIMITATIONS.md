# XFA SDK — Known Limitations

Enterprise reference document. Last updated: **2026-08-18**.

**Every entry below was re-verified against the source on 2026-08-18**, method
recorded per item. The previous revision was four months old and one entry
(L-001) described a deliberate security policy in the language of a defect,
which reads to a buyer as something we failed to build rather than something we
chose. That is corrected here; the technical facts were and are accurate.

---

This document lists known limitations of the XFA SDK for production deployment. Each
limitation includes a description, business impact, affected form types, workaround, and
roadmap status. Limitations are classified as **Critical** (must communicate to enterprise
customers) or **Minor** (low-frequency edge cases).

---

## Critical Limitations

### L-001: JavaScript Is Never Executed (by design)

> **This is a security policy, not a missing feature.** Document-supplied
> JavaScript is parsed for inspection and never run — see
> `crates/pdf-xfa/src/javascript_policy.rs`, which denies execution for every
> document entrypoint and strips JavaScript during flattening. A PDF processing
> service that executes script from an untrusted upload is a liability, and for
> most enterprise intake pipelines this behaviour is the requirement rather than
> the compromise. It is listed here because it has consequences for output, not
> because it is a defect to be fixed.
>
> **FormCalc is a different matter and is implemented**: a full lexer, parser
> and interpreter (~5,800 lines, 135 built-in functions, SOM resolution) runs
> `calculate` and `validate` events per XFA 3.3 §14.3.2. Forms scripted in
> FormCalc — the majority of forms authored in LiveCycle/Designer — do compute.

**Description:** Scripts with `contentType="application/x-javascript"` are not executed.
The form is processed with field values as-bound from the XFA data packet, ignoring any
JS-driven visibility, calculation, or validation logic.

**Impact:** Forms relying on JavaScript for field visibility (show/hide), calculated field
values, or runtime validation will render with all fields visible and no calculated values
populated. The output is structurally valid but semantically incomplete for JS-dependent
forms.

**Affected:** ~15–25% of enterprise XFA forms (estimated from corpus analysis).

**Workaround:** Use forms with FormCalc scripting instead of JavaScript. Alternatively,
pre-process forms to inline final field values into the XFA data packet before submitting
to the SDK, so the rendered output reflects the intended state.

**Roadmap:** Deliberately not planned. Executing document-supplied JavaScript would
undo the policy above. If a customer needs JS-driven values, the supported route is to
resolve them upstream and submit the resulting data packet.

**Verified 2026-08-18:** `javascript_policy.rs` — `execution_policy()` denies all
document entrypoints; `strip_javascript_for_flatten()` removes it; the flatten path logs
the denial. Unchanged.

---

### L-002: Barcode Fields Rendered as Empty Boxes

**Description:** `<barcode>` elements are rendered as empty placeholder rectangles. The
barcode data (from XFA data binding) is present internally but is not encoded or rendered
visually.

**Impact:** Forms with barcode fields will show blank boxes in the flattened PDF instead of
scannable barcodes. Affected use cases include shipping labels, inventory tags, retail
price labels, and patient wristbands.

**Affected:** Logistics, inventory, retail, and healthcare forms that include barcode fields.

**Workaround:** Post-process the flattened PDF to add barcodes using a barcode library.
The XFA data binding values are preserved in the flattening pipeline; coordinate the
barcode data extraction from the source XFA with the post-processing step.

**Roadmap:** Candidate for a future sprint (medium complexity). Requires mapping XFA
`<barcode>` attributes (`symbology`, `dataLength`, `wideNarrowRatio`, `dataColumnCount`)
to a barcode encoding library.

**Verified 2026-08-18: still accurate.** Barcode fields are fully recognised —
`<ui><barcode>` parses to `FieldKind::Barcode`, layout honours the no-split rule
(XFA 3.3 §8.7), and they are correctly excluded from the fillable-field count.
Only the last step is missing: there is no encoder anywhere in the workspace (no
Code128/Code39/EAN/QR/PDF417/DataMatrix, no barcode dependency, no module-width
or quiet-zone handling). The scaffolding is in place, so this is bounded work
rather than research.

---

### L-003: Digital Signature Fields Stripped

**Description:** XFA `<signature>` widgets are stripped during flattening. No visual
placeholder is rendered in the flattened output, and no signature appearance stream is
generated.

**Impact:** Signature fields will not appear in the flattened PDF. Workflows requiring
legally binding documents must sign the flattened PDF separately using a dedicated PDF
signing solution.

**Affected:** All forms containing `<signature>` widget elements.

**Workaround:** After flattening, add signature fields and sign the PDF using a PDF signing
library or a qualified trust service provider (TSP). The flattened content is stable and
byte-range signing is supported by downstream signing tools.

**Roadmap:** Rendering a visible placeholder box (without cryptographic signing) is planned.
Actual cryptographic signing is out of scope for the flattening SDK.

**Verified 2026-08-18: still accurate.** `<signature>` parses to
`FieldKind::Signature`, the flatten path logs "elements skipped", and no
appearance stream is generated.

---

### L-004: Password-Protected PDFs Return Error

**Description:** PDFs protected with a user password cannot be processed. PDFs with only an
owner password (empty user password) are supported and will be processed normally.

**Impact:** End-users who have applied a user password to their PDF forms cannot submit
those files to the SDK directly. The SDK returns an error rather than a partial result.

**Affected:** ~3–5% of enterprise corpora (estimated). More common in HR, legal, and finance
where document protection is policy-enforced.

**Workaround:** Pre-process the PDF with the user-supplied password to remove or bypass
protection before submitting to the SDK. This can be automated in an intake pipeline.

**Roadmap:** An API parameter for supplying a user password at call time is planned, allowing
the SDK to decrypt and process the form without requiring a separate pre-processing step.

**Verified 2026-08-18: still accurate.** The decrypt path returns
`DecryptResult::NeedsPassword` for user-password files; the explicit
`load_mem_with_password(bytes, "")` call covers exactly the empty-user-password
case described above. Neither `flattenXfa()` nor the native session takes a
password argument.

---

## Minor Limitations

### L-005: Overflow Bookend Leaders Not Supported

**Description:** XFA §8.10 defines "bookend leaders" — overflow leader/trailer content that
appears on every page of a multi-page overflow sequence. The SDK supports basic overflow
(content continues on the next page) but does not implement bookend leader semantics.

**Impact:** Low. Forms using bookend leaders will overflow correctly, but the repeating
leader content will not appear on each page. Affected forms are rare in practice.

**Workaround:** Redesign the form template to use per-page leaders via standard overflow
leader/trailer, which is supported on a per-page basis (see L-008).

**Verified 2026-08-18: still accurate.** `layout.rs` records §8.10 as
"✅ per-page leader/trailer; ⚠️ overflow/bookend".

---

### L-006: FormCalc Complex Expressions May Not Execute

**Description:** The FormCalc interpreter handles basic arithmetic, string operations, and
common built-in functions. Complex expressions involving advanced financial functions,
deeply nested conditionals, or non-standard extensions may not execute and will fall back
to the uncomputed field value.

**Impact:** Medium. Basic FormCalc (the majority of enterprise usage) works correctly.
Complex financial or actuarial FormCalc may produce blank or zero values in calculated
fields.

**Workaround:** Review FormCalc expressions in affected forms. Simplify or replace complex
expressions with pre-computed values in the XFA data packet where possible.

**Verified 2026-08-18: accurate, but the heading understates what exists.** The
interpreter is complete enough to matter — ~5,800 lines across lexer, parser,
interpreter, builtins and a SOM bridge, with 135 built-in functions. Individual
builtins that are not fully implemented raise a specific spec-referenced error
rather than failing quietly, so a form hitting one is diagnosable.

---

### L-007: Rich Text / XHTML Rendering Partial

**Description:** XFA supports rich text content via XHTML in `<exData>` elements and
`<text>` fields. The SDK renders basic inline formatting (bold, italic, basic span styles)
but does not support full XHTML layout, block-level elements, lists, or complex CSS
properties.

**Impact:** Low to medium. Most enterprise forms use plain text or simple bold/italic
formatting, which is fully supported. Forms using structured XHTML content (tables within
text, nested lists) may render with reduced fidelity.

**Workaround:** Review rich text fields in affected forms. For critical content, replace
XHTML rich text with plain text or simplified markup.

**Verified 2026-08-18: still accurate.** `RichTextSpan` parses
`<exData contentType="text/html">`, so support is partial as described rather
than absent.

---

### L-008: Overflow Leader/Trailer Scoped Per-Page Only

**Description:** Overflow leader and trailer subforms are rendered per-page (appearing once
on the first overflow page and once on the last). XFA also defines cross-page "bookend"
semantics (see L-005) where leaders repeat on every intermediate page; this is not
supported.

**Impact:** Low. Standard per-page leader/trailer overflow — the most common usage — is
fully supported. Only the "bookend" repeating variant is affected.

**Workaround:** None required for standard overflow leader/trailer usage.

**Verified 2026-08-18: still accurate.** `layout.rs` resolves `<overflow leader
trailer>` SOM references, and the same §8.10 line marks overflow scope as
partial.

---

## Font Rendering Notes

The SDK resolves fonts using the following priority order:

1. Fonts embedded in the PDF
2. System fonts matched by family name and weight
3. Fallback font substitution

When a requested font is not available, the SDK substitutes the closest available system
font. Substituted fonts may have different glyph metrics (advance widths, kerning), which
can cause minor text reflow and character spacing differences compared to the reference
rendering.

**SSIM impact:** Font substitution is the dominant source of SSIM score degradation in
corpus benchmarks. Metrics drift from substituted fonts typically results in SSIM scores
in the 0.90–0.96 range rather than 0.98+. This is expected behavior, not a rendering
defect. Deployers who require pixel-accurate rendering should ensure the required fonts
are available on the deployment system or embedded in the source PDFs.

CID fonts with `/W` (glyph width) arrays are fully supported, which covers the primary
path for CJK (Chinese, Japanese, Korean) and other non-Latin character sets.

---

## Performance Limits

The following latency expectations apply to the current SDK release. All figures are
indicative; confirmed benchmarks will be published in EVH-BASELINE-02.

| Form class | p50 latency | p95 latency |
|---|---|---|
| Simple static (< 5 pages) | < 50 ms | < 150 ms |
| Standard dynamic (5–20 pages) | < 200 ms | < 500 ms |
| Complex multi-page (20–50 pages) | < 500 ms | < 1500 ms |
| Large forms (> 50 pages) | TBD | TBD |

Note: Latency is dominated by PDF parsing and font resolution for first-run processing.
Repeated processing of forms with the same font set benefits from font cache warmup.
Password-protected and heavily scripted forms are excluded from these estimates.

**To be confirmed by EVH-BASELINE-02 corpus baseline run.**
