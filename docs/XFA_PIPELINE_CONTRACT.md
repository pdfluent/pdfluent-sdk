# XFA Pipeline Contract

**Issue**: XFA-F2-02 (#1090)  
**Spec reference**: XFA 3.3 §1.7, §2.9, §4.4, §8, §9  
**Source files**: `crates/pdf-xfa/src/`

This document specifies the input/output contract for each stage of the XFA flattening
pipeline. It covers error handling, failure modes, fallback strategies, and the two
main code paths (dynamic form path and static form fallback).

---

## Pipeline Overview

```
PDF bytes
  │
  ▼  Stage 1 — Parse
XfaPackets
  │
  ▼  Stage 2 — Bind
FormTree + FormNodeId (root)
  │
  ▼  Stage 3 — Layout
LayoutDom
  │
  ▼  Stage 4 — Render
Vec<PageOverlay> (per-page PDF content streams)
  │
  ▼  Stage 5 — Flatten
PDF bytes (flattened, AcroForm removed)
```

The top-level entry point is `flatten_xfa_to_pdf(pdf_bytes: &[u8]) -> Result<Vec<u8>>`
in `crates/pdf-xfa/src/flatten.rs`. All five stages run inside a spawned worker thread
with a 30-second timeout.

---

## Stage 1 — Parse

**Function**: `extract_xfa_from_bytes` / `extract_xfa`  
**Source**: `crates/pdf-xfa/src/extract.rs`

### Input

- `pdf_bytes: &[u8]` — raw bytes of a PDF file (any version, encrypted or not)

### Pre-checks (before stage 1)

Before parsing, `flatten_xfa_to_pdf` applies two fast pre-checks:

1. **Re-entrance guard** — a `thread_local! { static FLATTEN_DEPTH: Cell<u32> }` counter
   prevents recursive calls (e.g. a fallback path re-entering flatten). If `depth >= 1`,
   returns `Err(XfaError::LayoutFailed("flatten_xfa_to_pdf called recursively"))`.

2. **Byte-level XFA pre-check** — scans for `/AcroForm` or `xdp:xdp` in raw bytes.
   If neither is present, returns `Ok(pdf_bytes.to_vec())` immediately (not an XFA PDF).

3. **Encryption handling** — tries empty-password decryption via lopdf.
   - Not encrypted: proceeds with original bytes.
   - Decrypted successfully: proceeds with decrypted bytes.
   - Needs password: returns `Err(XfaError::Encrypted(...))`.

### Output

```rust
pub struct XfaPackets {
    pub full_xml: Option<String>,           // monolithic XDP stream (rare)
    pub packets: Vec<(String, String)>,     // (packet_name, xml_content)
}
```

Key accessors:
- `packets.template()` — `<template>` packet XML
- `packets.datasets()` — `<xfa:datasets>` packet XML (prefers largest when multiple exist)
- `packets.config()` — `<config>` packet XML
- `packets.locale_set()` — `<localeSet>` packet XML

### Error handling

| Condition                         | Behaviour                                    |
|-----------------------------------|----------------------------------------------|
| PDF parse fails                   | Returns `Ok(pdf_bytes.to_vec())` (no XFA)    |
| No XFA content found              | Returns `Ok(pdf_bytes.to_vec())`             |
| Template packet missing/corrupt   | `static_fallback(pdf_bytes)` (see §Fallback) |
| Template is corrupt/minimal XFA   | `static_fallback(pdf_bytes)`                 |

A "corrupt/minimal" template is detected by `is_corrupt_xfa_template`: tiny PDFs (<1 KB)
whose template has no `<subform>` or `<pageSet>` children, which would produce blank output.

---

## Stage 2 — Bind

**Type**: `FormMerger`  
**Function**: `FormMerger::new(data_dom).merge(template_xml) -> Result<(FormTree, FormNodeId)>`  
**Source**: `crates/pdf-xfa/src/merger.rs`

### Input

- `template_xml: &str` — the `<template>` packet XML string
- `data_dom: &DataDom` — the parsed Data DOM (from `datasets` packet, or empty `DataDom::new()`)
- `image_files: HashMap<String, Vec<u8>>` — embedded PDF image files for `<image href="…">` resolution

If no `datasets` packet is present, `data_dom` is an empty `DataDom::new()` and fields
receive empty values.

### Processing

1. `DataDom::from_xml(datasets_xml)` parses the datasets packet into a `DataDom`.
   The `<xfa:datasets><xfa:data>` wrapper is unwrapped automatically.

2. `FormMerger::merge(template_xml)` walks the template XML, producing a `FormTree`:
   - Each template element is mapped to a `FormNode` with a `FormNodeType`.
   - Data binding (`consumeData` mode): for each named node, `DataDom::children_by_name`
     finds matching data nodes; repeating subforms are expanded per data instance.
   - `<bind ref="...">` overrides are resolved via `som::resolve_data_path`.
   - Explicit `<bind match="none">` marks a node as `data_bind_none`.

3. `apply_dynamic_scripts(&mut tree, root_id)` runs FormCalc `<calculate>` scripts.

4. If a `form` packet is present (Adobe-saved pre-merged form DOM), `apply_form_dom_presence`
   overlays `presence` attributes from it onto the `FormTree` to capture script-driven
   visibility changes that the engine cannot re-execute.

5. `resolve_template_fonts` / `inject_resolved_metrics` resolve embedded PDF fonts
   and push width/metric data into `FontMetrics` for accurate text measurement.

### Output

```rust
(FormTree, FormNodeId)
```

- `FormTree` — arena of `FormNode` + parallel `FormNodeMeta`, plus `node_ids` lookup.
- `FormNodeId` — the root node of the tree (always the `<template>` element, type `Root`).

### Error handling

| Condition                              | Behaviour                             |
|----------------------------------------|---------------------------------------|
| Template XML parse error               | `Err(XfaError::ParseFailed(...))`     |
| No `<template>` element found          | `Err(XfaError::PacketNotFound(...))`  |
| Datasets XML parse error               | `Err(XfaError::ParseFailed(...))`     |

On any error from this stage, `flatten_xfa_to_pdf` catches it and calls `static_fallback`.

---

## Stage 3 — Layout

**Type**: `LayoutEngine`  
**Function**: `LayoutEngine::new(&form_tree).layout(root_id) -> Result<LayoutDom>`  
**Source**: `crates/xfa-layout-engine/src/layout.rs`

### Input

- `form: &FormTree` — the merged Form DOM from Stage 2
- `root: FormNodeId` — root node of the form tree
- Font metrics are embedded in `FormNode.font: FontMetrics` (injected in Stage 2)

### Processing

The layout algorithm (XFA 3.3 §8.6) performs a content-driven single traversal:

1. **Page structure extraction** — scans the root's children for `PageSet`/`PageArea` nodes
   to obtain page dimensions and `ContentArea` definitions.

2. **Content queueing** — remaining non-page-structure nodes are queued as `QueuedNode`
   entries carrying `break_before`/`break_after` flags from `<break>` elements.

3. **Pagination loop** — for each page:
   - `layout_content_fitting` places as many queued nodes as fit in the current `ContentArea`.
   - When a container fills, remaining content is carried to the next page.
   - The last `PageArea` template is repeated as needed (XFA §9.3).
   - Maximum `MAX_PAGES = 500` pages to prevent explosion.

4. **Node positioning** — each node's `BoxModel` (x, y, w, h, margins, borders, caption)
   determines its `Rect` in page coordinates. `LayoutStrategy` controls child placement
   (positioned/tb/lr-tb/rl-tb/table/row).

5. **Text wrapping** — text content is wrapped into `LayoutContent::WrappedText` lines
   using font metrics for accurate character width measurement.

### Output

```rust
pub struct LayoutDom {
    pub pages: Vec<LayoutPage>,
}
pub struct LayoutPage {
    pub width: f64,     // page width in points
    pub height: f64,    // page height in points
    pub nodes: Vec<LayoutNode>,
}
```

All coordinates are in **points**, **top-left origin** (XFA coordinate system).

### Error handling

| Condition                             | Behaviour                                  |
|---------------------------------------|--------------------------------------------|
| Layout produces 0 pages               | `Err(XfaError::LayoutFailed("layout produced 0 pages"))` |
| Page limit exceeded                   | Warning to stderr; truncates layout at `MAX_PAGES` |
| Infinite loop guard (empty page)      | Forces placement of one item to break loop |

If layout fails, the calling `flatten_xfa_to_pdf` calls `static_fallback`.

**Empty page suppression** (XFA §4.3): after layout, pages that contain fields but no
populated field values are suppressed (data-empty pages). At least one page is always retained.

---

## Stage 4 — Render

**Function**: `generate_all_overlays(layout_dom, page_dims, config) -> Vec<PageOverlay>`  
**Source**: `crates/pdf-xfa/src/render_bridge.rs`

### Input

- `layout_dom: &LayoutDom` — positioned layout from Stage 3
- `page_dims: &[(f64, f64)]` — (width, height) for each page, from original PDF or layout
- `config: XfaRenderConfig` — rendering configuration:
  - `font_map: HashMap<String, String>` — typeface name → PDF font resource name (`/XFA_F0`, etc.)
  - `font_metrics_data: HashMap<String, FontMetricsData>` — resolved width/metrics per typeface
  - `default_font`, `default_font_size`, `draw_borders`, `text_color`, `background_color`, etc.

### Processing

For each `LayoutPage`:

1. **Coordinate transform** — XFA uses top-left origin (y down); PDF uses bottom-left (y up).
   `CoordinateMapper` applies the transformation: `pdf_y = page_height - xfa_y - element_height`.

2. **Node rendering** — each `LayoutNode` is rendered in document order (painter's algorithm,
   XFA §2.7). Rendering order: background fill → border → caption → content.

3. **Content stream generation** — PDF operators are emitted as a UTF-8 string:
   - Text: `BT ... Tf ... Td ... Tj ET`
   - Rectangles/borders: `re`, `S`, `f`, `f*` operators
   - Lines/arcs: `m`, `l`, `c`, `S` operators
   - Images: XObject references (`Do`)

4. **Font encoding** — Identity-H encoded fonts use glyph ID lookup from embedded font data;
   simple fonts with custom encodings use `simple_unicode_to_code` remapping.

5. **Image collection** — images encountered during traversal are collected into `ImageInfo`
   records for embedding as PDF XObjects.

### Output

```rust
pub struct PageOverlay {
    pub content_stream: String,     // PDF content stream operators
    pub images: Vec<ImageInfo>,     // images to embed as XObjects
}
```

One `PageOverlay` per page in `LayoutDom`.

### Error handling

The render stage does not return `Result`; it produces best-effort output.
Missing font resources fall back to the `default_font`. Unresolvable images are silently
skipped (no placeholder is rendered).

---

## Stage 5 — Flatten

**Function**: embedded in `xfa_flatten_inner` and the lopdf post-processing in `flatten_xfa_to_pdf`  
**Source**: `crates/pdf-xfa/src/flatten.rs`

### Input

- `pdf_bytes: &[u8]` — original PDF bytes (possibly decrypted in pre-check)
- `Vec<PageOverlay>` — content streams from Stage 4
- Resolved font data for embedding

### Processing

1. **PDF load** — `lopdf::Document::load_mem(pdf_bytes)` parses the original PDF structure.

2. **Font embedding** — `embed_resolved_fonts` writes resolved font programs as PDF stream
   objects and creates `/Font` resource dictionaries.

3. **Page content replacement** — for each page, the XFA-generated content stream is written
   as the page's content stream. The original page streams (which may contain blank or
   NeedsRendering placeholders) are replaced.

