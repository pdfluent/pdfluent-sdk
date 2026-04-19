# XFA Internal Data Model

**Issue**: XFA-F2-01 (#1089)  
**Spec reference**: XFA 3.3 §2, §3, §4, §8  
**Source files**: `crates/xfa-dom-resolver/src/`, `crates/xfa-layout-engine/src/`

This document formally specifies the internal data model used by the XFA engine.
The engine works with four distinct tree representations that are produced in sequence:
Template DOM → Data DOM → Form DOM (merged) → Layout DOM.

---

## 1. Template DOM

**Source**: `crates/xfa-dom-resolver/src/template_dom.rs`  
**Spec**: XFA 3.3 §2 and §17

The Template DOM represents the fixed form structure as designed by the form author.
It is parsed from the `template` XFA packet and describes what fields, subforms, and
static elements exist, independent of any data.

### Status

The Template DOM module (`template_dom.rs`) currently holds its module declaration
and spec references but the full `TemplateNode` type hierarchy is not yet implemented
(deferred to Epic 1.1). At runtime, template parsing is handled directly in
`crates/pdf-xfa/src/merger.rs` using `roxmltree` XML nodes, and `FormMerger` builds
a `FormTree` in a single parse-and-merge pass.

### Logical node types (from XFA 3.3 §17)

The following element types are recognised and parsed by `FormMerger::parse_node`:

| XFA element   | Role                                              |
|---------------|---------------------------------------------------|
| `template`    | Root container (`FormNodeType::Root`)             |
| `subform`     | Repeatable container (`FormNodeType::Subform`)    |
| `exclGroup`   | Exclusive-choice radio group                      |
| `area`        | Positioned/flowing container                      |
| `field`       | Interactive form field (`FormNodeType::Field`)    |
| `draw`        | Static content: text, line, rect, arc, image      |
| `pageSet`     | Collection of page templates                      |
| `pageArea`    | Page template with content areas                  |

### Key attributes per element

- **`name`** — XFA binding name; used to match data nodes.
- **`x`, `y`, `w`, `h`** — Position/size in the XFA measurement system (see §4 Box Model).
- **`layout`** — Layout strategy: `positioned` (default), `tb`, `lr-tb`, `rl-tb`, `table`, `row`.
- **`occur`** — Repetition rules (`min`, `max`, `initial`).
- **`presence`** — Visibility: `visible`, `hidden`, `invisible`, `inactive`.
- **`bind`** — Data binding override (`ref`, `match`).

---

## 2. Data DOM

**Source**: `crates/xfa-dom-resolver/src/data_dom.rs`  
**Spec**: XFA 3.3 §4.1

The Data DOM represents the XML data payload that will be merged into the template.
It is parsed from the `datasets` XFA packet (the `<xfa:datasets><xfa:data>` wrapper
is automatically stripped so the root points to the actual form data).

### Node types

The Data DOM has exactly two node types, per XFA 3.3 §4.1 p126:

```rust
pub enum DataNode {
    DataGroup {
        name: String,
        namespace: Option<String>,
        children: Vec<DataNodeId>,
        is_record: bool,
        parent: Option<DataNodeId>,
    },
    DataValue {
        name: String,
        namespace: Option<String>,
        value: String,
        contains: DataContains,   // Data | MetaData
        content_type: Option<String>,
        is_null: bool,
        null_type: NullType,      // Exclude | Empty | Xsi
        parent: Option<DataNodeId>,
    },
}
```

**Classification rule** (§4.1 p126):
- Elements with child elements → `DataGroup`
- Elements with only character data → `DataValue`
- XML attributes on a `DataGroup` → `DataValue` children with `contains = MetaData`

### Null values

`xsi:nil="true"` on a leaf element marks it as null (`is_null = true`, `null_type = Xsi`).
On serialisation via `DataDom::to_xml()`, null values are written as `<field xsi:nil="true"/>`.

### Arena allocation

The `DataDom` uses a `Vec<DataNode>` arena. Node identity is a `DataNodeId(usize)` index.

```rust
pub struct DataNodeId(pub(crate) usize);

pub struct DataDom {
    nodes: Vec<DataNode>,
    root: Option<DataNodeId>,
}
```

**Allocation**: `DataDom::alloc(node)` appends to `nodes` and returns the new `DataNodeId`.
Nodes are never freed; `detach` / `remove_child` only clear parent/child references.
IDs are stable for the lifetime of the arena.

### Key invariants

- Every `DataGroup`'s `children` vec contains only valid IDs within the same `DataDom`.
- `DataNodeId(n)` where `n >= nodes.len()` is invalid.
- A node's `parent` field reflects its current location in the tree; orphaned nodes have `parent = None`.
- The root node has `parent = None`.
- `DataValue` nodes have an empty `children` list (they are leaf nodes).
- Attribute-derived `DataValue` nodes have `contains = MetaData`; all others have `contains = Data`.

---

## 3. Form DOM (merged tree)

**Source**: `crates/pdf-xfa/src/merger.rs`, `crates/xfa-layout-engine/src/form.rs`  
**Spec**: XFA 3.3 §3, §4.4, §5

The Form DOM is produced by merging the Template DOM with the Data DOM. It is the
input to the layout engine. In the current implementation the Form DOM is represented
as a `FormTree` (in `xfa_layout_engine::form`) rather than as a separate crate, because
`FormMerger` in `pdf-xfa` builds the tree in a single combined parse+bind pass.

The `XfaDom` type in `xfa-dom-resolver` is the planned top-level container; it currently
wraps only the `DataDom` pending full Template DOM implementation.

### FormTree

```rust
pub struct FormTree {
    pub nodes: Vec<FormNode>,
    pub metadata: Vec<FormNodeMeta>,  // parallel vec, same index as nodes
    pub node_ids: HashMap<String, FormNodeId>,  // xfa:id -> FormNodeId lookup
}
```

### FormNode

```rust
pub struct FormNode {
    pub name: String,
    pub node_type: FormNodeType,
    pub box_model: BoxModel,
    pub layout: LayoutStrategy,
    pub children: Vec<FormNodeId>,
    pub occur: Occur,
    pub font: FontMetrics,
    pub calculate: Option<String>,
    pub validate: Option<String>,
    pub column_widths: Vec<f64>,
    pub col_span: i32,
}
```

### FormNodeType variants

```rust
pub enum FormNodeType {
    Root,
    PageSet,
    PageArea { content_areas: Vec<ContentArea> },
    Subform,
    Field { value: String },     // value: bound data value after merge
    Draw(DrawContent),
    Image { data: Vec<u8>, mime_type: String },
}
```

`DrawContent` covers: `Text(String)`, `Line { x1,y1,x2,y2 }`,
`Rectangle { x,y,w,h,radius }`, `Arc { x,y,w,h,start_angle,sweep_angle }`.

### FormNodeMeta

A parallel `Vec<FormNodeMeta>` carries all attributes that the layout engine or
scripting system needs but that are not part of the core shape:

Key fields:
- `presence: Presence` — visibility lifecycle (Visible / Hidden / Invisible / Inactive)
- `page_break_before / page_break_after` — explicit page breaks
- `event_scripts: Vec<EventScript>` — FormCalc / JavaScript handlers
- `data_bind_ref: Option<String>` — explicit `<bind ref="...">` override
- `field_kind: FieldKind` — Text / Checkbox / Radio / Button / Dropdown / Signature / DateTimePicker / NumericEdit / PasswordEdit / ImageEdit / Barcode
- `style: FormNodeStyle` — font family/size/weight, colors, borders, paragraph settings
- `display_items / save_items` — choice list data for dropdowns

### Arena allocation

`FormTree` uses the same arena pattern as `DataDom`:

```rust
pub struct FormNodeId(pub usize);
```

`FormTree::add_node(node)` appends and returns a `FormNodeId`. IDs are stable. The
parallel `metadata` vec is indexed by the same `FormNodeId.0`.

### Data binding (consumeData mode)

`FormMerger` walks the template XML top-down, tracking a `data_context: Option<DataNodeId>`.
For each template node:

1. The node's `name` is matched against `DataDom::children_by_name(context, name)`.
2. If a matching data node is found, its text value is used as `FormNodeType::Field { value }`.
3. For repeating subforms (`occur.max > 1`), one `FormNode` instance is created per
   matching `DataGroup` sibling in the data.
4. If no data match exists, the node is created with an empty value.

**Implemented**: `consumeData` mode only.  
**Not implemented**: `matchTemplate` mode (data drives the merge), `bind match="global"`,
scope matching, transparent nameless subforms, re-normalisation.

### Key invariants

- Each `FormNode` in the tree has exactly one `FormNodeId`.
- `FormNodeMeta` at index `i` corresponds to `FormNode` at index `i`.
- Children are listed as `Vec<FormNodeId>` references into the same `FormTree`.
- The root node is returned separately from `FormMerger::merge()` as `(FormTree, FormNodeId)`.
- A `Field` node's `value` string contains the bound data value; empty string means unbound.

---

## 4. Layout DOM

**Source**: `crates/xfa-layout-engine/src/layout.rs`, `crates/xfa-layout-engine/src/types.rs`  
**Spec**: XFA 3.3 §4 (Box Model), §8 (Layout for Growable Objects)

The Layout DOM is the output of the layout engine. It contains fully positioned
rectangles for every visible element on every page of the output document.

### Top-level structure

```rust
pub struct LayoutDom {
    pub pages: Vec<LayoutPage>,
}

pub struct LayoutPage {
    pub width: f64,    // points
    pub height: f64,   // points
    pub nodes: Vec<LayoutNode>,
}
```

### LayoutNode

```rust
pub struct LayoutNode {
    pub form_node: FormNodeId,         // back-reference into FormTree
    pub rect: Rect,                    // bounding box in page coordinates (points)
    pub name: String,                  // debug label
    pub content: LayoutContent,        // leaf content
    pub children: Vec<LayoutNode>,     // inline child tree (not arena-based)
    pub style: FormNodeStyle,          // visual style from template
    pub display_items: Vec<String>,    // choice list display values
    pub save_items: Vec<String>,       // choice list save values
}
```

### LayoutContent variants

```rust
pub enum LayoutContent {
    None,
    Text(String),
    Field {
        value: String,
        field_kind: FieldKind,
        font_size: f64,
        font_family: FontFamily,
    },
    WrappedText {
        lines: Vec<String>,
        first_line_of_para: Vec<bool>,
        font_size: f64,
        text_align: TextAlign,
        font_family: FontFamily,
        space_above_pt: Option<f64>,
        space_below_pt: Option<f64>,
    },
    Image { data: Vec<u8>, mime_type: String },
    Draw(DrawContent),
}
```

### Coordinate system

All coordinates are in **points** (1 pt = 1/72 inch), using a **top-left origin** (y
increases downward), matching the XFA coordinate model. The render bridge transforms
these to PDF's bottom-left origin when emitting content stream operators.

### Box Model types

Defined in `types.rs`:

| Type            | Purpose                                                        |
|-----------------|----------------------------------------------------------------|
| `Point`         | 2D coordinate `{ x, y }`                                      |
| `Size`          | 2D extent `{ width, height }`                                  |
| `Rect`          | Axis-aligned bounding box `{ x, y, width, height }`           |
| `Insets`        | Four-sided insets `{ top, right, bottom, left }`               |
| `BoxModel`      | Full XFA box model: nominal size, margins, border, caption     |
| `Measurement`   | Value + unit (`in`, `cm`, `mm`, `pt`, `em`, `%`)              |
| `LayoutStrategy`| `Positioned`, `TopToBottom`, `LeftToRightTB`, `RightToLeftTB`, `Table`, `Row` |
| `Caption`       | Caption placement, reserved space, text                        |

### Memory model

`LayoutNode.children` is an **inline tree** (not arena-based). Each `LayoutNode` owns
its children directly. This differs from `DataDom` and `FormTree` which use flat arenas
with ID references.

`LayoutNode.form_node: FormNodeId` provides the back-reference to the originating
`FormNode` in the `FormTree`, enabling the render bridge to retrieve style and
field metadata.

### Key invariants

- `LayoutDom.pages` is never empty after a successful layout pass (the engine
  errors with `LayoutFailed("layout produced 0 pages")` if this would occur).
- Every `LayoutNode.rect` is in page-local coordinates (origin at top-left of the page).
- `LayoutNode.form_node` is always a valid `FormNodeId` in the `FormTree` that was
  passed to `LayoutEngine::new`.
- The maximum page count is capped at `MAX_PAGES = 500` to prevent pagination explosion.

---

## 5. Relationship between the four DOMs

```
PDF bytes
    │
    ▼  extract_xfa()
XfaPackets { template: &str, datasets: &str, ... }
    │
    ├──► DataDom::from_xml(datasets_xml)   → Data DOM (arena of DataNode)
    │
    └──► FormMerger::merge(template_xml)   → Form DOM (FormTree of FormNode)
             │  (reads Data DOM during merge)
             ▼
         (FormTree, FormNodeId)
             │
             ▼  LayoutEngine::layout(root)
         LayoutDom { pages: Vec<LayoutPage> }
             │
             ▼  render_bridge::generate_all_overlays()
         Vec<PageOverlay>  →  written into PDF content streams
```

The four DOMs are **not shared at runtime** — each stage consumes the output of the
previous one. The `Data DOM` is kept alive only for the duration of `FormMerger::merge`;
it is not referenced by the `FormTree` or `LayoutDom`.
