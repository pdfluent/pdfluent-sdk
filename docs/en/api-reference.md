# API Reference

## Rust API

### xfa-json

The primary API for XFA form data conversion.

#### `form_tree_to_json(tree: &FormTree, root: FormNodeId) -> FormData`

Extracts field values from a `FormTree` into a JSON-friendly `FormData` structure.
Fields are keyed by SOM-style dotted paths (e.g., `form1.Customer.Name`).

Repeating subforms become `FieldValue::Array` entries.

#### `form_tree_to_value(tree: &FormTree, root: FormNodeId) -> serde_json::Value`

Convenience wrapper that returns a raw `serde_json::Value`.

#### `json_to_form_tree(data: &FormData, tree: &mut FormTree, root: FormNodeId)`

Merges JSON field values back into an existing `FormTree`.
Matches fields by SOM path and handles repeating sections by array index.

#### `export_schema(tree: &FormTree, root: FormNodeId) -> FormSchema`

Exports field metadata (types, required flags, repeat rules, scripts) as a `FormSchema`.

### pdfium-ffi-bridge / template_parser

#### `parse_template(template_xml: &str, datasets_xml: Option<&str>) -> Result<(FormTree, FormNodeId)>`

Parses an XFA template XML string into a `FormTree`. Optionally merges data
values from a datasets XML string.

### Types

#### `FormData`

```rust
pub struct FormData {
    pub fields: IndexMap<String, FieldValue>,
}
```

#### `FieldValue`

```rust
pub enum FieldValue {
    Number(f64),      // Numeric values
    Boolean(bool),    // true/false, 1/0
    Text(String),     // Text strings
    Null,             // Empty/missing fields
    Array(Vec<IndexMap<String, FieldValue>>), // Repeating sections
}
```

Values are automatically coerced from XFA string representations:
- `"123.45"` → `Number(123.45)`
- `"true"`, `"1"` → `Boolean(true)`
- `""` → `Null`

#### `FormSchema`

```rust
pub struct FormSchema {
    pub fields: IndexMap<String, FieldSchema>,
}

pub struct FieldSchema {
    pub som_path: String,
    pub field_type: FieldType,  // text, numeric, boolean, static
    pub required: bool,
    pub repeatable: bool,
    pub max_occurrences: Option<u32>,
    pub calculate: Option<String>,  // FormCalc script
    pub validate: Option<String>,   // FormCalc script
}
```

### pdfium-ffi-bridge

#### PDF Reading

```rust
// From file
let reader = PdfReader::from_file(Path::new("form.pdf"))?;

// From bytes
let reader = PdfReader::from_bytes(&pdf_bytes)?;

// Extract XFA packets
let packets: XfaPackets = reader.extract_xfa()?;
```

#### `XfaPackets`

```rust
pub struct XfaPackets { /* private fields */ }

impl XfaPackets {
    /// Get a specific packet by name.
    pub fn get_packet(&self, name: &str) -> Option<&str>;
    /// Get the template packet.
    pub fn template(&self) -> Option<&str>;
    /// Get the datasets packet.
    pub fn datasets(&self) -> Option<&str>;
    /// Get the config packet.
    pub fn config(&self) -> Option<&str>;
}
```

#### Rendering

```rust
use pdfium_ffi_bridge::pipeline::{render_form_tree, save_pages_as_png};
use pdfium_ffi_bridge::native_renderer::RenderConfig;

let config = RenderConfig::default();        // 72 DPI
let config = RenderConfig::with_dpi(144.0);  // 144 DPI (2x)

let images = render_form_tree(&mut tree, root, &config)?;
save_pages_as_png(&images, Path::new("output/"), "form")?;
```

## REST API Endpoints

### `POST /extract`

Extract field values from an XFA PDF.

**Request:** `multipart/form-data` with `file` field
**Response:** `application/json`

```json
{
  "fields": {
    "form1.Name": "John Doe",
    "form1.Amount": 100.0
  }
}
```

### `POST /schema`

Export the form schema (field types, constraints).

**Request:** `multipart/form-data` with `file` field
**Response:** `application/json`

```json
{
  "fields": {
    "form1.Name": {
      "som_path": "form1.Name",
      "field_type": "text",
      "required": true,
      "repeatable": false,
      "max_occurrences": 1
    }
  }
}
```

### `POST /validate`

Validate form field values against their schemas and scripts.

**Request:** `multipart/form-data` with `file` field
**Response:** `application/json`

```json
{
  "valid": true,
  "errors": []
}
```

### `POST /fill`

Fill form fields with provided data and return the modified PDF.

**Request:** `multipart/form-data` with `file` and `data` fields
**Response:** `application/pdf`

### `POST /flatten`

Flatten XFA form to static AcroForm PDF.

**Request:** `multipart/form-data` with `file` field
**Response:** `application/pdf`

### `POST /render`

Render form pages to PNG images.

**Request:** `multipart/form-data` with `file` field, optional `dpi` parameter
**Response:** `application/json` with base64-encoded images

## Error Responses

All endpoints return errors in this format:

```json
{
  "error": "Description of what went wrong"
}
```

HTTP status codes:
- `200` — Success
- `400` — Invalid input (missing file, bad JSON)
- `422` — XFA parsing or validation error
- `500` — Internal server error