4. **Image embedding** — `embed_image` writes image XObjects for each `ImageInfo`.

5. **AcroForm removal** — the `/AcroForm` entry is removed from the PDF catalog, and
   `NeedsRendering` is cleared. The result is a plain PDF/1.4 document with no XFA payload.

6. **Save** — `lopdf::Document::save_to` serialises the modified document to bytes.

### Output

- `Ok(Vec<u8>)` — flattened PDF bytes; plain PDF, no XFA, no AcroForm

### Error handling

| Condition                              | Behaviour                             |
|----------------------------------------|---------------------------------------|
| lopdf cannot parse the PDF             | `static_fallback(pdf_bytes)`          |
| Stage 4 produces no pages              | `static_fallback(pdf_bytes)`          |
| Thread panics / timeout (30s)          | `static_fallback(pdf_bytes)`          |

---

## Fallback Strategy

### `static_fallback(pdf_bytes)`

When the dynamic XFA pipeline fails at any stage, `static_fallback` is called.
It preserves the original PDF page content and removes the XFA machinery:

1. Load the PDF with lopdf.
2. Remove `/AcroForm` from the PDF catalog (strips XFA data and widget annotations).
3. Clear `NeedsRendering` flag.
4. Bake existing widget appearances into page streams if present (preserves pre-rendered content).
5. Save and return the modified bytes.

