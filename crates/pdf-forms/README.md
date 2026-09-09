# pdfluent-forms

AcroForm engine for PDF interactive forms — parse, fill, inspect, and export
form data.  Powers the form layer of the [PDFluent](https://pdfluent.com) Rust
SDK and all language bindings.

**AGPL-3.0 or a commercial licence**, at your option — see the Licence section below.

## What it does

- **Parse** any ISO 32000 AcroForm into a typed field tree — text, checkbox,
  radio, combo/list, push-button, and signature fields; hierarchical names
  (`parent.kid`); inherited flags and attributes.
- **Fill** fields reliably: `apply_field_value` is the single writeback chain
  for the whole SDK. One call keeps `/V`, per-widget `/AS`, and regenerated
  `/AP` streams consistent, working in every viewer without `/NeedAppearances`.
- **Inspect** the complete form model: one `FormFieldModel` per logical field
  with per-page widget rectangles, typed kind, on-state names, choice options,
  comb/multiline/password flags, access flags, and resolved font info.
- **Regenerate** appearances for documents filled by other tools that only
  wrote `/V` (e.g. with `/NeedAppearances true`).

## Quick Start

```rust
use std::sync::Arc;
use lopdf::Document;
use pdfluent_forms::{
    apply_field_value, build_form_model, parse_acroform, WriteValue,
};

// --- Read the form model ---
let pdf = pdf_syntax::Pdf::new(Arc::new(std::fs::read("form.pdf")?)).unwrap();
let tree = parse_acroform(&pdf).expect("no AcroForm");

for field in build_form_model(&tree) {
    println!("[{:?}] {} = {:?}", field.kind, field.name, field.value);
}

// --- Fill a field ---
let mut doc = Document::load("form.pdf")?;
apply_field_value(&mut doc, "Name", WriteValue::Text("Jane Doe"))?;
apply_field_value(&mut doc, "Agree", WriteValue::Checkbox(true))?;
apply_field_value(&mut doc, "Country", WriteValue::Radio("NL"))?;
doc.save("form-filled.pdf")?;
```

## Writeback Guarantees

`apply_field_value` maintains the following invariants, mirroring the
convergent behaviour of pdfium, mupdf, and pdf.js:

| Property | Guarantee |
|---|---|
| `/V` encoding | ASCII → PDF literal string; non-ASCII → UTF-16BE+BOM whole string |
| Button `/V` | Byte-exact `/Name` object; WinAnsi re-encoding fallback for Latin-1 on-state names |
| `/AS` consistency | Set per widget to the on-state iff that name is a key of the widget's own `/AP /N`; else `/Off` (mupdf `set_check_grp` rule) |
| `/AP /N` | Regenerated as a self-contained Form XObject: `/Tx BMC … EMC`, WinAnsiEncoding show-text bytes, embedded Standard-14 AFM widths, comb/multiline/quadding support, ~2 pt inset |
| Font resources | Self-contained `/Resources` with explicit `WinAnsiEncoding` — never references `/DR` fonts (which carry PDFDocEncoding Differences that remap characters) |
| `/NeedAppearances` | Set `true` only for non-WinAnsi values (Cyrillic, CJK, …); stale `/AP` removed in that case |
| Read-only | Rejected at set-time (`WritebackError::ReadOnly`) — the SDK has no UI layer, so this is the only enforceable point |
| Inline `/AcroForm` | Promoted to an indirect object before mutation (most real-world documents keep `/AcroForm` inline) |
| Hierarchical names | Resolved through `/Kids` recursion, not top-level-only |

## API Reference

### Entry points

| Function | Description |
|---|---|
| `parse_acroform(pdf)` | Parse an AcroForm from a `pdf_syntax::Pdf`; `None` if absent |
| `build_form_model(tree)` | Build `Vec<FormFieldModel>` from a parsed `FieldTree` |
| `apply_field_value(doc, name, value)` | Fill one field in a `lopdf::Document` |
| `apply_choice_multi(doc, name, &[String])` | Fill a multi-select list box: `/V` array + rebuilt sorted `/I` index cache |
| `regenerate_appearances(doc)` | Regenerate all appearance streams in a `lopdf::Document` |

### Key types

| Type | Description |
|---|---|
| `FieldTree` | Arena-based tree mirroring the AcroForm parent/child hierarchy |
| `FieldNode` | A single field or group (partial name, type, value, flags, …) |
| `FieldId` | Opaque index into the `FieldTree` arena |
| `FieldType` | `Text` / `Button` / `Choice` / `Signature` |
| `FieldValue` | Text string or array of selected strings |
| `FieldFlags` | Bit-field from `/Ff` — read-only, required, multiline, comb, … |
| `FormFieldModel` | Complete editor-facing model for one logical field |
| `FormFieldKind` | Typed variant: `Text`, `Checkbox`, `RadioGroup`, `ComboBox`, `ListBox`, `PushButton`, `Signature` |
| `WidgetModel` | One widget annotation: page index, rect, on-state, `/AS` |
| `DaInfo` | Resolved `/DA`: font name, size, color |
| `WriteValue` | `Text(&str)` / `Checkbox(bool)` / `Radio(&str)` / `Choice(&str)` |
| `WriteOutcome` | How many appearances were generated, whether fallback was used |
| `WritebackError` | `FieldNotFound` / `ReadOnly` / `WrongType` / `InvalidOption` / `Malformed` |

### `FormFieldKind` variants

```rust
pub enum FormFieldKind {
    Text { multiline: bool, comb: bool, password: bool },
    Checkbox { on_state: String, checked: bool },
    RadioGroup { options: Vec<String> },     // one entry per widget
    ComboBox  { editable: bool, options: Vec<ChoiceOption> },
    ListBox   { multi_select: bool, options: Vec<ChoiceOption> },
    PushButton,
    Signature,
}
```

### `FormFieldModel`

```rust
pub struct FormFieldModel {
    pub name:          String,               // fully qualified ("parent.kid")
    pub kind:          FormFieldKind,
    pub value:         Option<String>,       // current /V
    pub default_value: Option<String>,       // /DV
    pub tooltip:       Option<String>,       // /TU
    pub read_only:     bool,
    pub required:      bool,
    pub max_len:       Option<u32>,          // /MaxLen (text fields)
    pub quadding:      Quadding,             // /Q: Left/Center/Right
    pub da:            Option<DaInfo>,       // resolved /DA
    pub widgets:       Vec<WidgetModel>,     // one per visual occurrence
}
```

## Known Limitations

- **Non-WinAnsi values** (Cyrillic, CJK): `/V` is always lossless via UTF-16BE;
  appearance generation falls back to `/NeedAppearances true`. The SDK renderer
  does not honour `/NeedAppearances` — call `regenerate_appearances` after
  opening such documents for correct rendering.
- **DA fonts outside Standard-14**: the nearest standard face is substituted in
  generated appearances (Helvetica unless `/DA` names a Times/Courier variant).
- **Widget rotation** (`/MK /R`): not yet applied in generated appearances.

## Licence

AGPL-3.0 or a commercial licence, at your option. The AGPL is the default and
the complete product: there is no licence key, no activation call and no tier,
and every feature works in every build. If you cannot accept the copyleft
obligation, the commercial licence is sold yearly and self-service at
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing), in four options: Commercial (per
organisation), OEM Startup and OEM (per product), and Priority support (an
add-on). The two texts are `LICENSE` and `LICENSE-COMMERCIAL` in this crate.

## Links

- Main crate: <https://crates.io/crates/pdfluent>
- Documentation: <https://pdfluent.com/docs>
- Pricing: <https://pdfluent.com/sdk/pricing>
- Changelog: [CHANGELOG.md](CHANGELOG.md)
