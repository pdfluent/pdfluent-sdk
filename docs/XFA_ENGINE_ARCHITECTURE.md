# XFA Engine Architecture

**Milestone**: Full XFA Rendering & Flattening Engine (#47)  
**Created**: 2026-04-18  
**Status**: Design phase — implementation follows Phase 1–9 issues

---

## 1. Overview

### What is XFA?

XFA (XML Forms Architecture) is Adobe's XML-based form technology for PDF. An XFA form is a PDF file that contains an embedded XML packet (called the XDP, or XML Data Package) instead of — or in addition to — standard PDF content streams.

XFA forms come in two fundamentally different variants:

| Type | Description | Rendering |
|------|-------------|-----------|
| **Static XFA** | PDF with XFA metadata, but pre-rendered content streams already present | Skip flatten — render from PDF content |
| **Dynamic XFA** | Live XFA template + data packet; no pre-rendered content | Full flatten pipeline required |

The XFA engine in this SDK processes **dynamic XFA** forms: it reads the template and data, executes the binding and layout logic, and produces a flat static PDF output.

### Why Flattening?

PDFluent's rendering engine (including WASM) processes standard PDF content streams. It cannot execute XFA logic. The flatten pipeline converts a dynamic XFA form into a static PDF that the standard renderer can display.

Without flattening, a dynamic XFA PDF opened in PDFluent would show a blank page — the PDF has no pre-rendered content, only the XFA packet.

### High-Level Pipeline

```
Input PDF (dynamic XFA)
         │
         ▼
┌─────────────────────┐
│  1. Type Detection   │  → Is this XFA? Static or dynamic?
└─────────────────────┘
         │ (dynamic)
         ▼
┌─────────────────────┐
│  2. Packet Extraction│  → Extract template, datasets, config from XDP
└─────────────────────┘
         │
         ▼
┌─────────────────────┐
│  3. Data Binding     │  → Merge template + data → bound node tree
└─────────────────────┘
         │
         ▼
┌─────────────────────┐
│  4. Layout Engine    │  → Compute x,y,w,h for every node → layout tree
└─────────────────────┘
         │
         ▼
┌─────────────────────┐
│  5. PDF Renderer     │  → Emit PDF content streams from layout tree
└─────────────────────┘
         │
         ▼
┌─────────────────────┐
│  6. Flatten Finalizer│  → Remove XFA/AcroForm artifacts → clean PDF
└─────────────────────┘
         │
         ▼
Output PDF (static, no XFA)
```

---

## 2. XFA Packet Structure

An XFA form's PDF contains a `/AcroForm` dictionary with an `/XFA` key. The `/XFA` value is an array of alternating name/stream pairs — the XDP sub-packets:

```
/XFA [(template) <stream> (datasets) <stream> (config) <stream> ...]
```

### Sub-Packets

| Packet | Required | Description |
|--------|----------|-------------|
| `template` | Yes | The form template: all fields, subforms, layout rules, scripts |
| `datasets` | Conditional | The form data: field values, repeated records |
| `config` | No | Processing configuration: logging, locale, script settings |
| `sourceSet` | No | Data connection configurations |
| `connectionSet` | No | Network connection definitions |

The `template` packet is an XML document rooted at `<template>` in the XFA namespace. The `datasets` packet is rooted at `<xfa:datasets>`. Both use XML namespaces from the XFA 2.x or 3.x specification.

### Namespace Versions

| XFA Version | Namespace URI |
|-------------|--------------|
| XFA 2.5 | `http://www.xfa.org/schema/xfa-template/2.5/` |
| XFA 2.6 | `http://www.xfa.org/schema/xfa-template/2.6/` |
| XFA 3.0 | `http://www.xfa.org/schema/xfa-template/3.0/` |

The engine must handle all three versions — namespace differences are minor and do not affect logic.

---

## 3. Pipeline Stages

### Stage 1: Type Detection

**Crate**: `pdf-xfa`  
**Input**: Raw PDF bytes (`&[u8]`)  
**Output**: `XfaType` enum  
**Error conditions**: None — always returns a definitive classification

```rust
enum XfaType {
    None,          // No XFA in this PDF
    Static,        // XFA metadata present, content streams already rendered
    Dynamic,       // XFA template + data, must flatten
    Hybrid,        // Both AcroForm and XFA, non-standard
}
```

**Logic**:
1. Check for `/AcroForm → /XFA` in the document catalog
2. If absent → `None`
3. If present: check page content streams
   - All pages have non-empty content streams → `Static`
   - Pages have empty/absent content streams → `Dynamic`
4. Both non-empty streams AND XFA with datasets → `Hybrid`

**Known limitation**: Some malformed PDFs have XFA keys but invalid XML — these are classified as `Dynamic` and will fail at Stage 2 with an extraction error.

---

### Stage 2: Packet Extraction

**Crate**: `pdf-xfa`  
**Input**: Raw PDF bytes  
**Output**: `XdpPacket { template, datasets, config, ... }`  
**Error conditions**: `XfaExtractError::MissingXfaKey`, `InvalidXml`, `TruncatedStream`

The XDA array is parsed from the PDF cross-reference table. Each sub-stream is decoded (potentially compressed with FlateDecode) and parsed as XML.

**Validation**:
- `PacketValidation::Complete` — all required packets present and valid
- `PacketValidation::Partial` — template present, datasets absent (empty form)
- `PacketValidation::Malformed` — XML parse error in any packet

---

### Stage 3: Data Binding

**Crate**: `pdf-xfa`  
**Input**: `XdpPacket`  
**Output**: `BoundNode` tree (template nodes annotated with their data values)  
**Error conditions**: `XfaBindError::SomResolutionFailed`

The data binding stage merges the `template` XML tree with the `datasets` XML tree, producing a new tree where each field node carries its bound data value.

#### Binding Modes

| Mode | Description | Default |
|------|-------------|---------|
| `consumeData` | Each data node consumed once, matched positionally | No |
| `matchTemplate` | Template structure drives matching; data found by name in scope | **Yes** |

`matchTemplate` is the correct default for virtually all real-world XFA forms.

#### Scope Resolution Order

When resolving a template node's data:
1. **Local**: same depth in the data tree
2. **Ancestor**: walk up the data hierarchy
3. **Global** (when `match="global"`): search entire datasets tree

#### Transparent Subforms

A subform without a `name` attribute (or with `xfa:bindingAttributes match="none"`) is **transparent**: it does not introduce a new scope level. Its children continue resolving data at the parent's scope.

This is common for anonymous layout containers.

#### Occurrence (Repeating Sections)

Subforms with `<occur minOccur="0" maxOccur="-1">` repeat once per matching data record. The binding engine creates one `BoundNode` instance per data record, up to `maxOccur`.

---

### Stage 4: Layout Engine

**Crate**: `pdf-xfa`  
**Input**: `BoundNode` tree  
**Output**: `LayoutTree` — each node has computed `x, y, w, h, page`  
**Error conditions**: `XfaLayoutError::ZeroPages`, `XfaLayoutError::InfinitePagination`

The layout engine traverses the bound node tree and computes the absolute position of each element on each page.

#### Layout Types

| XFA `layout` Attribute | Description |
|------------------------|-------------|
| `tb` (top-to-bottom) | Stack children vertically in the container |
| `lr` (left-to-right) | Stack children horizontally |
| `rl` (right-to-left) | Stack children right-to-left (RTL scripts) |
| `position` | Absolute positioning using `x, y` coordinates |
| `row` (in `table`) | Row-based layout within a table container |

#### AnchorType

For positioned layout, the `anchorType` attribute specifies which corner/edge of the element is pinned to its `x, y` position:

```
topLeft | topCenter | topRight
middleLeft | middleCenter | middleRight
bottomLeft | bottomCenter | bottomRight
```

Default is `topLeft`. Non-default anchors require an offset calculation before placement.

#### hAlign in TB Containers

Children of `layout="tb"` parents can have `hAlign="left|center|right"`. When narrower than the container, the child is offset horizontally.

#### Pagination

Page dimensions come from the `<pageSet>` → `<pageArea>` hierarchy. When content overflows a page's `<contentArea>`, a new page is started and the layout cursor moves to the new page's content area origin.

**Keep chains** (`<keep>`) prevent splitting related elements across pages.

**Bookend leaders/trailers**: overflow continuation pages can have a repeated header (`<leader>`) and a final-page footer (`<trailer>`).

---

### Stage 5: PDF Renderer

**Crate**: `pdf-xfa` + `pdf-interpret`  
**Input**: `LayoutTree` (via `RenderTree` intermediate)  
**Output**: PDF bytes (rendered pages)  
**Error conditions**: `XfaRenderError::FontNotFound`, `XfaRenderError::InvalidColorSpace`

The renderer converts the layout tree into PDF content stream operations.

#### Render Tree (Intermediate Representation)

Between layout and PDF encoding, the engine produces a `RenderTree` — a typed tree of render operations:

```rust
enum RenderNode {
    Page { width, height, children },
    Rect { x, y, w, h, fill, border },
    Text { x, y, runs: Vec<TextRun>, clip },
    Image { x, y, w, h, data, format },
    Group { transform: Matrix, children },
    Clip { rect, children },
}
```

The PDF encoder takes a `RenderTree` and emits PDF operators (`BT`, `ET`, `Tf`, `Tj`, `re`, `f`, etc.).

#### Font Strategy

| Font Type | Metric Source | Rendering |
|-----------|--------------|-----------|
| Embedded (Type1, TrueType, CFF) | PDF `/Widths` or `FontDescriptor` | Exact — use embedded metrics |
| Standard-14 (Helvetica, Times, Courier) | AFM metrics tables | Exact — Standard-14 AFM is normative |
| Non-embedded, non-Standard-14 | Best-match Standard-14 AFM | Approximate — metric drift possible |
| CIDFont Type0/2 | `/W` array for horizontal, `/W2` for vertical | From PDF arrays |

**Irreducible floor**: Forms with non-embedded, non-Standard-14 fonts will always have some SSIM loss due to metric drift between the reference renderer (Adobe) and our fallback metrics. This is architecturally unavoidable without embedding the exact reference fonts.

#### Widget Rendering

| Field Type | Rendered As |
|-----------|-------------|
| Text field | Text run at field position with clip |
| Numeric field | Formatted text (format mask applied) |
| Date/time field | Formatted text (pattern applied) |
| Checkbox | Border rect + check mark glyph (if checked) |
| Radio button | Circle + filled center (if selected) |
| Choice list | Selected item text |
| Image field | Embedded image scaled to field rect |
| Signature | Placeholder rectangle |
| Barcode | Not supported (graceful degradation: empty rect) |

---

### Stage 6: Flatten Finalizer

**Crate**: `pdf-xfa`  
**Input**: Rendered PDF bytes  
**Output**: Clean PDF bytes (no XFA, no widget annotations)  
**Error conditions**: `XfaFlattenError::CatalogCorrupt`

After rendering, the output PDF is opened with lopdf and cleaned:

1. Remove `/AcroForm → /XFA` key (or replace `/AcroForm` with `<< /Fields [] >>`)
2. Remove all `/Widget` annotations from all page `/Annots` arrays
3. Remove orphaned XFA streams from the cross-reference table (objects with `/Subtype /XML`)
4. Rebuild the PDF with a clean cross-reference table

**Verification**: After cleanup, `XfaType::detect()` must return `None`.

---

## 4. Data Binding Semantics

### consumeData Mode

The data tree is traversed in parallel with the template tree. Each data node is matched to the first template node that needs data at that position. The data node is "consumed" and not available to other template nodes.

**When to use**: Simple, flat forms where data and template have the same structure.

**Current limitation**: This mode is implemented but does not handle most real-world forms.

### matchTemplate Mode (default)

The template tree drives the traversal. For each template node with a binding:
1. Find the current data scope (the data subtree corresponding to the current template subform)
2. Search the scope for a child with the same local name as the template node
3. If found: bind the template node to that data child
4. If not found: search ancestor scopes (unless `match="none"`)

**Scope inheritance**: Child template nodes inherit the scope of their closest named ancestor subform.

### SOM Expressions

Fields can have explicit data references using SOM (Script Object Model) expressions:
- `xfa.form.subform1.field1.rawValue` — absolute path from form root
- `$.fieldName` — relative path from current scope
- `../sibling` — parent-relative path

The FormCalc evaluator resolves SOM expressions during binding and script execution.

---

## 5. Layout Model

### Container Types

| Element | Description |
|---------|-------------|
| `subform` | Primary layout container; can have `tb`, `lr`, or `position` layout |
| `area` | Absolute positioning container; height = tallest child's bottom edge |
| `exclGroup` | Mutually exclusive selection group (radio buttons); only selected child is active |
| `subformSet` | Ordered collection with selection policy (`ordered`, `choice`, `use`) |

### Page Model

```
pageSet
  └─ pageArea (one page template, can repeat)
       ├─ contentArea (printable region, can have multiple per page for columns)
       └─ draw/field elements (background graphics, page numbers, etc.)
```

Content subforms flow into the `contentArea`. When a `contentArea` is full, a new `pageArea` instance is started (the page repeats according to its `<occur>` settings).

### Overflow Model

```
subform (with overflow child)
  ├─ <overflow leader="headerSubform" trailer="footerSubform">
  ├─ ... content ...
  └─ ... more content → overflows to new page ...
       └─ new page: header rendered, remaining content continues
```

---

## 6. Font Strategy

### Priority Order

1. **Embedded font with metrics**: use `/Widths` array (for Type1/TrueType) or `CIDFont /W` array
2. **Standard-14 exact match**: font name is exactly one of the 14 (e.g., `Helvetica`, `Helvetica-Bold`, `Times-Roman`). Use AFM metrics. `exact=true`.
3. **Standard-14 heuristic match**: font name contains "helvetica", "arial", "times", "courier". Use AFM metrics. `exact=false` — the PDF's `/Widths` array (if present) takes priority.
4. **Unknown font**: substitute closest Standard-14 by style (sans-serif, serif, monospace). Log WARN.

### Why `exact=false` for Heuristic Matches

When a PDF contains a non-standard font named "HelveticaNeue" with a custom `/Widths` array, those widths describe the actual font metrics. Silently ignoring them and using Standard-14 Helvetica widths instead produces metric drift. The fix: always treat heuristic matches as `exact=false` so the PDF's `/Widths` array is respected.

### CIDFont Metrics

CIDFont uses `/W` (horizontal) and `/W2` (vertical) arrays in the `CIDFont` dictionary:
- `/W` format: `[c1 [w1 w2 w3] c2 c3 w4 ...]` (compact and expanded forms)
- `/W2` default: `[880 -1000]` (vertical advance 880 units, vertical origin Y -1000 units per PDF §9.7.4.3)

---

## 7. Error Taxonomy

All errors are structured and typed. Silent failures are not permitted.

```rust
enum XfaError {
    Extraction(XfaExtractError),
    Binding(XfaBindError),
    Layout(XfaLayoutError),
    Rendering(XfaRenderError),
    Flatten(XfaFlattenError),
}
```

### Error Codes → CLI Exit Codes

| Exit Code | Meaning |
|-----------|---------|
| 0 | Success |
| 1 | Generic render failure (unexpected) |
| 2 | PDF is encrypted (PasswordProtected or Decryption error) |
| 3 | Degenerate document (zero pages, invalid geometry) |
| 4 | XFA processing error (extraction, binding, layout, or rendering failed) |

### No Silent Failures

Every function that can fail returns `Result<T, XfaError>`. Default values are **never** returned on error. When a feature is unsupported, a `WARN` log is emitted and the engine continues with graceful degradation (e.g., renders an empty field instead of crashing).

### Debug Dump Format

All intermediate representations can be serialized to JSON for debug inspection:
- `--debug-dump-model` → `XfaDataModel` after binding
- `--debug-dump-layout` → `LayoutTree` with computed positions
- `--debug-render-tree` → `RenderTree` before PDF encoding
- `--debug-pipeline-timing` → Per-stage timing in milliseconds

---

## 8. Quality Targets

### Per-Tier SSIM Targets

| Tier | Form Complexity | SSIM Target |
|------|----------------|-------------|
| 1 | Simple: 1–5 fields, no scripts, embedded fonts | ≥ 0.97 |
| 2 | Moderate: 5–20 fields, FormCalc scripts, mixed fonts | ≥ 0.95 |
| 3 | Complex: 20+ fields, multi-page, images, non-embedded fonts | ≥ 0.90 |

### Crash Target

**0 crashes** (exit 1) on any non-adversarial PDF from the quality corpus.

Adversarial/fuzzing PDFs (GHOSTSCRIPT, MOZILLA, PDFIUM fuzz inputs) are excluded from the crash SLA — they are intentionally malformed inputs that no renderer guarantees to handle.

### Latency Targets (aspirational)

| Form Tier | p95 Flatten Latency |
|-----------|---------------------|
| Tier 1 (simple) | < 100ms |
| Tier 2 (moderate) | < 500ms |
| Tier 3 (complex) | < 2000ms |

### Irreducible Floor

The following categories of SSIM loss are **architecturally irreducible** — they cannot be fixed without fundamental changes to the approach:

1. **Non-embedded font metric drift**: ~28% of near-miss entries (gate-5k-03 analysis). Forms that reference fonts not in the Standard-14 set and do not embed them will have some metric drift. The magnitude depends on how different the actual font metrics are from our fallback.

2. **JavaScript-controlled forms**: ~10–15 entries depend on JavaScript for field values and visibility. The engine executes FormCalc only — JavaScript is not planned.

3. **Hybrid AcroForm/XFA forms**: ~8 entries where the oracle uses the AcroForm fallback path (rendering pre-existing AcroForm fields) while our engine uses the XFA path. These produce structurally different but both-valid outputs.

---

## 9. Crate Responsibilities

| Crate | Responsibility |
|-------|---------------|
| `pdf-xfa` | XFA type detection, packet extraction, data binding, layout engine, flatten finalizer |
| `pdf-interpret` | Font metrics, color spaces, PDF content stream parsing and rendering |
| `pdf-syntax` | Low-level PDF parsing (cross-reference tables, dictionaries, streams) |
| `xfa-cli` | CLI entry point, exit code dispatch, error formatting |

The pipeline stages in `pdf-xfa` use the rendering infrastructure from `pdf-interpret` for the PDF emission step.

---

## 10. Implementation Phases (Milestone #47)

| Phase | Issues | Focus |
|-------|--------|-------|
| 1 | #1084–#1088 | Foundation: type detection, packet extraction, corpus, quality targets |
| 2 | #1089–#1091 | Architecture: data model, pipeline contracts, this document |
| 3 | #1092–#1097 | Data binding: matchTemplate, scope matching, repeating sections, presence, FormCalc SOM |
| 4 | #1098–#1103 | Layout: anchorType, hAlign, text splitting, overflow, exclGroup, multi-page |
| 5 | #1104–#1108 | Rendering: render tree, text fidelity, widgets, field formatting, debug tools |
| 6 | #1109–#1112 | Flatten quality: pipeline correctness, AcroForm cleanup, validation, visual diff |
| 7 | #1113–#1116 | Testing: content validation, edge cases, CI integration |
| 8 | #1117–#1119 | Performance: benchmarks, memory, stress tests |
| 9 | #1120–#1122 | Polish: error types, feature matrix, production logging |

See individual GitHub issues for detailed technical approaches and acceptance criteria.
