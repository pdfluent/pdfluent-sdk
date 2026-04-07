# T3 Spec Review: XFA 3.3 Chapters 1-2 vs Implementation

Review date: 2026-04-07
Spec: XFA Specification 3.3, pages 16-73
Reviewer: Claude (systematic review)

---

## Chapter 1: Introduction to XFA (p16-30)

### §1.1 Key Features (p16) — ✅ Correct
XFA capabilities: workflow, dynamic interactions, dynamic layout, scalability.
Our engine supports all core features: template parsing, data binding, layout, rendering.

### §1.2 Scenarios for Using XFA (p16-18) — ✅ Correct
Scenarios: interactive user forms, printing, machine-generated data.
Our flatten pipeline covers the "printing" and "pre-rendering" scenarios.

### §1.3 Family of XFA Grammars (p18-22) — ✅ Correct
Grammars: datasets, data, template, PDF, config, etc.
We extract all key packets: template, datasets, config.
XDP and shell PDF packaging correctly handled in `extract.rs`.

### §1.4 Major Components: Template and Data (p22-26) — ✅ Correct
- Template defines appearance and behavior ✅
- Data provides variable content ✅
- draw = fixed content, field = variable content ✅
- Containers of other containers: subform, exclGroup, area ✅ (area added this review)
- Scripted components: FormCalc and JavaScript ✅ (partial)

### §1.5 Data Binding (p26) — ✅ Correct
- Like-named mapping of data values to template fields ✅
- Repeated subforms from data ✅ (expand_repeating_subform)
- Static vs dynamic binding distinction ✅

### §1.6 Lifecycle of an XFA Form (p27-28) — ✅ Correct
Steps: draw initial form → associate data → display data → activate events → update.
Our pipeline: parse template → bind data → layout → render → write PDF.

### §1.7 Static versus Dynamic Forms (p28-30) — ✅ Correct
- Static (XFAF): boilerplate in PDF, fields in XFA. `baseProfile="interactiveForms"` ✅
- Dynamic: all content in XFA, NeedsRendering=true ✅
- Detection in `flatten.rs:296` correctly checks baseProfile ✅
- Static forms preserve original PDF pages (commit 0be8baa8) ✅

---

## Chapter 2: Template Features for Designing Static Forms (p31-73)

### §2.1 Form Structural Building Blocks (p31-35) — ✅ Correct (fixed)

**Spec requires 5 container elements:** subform, field, draw, exclGroup, area.

| Element | Status | Notes |
|---------|--------|-------|
| subform | ✅ | Full support: layout, children, occur, break, keep |
| field | ✅ | Full support: value, caption, UI widgets, font, border |
| draw | ✅ | Text, line, rectangle, arc, image |
| exclGroup | ✅ | Radio button promotion from checkButton |
| area | ✅ **FIXED** | Was falling through to `blank_node`; now parsed as subform-like container |

**Content elements (p35):**
- value → content type → default data hierarchy ✅
- Content types: text, image, exData ✅

**User Interface (p35-36):**

| Widget | Status | Notes |
|--------|--------|-------|
| textEdit | ✅ | Default widget |
| checkButton | ✅ | shape="round" → radio |
| button | ✅ **FIXED** | Was not detected; now maps to FieldKind::Button |
| choiceList | ✅ | Dropdown |
| dateTimeEdit | ✅ | DateTimePicker |
| numericEdit | ✅ | NumericEdit |
| passwordEdit | ✅ | PasswordEdit |
| imageEdit | ✅ | ImageEdit |
| signature | ✅ | Signature |
| barcode | ⚠️ Gap | Detected but no barcode rendering engine |

### §2.2 Basic Composition (p36-39) — ✅ Correct (enhanced)

**Measurements (p36-37):**

| Unit | Status | Notes |
|------|--------|-------|
| in (inches, default) | ✅ | Default unit for dimensions |
| pt (points, 1/72 inch) | ✅ | Default unit for font sizes |
| cm (centimeters) | ✅ | |
| mm (millimeters) | ✅ | |
| em (em width) | ✅ | Approximated at 12pt default |
| % (space width percentage) | ✅ **ADDED** | Was missing; now parsed as MeasurementUnit::Percent |

**Angles (p37):** Not yet needed (see rotate TODO under §2.6).

**Border Formatting (p38-39):**
- Borders with edge/corner elements ✅
- Per-edge visibility ✅
- Fill (solid, color) ✅
- Pattern fills (hatching, stippling, gradient) ❌ Not implemented
- Corner radius ✅

### §2.3 Content Types (p39-42) — ⚠️ Partial

