# System map — what exists, what does not, how it fits

**Verified 2026-08-18** with authoritative tools (`cargo tree`, `cargo metadata`,
`scripts/ci/api_coverage.py`), not by grepping. Method matters here: nine
structural claims made by searching were wrong in one session, all of them in the
direction of understating what exists. See the `codebase-questions` skill.

**Update this in the same change** as anything below moves. A map that lags is
worse than none, because it is trusted.

---

## The shape of it

48 workspace crates, 32 publishable. Customers reach them one of two ways:

**`pdfluent` — the facade.** One crate, one `Document` type. This is what an SDK
customer is expected to use, and what the website's SDK claims describe.

**The bindings.** Five, all built on the facade or on `pdf-manip` directly:

| binding | package | tests run in CI since |
|---|---|---|
| Python | `pdfluent` (PyPI) | 2026-08-18 |
| WASM | `@pdfluent/sdk-wasm` (npm) | 2026-08-18 |
| Node | `@pdfluent/node` (npm) | 2026-08-18 |
| C ABI | feeds .NET and Java | 2026-08-18 |
| Java | Maven, via JNI | 2026-08-18 |

Before that date none of their test suites was executed by any job, though all
five had tests.

---

## Reachable from the facade

Per `cargo tree -p pdfluent --depth 1`:

`pdf-annot` · `pdf-compliance` · `pdf-docx` · `pdf-engine` · `pdf-interpret` ·
`pdf-manip` · `pdf-redact` · `pdf-render` · `pdf-syntax` · `pdfluent-forms` ·
`pdfluent-lopdf` · `pdfluent-sign`

So these advertised capabilities **are** reachable from the facade: merge, split,
page organisation, compression, password protection, watermarking, redaction,
signing, annotations, forms, rendering to image, PDF/A, and **PDF → Word**
(`Document::to_docx`).

## NOT reachable from the facade

These crates exist, are published, and have tests — but the facade does not
depend on them, **not even with `--all-features`**:

| capability | crate | consequence |
|---|---|---|
| OCR | `pdf-ocr` | `ocr-tesseract` / `ocr-paddle` are empty flags; a customer must add `pdf-ocr` themselves |
| PDF → Excel | `pdf-xlsx` | same |
| PDF → PowerPoint | `pdf-pptx` | same |
| HTML → PDF | — | `html-to-pdf` is an empty flag with no crate behind it |

This matters because the `/features/` page and the SaaSHub listing present OCR
and the Office conversions as SDK capabilities. They are capabilities of the
*suite*; they are not reachable from the crate a customer is told to use.

## Empty feature flags in the facade

Eleven flags enable nothing: `signing`, `redaction`, `ocr-tesseract`,
`ocr-paddle`, `html-to-pdf`, `docx-export`, `xlsx-export`, `pptx-export`,
`xfa-flatten`, `wasm`, `internal-legacy`.

They fall into two groups, and the distinction is the whole point:

- **Harmless.** `signing` and `redaction` are empty because `pdfluent-sign` and
  `pdf-redact` are unconditional dependencies — the capability is always there.
  `docx-export` is documented in the source as reserved for future use.
- **Misleading.** `ocr-tesseract`, `ocr-paddle`, `xlsx-export`, `pptx-export` and
  `html-to-pdf` name capabilities the facade cannot deliver. Enabling one is a
  no-op, which is worse than the flag not existing: it reads as consent.

---

## XFA

Eight limitations, each re-verified against the source on 2026-08-18
(`docs/XFA_KNOWN_LIMITATIONS.md`). Seven still hold.

The one worth knowing: **JavaScript is never executed, by policy**, not for want
of an implementation. `crates/pdf-xfa/src/javascript_policy.rs` denies every
document entrypoint and strips JavaScript during flattening. FormCalc *is*
implemented — a lexer, parser, interpreter and 135 builtins, ~5,800 lines — and
that is what LiveCycle forms are actually scripted in.

Genuine gap: barcodes are parsed, laid out and classified, but never encoded.
There is no barcode encoder anywhere in the workspace.

---

## Who owns which working copy

| path | owner | rule |
|---|---|---|
| `Documents/XFA` (main checkout) | **Kimi**, branch `pdfa/retention-round4` | never build or test here; a stray `maturin` run once rewrote his lockfile mid-session |
| `Documents/XFA/.worktrees/master-r3` | this Claude session | all own work goes here |
| other `.worktrees/*` | historical | dead ends on old branches |
| `Documents/PDFluent/ROADMAP.md` | **shared with the SEO terminal** | edit surgically; never `cat >` |

---

## Where the guards are

| question | gate |
|---|---|
| does every exported symbol have a test? | `sanity:api-coverage` (198/262 today) |
| does every advertised feature have a test? | `sanity:feature-promises` (11/11) |
| does the WASM binding survive the crossing? | `sanity:wasm-binding-smoke` |
| can our output be read back by someone else? | `sanity:external-reader-verification` |
| does our output pass the standard? | `sanity:verapdf-on-our-own-output` |
| ...on the pipeline that blocks a merge? | GitHub `CI (ephemeral Hetzner) / workspace`, step "PDF/A conversion output conforms and keeps its text" — `scripts/ci/pdfa_output_conformance_gate.py`, five fixtures, both axes |
| ...on a thousand real documents? | **only on GitLab**, and only automatically on a schedule or a tag: `corpus:pdfa-holdout` and `corpus:pdfa-holdout-retention` are `when: manual` for a merge request. GitLab has been the mirror since 25-08, so nothing about the holdout stands between a change and master. Written down here rather than implied, because the five-fixture gate above is easy to mistake for the whole answer (#286, blocked on #276 and #284) |
| do tests skip in silence? | `sanity:test-skip-lint` |
| does the whole suite pass? | `quality:cargo-test` — **automatic since 2026-08-18**, was manual with no schedule, so it ran on nothing |
