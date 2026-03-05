# Quickstart: PDF naar JSON in 5 Minuten

## Installatie

Voeg `xfa-json` en `pdfium-ffi-bridge` toe aan je `Cargo.toml`:

```toml
[dependencies]
xfa-json = { git = "https://github.com/jasperdew/xfa-native-rust" }
pdfium-ffi-bridge = { git = "https://github.com/jasperdew/xfa-native-rust" }
```

## Velden Extraheren uit een XFA PDF

```rust
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use pdfium_ffi_bridge::pipeline::extract_xfa_from_file;
use xfa_json::form_tree_to_json;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    // 1. Open de PDF en extraheer XFA-pakketten
    let packets = extract_xfa_from_file(Path::new("formulier.pdf"))?;

    // 2. Parseer het template naar een FormTree
    let (tree, root) = xfa_json::import::parse_template(&packets.template)?;

    // 3. Exporteer veldwaarden als JSON
    let data = form_tree_to_json(&tree, root);
    println!("{}", serde_json::to_string_pretty(&data)?);

    Ok(())
}
```

### Uitvoer

```json
{
  "fields": {
    "form1.Klant.Naam": "Jan de Vries",
    "form1.Klant.Email": "jan@voorbeeld.nl",
    "form1.Factuur.Bedrag": 1250.00,
    "form1.Factuur.Betaald": true
  }
}
```

## Formulier Invullen met JSON Data

```rust
use xfa_json::{json_to_form_tree, FormData, FieldValue};
use indexmap::IndexMap;

// Formulierdata opbouwen
let mut fields = IndexMap::new();
fields.insert("form1.Klant.Naam".into(), FieldValue::Text("Maria Jansen".into()));
fields.insert("form1.Factuur.Bedrag".into(), FieldValue::Number(2500.0));
let data = FormData { fields };

// Samenvoegen met bestaande FormTree
json_to_form_tree(&data, &mut tree, root);
```

## Schema Exporteren

```rust
use xfa_json::export_schema;

let schema = export_schema(&tree, root);
println!("{}", serde_json::to_string_pretty(&schema)?);
```

## REST API

```bash
# Velden extraheren
curl -X POST http://localhost:3000/extract \
  -F "file=@formulier.pdf" | jq .

# Formulier invullen
curl -X POST http://localhost:3000/fill \
  -F "file=@formulier.pdf" \
  -F 'data={"form1.Naam": "Maria"}' \
  --output ingevuld.pdf

# Valideren
curl -X POST http://localhost:3000/validate \
  -F "file=@formulier.pdf" | jq .

# Afvlakken (XFA naar AcroForm)
curl -X POST http://localhost:3000/flatten \
  -F "file=@formulier.pdf" \
  --output afgevlakt.pdf
```

## Volgende Stappen

- [API Referentie](api-reference.md) — Volledige endpoint- en typedocumentatie
- [Codevoorbeelden](examples.md) — Python, JavaScript, C# en Java integratie