| Content Type | Status | Notes |
|-------------|--------|-------|
| text | ✅ | Extracted from `<value><text>` |
| date, time, dateTime | ⚠️ | Treated as text (no canonical format validation) |
| boolean | ⚠️ | Treated as text |
| integer, decimal, float | ⚠️ | Treated as text |
| image | ✅ | JPEG, PNG, BMP→PNG conversion |
| exData (rich text HTML) | ✅ | Plain text extraction from XHTML |
| exObject | ❌ | Not handled |
| line, rectangle, arc | ✅ | Via DrawContent enum |

### §2.4 Formatting Text (p43-48) — ⚠️ Partial

**Alignment and Justification (p44):**

| hAlign value | Status | Notes |
|-------------|--------|-------|
| left | ✅ | Default |
| center | ✅ | |
| right | ✅ | |
| justify | ✅ | Maps to TextAlign::Justify |
| justifyAll | ⚠️ | Maps to Justify (should distribute all lines including last) |
| radix | ❌ | Requires radixOffset support |

| vAlign value | Status | Notes |
|-------------|--------|-------|
| top | ✅ | Default |
| middle | ✅ | |
| bottom | ✅ | |

**Other formatting features:**

| Feature | Status | Notes |
|---------|--------|-------|
| lineHeight (para) | ❌ | Not parsed from `<para lineHeight>` |
| textIndent (para) | ❌ | Not parsed |
| spaceAbove/spaceBelow | ✅ | Parsed in parse_node_style |
| marginLeft/marginRight | ✅ | Parsed in parse_node_style |
| Tab stops (tabStops, tabDefault) | ❌ | Not implemented |
| Hyphenation | ❌ | Not implemented |
| Rich text HTML formatting | ⚠️ | Text extracted but formatting not rendered |
| Picture clauses (format) | ❌ | Not implemented |
| Barcode formatting | ⚠️ | Detected but not rendered |

**Font properties (p57-58):**

