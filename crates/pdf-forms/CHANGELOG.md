# Changelog — pdfluent-forms

All notable changes are documented here.

## [acroform/sdk-foundation] — 2026-06-12

### Added

- `apply_field_value(doc, name, value)` — single writeback chain for all field
  types (text, checkbox, radio, choice). Updates `/V`, per-widget `/AS`, and
  regenerated `/AP /N` Form XObjects in one call. Replaces seven previously
  independent incomplete writeback paths spread across the SDK.
- `regenerate_appearances(doc)` — regenerate `/AP` streams for all filled
  text/choice fields in a lopdf document; for use on PDFs filled externally.
- `build_form_model(tree)` — flatten a `FieldTree` into a `Vec<FormFieldModel>`
  carrying everything an interactive form UI needs (typed kind, widget rects,
  on-states, options, max-len, access flags, DA info).
- `WriteValue` enum: `Text(&str)`, `Checkbox(bool)`, `Radio(&str)`, `Choice(&str)`.
- `WriteOutcome` struct: `appearances_generated`, `appearance_states_set`,
  `need_appearances_fallback`.
- `WritebackError` enum: `FieldNotFound`, `ReadOnly`, `WrongType`, `InvalidOption`,
  `Malformed`.
- `FormFieldModel`, `FormFieldKind`, `WidgetModel`, `DaInfo` — complete
  editor-facing model types.
- `encoding` module: `encode_winansi`, `decode_pdf_text_bytes`, `decode_name_bytes`,
  `escape_string_bytes`.
- `metrics` module: `StandardFace` enum with `glyph_width` and `text_width` helpers
  backed by embedded Standard-14 WinAnsiEncoding AFM tables.
- Integration tests: `crates/pdf-forms/tests/writeback_roundtrip.rs` (14 tests).
- Corpus gate example: `crates/pdf-forms/examples/corpus_gate.rs`.

### Changed

- `FieldNode` gains `pub on_state: Option<String>` field.
- `ChoiceOption` derives `PartialEq`.

## [1.0.0-beta.1] — 2026-05-02

Initial beta release as part of the PDFluent SDK 1.0.0-beta.1.
