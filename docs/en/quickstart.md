# Quickstart: PDF to JSON in 5 Minutes

## Installation

Add `xfa-json` and `pdfium-ffi-bridge` to your `Cargo.toml`:

```toml
[dependencies]
xfa-json = { git = "https://github.com/jasperdew/xfa-native-rust" }
pdfium-ffi-bridge = { git = "https://github.com/jasperdew/xfa-native-rust" }
```

## Extract Fields from an XFA PDF

```rust
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use pdfium_ffi_bridge::pipeline::extract_xfa_from_file;
use xfa_json::form_tree_to_json;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    // 1. Open the PDF and extract XFA packets
    let packets = extract_xfa_from_file(Path::new("form.pdf"))?;

    // 2. Parse the template into a FormTree
    let (tree, root) = xfa_json::import::parse_template(&packets.template)?;

    // 3. Export field values as JSON
    let data = form_tree_to_json(&tree, root);
    println!("{}", serde_json::to_string_pretty(&data)?);

    Ok(())
}
```

### Output

```json
{
  "fields": {
    "form1.Customer.Name": "John Doe",
    "form1.Customer.Email": "john@example.com",
    "form1.Invoice.Amount": 1250.00,
    "form1.Invoice.Paid": true
  }
}
```

## Fill a Form with JSON Data

```rust
use xfa_json::{json_to_form_tree, FormData, FieldValue};
use indexmap::IndexMap;

// Build form data
let mut fields = IndexMap::new();
fields.insert("form1.Customer.Name".into(), FieldValue::Text("Jane Smith".into()));
fields.insert("form1.Invoice.Amount".into(), FieldValue::Number(2500.0));
let data = FormData { fields };

// Merge into existing FormTree
json_to_form_tree(&data, &mut tree, root);
```

## Export Schema

```rust
use xfa_json::export_schema;

let schema = export_schema(&tree, root);
println!("{}", serde_json::to_string_pretty(&schema)?);
```

### Schema Output

```json
{
  "fields": {
    "form1.Customer.Name": {
      "som_path": "form1.Customer.Name",
      "field_type": "text",
      "required": true,
      "repeatable": false,
      "max_occurrences": 1
    },
    "form1.Invoice.Amount": {
      "som_path": "form1.Invoice.Amount",
      "field_type": "numeric",
      "required": true,
      "repeatable": false,
      "max_occurrences": 1
    }
  }
}
```

## REST API

If you're using the REST API server:

```bash
# Extract fields
curl -X POST http://localhost:3000/extract \
  -F "file=@form.pdf" | jq .

# Fill form
curl -X POST http://localhost:3000/fill \
  -F "file=@form.pdf" \
  -F 'data={"form1.Name": "Jane"}' \
  --output filled.pdf

# Validate
curl -X POST http://localhost:3000/validate \
  -F "file=@form.pdf" | jq .

# Flatten (XFA → AcroForm)
curl -X POST http://localhost:3000/flatten \
  -F "file=@form.pdf" \
  --output flattened.pdf
```

## Next Steps

- [API Reference](api-reference.md) — Full endpoint and type documentation
- [Code Examples](examples.md) — Python, JavaScript, C#, and Java integration
