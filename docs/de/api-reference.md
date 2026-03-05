# API-Referenz

## Rust API

### xfa-json

Die primaere API fuer XFA-Formulardaten-Konvertierung.

#### `form_tree_to_json(tree: &FormTree, root: FormNodeId) -> FormData`

Extrahiert Feldwerte aus einem `FormTree` in eine JSON-freundliche `FormData`-Struktur.
Felder werden durch SOM-Pfade indiziert (z.B. `form1.Kunde.Name`).

Wiederholende Unterformulare werden zu `FieldValue::Array`-Eintraegen.

#### `json_to_form_tree(data: &FormData, tree: &mut FormTree, root: FormNodeId)`

Fuehrt JSON-Feldwerte mit einem bestehenden `FormTree` zusammen.
Gleicht Felder nach SOM-Pfad ab und verarbeitet wiederholende Abschnitte nach Array-Index.

#### `export_schema(tree: &FormTree, root: FormNodeId) -> FormSchema`

Exportiert Feldmetadaten (Typen, Pflichtflags, Wiederholungsregeln, Skripte) als `FormSchema`.

### Typen

#### `FieldValue`

```rust
pub enum FieldValue {
    Number(f64),      // Numerische Werte
    Boolean(bool),    // true/false, 1/0
    Text(String),     // Textzeichenketten
    Null,             // Leer/fehlend
    Array(Vec<IndexMap<String, FieldValue>>), // Wiederholende Abschnitte
}
```

Werte werden automatisch aus XFA-String-Darstellungen konvertiert:
- `"123.45"` → `Number(123.45)`
- `"true"`, `"1"` → `Boolean(true)`
- `""` → `Null`

### PDF Lesen und Rendern

```rust
// Aus Datei
let reader = PdfReader::from_file(Path::new("formular.pdf"))?;

// Aus Bytes
let reader = PdfReader::from_bytes(&pdf_bytes)?;

// XFA-Pakete extrahieren
let packets: XfaPackets = reader.extract_xfa()?;

// Zu Bildern rendern
let config = RenderConfig::with_dpi(144.0);
let images = render_form_tree(&mut tree, root, &config)?;
save_pages_as_png(&images, Path::new("ausgabe/"), "formular")?;
```

## REST API Endpunkte

### `POST /extract`

Extrahiert Feldwerte aus einem XFA PDF.

**Anfrage:** `multipart/form-data` mit `file`-Feld
**Antwort:** `application/json`

### `POST /schema`

Exportiert das Formularschema (Feldtypen, Einschraenkungen).

**Anfrage:** `multipart/form-data` mit `file`-Feld
**Antwort:** `application/json`

### `POST /validate`

Validiert Formularfeldwerte gegen ihre Schemata und Skripte.

**Anfrage:** `multipart/form-data` mit `file`-Feld
**Antwort:** `application/json`

### `POST /fill`

Fuellt Formularfelder mit bereitgestellten Daten und gibt das geaenderte PDF zurueck.

**Anfrage:** `multipart/form-data` mit `file`- und `data`-Feldern
**Antwort:** `application/pdf`

### `POST /flatten`

Flacht XFA-Formular zu statischem AcroForm PDF ab.

**Anfrage:** `multipart/form-data` mit `file`-Feld
**Antwort:** `application/pdf`

## Fehlermeldungen

Alle Endpunkte geben Fehler in diesem Format zurueck:

```json
{
  "error": "Beschreibung des aufgetretenen Fehlers"
}
```

HTTP-Statuscodes:
- `200` — Erfolg
- `400` — Ungueltige Eingabe (fehlende Datei, fehlerhaftes JSON)
- `422` — XFA-Parsing- oder Validierungsfehler
- `500` — Interner Serverfehler
