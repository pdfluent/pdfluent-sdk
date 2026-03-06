# Fase B — AcroForm Engine — Design Decisions

## Architecture

### Arena-based field tree (B.1)
- **Decision:** Use `Vec<FieldNode>` + `FieldId(usize)` arena pattern (same as `xfa-dom-resolver`)
- **Rationale:** Consistent with existing codebase, avoids lifetime complexity of tree references, O(1) node access
- **Consequence:** Parent/child relationships stored as FieldId values; tree walking uses explicit loops

### Dual-library strategy: pdf-syntax + lopdf
- **Decision:** Use `pdf-syntax` (read-only) for parsing, `lopdf` (read-write) for flattening/mutation
- **Rationale:** pdf-syntax is our fork with superior parsing API; lopdf provides mutation capabilities needed for flattening
- **Consequence:** Parse module depends on pdf-syntax, flatten module depends on lopdf; both are already workspace dependencies

### Property inheritance
- **Decision:** Field type, DA, quadding, and value walk up the parent chain; flags do not
- **Rationale:** PDF spec §12.7.3.1 specifies which properties are inheritable. Flags are per-widget.
- **Consequence:** `effective_*` methods on FieldTree handle inheritance transparently

## Field Types

### Text fields (B.2)
- **Decision:** Detect sub-kind from flags: Normal, Multiline, Password, Comb, RichText, FileSelect
- **Rationale:** PDF spec uses flag bits to distinguish text field variants
- **Decision:** MaxLen enforcement in `set_text_value()` truncates rather than rejecting
- **Rationale:** More user-friendly; matches Adobe Reader behavior

### Buttons (B.3 + B.5)
- **Decision:** Combined checkbox, radio, and push button in single module
- **Rationale:** All share `/Btn` field type, distinguished only by flags
- **Decision:** Radio button mutual exclusion walks sibling list from parent
- **Rationale:** PDF spec models radio groups as parent with widget children

### Choice fields (B.4)
- **Decision:** Non-editable combo boxes validate against option list; editable combos accept any value
- **Rationale:** Matches PDF spec behavior for Combo with/without Edit flag

## Appearance Generation (B.6)

### Font metrics approximation
- **Decision:** Use `0.5 * font_size` as character width approximation for text alignment
- **Rationale:** Without embedded font metrics, this gives reasonable centered/right alignment for common fonts
- **Consequence:** Perfect alignment requires font-aware rendering (future enhancement)

### Auto-sizing
- **Decision:** When font_size is 0, auto-size to `(height - 2.0).clamp(4.0, 24.0)`
- **Rationale:** Matches typical form field heights; prevents absurdly small or large text

### Checkbox/radio rendering
- **Decision:** Generate simple geometric shapes (checkmark path for checkbox, circles for radio)
- **Rationale:** Renders correctly without font dependencies; matches common PDF viewer rendering
- **Decision:** Use Bézier curves (κ=0.5523) for radio button circles
- **Rationale:** Standard approximation for circular arcs with cubic Bézier curves

## Actions Framework (B.7)

### JavaScript execution
- **Decision:** Define `JsActionHandler` trait but do not implement JS execution
- **Rationale:** JS execution is out of scope for the core AcroForm engine; trait allows external engines to plug in
- **Consequence:** `run_calculations()` iterates the /CO order but is a no-op without a handler implementation

## Flattening (B.8)

### XObject-based approach
- **Decision:** Create Form XObjects from appearance streams and reference them from page content
- **Rationale:** Standard PDF approach; preserves appearance fidelity; works with any viewer

### Annotation removal
- **Decision:** Remove widget annotations from page /Annots arrays after flattening; optionally remove /AcroForm dictionary
- **Rationale:** Ensures flattened forms appear static; removing /AcroForm prevents viewers from showing form UI

### Signature fields
- **Decision:** Skip signature fields during flattening
- **Rationale:** Flattening signature fields would invalidate digital signatures

## Testing

- 34 unit tests covering all modules
- Tests focus on data model correctness (tree structure, value get/set, flag detection)
- Integration testing with real PDFs deferred to Fase E test infrastructure
