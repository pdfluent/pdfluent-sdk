# XFA-Native-Rust — Architecture Specification

> Based on XFA 3.3 Specification (https://pdfa.org/norm-refs/XFA-3_3.pdf)
> and FormCalc Reference (https://helpx.adobe.com/pdf/aem-forms/6-2/formcalc-reference.pdf)

---

## 1. System Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         xfa-cli (Entry Point)                       │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌──────────────┐   ┌───────────────────┐   ┌───────────────────┐  │
│  │  pdfium-ffi  │──▶│  xfa-dom-resolver │──▶│    formcalc       │  │
│  │   -bridge    │   │                   │   │   -interpreter    │  │
│  │              │   │  Template DOM     │   │                   │  │
│  │  PDF ↔ XFA   │   │  Data DOM        │   │  Lexer → Parser   │  │
│  │  Rendering   │   │  Form DOM        │   │  → AST → Eval     │  │
│  │  Events      │   │  SOM Resolution  │   │  Built-in Funcs   │  │
│  └──────┬───────┘   └────────┬──────────┘   └───────────────────┘  │
│         │                    │                                      │
│         │                    ▼                                      │
│         │           ┌───────────────────┐                          │
│         └──────────▶│  xfa-layout       │                          │
│                     │   -engine         │                          │
│                     │                   │                          │
│                     │  Box Model        │                          │
│                     │  Flow/Position    │                          │
│                     │  Pagination       │                          │
│                     │  Content Split    │                          │
│                     │  Tables           │                          │
│                     └───────────────────┘                          │
│                                                                     │
├─────────────────────────────────────────────────────────────────────┤
│                    xfa-golden-tests (Visual Regression)              │
└─────────────────────────────────────────────────────────────────────┘
```

### Processing Pipeline

```
PDF File
  │
  ▼ (1) pdfium-ffi-bridge: Extract XFA packets
  │
  ▼ (2) xfa-dom-resolver: Parse XML → Template DOM + Data DOM
  │
  ▼ (3) xfa-dom-resolver: Data Binding (Merge) → Form DOM
  │
  ▼ (4) formcalc-interpreter: Execute calculations & validations
  │
  ▼ (5) xfa-layout-engine: Layout Form DOM → Layout DOM
  │
  ▼ (6) pdfium-ffi-bridge: Render Layout DOM → Pixel Output
  │
  ▼ (7) Interactive: Events → FormCalc → Re-layout → Re-render
```

---

## 2. Module A: `xfa-dom-resolver`

### 2.1 DOM Hierarchy

The XFA DOM is the root container for all sub-DOMs:

```
xfa (root)
  ├── config          → Config DOM
  ├── datasets
  │     ├── data            → Data DOM (dataGroup / dataValue nodes)
  │     └── dataDescription → Data Description DOM
  ├── form             → Form DOM (result of merge)
  ├── layout           → Layout DOM (result of layout)
  └── template         → Template DOM
```

#### Rust Type Design

```rust
/// Root of the XFA object model
pub struct XfaDom {
    pub config: ConfigDom,
    pub template: TemplateDom,
    pub data: DataDom,
    pub form: FormDom,
    pub layout: LayoutDom,
}

/// Template DOM — parsed from <template> XFA packet
pub struct TemplateDom {
    arena: Vec<TemplateNode>,  // Arena-allocated nodes
    root: NodeId,
}

/// Data DOM — two node types only
pub enum DataNode {
    DataGroup {
        name: String,
        namespace: Option<String>,
        children: Vec<NodeId>,
        attributes: Vec<NodeId>,  // dataValue nodes for XML attributes
        is_record: bool,
    },
    DataValue {
        name: String,
        namespace: Option<String>,
        value: String,
        contains: DataContains,  // Data or MetaData
        content_type: Option<String>,
        is_null: bool,
        null_type: NullType,     // Exclude, Empty, Xsi
    },
}

/// Form DOM — merged template + data, actively linked to Data DOM
pub struct FormNode {
    template_ref: NodeId,         // Back-reference to Template DOM node
    data_binding: Option<NodeId>, // Bound Data DOM node (live link)
    children: Vec<NodeId>,
    properties: HashMap<String, PropertyValue>,
}
```

### 2.2 SOM Path Resolution

**Grammar** (from XFA 3.3 §3):

```
som_expr     := qualified | unqualified
qualified    := ('$' shortcut | 'xfa') ('.' segment)*
unqualified  := segment ('.' segment)*
segment      := (name | '#' class) index? predicate?
name         := XFA_NAME              // XML name without ':'
class        := XFA_NAME
index        := '[' (integer | '*' | relative) ']'
relative     := ('+' | '-') integer
predicate    := '.[' formcalc_expr ']' | '.(' javascript_expr ')'
```

**Shortcuts:**

| Shortcut | Resolves to |
|----------|-------------|
| `$data` | `xfa.datasets.data` |
| `$template` | `xfa.template` |
| `$form` | `xfa.form` |
| `$layout` | `xfa.layout` |
| `$host` | `xfa.host` |
| `$record` | Current data record |
| `$event` | Current event properties |
| `!` | `xfa.datasets.` (no dot needed after `!`) |

#### Unqualified Reference Resolution Algorithm

Search outward from the current container (spec §3, p.114):

```
fn resolve_unqualified(name: &str, current: NodeId) -> Option<NodeId> {
    let mut scope = current;
    loop {
        // 1. Search children of scope
        if let Some(node) = find_child_by_name(scope, name) {
            return Some(node);
        }
        // 2. Search scope itself and siblings
        if node_name(scope) == name {
            return Some(scope);
        }
        if let Some(node) = find_sibling_by_name(scope, name) {
            return Some(node);
        }
        // 3. Move to parent
        match parent(scope) {
            Some(p) => scope = p,
            None => return None, // Reached root, not found
        }
    }
}
```

#### Index Inferral (spec §3, p.115-119)

When an unqualified reference omits an index and scope-matching is used:

```
fn infer_index(target_name: &str, current: NodeId) -> usize {
    // Walk up from current to find the scope level where target was found
    // Use the index of the current container (or its ancestor) at that level
    // If no index can be inferred, default to 0
}
```

#### Transparency Rules

- **Unnamed subforms** are transparent: their children are adopted by their parent for SOM purposes
- **`area` elements** are always transparent, even if named
- **`variables` elements** are always transparent
- **`traverse` elements** are never transparent (always named via `operation` attribute)
- Transparency only applies in Form and Template DOMs

### 2.3 Data Binding (Merge Algorithm)

The 8-step merge process (spec §4, p.122-214):

```
Step 1-2: Create Form nodes, match with Data nodes
  ├── Direct match: name + ancestor path must match
  ├── Scope match (consumeData mode only):
  │     ├── Ancestor match (higher priority)
  │     └── Sibling match (lower priority)
  └── Type compatibility:
        ├── subform ↔ dataGroup only
        └── field ↔ dataValue only

Step 3: Match remaining attributes

Step 4: Re-normalization (consumeData mode)
  └── Adjust Data DOM to mirror Form DOM structure

Step 5: Bind to properties (setProperty, bindItems)

Step 6: Execute calculations and validations

Step 7: Fire form:ready event

Step 8: Handle remerge if scripts modified Data DOM
```

---

## 3. Module B: `xfa-layout-engine`

### 3.1 Box Model (spec §4)

```
┌──────────────────────────────────────────┐
│              Nominal Extent (w × h)       │
│  ┌──────── Margins ─────────────────┐    │
│  │  ┌──── Border ───────────────┐   │    │
│  │  │                           │   │    │
│  │  │    Nominal Content Region │   │    │
│  │  │  ┌── Caption Region ──┐  │   │    │
│  │  │  │                    │  │   │    │
│  │  │  └────────────────────┘  │   │    │
│  │  │       Content Area       │   │    │
│  │  └───────────────────────────┘   │    │
│  └──────────────────────────────────┘    │
└──────────────────────────────────────────┘
```

#### Rust Types

```rust
pub struct BoxModel {
    pub nominal_extent: Size,        // w, h (nullable = growable)
    pub margins: Insets,             // topInset, rightInset, bottomInset, leftInset
    pub border: Option<Border>,
    pub caption: Option<CaptionRegion>,
}

pub struct Size {
    pub width: Option<Measurement>,  // None = growable in X
    pub height: Option<Measurement>, // None = growable in Y
}

pub struct Measurement {
    pub value: f64,
    pub unit: MeasurementUnit,
}

pub enum MeasurementUnit {
    Inches,      // default
    Centimeters,
    Millimeters,
    Points,      // 1pt = 1/72 inch
    Em,          // relative to current font
    Percent,     // relative to space width
}

pub struct Insets {
    pub top: f64,    // in points (internal unit)
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

pub struct Border {
    pub edges: [Edge; 4],     // top, right, bottom, left
    pub corners: [Corner; 4], // TL, TR, BR, BL
    pub fill: Fill,
    pub break_behavior: BorderBreak, // Close or Open (for splitting)
}

pub struct CaptionRegion {
    pub placement: CaptionPlacement, // Top, Bottom, Left, Right
    pub reserve: Option<f64>,        // explicit size; None = auto-calculate
    pub content: ContentValue,
}
```

#### Growability Rules

| `h` | `w` | Growth |
|-----|-----|--------|
| Set | Set | Fixed — `min/max` ignored |
| Set | None | X axis — `minW`/`maxW` apply |
| None | Set | Y axis — `minH`/`maxH` apply |
| None | None | Both axes |

- Default `minH`/`minW` = 0
- Default `maxH`/`maxW` = ∞

### 3.2 Layout Strategies

```rust
pub enum LayoutStrategy {
    Positioned,  // Fixed x,y coordinates
    TopToBottom,  // layout="tb"
    LeftToRightTB, // layout="lr-tb"
    RightToLeftTB, // layout="rl-tb"
    Table,        // layout="table"
    Row,          // layout="row"
    RlRow,        // layout="rl-row"
}
```

#### Top-to-Bottom Algorithm (`tb`)

```rust
fn layout_tb(container: &mut LayoutContainer, children: &[FormNode]) {
    let mut y_cursor = 0.0;

    for child in children {
        let child_extent = compute_extent(child);

        if y_cursor + child_extent.height > container.available_height() {
            // Try to split
            match try_split(child, container.available_height() - y_cursor) {
                SplitResult::Success(top, bottom) => {
                    place(container, top, 0.0, y_cursor);
                    defer_to_next(container, bottom);
                }
                SplitResult::CannotSplit => {
                    defer_to_next(container, child);
                }
            }
        } else {
            place(container, child, 0.0, y_cursor);
            y_cursor += child_extent.height;
        }
    }
}
```

#### Left-to-Right Top-to-Bottom Algorithm (`lr-tb`)

```rust
fn layout_lr_tb(container: &mut LayoutContainer, children: &[FormNode]) {
    let mut x_cursor = 0.0;
    let mut y_cursor = 0.0;
    let mut row_height = 0.0;

    for child in children {
        let child_extent = compute_extent(child);

        // Does it fit horizontally?
        if x_cursor + child_extent.width > container.available_width() {
            // Wrap to next row
            y_cursor += row_height;
            x_cursor = 0.0;
            row_height = 0.0;
        }

        // Does it fit vertically?
        if y_cursor + child_extent.height > container.available_height() {
            match try_split(child, container.available_height() - y_cursor) {
                SplitResult::Success(top, bottom) => {
                    place(container, top, x_cursor, y_cursor);
                    defer_to_next(container, bottom);
                }
                SplitResult::CannotSplit => {
                    defer_to_next(container, child);
                }
            }
            continue;
        }

        place(container, child, x_cursor, y_cursor);
        x_cursor += child_extent.width;
        row_height = row_height.max(child_extent.height);
    }
}
```

### 3.3 Content Splitting

#### Split Consensus Algorithm

```rust
fn find_split_point(container: &LayoutObject, desired_y: f64) -> f64 {
    let mut split_y = desired_y;

    loop {
        let mut all_ok = true;
        for child in container.children() {
            if !child.can_split_at(split_y) {
                let new_y = child.highest_permissible_split_below(split_y);
                if new_y < split_y {
                    split_y = new_y;
                    all_ok = false;
                    break; // restart checking with new split_y
                }
            }
        }
        if all_ok || split_y <= 0.0 {
            break;
        }
    }

    split_y // 0.0 means cannot split at all
}
```

#### Split Rules by Content Type

| Content | Splittable | Notes |
|---------|-----------|-------|
| Text (variable) | Yes | Between lines, subject to orphan/widow |
| Barcode | No | |
| Image | No | |
| Widget (button, checkbox, etc.) | No | |
| Rotated text | No | |

#### `keep.intact` Property

| Value | Meaning |
|-------|---------|
| `none` | Free to split |
| `contentArea` | Must stay in single contentArea |
| `contentArea.pageArea` | May split within contentArea but not across pages |

### 3.4 Pagination

#### Three Strategies

```rust
pub enum PaginationStrategy {
    OrderedOccurrence,  // Default: depth-first traversal
    SimplexPaginated,   // One-sided printing
    DuplexPaginated,    // Two-sided printing
}
```

#### Page Selection (Qualified Pagination)

```rust
struct PageLayoutState {
    page_position: PagePosition,    // First, Rest, Last, Only
    odd_or_even: OddOrEven,        // Odd, Even
    blank_or_not: BlankOrNotBlank,  // Blank, NotBlank
}

fn select_page_area(
    page_set: &PageSet,
    state: &PageLayoutState,
) -> Result<&PageArea, LayoutError> {
    page_set.children()
        .filter(|pa| !pa.is_exhausted())
        .find(|pa| pa.matches(state))
        .ok_or(LayoutError::NoMatchingPageArea)
}
```

#### ContentArea Overflow

```rust
fn handle_overflow(layout: &mut LayoutDom, current: ContentAreaId) -> ContentAreaId {
    // 1. Try next sibling contentArea on same page
    if let Some(next) = next_sibling_content_area(current) {
        return next;
    }

    // 2. All contentAreas on page are full → new page
    let page_area = select_next_page_area(layout);
    let first_content_area = page_area.first_content_area();
    first_content_area
}
```

#### Boundary Override (XFA 2.8+)

When both current and next content regions are empty AND content won't fit in either:
→ Override boundaries and force content onto current page (prevents infinite loops).

### 3.5 Adhesion (Keep Together)

```rust
pub struct KeepConstraint {
    pub next: KeepValue,      // Adhesion to next sibling
    pub previous: KeepValue,  // Adhesion to previous sibling
    pub intact: IntactValue,  // Splitting constraint
}

pub enum KeepValue {
    None,        // No adhesion
    ContentArea, // Must be in same content region
    PageArea,    // Must be on same page
}
```

Two objects adhere if either declares adhesion to the other. Adhesion chains until a non-adhering object is found.

### 3.6 Leaders & Trailers

Three types, each with distinct placement:

| Type | When | Leader placement | Trailer placement |
|------|------|-----------------|-------------------|
| Break | At page/contentArea breaks | Current region (before break) | New region (after break) |
| Bookend | First/last of flowing subform | First child | Last child |
| Overflow | Content overflows to next region | First in new fragment | Last in current fragment |

**Overflow trailer space reservation:** The layout engine must reserve space for the overflow trailer, which may itself cause overflow.

### 3.7 Table Layout

```rust
fn layout_table(table: &TableSubform) -> TableLayout {
    let col_widths = resolve_column_widths(table);

    let mut rows = Vec::new();
    for row in table.rows() {
        let mut cells = Vec::new();
        let mut col_idx = 0;

        for cell in row.cells() {
            let span = cell.col_span(); // 1, N, or -1 (rest)
            let cell_width = if span == -1 {
                col_widths[col_idx..].iter().sum()
            } else {
                col_widths[col_idx..col_idx + span].iter().sum()
            };

            let cell_layout = layout_cell(cell, cell_width);
            cells.push(cell_layout);
            col_idx += span.max(1) as usize;
        }

        // All cells in row expand to tallest cell height
        let row_height = cells.iter().map(|c| c.height).fold(0.0, f64::max);
        for cell in &mut cells {
            cell.height = row_height;
        }

        rows.push(RowLayout { cells, height: row_height });
    }

    TableLayout { rows, col_widths }
}

fn resolve_column_widths(table: &TableSubform) -> Vec<f64> {
    let specified = table.column_widths(); // space-separated list
    let max_cols = table.rows().map(|r| r.cell_count()).max();

    (0..max_cols).map(|i| {
        match specified.get(i) {
            Some(&w) if w >= 0.0 => w,
            _ => { // -1 or unspecified: natural width of widest cell
                table.rows()
                    .filter_map(|r| r.cell_at(i))
                    .map(|c| c.natural_width())
                    .fold(0.0, f64::max)
            }
        }
    }).collect()
}
```

---

## 4. Module C: `formcalc-interpreter`

### 4.1 Lexer

Token types based on FormCalc grammar (spec §25.3):

```rust
pub enum Token {
    // Literals
    StringLiteral(String),
    NumberLiteral(f64),

    // Keywords
    Break, Continue, Do, Downto, Else, ElseIf, End, EndFor,
    EndFunc, EndIf, EndWhile, For, Foreach, Func, If, In,
    Null, Return, Step, Then, Throw, Upto, Var, While,

    // Operators
    Plus, Minus, Star, Slash,         // arithmetic
    Eq, Ne, Lt, Le, Gt, Ge,          // relational (== <> < <= > >=)
    And, Or, Not,                     // logical
    Ampersand,                        // string concatenation
    Assign,                           // =
    Dot, DotDot, DotStar, Hash,      // SOM navigation

    // Delimiters
    LParen, RParen, LBracket, RBracket,
    Comma, Semicolon,

    // Special
    Identifier(String),
    Eof,
}
```

### 4.2 AST

```rust
pub enum Expr {
    Literal(Literal),
    Identifier(String),
    SomRef(SomExpression),

    // Unary
    UnaryMinus(Box<Expr>),
    Not(Box<Expr>),

    // Binary
    BinaryOp { op: BinOp, left: Box<Expr>, right: Box<Expr> },

    // Control flow
    If { condition: Box<Expr>, then_branch: Vec<Expr>, else_branch: Option<Vec<Expr>> },
    While { condition: Box<Expr>, body: Vec<Expr> },
    For { var: String, start: Box<Expr>, end: Box<Expr>, step: Option<Box<Expr>>, body: Vec<Expr> },
    Foreach { var: String, list: Box<Expr>, body: Vec<Expr> },

    // Functions
    FuncDecl { name: String, params: Vec<String>, body: Vec<Expr> },
    FuncCall { name: String, args: Vec<Expr> },

    // Assignment
    Assign { target: Box<Expr>, value: Box<Expr> },

    // Special
    Return(Option<Box<Expr>>),
    Break,
    Continue,
    Throw(Box<Expr>),

    // Expression list (script = sequence of expressions)
    ExprList(Vec<Expr>),
}
```

### 4.3 Built-in Functions

| Category | Functions |
|----------|-----------|
| Arithmetic | `Abs`, `Avg`, `Ceil`, `Count`, `Floor`, `Max`, `Min`, `Mod`, `Round`, `Sum` |
| Date/Time | `Date`, `Date2Num`, `DateFmt`, `IsoDate2Num`, `IsoTime2Num`, `LocalDateFmt`, `LocalTimeFmt`, `Num2Date`, `Num2GMTime`, `Num2Time`, `Time`, `Time2Num`, `TimeFmt` |
| String | `At`, `Concat`, `Decode`, `Encode`, `Format`, `Left`, `Len`, `Lower`, `Ltrim`, `Parse`, `Replace`, `Right`, `Rtrim`, `Space`, `Str`, `Stuff`, `Substr`, `Uuid`, `Upper`, `WordNum` |
| Financial | `Apr`, `Pmt`, `Ppmt`, `Pv`, `Rate`, `Term` |
| Logical | `Choose`, `If`, `Oneof`, `Within` |
| Misc | `Eval`, `Null`, `Return` |

### 4.4 Type System

FormCalc has automatic type coercion between three types:

```rust
pub enum FormCalcValue {
    Number(f64),
    String(String),
    Null,
}

impl FormCalcValue {
    pub fn to_number(&self) -> f64 { /* coercion rules */ }
    pub fn to_string(&self) -> String { /* coercion rules */ }
    pub fn to_bool(&self) -> bool { /* 0/empty = false, else true */ }
}
```

### 4.5 SOM Integration

The interpreter resolves SOM expressions to DOM nodes:

```rust
fn eval_som_ref(expr: &SomExpression, ctx: &ScriptContext) -> FormCalcValue {
    let node = ctx.dom.resolve_node(expr, ctx.current_container);
    match node {
        Some(n) => n.value().into(),  // reads rawValue
        None => FormCalcValue::Null,
    }
}
```

---

## 5. Module D: `pdfium-ffi-bridge`

### 5.1 XFA Packet Extraction

```rust
pub fn extract_xfa_packets(pdf_path: &Path) -> Result<XfaPackets, Error> {
    let pdfium = Pdfium::new(/* ... */);
    let doc = pdfium.load_pdf_from_file(pdf_path)?;

    // XFA data is stored in the AcroForm dictionary under key "XFA"
    // It contains paired entries: (packet_name, stream_ref)*
    let xfa_xml = doc.get_xfa_xml()?;

    // Parse individual packets
    let template = extract_packet(&xfa_xml, "template")?;
    let datasets = extract_packet(&xfa_xml, "datasets")?;
    let config = extract_packet(&xfa_xml, "config")?;

    Ok(XfaPackets { template, datasets, config })
}
```

### 5.2 FPDF_FORMFILLINFO Callbacks

```rust
/// FFI callback structure matching PDFium's FPDF_FORMFILLINFO
#[repr(C)]
pub struct XfaFormFillInfo {
    pub version: c_int,
    // Required callbacks
    pub release: Option<extern "C" fn(*mut XfaFormFillInfo)>,
    pub ffi_invalidate: Option<extern "C" fn(/* page, rect params */)>,
    pub ffi_set_timer: Option<extern "C" fn(/* timer params */) -> c_int>,
    pub ffi_kill_timer: Option<extern "C" fn(/* timer_id */)>,
    // XFA-specific
    pub ffi_display_caret: Option<extern "C" fn(/* caret params */)>,
    pub ffi_get_rotation: Option<extern "C" fn(/* page */) -> c_int>,
    pub ffi_execute_named_action: Option<extern "C" fn(/* action name */)>,
}
```

### 5.3 Render Pipeline

```rust
pub fn render_xfa_to_png(
    pdf_path: &Path,
    page_index: usize,
    dpi: u32,
    output_path: &Path,
) -> Result<(), Error> {
    let pdfium = Pdfium::new(/* ... */);
    let doc = pdfium.load_pdf_from_file(pdf_path)?;
    let page = doc.pages().get(page_index)?;

    let width = (page.width().value * dpi as f32 / 72.0) as u32;
    let height = (page.height().value * dpi as f32 / 72.0) as u32;

    let bitmap = page.render(width, height, /* render config */)?;
    bitmap.save_as_png(output_path)?;
    Ok(())
}
```

---

## 6. Persistence & Security

### 6.1 Dataset Sync

On save, the Data DOM must be serialized back into the PDF:

```rust
pub fn save_form_data(doc: &mut PdfDocument, form: &FormDom) -> Result<(), Error> {
    // 1. Serialize Data DOM to XML
    let data_xml = form.data_dom.to_xml()?;

    // 2. Handle null values per nullType setting
    //    - Exclude: omit null nodes
    //    - Empty: write <element/>
    //    - Xsi: write <element xsi:nil="true"/>

    // 3. Replace <xfa:datasets> packet in PDF
    doc.replace_xfa_packet("datasets", &data_xml)?;

    Ok(())
}
```

### 6.2 UR3 Signature Handling

```rust
pub fn detect_ur3_signatures(doc: &PdfDocument) -> Vec<Ur3Signature> {
    // Check for Usage Rights (UR3) signatures in the PDF
    // These restrict editing in Adobe Reader
    // Located in the document's Perms dictionary
}

pub fn remove_ur3_signatures(doc: &mut PdfDocument) -> Result<(), Error> {
    // Safely remove UR3 signatures:
    // 1. Remove Perms dictionary entry
    // 2. Remove associated signature objects
    // 3. Update cross-reference table
    // 4. Do NOT invalidate document structure
}
```

---

## 7. Golden Render Test Infrastructure

### 7.1 Pipeline

```
XFA PDF → pdfium-render → PNG (actual)
                                ↓
Adobe Reader screenshot → PNG (expected)
                                ↓
                        Pixel diff → Report
                                ↓
                        Pass/Fail (threshold)
```

### 7.2 Comparison Algorithm

```rust
pub struct GoldenTestResult {
    pub total_pixels: u64,
    pub differing_pixels: u64,
    pub max_channel_diff: u8,
    pub diff_percentage: f64,
    pub passed: bool,
}

pub fn compare_golden(
    actual: &Path,
    expected: &Path,
    threshold: f64,  // e.g., 0.01 = 1% pixel difference allowed
) -> GoldenTestResult {
    // Load both images
    // Compare pixel-by-pixel (RGBA channels)
    // Generate diff image highlighting differences
    // Return pass/fail based on threshold
}
```

---

## 8. Internal Units & Coordinate System

All internal calculations use **points** (1 pt = 1/72 inch) as the canonical unit.

Conversion factors:
- 1 inch = 72 pt
- 1 cm = 28.3465 pt
- 1 mm = 2.83465 pt

Coordinate system:
- Origin at **top-left** of content area
- X increases to the right
- Y increases downward
- Angles measured counterclockwise from horizontal

---

## 9. Error Handling Strategy

```rust
#[derive(Debug, thiserror::Error)]
pub enum XfaError {
    // DOM errors
    #[error("SOM path resolution failed: {path}")]
    SomResolutionFailed { path: String },

    #[error("Invalid node type: expected {expected}, got {got}")]
    InvalidNodeType { expected: &'static str, got: String },

    // Layout errors
    #[error("No matching page area for layout state")]
    NoMatchingPageArea,

    #[error("Page area occurrence limit exhausted")]
    PageAreaExhausted,

    // FormCalc errors
    #[error("Parse error at line {line}, col {col}: {message}")]
    ParseError { line: usize, col: usize, message: String },

    #[error("Runtime error: {0}")]
    RuntimeError(String),

    // PDF errors
    #[error("PDF error: {0}")]
    PdfError(String),

    #[error("XFA packet not found: {0}")]
    XfaPacketNotFound(String),
}
```
