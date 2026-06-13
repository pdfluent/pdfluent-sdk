# AcroForm Support Contract

**Status:** Stable for 1.0. Applies to standards-compliant ISO 32000-1
(PDF 1.7) and ISO 32000-2 (PDF 2.0) AcroForms.
**Scope:** Interactive form **reading** and **filling**. XFA dynamic forms are
out of scope (see [XFA_FEATURE_SUPPORT.md](XFA_FEATURE_SUPPORT.md)).

This document defines exactly what "AcroForm support" means across the SDK and
its language bindings — the field types, the writeback behaviour, the
guarantees, and the explicit exclusions. It is the reference for the corpus
gate (`crates/pdf-forms/tests/corpus_gate.rs`) and the external-compatibility
verifier (`scripts/forms/verify_acroform_output.py`).

---

## 1. Supported field types

Every standard AcroForm field type (ISO 32000-1 §12.7.4) is **modeled** (read
into [`FormFieldModel`]) and, where it carries a value, **fillable** through
the single writeback chain.

| Field type (`/FT` + `/Ff`) | Modeled | Fillable | Fill API |
|---|---|---|---|
| Text — single-line (`/Tx`) | ✅ | ✅ | `Text` |
| Text — multiline (`/Tx`, bit 13) | ✅ | ✅ | `Text` (greedy word-wrap appearance) |
| Text — comb (`/Tx`, bit 25 + `/MaxLen`) | ✅ | ✅ | `Text` (per-cell appearance) |
| Text — password (`/Tx`, bit 14) | ✅ | ✅ | `Text` (masked appearance) |
| Text — rich text (`/Tx`, bit 26) | ✅ | ✅ | `Text` (stored as plain `/V`; XHTML not synthesised) |
| Text — file-select (`/Tx`, bit 21) | ✅ | ✅ | `Text` (path stored verbatim) |
| Checkbox (`/Btn`) | ✅ | ✅ | `Checkbox(bool)` |
| Radio group (`/Btn`, bit 16) | ✅ | ✅ | `Radio(export)` |
| Combo box (`/Ch`, bit 18) | ✅ | ✅ | `Choice(option)` |
| Editable combo (`/Ch`, bits 18+19) | ✅ | ✅ | `Choice(any)` |
| List box — single (`/Ch`) | ✅ | ✅ | `Choice(option)` |
| **List box — multi-select (`/Ch`, bit 22)** | ✅ | ✅ | **`apply_choice_multi` / `set_multi_select`** |
| Push button (`/Btn`, bit 17) | ✅ | ❌ (no value) | rejected: `WrongType` |
| Signature (`/Sig`) | ✅ | ❌ (signing is a separate API) | rejected: `WrongType` |

[`FormFieldModel`]: ../crates/pdf-forms/src/model.rs

---

## 2. Writeback behaviour

Every fill updates the document so it renders correctly in conforming
viewers **without** relying on `/NeedAppearances`, except where noted.

A single fill of a value-carrying field updates, in order:

1. **`/V` (field value)** —
   - Text/choice: text string. **ASCII-literal** when representable, else
     **UTF-16BE with BOM** (ISO 32000-1 §7.9.2.2).
   - Button (checkbox/radio): a `/Name` object, byte-exact (handles Latin-1
     on-state names such as a raw `0xF6`).
   - Multi-select: an **array** of text strings.
2. **`/AS` (appearance state)** — synced on every button widget so the visible
   state matches `/V`. Radio: exactly the selected kid is on, siblings `Off`.
3. **`/AP /N` (appearance stream)** — regenerated for text and single-choice
   widgets: WinAnsi-encoded, AFM-measured, with comb-cell, multiline-wrap,
   quadding and password-mask support. Buttons reuse their existing `/AP`.
4. **`/I` (selected-index cache)** — for multi-select, rebuilt as the sorted,
   de-duplicated zero-based indices into `/Opt` (Acrobat-faithful).
5. **`/NeedAppearances`** — set **only** as a fallback when a text value is not
   WinAnsi-representable (e.g. CJK), in which case the stale `/AP` is removed so
   no viewer shows an outdated value. Multi-select sets it because per-option
   highlight rendering is viewer-native.

