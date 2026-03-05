# API Referentie

## Rust API

### xfa-json

De primaire API voor XFA-formulierdata conversie.

#### `form_tree_to_json(tree: &FormTree, root: FormNodeId) -> FormData`

Extraheert veldwaarden uit een `FormTree` naar een JSON-vriendelijke `FormData` structuur.
Velden worden geindexeerd op SOM-paden (bijv. `form1.Klant.Naam`).

Herhalende subformulieren worden `FieldValue::Array` items.

#### `json_to_form_tree(data: &FormData, tree: &mut FormTree, root: FormNodeId)`

Voegt JSON-veldwaarden samen met een bestaande `FormTree`.
Matcht velden op SOM-pad en verwerkt herhalende secties op array-index.

#### `export_schema(tree: &FormTree, root: FormNodeId) -> FormSchema`

Exporteert veldmetadata (types, verplicht-vlaggen, herhalingsregels, scripts) als `FormSchema`.

### Types

#### `FieldValue`

```rust
pub enum FieldValue {
    Number(f64),      // Numerieke waarden
    Boolean(bool),    // true/false, 1/0
    Text(String),     // Tekst
    Null,             // Leeg/ontbrekend
    Array(Vec<IndexMap<String, FieldValue>>), // Herhalende secties
}
```

Waarden worden automatisch gecoerceerd vanuit XFA-stringrepresentaties:
- `"123.45"` → `Number(123.45)`
- `"true"`, `"1"` → `Boolean(true)`
- `""` → `Null`

### PDF Lezen en Renderen

```rust
// Vanuit bestand
let reader = PdfReader::from_file(Path::new("formulier.pdf"))?;

// Vanuit bytes
let reader = PdfReader::from_bytes(&pdf_bytes)?;

// XFA-pakketten extraheren
let packets: XfaPackets = reader.extract_xfa()?;

// Renderen naar afbeeldingen
let config = RenderConfig::with_dpi(144.0);
let images = render_form_tree(&mut tree, root, &config)?;
save_pages_as_png(&images, Path::new("output/"), "formulier")?;
```

## REST API Endpoints

### `POST /extract`

Extraheert veldwaarden uit een XFA PDF.

**Request:** `multipart/form-data` met `file` veld
**Response:** `application/json`

### `POST /schema`

Exporteert het formulierschema (veldtypes, beperkingen).

**Request:** `multipart/form-data` met `file` veld
**Response:** `application/json`

### `POST /validate`

Valideert formulierveldwaarden tegen hun schema's en scripts.

**Request:** `multipart/form-data` met `file` veld
**Response:** `application/json`

### `POST /fill`

Vult formuliervelden in met aangeleverde data en retourneert de aangepaste PDF.

**Request:** `multipart/form-data` met `file` en `data` velden
**Response:** `application/pdf`

### `POST /flatten`

Vlakt XFA-formulier af naar statische AcroForm PDF.

**Request:** `multipart/form-data` met `file` veld
**Response:** `application/pdf`

## Foutmeldingen

Alle endpoints retourneren fouten in dit formaat:

```json
{
  "error": "Beschrijving van wat er fout ging"
}
```

HTTP-statuscodes:
- `200` — Succes
- `400` — Ongeldige invoer (ontbrekend bestand, foutieve JSON)
- `422` — XFA-parsing- of validatiefout
- `500` — Interne serverfout