If lopdf also fails to parse the PDF (severely malformed file), `static_fallback` returns
the original bytes unchanged.

### When fallback is triggered

| Stage     | Trigger                                               |
|-----------|-------------------------------------------------------|
| Pre-check | No XFA markers → return original bytes unchanged     |
| Pre-check | Encryption with password → `Err(XfaError::Encrypted)` |
| Stage 1   | Template packet missing → `static_fallback`           |
| Stage 1   | Corrupt/minimal template → `static_fallback`          |
| Stage 2   | XML parse error or merge error → `static_fallback`    |
| Stage 3   | Layout produces 0 pages → `static_fallback`           |
| Stage 3   | Layout error → `static_fallback`                      |
| Stage 4–5 | Thread panic → `static_fallback`                      |
| Stage 4–5 | 30-second timeout → `static_fallback`                 |

---

## Two Main Code Paths

### Dynamic Form Path (full XFA pipeline)

Applies to: PDFs with a `<template>` packet and no `baseProfile="interactiveForms"`.
Detected by `classify::XfaType::Dynamic`.

```
PDF bytes
  → extract_xfa()           [Stage 1: produces XfaPackets]
  → DataDom::from_xml()     [Stage 2a: parse data]
  → FormMerger::merge()     [Stage 2b: bind template + data → FormTree]
  → apply_dynamic_scripts() [Stage 2c: run FormCalc calculations]
  → resolve/inject fonts    [Stage 2d: resolve embedded font metrics]
  → LayoutEngine::layout()  [Stage 3: compute page layout]
  → generate_all_overlays() [Stage 4: render to content streams]
  → embed fonts + images    [Stage 5a: embed resources]
  → replace page content    [Stage 5b: write into PDF]
  → remove AcroForm         [Stage 5c: strip XFA machinery]
  → save PDF                [Stage 5d: serialise]
```