### Guarantees

- **Hierarchical names** (`parent.child`) are resolved through `/Kids`
  recursion in every setter.
- **Read-only** fields (`/Ff` bit 1, inherited) are rejected at set time
  (`ReadOnly`).
- **`/MaxLen`** truncates text by character count.
- **Choice validation**: non-editable combo/list values must be in `/Opt`
  (`InvalidOption`); editable fields accept free text.
- **Inline `/AcroForm`** dictionaries are promoted to indirect objects before
  mutation.
- Output round-trips: a third-party reader (pikepdf/qpdf) reads back the
  correct `/V`, `/AS`, `/AP`, `/I`, `/NeedAppearances` (proven by
  `scripts/forms/verify_acroform_output.py`).

---

## 3. Repairing tool-filled documents

`regenerate_appearances` / `PdfDocument::regenerate_form_appearances`
materialise trustworthy `/AP /N` for every filled text/choice field — for
PDFs filled by tools that only wrote `/V` (+ `/NeedAppearances`). Values that
are not WinAnsi-representable are left to the viewer and counted in the
outcome.

---

## 4. Known exclusions

These are **out of scope** for AcroForm fill and are not defects:

- **Push buttons** and **signature fields** are read-only by nature; filling
  them returns `WrongType`. (Digital signing is the separate `pdf-sign` API.)
- **Rich-text (`/RV`)**: the plain `/V` is written; the SDK does not synthesise
  XHTML/`/RV` markup.
- **JavaScript / calculation actions** (`/AA`, `/CO`): field actions are read
  (`has_actions`) but **not executed**. The SDK does not run form JS.
- **XFA dynamic forms**: handled by `pdf-xfa`, not this contract. A static-XFA
  *shell* (an AcroForm carrying an inert `/XFA`) **is** supported — the
  AcroForm side fills normally.
- **Non-WinAnsi appearance rendering**: values outside WinAnsi (CJK, etc.) are
  stored correctly in `/V` (UTF-16BE) but their `/AP` is deferred to the viewer
  via `/NeedAppearances`; the SDK's own renderer does not synthesise a CJK
  glyph appearance. Call `regenerate_form_appearances` is a no-op for these.
- **Widget rotation (`/MK /R`)** is not applied in generated appearances.

---

## 5. What "100% supported" means — and does not mean

**Means:** every standard AcroForm field type in the table above can be read
and (where it has a value) filled through one writeback chain, on every
supported surface (Rust SDK + Node + Python + Java + WASM); the saved output is
structurally correct per ISO 32000 and reads back in third-party tools; the
behaviour is covered by an in-CI corpus gate spanning all categories plus a
real 289-field government form.

**Does not mean:** executing form JavaScript, rendering glyph appearances for
scripts the Standard-14 WinAnsi fonts cannot draw, synthesising XHTML rich
text, signing signature fields, or processing XFA dynamic forms. Those are
either separate APIs or explicit non-goals.

---

## 6. Surface availability

| Capability | Rust SDK | Node | Python | Java | WASM |
|---|---|---|---|---|---|
| Enumerate / read fields | `form_model` / `form_fields` | `formFields` | `get_form_fields` | `getFormFields` | via `metadata` (read), native full |
| Fill text/checkbox/radio/choice | `form_mut().set_*` | `setFieldValue` | `set_form_field` | `setFormField` | `PdfDocMut.setFormField` |
| **Fill multi-select list box** | `form_mut().set_multi_select` | `setMultiSelect` | `set_form_field_multi` | `setMultiSelect` | `PdfDocMut.setMultiSelect` |
| Regenerate appearances | `regenerate_form_appearances` | — | — | — | — |

The C-API exposes form **enumeration** only (`pdf_form_field_count` /
`pdf_form_field_name`); form fill via the C-API is not exposed and is tracked
as a follow-up. `PdfDoc.setFormField` in WASM (the stateless one-shot handle)
is **text-only by design** — use the stateful `PdfDocMut` for all field types.
