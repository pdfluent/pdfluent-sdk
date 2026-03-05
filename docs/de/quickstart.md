# Schnellstart: PDF zu JSON in 5 Minuten

## Installation

Fuegen Sie `xfa-json` und `pdfium-ffi-bridge` zu Ihrer `Cargo.toml` hinzu:

```toml
[dependencies]
xfa-json = { git = "https://github.com/jasperdew/xfa-native-rust" }
pdfium-ffi-bridge = { git = "https://github.com/jasperdew/xfa-native-rust" }
```

## Felder aus einem XFA PDF extrahieren

```rust
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use pdfium_ffi_bridge::pipeline::extract_xfa_from_file;
use xfa_json::form_tree_to_json;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    // 1. PDF oeffnen und XFA-Pakete extrahieren
    let packets = extract_xfa_from_file(Path::new("formular.pdf"))?;

    // 2. Template in einen FormTree parsen
    let (tree, root) = xfa_json::import::parse_template(&packets.template)?;

    // 3. Feldwerte als JSON exportieren
    let data = form_tree_to_json(&tree, root);
    println!("{}", serde_json::to_string_pretty(&data)?);

    Ok(())
}
```

### Ausgabe

```json
{
  "fields": {
    "form1.Kunde.Name": "Max Mustermann",
    "form1.Kunde.Email": "max@beispiel.de",
    "form1.Rechnung.Betrag": 1250.00,
    "form1.Rechnung.Bezahlt": true
  }
}
```

## Formular mit JSON-Daten ausfuellen

```rust
use xfa_json::{json_to_form_tree, FormData, FieldValue};
use indexmap::IndexMap;

// Formulardaten aufbauen
let mut fields = IndexMap::new();
fields.insert("form1.Kunde.Name".into(), FieldValue::Text("Maria Mueller".into()));
fields.insert("form1.Rechnung.Betrag".into(), FieldValue::Number(2500.0));
let data = FormData { fields };

// In bestehenden FormTree zusammenfuehren
json_to_form_tree(&data, &mut tree, root);
```

## Schema exportieren

```rust
use xfa_json::export_schema;

let schema = export_schema(&tree, root);
println!("{}", serde_json::to_string_pretty(&schema)?);
```

## REST API

```bash
# Felder extrahieren
curl -X POST http://localhost:3000/extract \
  -F "file=@formular.pdf" | jq .

# Formular ausfuellen
curl -X POST http://localhost:3000/fill \
  -F "file=@formular.pdf" \
  -F 'data={"form1.Name": "Maria"}' \
  --output ausgefuellt.pdf

# Validieren
curl -X POST http://localhost:3000/validate \
  -F "file=@formular.pdf" | jq .

# Abflachen (XFA nach AcroForm)
curl -X POST http://localhost:3000/flatten \
  -F "file=@formular.pdf" \
  --output abgeflacht.pdf
```

## Naechste Schritte

- [API-Referenz](api-reference.md) — Vollstaendige Endpoint- und Typdokumentation
- [Codebeispiele](examples.md) — Python, JavaScript, C# und Java Integration