This path produces a fully re-rendered PDF where every page's content is derived
from the XFA layout computation.

### Static Form Fallback Path

Applies to:
1. PDFs detected as `XfaType::Static` (`baseProfile="interactiveForms"`)
2. Any PDF where the dynamic path fails for any reason

```
PDF bytes
  → static_fallback()
      → load with lopdf
      → strip /AcroForm
      → clear NeedsRendering
      → bake widget appearances (preserve pre-rendered content)
      → save PDF
```

This path preserves the existing page content streams. The form data is not re-rendered
from XFA; any pre-rendered content in the original PDF is kept as-is.

---

## Error Types

All pipeline errors use `XfaError` from `crates/pdf-xfa/src/error.rs`:

```rust
pub enum XfaError {
    LoadFailed(String),      // PDF load failure
    PacketNotFound(String),  // XFA packet missing
    Encrypted(String),       // PDF encrypted, password required
    XmlParse(String),        // XML parse error
    FontError(String),       // Font resolution error
    LayoutError(String),     // Layout engine error
    LayoutFailed(String),    // Layout produced invalid output
    ParseFailed(String),     // Template/datasets parse failure
    FormCalcError(String),   // FormCalc script error
    Io(std::io::Error),      // I/O error
}
```

Most errors from Stages 2–5 are caught by `flatten_xfa_to_pdf` and converted to a
`static_fallback` call rather than propagated to the caller. Only `XfaError::Encrypted`
and re-entrance errors propagate to the caller as `Err(...)`.

---

## Known Spec Gaps

| Gap                                   | Spec reference         | Location              |
|---------------------------------------|------------------------|-----------------------|
| `matchTemplate` merge mode            | §4.4 p176              | `merger.rs`           |
| Scope matching (ancestor/sibling)     | §4.4.3 p185            | `merger.rs`           |
| `bind match="global"`                 | §4.4.3 p176            | `merger.rs`           |
| Namespace exclusion (`excludeNS`)     | §4.1 p134              | `data_dom.rs`         |
| Locale/canonicalization               | §4.2 p143              | `merger.rs`           |
| anchorType positioning                | §2.6, Appendix A p1510 | `layout.rs`           |
| Text-line splitting across pages      | §8.7                   | `layout.rs`           |
| simplexPaginated/duplexPaginated      | §8.8                   | `layout.rs`           |
| Leaders/trailers overflow/bookend     | §8.10                  | `layout.rs`           |
| `area`, `exclGroup`, `subformSet`     | Appendix B             | `layout.rs`           |
| CID-to-Unicode (ToUnicode CMap)       | PDF §9.7.4.3           | `flatten.rs`          |
| Picture clause canonicalization       | §4.3 p171              | `data_dom.rs`         |