| Property | Status | Notes |
|----------|--------|-------|
| typeface | ✅ | Parsed, default Courier for data-entry |
| size | ✅ | Parsed, default 10pt |
| weight (bold/normal) | ✅ | |
| posture (normal/italic) | ✅ | |
| fontHorizontalScale | ✅ | Parsed in parse_node_style |
| letterSpacing | ✅ | Parsed (em and pt units) |
| baselineShift | ❌ | Not parsed |
| kerningMode | ❌ | Not parsed |
| lineThrough / lineThroughPeriod | ❌ | Not parsed |
| overline / overlinePeriod | ❌ | Not parsed (Note: Adobe doesn't implement either) |
| underline / underlinePeriod | ❌ | Not parsed |

### §2.5 Access Restrictions (p48-49) — ❌ Not implemented

Spec defines 4 access levels: nonInteractive, protected, readOnly, open (default).
**The `access` attribute is not parsed anywhere in the codebase.**
Impact: Low for static form rendering (forms are read-only when flattened).

### §2.6 Basic Layout (p49-70) — ⚠️ Mostly correct (1 bug fixed)

**Box Model (p49-50):** ✅
- Nominal extent (w × h) ✅
- Margins (via `<margin>` element) ✅
- Caption region (placement + reserve) ✅
- Content region calculation ✅
- min/max constraints ✅

**Presence attribute (p67-68):** ✅ **BUG FIXED**

| presence | Spec: Layout space? | Old code | Fixed code |
|----------|-------------------|----------|------------|
| visible | Yes | ✅ | ✅ |
| invisible | Yes (space, not visible) | ❌ No space | ✅ Space reserved |
| hidden | No (absent) | ❌ Space reserved | ✅ No space |
| inactive | No (absent) | ✅ | ✅ |

**`is_layout_hidden` was swapped between hidden/invisible.**
- `invisible` elements were excluded from layout (should take space)
- `hidden` elements were included in layout (should NOT take space)
- **Fixed in form.rs: `is_layout_hidden` now returns true for Hidden|Inactive**

**Layout Strategies (p43):**

| Strategy | Status | Notes |
|----------|--------|-------|
| position (x,y coords) | ✅ | Default |
| tb (top-to-bottom) | ✅ | |
| lr-tb (left-to-right, top-to-bottom) | ✅ | |
| rl-tb (right-to-left, top-to-bottom) | ✅ | |
| table | ✅ | With columnWidths, colSpan |
| row | ✅ | |

**Missing layout features:**

| Feature | Status | Notes |
|---------|--------|-------|
| anchorType | ❌ | Not parsed — affects anchor point for positioned layout |
| rotate | ❌ | Not parsed — counter-clockwise rotation (multiples of 90) |
| Clipping (p54) | ❌ | Not implemented |
| Widget natural sizes (p53-54) | ⚠️ | Simplified — no distinction by widget type |
| relevant attribute (p69-70) | ❌ | View-based concealment not implemented |

**Text within containers (p56-67):**
- Basic text flow in fixed-width containers ✅
- Text wrapping at word boundaries ✅
- Tab stops and tab leaders ❌
- Automatic hyphenation ❌
- Text overflow / widows / orphans ❌

### §2.7 Appearance Order / Z-Order (p70) — ✅ Correct
Objects rendered in document order (painter's algorithm).
`render_nodes` iterates nodes sequentially — correct Z-order.

### §2.8 Extending XFA Templates (p71-72) — ✅ Correct
- `extras` element: in skip list in `add_children` ✅
- `desc` element: in skip list in `add_children` ✅
- Custom namespaces not processed (correct per spec) ✅

### §2.9 Connecting the PDF to the XFA Template (p72-73) — ✅ Correct
- NeedsRendering flag: handled in pdfa_cleanup and flatten ✅
- AcroForm/XFA entry: extracted correctly ✅
- Field names (XFA-SOM expressions): handled via xfa-dom-resolver ✅
- Shell PDF detection via baseProfile ✅

---

## Summary

### Changes Made

**Bug fixes:**
1. **Presence hidden/invisible swap** (form.rs) — `is_layout_hidden` had `Invisible|Inactive` instead of `Hidden|Inactive`. This caused `invisible` elements to lose their layout space and `hidden` elements to consume space they shouldn't. Impacts all forms with `presence="hidden"` or `presence="invisible"`.

2. **`area` element not recognized** (template_parser.rs) — The `area` container element from §2.1 was not in the match arms, falling through to `blank_node`. Now parsed like subform.

3. **`button` UI not detected** (template_parser.rs) — `<ui><button/>` was not matched in `detect_field_kind`, defaulting to text. Now correctly maps to `FieldKind::Button`.

4. **`%` measurement unit missing** (types.rs) — The relative measurement unit `%` (percentage of space width) was not supported in `Measurement::parse`. Added `MeasurementUnit::Percent`.

5. **FormCalc `Expr::Positive` compile fix** (interpreter.rs) — Pre-existing Codex-generated compile error; added trivial match arm.

**Spec annotations added to:**
- `template_parser.rs` — Module doc, parse_node, parse_layout_attr, parse_box_model, parse_font_metrics, parse_caption, detect_field_kind, extract_draw_content, presence parsing
- `form.rs` — DrawContent, Presence, FormNodeMeta
- `types.rs` — Measurement, MeasurementUnit, LayoutStrategy, BoxModel
- `render_bridge.rs` — Module doc (coordinate mapping, Z-order)
- `flatten.rs` — Module doc (static vs dynamic, PDF-XFA connection)

### Test Results
- xfa-layout-engine: **90 passed**, 0 failed
- pdf-xfa: **108 passed**, 0 failed
- Clippy: blocked by pre-existing formcalc-interpreter errors (unrelated)

### Gaps for Future Work (not in T3 scope)

| Priority | Gap | Spec Section | Impact |
|----------|-----|-------------|--------|
| Medium | anchorType attribute | §2.6 p55 | Positioned layout anchor point |
| Medium | rotate attribute | §2.6 p55 | Rotated containers |
| Medium | lineHeight from para | §2.6 p61 | Text line spacing |
| Medium | textIndent from para | §2.6 p58 | First-line indent |
| Low | access attribute | §2.5 p48-49 | Read-only/protected (N/A for static) |
| Low | relevant attribute | §2.6 p69 | View-based concealment |
| Low | Tab stops | §2.6 p61-63 | Tabular text alignment |
| Low | Hyphenation | §2.6 p65-66 | Word breaking |
| Low | Pattern/gradient fills | §2.2 p39 | Visual fidelity |
| Low | Picture clauses | §2.4 p47 | Data formatting |
| Low | Barcode rendering | §2.4 p47-48 | Barcode visual output |
| Low | Rich text formatting | §2.4 p45-46 | Bold/italic in rich text |
| Low | baselineShift, underline, lineThrough | §2.6 p57-58 | Text decoration |
| Very Low | exObject content type | §2.3 p42 | External data objects |
| Very Low | Clipping | §2.6 p54 | Content overflow |
| Very Low | Widows/orphans | §2.6 p67 | Paragraph splitting |
