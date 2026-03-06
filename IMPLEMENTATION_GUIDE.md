# PDF Engine — Rust Ecosystem & Implementation Guide

**Companion bij:** `eager-orbiting-peacock.md` (strategieplan)
**Datum:** 2026-03-06
**Scope:** Alle 15 fasen (A-O), 100+ crates geanalyseerd, concrete implementatieaanpak per fase

---

## Inhoudsopgave

1. [Ecosystem Overzicht](#1-ecosystem-overzicht)
2. [Fase A: Workspace + hayro Fork](#2-fase-a)
3. [Fase B: AcroForm Engine](#3-fase-b)
4. [Fase C: Annotatie Engine](#4-fase-c)
5. [Fase D: XFA-hayro Integratie](#5-fase-d)
6. [Fase E: Test Infrastructure](#6-fase-e)
7. [Fase M: PDF Manipulation + Security](#7-fase-m)
8. [Fase F: Rendering Pipeline](#8-fase-f)
9. [Fase G: Digitale Handtekeningen](#9-fase-g)
10. [Fase H: PDF/A + Accessibility](#10-fase-h)
11. [Fase N: Content Intelligence](#11-fase-n)
12. [Fase O: Data Exchange + EU Compliance](#12-fase-o)
13. [Fase I: C-compatible API](#13-fase-i)
14. [Fase J: Python Bindings](#14-fase-j)
15. [Fase K: WASM + CLI](#15-fase-k)
16. [Fase L: Node.js Bindings](#16-fase-l)
17. [Dependency Matrix](#17-dependency-matrix)
18. [Licentie Compliance](#18-licentie-compliance)
19. [Build-vs-Buy Samenvatting](#19-build-vs-buy)

---

## 1. Ecosystem Overzicht

### Sleutel-ecosystemen in Rust PDF

| Ecosystem | Maintainer | Crates | Belang voor ons |
|-----------|-----------|--------|-----------------|
| **hayro** | LaurenzV | 14+ subcrates | Fundament: PDF parser, renderer, image decoders |
| **krilla** | LaurenzV | 1 (hoog-niveau) | Referentie: PDF/A-1 t/m PDF/A-4, PDF/UA-1 |
| **typst** | typst.app | pdf-writer, xmp_writer, subsetter | PDF output, XMP metadata, font subsetting |
| **linebender** | Google-gesponsord | kurbo, peniko, vello, vello_cpu | 2D rendering, geometrie, kleuren |
| **RustCrypto** | Community | 50+ crates | Encryptie, hashing, signatures |
| **Google Fontations** | Google | skrifa, read-fonts, write-fonts | Next-gen font parsing |
| **PyO3** | Community | pyo3, maturin | Python bindings |
| **napi-rs** | napi-rs team | napi, napi-derive | Node.js bindings |

### Wat NIET bestaat in Rust (maart 2026)

| Feature | Status | Onze actie |
|---------|--------|------------|
| AcroForm engine | Geen bruikbare crate | Custom gebouwd (2.1K LOC, arena-based FieldTree) |
| Annotatie engine | Geen bruikbare crate | Trait stubs gebouwd (68 LOC), uitbreiding gepland |
| XFA engine | **Wij zijn de enige** | Al gebouwd (30K LOC) |
| FormCalc interpreter | **Wij zijn de enige** | Al gebouwd |
| PDF linearisatie | Geen crate | Custom bouwen (~2K LOC) |
| PDF watermarking | Geen crate | Custom bouwen (~800 LOC) |
| PDF redactie | Geen crate | Custom bouwen (~2.5K LOC) |
| FDF/XFDF parser | Geen crate | Custom bouwen (~800 LOC) |
| ZUGFeRD generator | Alleen early-stage | Custom bouwen (~2K LOC) |

---

## 2. Fase A: Workspace + hayro Fork {#2-fase-a}

### Strategie
Fork de relevante hayro subcrates naar ons eigen workspace. Integreer met bestaande XFA crates.

### Crate Stack

| Crate | Versie | Downloads | Licentie | Rol |
|-------|--------|-----------|----------|-----|
| `hayro-syntax` | latest | — | MIT+Apache-2.0 | PDF parser, object model, XRef, streams |
| `hayro-interpret` | latest | — | MIT+Apache-2.0 | Content stream interpreter, Device trait |
| `hayro-font` | latest | — | MIT+Apache-2.0 | Type1 + CFF font parsing |
| `hayro-cmap` | latest | — | MIT+Apache-2.0 | CMap parser |
| `hayro-postscript` | latest | — | MIT+Apache-2.0 | PostScript scanner |
| `hayro-jbig2` | latest | — | MIT+Apache-2.0 | Pure Rust JBIG2 decoder |
| `hayro-jpeg2000` | latest | — | MIT+Apache-2.0 | Pure Rust JPEG2000 decoder |
| `hayro-ccitt` | latest | — | MIT+Apache-2.0 | CCITT Group 3/4 decoder |
| `hayro` (renderer) | latest | — | MIT+Apache-2.0 | Bitmap rendering via vello_cpu (NIET tiny-skia) |

### Implementatieaanpak

```
workspace/
├── Cargo.toml                    # Workspace root
├── THIRD_PARTY_LICENSES.txt      # MIT+Apache-2.0 notice voor hayro
├── crates/
│   ├── hayro-syntax/             # Geforkt, uitgebreid met AcroForm/Annot dict access
│   ├── hayro-interpret/          # Geforkt, uitgebreid met rendering hooks
│   ├── hayro-font/               # Geforkt, minimale wijzigingen
│   ├── hayro-cmap/               # Geforkt, ongewijzigd
│   ├── hayro-postscript/         # Geforkt, ongewijzigd
│   ├── hayro-jbig2/              # Geforkt, ongewijzigd
│   ├── hayro-jpeg2000/           # Geforkt, ongewijzigd
│   ├── hayro-ccitt/              # Geforkt, ongewijzigd
│   ├── xfa-dom-resolver/         # Bestaand (Epic 1)
│   ├── formcalc-interpreter/     # Bestaand (Epic 2)
│   ├── xfa-layout-engine/        # Bestaand (Epic 3)
│   ├── pdf-forms/                # NIEUW (Fase B)
│   ├── pdf-annot/                # NIEUW (Fase C)
│   ├── pdf-xfa/                  # NIEUW (Fase D) — bridge
│   ├── pdf-render/               # NIEUW (Fase F)
│   ├── pdf-manip/                # NIEUW (Fase M)
│   ├── pdf-sign/                 # NIEUW (Fase G)
│   ├── pdf-compliance/           # NIEUW (Fase H)
│   ├── pdf-ocr/                  # NIEUW (Fase N)
│   ├── pdf-redact/               # NIEUW (Fase N)
│   ├── pdf-extract/              # NIEUW (Fase N)
│   ├── pdf-invoice/              # NIEUW (Fase O)
│   ├── pdf-capi/                 # NIEUW (Fase I)
│   ├── pdf-python/               # NIEUW (Fase J)
│   ├── pdf-wasm/                 # NIEUW (Fase K)
│   ├── pdf-cli/                  # NIEUW (Fase K)
│   └── pdf-node/                 # NIEUW (Fase L)
```

### Kerninterfaces (traits)

> **Status update (Fase A-E):** De oorspronkelijke trait-plannen zijn vervangen door een
> pragmatische aanpak. Unified traits (`FormAccess`, `DocumentOps`) worden in
> `pdf-forms/src/facade.rs` gedefinieerd. De onderstaande traits zijn indicatief
> voor de _doelarchitectuur_, niet wat er nu gebouwd is.

```rust
/// Unified form access — werkelijk gebouwd in pdf-forms/facade.rs
pub trait FormAccess {
    fn form_type(&self) -> FormKind;
    fn field_names(&self) -> Vec<String>;
    fn get_value(&self, path: &str) -> Option<String>;
    fn set_value(&mut self, path: &str, value: &str) -> Result<(), FormError>;
}

/// Unified document operations — werkelijk gebouwd in pdf-forms/facade.rs
pub trait DocumentOps {
    fn page_count(&self) -> usize;
    fn form(&self) -> Option<&dyn FormAccess>;
    fn form_mut(&mut self) -> Option<&mut dyn FormAccess>;
}

/// Rendering target — via hayro's Device<'a> trait (pdf-interpret)
/// RenderTarget en AnnotationHandler/SignatureHandler worden in latere fasen gebouwd.
```

### PDF I/O strategie (werkelijk gebouwd)

> **Dual I/O:** pdf-syntax (hayro fork, read-only) + lopdf (4.6M downloads, read-write mutatie).
> pdf-syntax ondersteunt geen PDF schrijven. Eigen writer bouwen = ~10K LOC effort.
> lopdf is battle-tested voor mutatie (AcroForm flattening, annotaties, encryptie).
>
> **Trade-offs:** Dubbele parsing, vertaling tussen object models, meer geheugengebruik.
> **Evaluatiemoment:** Fase M — als merge/split/encrypt snel genoeg werkt met lopdf, geen actie nodig.

### Wijzigingen aan hayro-syntax
- `AcroForm` dictionary uitlezen uit catalog
- `Annots` array per pagina
- Widget annotation dictionaries parsen
- Incremental update support voor signed PDFs

### Wijzigingen aan hayro-interpret
- Form field rendering hooks in `Device` trait
- Annotation appearance stream rendering
- Overridable rendering callbacks

**Geschatte omvang:** ~3K LOC + configuratie

---

## 3. Fase B: AcroForm Engine {#3-fase-b}

### Bestaand Rust Ecosystem

| Crate | Downloads | LOC | Licentie | Bruikbaarheid |
|-------|-----------|-----|----------|---------------|
| `acroform` | 415 | ~400 | MIT | **Referentie** — te klein, alleen extractie |
| `pdf_forms` | ~500 | ~300 | MIT | **Referentie** — read-only field listing |
| `pdf_form_ids` | ~100 | ~200 | MIT | **Referentie** — ID extractie |
| `lopdf` | 4.6M | ~15K | MIT | **Dependency** — PDF structuur I/O |

**Conclusie:** Geen bruikbare AcroForm engine in Rust. Alle gevonden crates zijn read-only en < 500 LOC. Volledig custom gebouwd.

### Werkelijke implementatie (Fase B — VOLTOOID)

> **Update:** De oorspronkelijke 4-laags / 12K LOC schatting bleek te hoog.
> De werkelijke implementatie is een arena-based FieldTree van **2.1K LOC** met **34 tests**.

**Architectuur: Arena-based FieldTree**

```
┌─────────────────────────────────────┐
│  Unified API (FormAccess trait)      │  ← facade.rs
├─────────────────────────────────────┤
│  Field Logic (text/button/choice)    │  ← text.rs, button.rs, choice.rs
├─────────────────────────────────────┤
│  AcroForm Model (arena FieldTree)    │  ← tree.rs, flags.rs
├─────────────────────────────────────┤
│  PDF I/O: pdf-syntax + lopdf         │  ← parse.rs (read), flatten.rs (write)
└─────────────────────────────────────┘
```

**10 modules:** actions, appearance, button, choice, facade, flags, flatten, parse, text, tree
**Types:** FieldType (Text, Button, Choice, Signature), FieldValue (Text, StringArray), FieldId(usize)
**Pattern:** `Vec<FieldNode>` + `FieldId(usize)` — arena-based, cache-friendly

### Oorspronkelijk plan (ter referentie)

**Stap 1: AcroForm Dictionary Parser (~2K LOC)**
- Lees `/AcroForm` dict uit PDF catalog via hayro-syntax
- Parse field tree: `/Fields`, `/Kids`, `/Parent` hiërarchie
- Flatten partial vs full field names (bijv. `form.name.first`)
- Ondersteun alle field types: Text, Button, Choice, Signature

```rust
pub enum FieldType {
    Text(TextField),         // Tx
    Button(ButtonField),     // Btn (push, radio, check)
    Choice(ChoiceField),     // Ch (combo, list)
    Signature(SignatureField), // Sig
}

pub struct AcroFormField {
    pub name: String,           // Fully qualified
    pub field_type: FieldType,
    pub value: Option<FieldValue>,
    pub default_value: Option<FieldValue>,
    pub flags: FieldFlags,      // ReadOnly, Required, NoExport, etc.
    pub appearance: Option<AppearanceDict>,
    pub actions: Option<FieldActions>,  // JavaScript triggers
    pub rect: Rect,
    pub page_index: usize,
}
```

**Stap 2: Field Value Handling (~2K LOC)**
- Type coercion: string ↔ number ↔ date
- Multi-line text, max length, comb fields
- Radio button groups (mutual exclusion)
- Checkbox on/off values (niet altijd "Yes"/"Off")
- Choice fields: combo, list, multi-select, editable

**Stap 3: Appearance Generation (~3K LOC)**
- Genereer `/AP` (appearance) streams bij value changes
- Font handling: lees `/DR` (default resources), embed fonts
- Text layout: alignment (left/center/right), font size auto-fit
- Variable text positioning (`/Q` alignment, `/DS` default style)
- Checkbox/radio: genereer ZapfDingbats glyphs of custom appearances

```rust
/// Genereer appearance stream voor een tekstveld
fn generate_text_appearance(
    field: &AcroFormField,
    value: &str,
    fonts: &FontResources,
) -> Result<ContentStream> {
    let rect = field.rect;
    let font = resolve_default_font(field, fonts)?;
    let font_size = auto_fit_font_size(value, &rect, &font);

    let mut stream = ContentStream::new();
    stream.begin_text();
    stream.set_font(&font.name, font_size);
    stream.set_text_matrix(/*...*/);
    stream.show_text(value);
    stream.end_text();
    Ok(stream)
}
```

**Stap 4: Flattening (~2K LOC)**
- Merge appearance streams in page content stream
- Verwijder `/AcroForm` dictionary
- Verwijder widget annotations
- Preserveer visuele output pixel-perfect

**Stap 5: Calculations & Validation (~2K LOC)**
- `/CO` (calculation order) array
- JavaScript-expressies in `/AA` (additional actions)
- Validatie: required, format, custom scripts
- Cross-field references

**Stap 6: Import/Export (~1K LOC)**
- FDF import/export (basis — volledige FDF in Fase O)
- XFDF import/export (basis — volledige XFDF in Fase O)
- XML form data

### Referentie-implementaties om te bestuderen
1. **krilla** (LaurenzV) — Hoe PDF structuren in Rust te modelleren
2. **lopdf** source — Dictionary/stream manipulatie patronen
3. **Apache PDFBox** (Java) — AcroForm model is goed gedocumenteerd
4. **pdf.js** (Mozilla) — JavaScript AcroForm renderer als visuele referentie

**Oorspronkelijke schatting:** ~12K LOC
**Werkelijke omvang:** ~2.1K LOC (arena-based FieldTree bleek voldoende)
**Besparing:** Eenvoudiger ontwerp maakte 10K LOC overbodig

---

## 4. Fase C: Annotatie Engine {#4-fase-c}

### Bestaand Rust Ecosystem

| Crate | Bruikbaarheid |
|-------|---------------|
| `lopdf` | **Dependency** — kan annotatie dictionaries lezen/schrijven |
| `printpdf` | **Referentie** — heeft beperkte annotatie support (links, bookmarks) |
| `pdf-writer` | **Referentie** — annotation writers voor basis types |

**Conclusie:** Geen annotatie engine in Rust. `printpdf` heeft rudimentaire support maar geen appearance generation of interactie.

> **Status update (Fase C):** Huidige implementatie bevat trait stubs (68 LOC):
> `AnnotationType` enum, `Annotation` struct, `AnnotationRenderer` + `Device` traits.
> Volledige implementatie gepland voor latere fase.

### Implementatieaanpak

**PDF Annotatie Types (ISO 32000-2, §12.5)**

```rust
pub enum AnnotationType {
    // Markup annotaties
    Text,           // Sticky note
    FreeText,       // Direct text on page
    Line,
    Square,
    Circle,
    Polygon,
    Polyline,
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
    Stamp,
    Caret,
    Ink,            // Freehand drawing

    // Non-markup
    Link,
    Popup,
    FileAttachment,
    Sound,
    Movie,
    Widget,         // Form fields (→ Fase B)
    Screen,
    PrinterMark,
    TrapNet,
    Watermark,
    Redact,         // → Fase N
}
```

**Stap 1: Annotatie Model (~2K LOC)**
- Parse `/Annots` array per pagina via hayro-syntax
- Annotatie dictionary → typed Rust struct
- Appearance dict (`/AP`) met normal/rollover/down states
- Border, color, opacity properties

**Stap 2: Appearance Stream Generation (~3K LOC)**
- Genereer default appearances voor alle annotatie types
- Markup: highlight (blend mode Multiply), underline, strikeout
- Shapes: line, rect, circle, polygon met borders en fill
- Stamps: predefined stamps (Approved, Draft, etc.) + custom
- FreeText: tekst met font, size, color, alignment
- Ink: Bézier curves van ink lists

**Stap 3: Annotatie Flattening (~1.5K LOC)**
- Merge appearance in page content stream
- Preserveer visuele output
- Verwijder annotatie dictionaries

**Stap 4: Interactie (~1.5K LOC)**
- Link annotaties: URI, GoTo, GoToR, Named
- Popup associaties
- Reply chains (IRT — In Reply To)
- Annotation flags (Hidden, Print, NoZoom, etc.)

### Pattern: Annotation als Content Stream

```rust
pub struct Annotation {
    pub annot_type: AnnotationType,
    pub rect: Rect,
    pub contents: Option<String>,
    pub color: Option<Color>,
    pub border: Option<Border>,
    pub opacity: f32,        // /CA
    pub flags: AnnotationFlags,
    pub appearance: Option<AppearanceStreams>,
    pub popup: Option<PopupAnnotation>,
}

impl Annotation {
    /// Genereer default appearance als die ontbreekt
    pub fn ensure_appearance(&mut self, resources: &PageResources) -> Result<()> {
        if self.appearance.is_none() {
            self.appearance = Some(self.generate_default_appearance(resources)?);
        }
        Ok(())
    }

    /// Flatten naar page content stream
    pub fn to_content_operations(&self) -> Vec<ContentOp> {
        let ap = self.appearance.as_ref().unwrap();
        // Transform annotation appearance naar page coordinates
        let matrix = self.calculate_appearance_matrix();
        vec![
            ContentOp::SaveState,
            ContentOp::ConcatMatrix(matrix),
            ContentOp::PaintXObject(ap.normal_stream()),
            ContentOp::RestoreState,
        ]
    }
}
```

**Oorspronkelijke schatting:** ~8K LOC
**Huidige status:** 68 LOC (trait stubs)

---

## 5. Fase D: XFA-hayro Integratie {#5-fase-d}

### Strategie
Brug bouwen tussen onze bestaande XFA engine (30K LOC, Epics 1-6) en het hayro rendering systeem.

### Kernprobleem
hayro heeft een abstract `Device` trait waarnaar content streams gerenderd worden. We moeten onze XFA layout output door dit systeem heen sturen.

> **Status update (Fase D — VOLTOOID):**
> - **v1 (gebouwd):** Raw PDF content stream operators in `render_bridge.rs` (~320 LOC).
>   Werkt maar mist transparency, gradients, blend modes.
> - **v2 (gebouwd):** `XfaPaintCommand` enum in `paint_bridge.rs` — renderer-agnostische
>   paint commands (FillRect, StrokeRect, DrawText, DrawMultilineText).
>   Kan door elke backend geconsumeerd worden: Device trait, content stream, SVG.
> - **Reden voor v1→v2:** `Color::new()` was `pub(crate)`, Device trait was onbruikbaar
>   vanuit onze XFA crate. Na #209 (publieke Color constructors) is paint bridge mogelijk.
> - **Font stack:** skrifa + ttf-parser (NIET rustybuzz + fontdb)
> - **Color management:** moxcms (ICC profiel conversie)

### Implementatieaanpak

**Stap 1: XFA Content Stream Generator (~2.5K LOC)**
```rust
/// Converteer XFA LayoutDOM naar PDF content stream operaties
pub struct XfaContentGenerator {
    layout_dom: LayoutDom,
    font_mapper: FontMapper,
}

impl XfaContentGenerator {
    /// Genereer content stream voor één pagina
    pub fn generate_page(&self, page_idx: usize) -> Result<ContentStream> {
        let page_nodes = self.layout_dom.nodes_for_page(page_idx);
        let mut stream = ContentStream::new();

        for node in page_nodes {
            match &node.content {
                LayoutContent::Text(text) => {
                    self.render_text(&mut stream, node, text)?;
                }
                LayoutContent::Rectangle(rect) => {
                    self.render_rect(&mut stream, node, rect)?;
                }
                LayoutContent::Line(line) => {
                    self.render_line(&mut stream, node, line)?;
                }
                LayoutContent::Image(img) => {
                    self.render_image(&mut stream, node, img)?;
                }
                LayoutContent::Field(field) => {
                    self.render_field(&mut stream, node, field)?;
                }
            }
        }
        Ok(stream)
    }
}
```

**Stap 2: Font Mapping (~1.5K LOC)**
- Map XFA font references naar embedded PDF fonts
- Gebruik hayro-font voor Type1/CFF parsing
- skrifa voor OpenType/TrueType
- Fallback chain voor ontbrekende fonts

**Stap 3: hayro Device Implementation (~2K LOC)**
- Implementeer hayro's `Device` trait voor XFA output
- XFA overlay rendering bovenop bestaande page content
- Z-order management (XFA content boven/onder AcroForm)

### Geen externe dependencies nodig
Dit is puur integratiecode tussen onze bestaande crates en hayro.

**Oorspronkelijke schatting:** ~6K LOC
**Werkelijke omvang:** ~620 LOC (render_bridge.rs ~320 LOC + paint_bridge.rs ~300 LOC + bridges)

---

## 6. Fase E: Test Infrastructure {#6-fase-e}

### Crate Stack

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| `image` | 48M | MIT/Apache-2.0 | PNG/JPEG laden voor vergelijking |
| `dssim` | — | AGPL-3.0 | **Alleen dev-dependency** — perceptuele beeldvergelijking |
| `pixelmatch` | — | ISC | Alternatief voor dssim (permissiever) |
| `insta` | 13M | Apache-2.0 | Snapshot testing |
| `criterion` | 8M | Apache-2.0/MIT | Benchmarking |
| `proptest` | 5M | MIT/Apache-2.0 | Property-based testing |

### Implementatieaanpak

**Golden Render Pipeline (al bewezen in xfa-golden-tests)**
```
PDF → Engine → PNG → pixel diff → rapport
                ↕
        Adobe Reader PNG (golden reference)
```

**Uitbreiden met:**
1. **AcroForm test suite:** 50+ real-world PDFs met ingevulde velden
2. **Annotatie test suite:** Alle 25 annotatie types
3. **Round-trip tests:** Open → edit → save → reopen → verify
4. **Conformance tests:** PDF/A validators (veraPDF), signature validators
5. **Fuzzing:** `cargo-fuzz` met AFL/libfuzzer voor parser robustness

**Geschatte omvang:** ~4K LOC + test fixtures

---

## 7. Fase M: PDF Manipulation + Security {#7-fase-m}

### Crate Stack

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| **`lopdf`** | 4.6M | MIT | PDF structuur lezen/schrijven, pagina manipulatie |
| **`aes`** | 26M | MIT/Apache-2.0 | AES-128/256 encryptie |
| **`cbc`** | 8M | MIT/Apache-2.0 | CBC block cipher mode |
| **`rc4`** | 0.4M | MIT/Apache-2.0 | RC4 stream cipher (legacy PDFs) |
| **`md5`** | 15M | MIT/Apache-2.0 | MD5 hash (PDF encryptie) |
| **`sha2`** | 28M | MIT/Apache-2.0 | SHA-256/384/512 |
| **`flate2`** | 85M | MIT/Apache-2.0 | Deflate compressie |
| **`weezl`** | 3M | MIT/Apache-2.0 | LZW compressie |
| **`image`** | 48M | MIT/Apache-2.0 | Image downsampling voor optimalisatie |

### Subfeatures en implementatieaanpak

#### M.1: Pagina Manipulatie (~2K LOC)

```rust
pub struct PdfManipulator {
    doc: Document,  // lopdf Document
}

impl PdfManipulator {
    /// Merge meerdere PDFs
    pub fn merge(docs: &[Document]) -> Result<Document> { ... }

    /// Split PDF op pagina ranges
    pub fn split(&self, ranges: &[PageRange]) -> Result<Vec<Document>> { ... }

    /// Insert pagina's uit andere PDF
    pub fn insert_pages(&mut self, source: &Document, pages: &[usize], at: usize) -> Result<()> { ... }

    /// Verwijder pagina's
    pub fn delete_pages(&mut self, pages: &[usize]) -> Result<()> { ... }

    /// Roteer pagina's (90, 180, 270 graden)
    pub fn rotate_pages(&mut self, pages: &[usize], degrees: i32) -> Result<()> { ... }

    /// Herschik pagina's
    pub fn rearrange(&mut self, new_order: &[usize]) -> Result<()> { ... }
}
```

**lopdf biedt al:** `Document::merge`, page tree manipulatie
**We bouwen zelf:** Smart merge (font dedup, resource sharing), batch operations

#### M.2: Encryption/Decryption (~2.5K LOC)

PDF encryption implementatie (ISO 32000-2, §7.6):

```rust
pub struct PdfEncryption;

impl PdfEncryption {
    /// Encrypt PDF met password
    pub fn encrypt(
        doc: &mut Document,
        user_password: &str,
        owner_password: &str,
        permissions: Permissions,
        algorithm: EncryptionAlgorithm,
    ) -> Result<()> { ... }

    /// Decrypt PDF
    pub fn decrypt(doc: &mut Document, password: &str) -> Result<()> { ... }
}

pub enum EncryptionAlgorithm {
    Rc4_40,      // V1, legacy
    Rc4_128,     // V2
    Aes128,      // V4
    Aes256,      // V5 (R=6), modern standaard
}

pub struct Permissions {
    pub print: bool,
    pub modify: bool,
    pub copy: bool,
    pub annotate: bool,
    pub fill_forms: bool,
    pub extract: bool,
    pub assemble: bool,
    pub print_high_quality: bool,
}
```

**Implementatie-detail:** V5/R6 gebruikt:
1. UTF-8 SASLprep password processing
2. SHA-256 + AES-256-CBC key derivation
3. Per-object AES-256-CBC encryptie
4. RustCrypto crates (`aes`, `cbc`, `sha2`) bieden alle primitieven

**Geen** bestaande Rust crate combineert dit tot PDF-specifieke encryption.
We bouwen de ~600 LOC "glue" die PDF key derivation + permission encoding doet.

#### M.3: Watermarking (~800 LOC)

```rust
pub struct Watermark {
    pub content: WatermarkContent,
    pub opacity: f32,          // 0.0 - 1.0
    pub rotation: f32,         // degrees
    pub position: WatermarkPosition,
    pub tiling: bool,          // repeat pattern
    pub pages: PageSelection,  // all, odd, even, specific
}

pub enum WatermarkContent {
    Text { text: String, font_size: f32, color: Color },
    Image { data: Vec<u8>, width: f32, height: f32 },
}
```

**Aanpak:** Genereer watermark als content stream, inject als onderlaag (`q ... Q` grafische state) of bovenlaag in elke pagina's content stream.

#### M.4: Compressie & Optimalisatie (~2K LOC)

- **Stream recompressie:** Hercomprimeer met optimale Deflate level
- **Image downsampling:** Verklein afbeeldingen > threshold DPI
- **Font subsetting:** Verwijder ongebruikte glyphs (gebruik `subsetter` crate)
- **Object dedup:** Detecteer identieke objecten en hergebruik references
- **Linearisatie (~2K LOC extra):** Reorganiseer PDF voor "fast web view"
  - Hint tables genereren
  - Object ordering voor progressive rendering
  - **Geen bestaande Rust crate** — geheel custom

#### M.5: Bookmarks/Outlines (~800 LOC)

```rust
pub struct Bookmark {
    pub title: String,
    pub destination: Destination,
    pub children: Vec<Bookmark>,
    pub style: BookmarkStyle,  // bold, italic, color
}

pub enum Destination {
    Page(usize),
    PageFit(usize),
    PageXYZ { page: usize, x: f32, y: f32, zoom: f32 },
    Named(String),
}
```

**lopdf biedt:** Outline dict lezen/schrijven
**We bouwen:** High-level API, geneste structuur, synchronisatie met headings

**Geschatte totaal Fase M:** ~10K LOC

---

## 8. Fase F: Rendering Pipeline {#8-fase-f}

### Crate Stack

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| **`tiny-skia`** | 21.8M | BSD-3-Clause | 2D rasterizer (primair) |
| **`kurbo`** | 9M | MIT/Apache-2.0 | 2D geometrie (Bézier curves, affine transforms) |
| **`rustybuzz`** | 4M | MIT | Text shaping (HarfBuzz port in Rust) |
| **`skrifa`** | 2M | MIT/Apache-2.0 | Font parsing (Google Fontations) |
| **`fontdb`** | 5M | MIT | System font discovery |
| **`cosmic-text`** | 0.5M | MIT/Apache-2.0 | Text layout engine |
| **`moxcms`** | — | MIT | Pure Rust color management (ICC profielen) |
| **`image`** | 48M | MIT/Apache-2.0 | Image I/O |
| **`zune-jpeg`** | 4M | MIT/Apache-2.0/Zlib | Snelle JPEG decoder |
| **`fast_image_resize`** | 1M | MIT/Apache-2.0 | SIMD image resize |
| **`png`** | 20M | MIT/Apache-2.0 | PNG encoder/decoder |

### Rendering engine: vello_cpu (via hayro)

> **Update:** De oorspronkelijke keuze was tiny-skia, maar hayro brengt vello_cpu mee.
> We gebruiken vello_cpu als primaire rendering engine.

| Criterium | tiny-skia | vello_cpu (gekozen) |
|-----------|-----------|-----------|
| Maturity | 5+ jaar, 21.8M downloads | Actief maintained, Google-gesponsord |
| Rendering | Scanline rasterizer | Analytisch path rendering (hogere kwaliteit) |
| Integratie | Aparte dependency | Meegeleverd met hayro fork |
| API | Simpel, Skia-achtig | Via hayro Device trait |
| WASM support | Ja | Ja |
| **Verdict** | Niet gebruikt | **Primair** |

**Font stack (werkelijk):** skrifa + ttf-parser (NIET rustybuzz + fontdb).
**Color management:** moxcms (pure Rust ICC profiel conversie).

### Implementatieaanpak

**Architectuur:**

```
┌────────────────────────────────────────┐
│           Public Render API            │
│  render_page(page, dpi) -> RgbaImage   │
├────────────────────────────────────────┤
│        Content Stream Interpreter      │
│   (hayro-interpret, uitgebreid)        │
├──────────┬──────────┬──────────────────┤
│ Text     │ Graphics │ Image            │
│ Pipeline │ Pipeline │ Pipeline         │
├──────────┼──────────┼──────────────────┤
│rustybuzz │tiny-skia │zune-jpeg,png     │
│skrifa    │kurbo     │hayro-jbig2       │
│cosmic-txt│          │hayro-jpeg2000    │
├──────────┴──────────┴──────────────────┤
│        Color Management (moxcms)       │
└────────────────────────────────────────┘
```

**Stap 1: hayro Device → tiny-skia backend (~2.5K LOC)**
```rust
pub struct TinySkiaDevice {
    pixmap: tiny_skia::Pixmap,
    transform: tiny_skia::Transform,
    clip_stack: Vec<tiny_skia::ClipMask>,
    graphics_state: Vec<GraphicsState>,
}

impl hayro_interpret::Device for TinySkiaDevice {
    fn fill_path(&mut self, path: &Path, rule: FillRule, paint: &Paint) {
        let sk_path = convert_path(path);
        let sk_paint = convert_paint(paint, &self.graphics_state);
        self.pixmap.fill_path(&sk_path, &sk_paint, rule.into(), self.transform, None);
    }

    fn stroke_path(&mut self, path: &Path, stroke: &Stroke, paint: &Paint) {
        let sk_path = convert_path(path);
        let sk_paint = convert_paint(paint, &self.graphics_state);
        let sk_stroke = convert_stroke(stroke);
        self.pixmap.stroke_path(&sk_path, &sk_paint, &sk_stroke, self.transform, None);
    }

    fn draw_image(&mut self, image: &Image, rect: &Rect) {
        let pixmap = decode_image(image);
        // Scale + blend image into target
    }

    fn draw_text(&mut self, text: &TextRun) {
        // Shape with rustybuzz → glyph IDs + positions
        // Rasterize glyphs via skrifa → tiny-skia paths
    }
}
```

**Stap 2: Text Pipeline (~3K LOC)**
- **Shaping:** rustybuzz (HarfBuzz port) voor complex text layout
- **Font loading:** skrifa voor glyph outlines, fontdb voor system fonts
- **Layout:** cosmic-text voor multi-line text met bidi support
- **Rendering:** Glyph outlines → tiny-skia paths → rasterize

**Stap 3: Form Field Overlay (~2K LOC)**
- Render AcroForm widget appearances bovenop pagina content
- Render XFA layout output via Fase D bridge
- Z-order: background → page content → annotations → form fields

**Stap 4: Color Management (~1.5K LOC)**
- ICC profiel conversie via moxcms (pure Rust)
- DeviceRGB, DeviceCMYK, DeviceGray, ICCBased, CalRGB, Lab
- Rendering intent: Perceptual, RelativeColorimetric, etc.

**Stap 5: Output Formaten (~1K LOC)**
- PNG (via `png` crate)
- JPEG (via `zune-jpeg` encoder)
- Raw RGBA pixels (voor WASM/embedding)

**Geschatte omvang:** ~10K LOC

---

## 9. Fase G: Digitale Handtekeningen {#9-fase-g}

### Game-Changer: `underskrift`

| Eigenschap | Detail |
|-----------|--------|
| **Crate** | `underskrift` v0.1.1 |
| **Gepubliceerd** | 2026-03-06 (vandaag!) |
| **LOC** | ~21K |
| **Licentie** | BSD-2-Clause |
| **Features** | PAdES B-B t/m B-LTA, PKCS#7, visible/invisible signatures, LTV, TSA, OCSP, CRL, remote signing |

### Strategie: Evalueer underskrift vs. custom build

**Optie A: underskrift als dependency (AANBEVOLEN als kwaliteit voldoende)**
- Pro: Bespaart ~8K LOC eigen code, covers alle PAdES niveaus
- Pro: BSD-2-Clause is commercieel compatibel
- Risico: v0.1.1, onbekende stabiliteit, mogelijk breaking changes
- Actie: Evalueer met onze test PDFs, check API completeness

**Optie B: Custom build op RustCrypto stack**

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| `cms` | 0.5M | MIT/Apache-2.0 | CMS/PKCS#7 SignedData |
| `der` | 5M | MIT/Apache-2.0 | ASN.1 DER encoding |
| `x509-cert` | 2M | MIT/Apache-2.0 | X.509 certificaat parsing |
| `rsa` | 5M | MIT/Apache-2.0 | RSA signing/verification |
| `ecdsa` | 4M | MIT/Apache-2.0 | ECDSA signing |
| `sha2` | 28M | MIT/Apache-2.0 | SHA-256/384/512 |
| `p256` / `p384` | 3M | MIT/Apache-2.0 | Elliptic curves |
| `reqwest` | 45M | MIT/Apache-2.0 | HTTP client voor OCSP/TSA |

### Implementatieaanpak (Optie B als fallback)

**Stap 1: PKCS#7 Signature Generation (~2.5K LOC)**
```rust
pub struct PdfSigner {
    cert: Certificate,
    key: SigningKey,
    hash_algo: HashAlgorithm,
}

impl PdfSigner {
    pub fn sign_document(&self, doc: &mut Document, field: &str) -> Result<()> {
        // 1. Bereid ByteRange voor (twee ranges rond signature placeholder)
        // 2. Hash document bytes (excl. signature placeholder)
        // 3. Genereer CMS SignedData met:
        //    - SignerInfo (cert, digest, signed attrs)
        //    - Encapsulated content (indirect ref)
        // 4. DER-encode signature
        // 5. Schrijf in placeholder
        Ok(())
    }
}
```

**Stap 2: Signature Verification (~2K LOC)**
- Parse CMS SignedData uit signature dictionary
- Verify digest over document byte ranges
- Certificate chain validation
- Revocation checking (OCSP, CRL)

**Stap 3: PAdES Conformance (~2K LOC)**
- B-B: Basic signature
- B-T: Timestamp token
- B-LT: Long-term validation data (OCSP, CRL embedded)
- B-LTA: Long-term archival (document timestamp)

**Stap 4: Visible Signatures (~1.5K LOC)**
- Render signature appearance (naam, datum, reden, locatie)
- Custom afbeelding in signature field
- Handtekening-stijl (handwritten look)

**Aanbeveling:** Start met evaluatie van underskrift (2-3 dagen). Als API en kwaliteit voldoen, gebruik als dependency. Anders fallback naar custom build op RustCrypto (~8K LOC, 4-6 weken).

**Geschatte omvang:** ~2K LOC (met underskrift) of ~8K LOC (custom)

---

## 10. Fase H: PDF/A + Accessibility {#10-fase-h}

### Crate Stack

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| **`pdf-writer`** | 1M | MIT/Apache-2.0 | PDF structuur output |
| **`xmp_writer`** | — | MIT/Apache-2.0 | XMP metadata (PDF/A vereist) |
| **`subsetter`** | 0.5M | MIT/Apache-2.0 | Font subsetting (PDF/A vereist) |

### Referentie: krilla (LaurenzV)

krilla is de meest complete PDF/A implementatie in Rust. Ondersteunt:
- PDF/A-1b, PDF/A-2b, PDF/A-2u, PDF/A-3b (en meer)
- PDF/UA-1
- Tagged PDF (structure tree)
- Font embedding/subsetting

**Strategie:** Bestudeer krilla's aanpak grondig, maar bouw onze eigen implementatie omdat:
1. krilla is een PDF *creator*, wij moeten bestaande PDFs *converteren*
2. krilla focust op nieuwe documenten, wij op bestaande + XFA
3. Onze architectuur is anders (hayro-gebaseerd)

### Implementatieaanpak

**Stap 1: PDF/A Metadata (~1.5K LOC)**
- XMP metadata packet generatie (via xmp_writer)
- PDF/A conformance level markering
- OutputIntent met ICC profiel embedding
- sRGB/FOGRA39/etc. als default profielen

```rust
pub enum PdfAConformance {
    A1b,  // PDF/A-1b (visual preservation)
    A2b,  // PDF/A-2b (JPEG2000, transparency)
    A2u,  // PDF/A-2u (Unicode text)
    A3b,  // PDF/A-3b (embedded files — voor ZUGFeRD!)
    A4,   // PDF/A-4 (modern, less restrictive)
}

pub struct PdfAConverter {
    conformance: PdfAConformance,
}

impl PdfAConverter {
    pub fn convert(&self, doc: &mut Document) -> Result<Vec<ConversionWarning>> {
        let mut warnings = Vec::new();
        self.embed_xmp_metadata(doc)?;
        self.embed_output_intent(doc)?;
        self.embed_all_fonts(doc, &mut warnings)?;
        self.convert_colorspaces(doc, &mut warnings)?;
        self.remove_prohibited_features(doc, &mut warnings)?;
        if matches!(self.conformance, PdfAConformance::A2u | PdfAConformance::A3b) {
            self.ensure_unicode_mapping(doc, &mut warnings)?;
        }
        Ok(warnings)
    }
}
```

**Stap 2: Font Embedding (~2K LOC)**
- Detecteer niet-embedded fonts
- Subset en embed via `subsetter` crate
- ToUnicode CMap generatie voor text extraction
- CIDFont structuren voor CJK

**Stap 3: Tagged PDF / Accessibility (~2K LOC)**
- Structure tree builder (/StructTreeRoot)
- Role mapping (H1-H6, P, Table, TR, TD, Figure, etc.)
- Alt text voor afbeeldingen
- Reading order
- PDF/UA-1 conformance
- **Bestudeer krilla's tagged PDF implementatie** als referentie

**Stap 4: Validatie (~1.5K LOC)**
- Interne validatie checks per conformance level
- Exporteer rapport met warnings/errors
- Integreer met veraPDF voor externe validatie (test pipeline)

**Geschatte omvang:** ~7K LOC

---

## 11. Fase N: Content Intelligence {#11-fase-n}

### Crate Stack

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| **`tesseract-sys`** | 0.2M | MIT | Tesseract OCR FFI bindings |
| **`tesseract-plumbing`** | — | MIT | Higher-level Tesseract wrapper |
| **`tantivy`** | 10.6M | MIT | Full-text search engine |
| **`fax`** | 0.1M | MIT | CCITT Group 3/4 decoder (alt) |
| **`image`** | 48M | MIT/Apache-2.0 | Image processing |

### N.1: OCR Integratie (~3K LOC)

**Architectuur:**
```
Scanned PDF page → Render to image → Tesseract OCR → Text + bounding boxes
                                                          ↓
                                            Invisible text layer in PDF
```

```rust
pub struct OcrEngine {
    tesseract: TesseractApi,
    languages: Vec<String>,
    dpi: u32,
}

impl OcrEngine {
    /// Maak een scanned PDF doorzoekbaar
    pub fn make_searchable(&self, doc: &mut Document) -> Result<OcrReport> {
        let mut report = OcrReport::new();
        for page_idx in 0..doc.page_count() {
            if self.page_needs_ocr(doc, page_idx)? {
                let image = render_page_to_image(doc, page_idx, self.dpi)?;
                let result = self.tesseract.recognize(&image)?;
                self.add_invisible_text_layer(doc, page_idx, &result)?;
                report.add_page(page_idx, result.confidence);
            }
        }
        Ok(report)
    }

    /// Detecteer of pagina text-based of image-based is
    fn page_needs_ocr(&self, doc: &Document, page: usize) -> Result<bool> {
        let text = extract_text(doc, page)?;
        Ok(text.trim().is_empty()) // Geen extractable text = OCR nodig
    }
}
```

**Let op:** `tesseract-sys` is een C-binding (Tesseract is C++). Dit is de enige niet-pure-Rust dependency. Maak het optioneel via feature flag:
```toml
[features]
default = []
ocr = ["tesseract-sys", "tesseract-plumbing"]
```

### N.2: Redactie (~2.5K LOC)

**GDPR/AVG compliance feature — permanente content verwijdering.**

```rust
pub struct Redactor {
    replacements: Vec<RedactionArea>,
}

pub struct RedactionArea {
    pub page: usize,
    pub rect: Rect,
    pub fill_color: Color,        // Typisch zwart
    pub overlay_text: Option<String>, // Bijv. "REDACTED"
}

impl Redactor {
    /// Markeer gebieden voor redactie (preview)
    pub fn mark(&mut self, area: RedactionArea) { ... }

    /// Pas redactie permanent toe — DESTRUCTIEF, niet omkeerbaar
    pub fn apply(&self, doc: &mut Document) -> Result<RedactionReport> {
        for area in &self.replacements {
            // 1. Verwijder tekst content in rect uit content stream
            self.remove_text_in_rect(doc, area)?;
            // 2. Verwijder afbeelding pixels in rect
            self.remove_images_in_rect(doc, area)?;
            // 3. Teken fill rectangle
            self.draw_redaction_rect(doc, area)?;
            // 4. Verwijder metadata die geredacteerde content kan bevatten
            self.clean_metadata(doc, area)?;
        }
        // 5. Verwijder incremental updates (kunnen origineel bevatten)
        doc.remove_incremental_updates()?;
        // 6. Verwijder XMP metadata, thumbnails, link annotations in rect
        self.clean_ancillary_data(doc)?;
        Ok(RedactionReport { areas_redacted: self.replacements.len() })
    }
}
```

**Kritiek:** Redactie moet 100% betrouwbaar zijn. Testen met:
- Text extraction na redactie (moet leeg zijn in geredacteerd gebied)
- Copy-paste uit PDF viewer (mag geen geredacteerde text bevatten)
- Metadata check (geen sporen in XMP, document info, etc.)

### N.3: Image Extraction (~1.5K LOC)

```rust
pub struct ImageExtractor;

impl ImageExtractor {
    pub fn extract_all(doc: &Document) -> Vec<ExtractedImage> {
        // Iterate over alle XObject streams van type Image
        // Decodeer per encoding:
        // - DCTDecode → JPEG (direct doorgeven)
        // - FlateDecode → PNG (via image crate)
        // - JBIG2Decode → via hayro-jbig2
        // - JPXDecode → via hayro-jpeg2000
        // - CCITTFaxDecode → via fax/hayro-ccitt
    }
}
```

### N.4: Full-text Search (~2K LOC)

```rust
pub struct PdfSearchEngine {
    index: tantivy::Index,
}

impl PdfSearchEngine {
    /// Indexeer PDF voor zoeken
    pub fn index_document(&mut self, doc: &Document) -> Result<()> {
        for page in 0..doc.page_count() {
            let text = extract_text_with_positions(doc, page)?;
            self.index.add_document(page, &text)?;
        }
        Ok(())
    }

    /// Zoek met regex/text, retourneer pagina + bounding boxes
    pub fn search(&self, query: &str) -> Vec<SearchResult> { ... }
}

pub struct SearchResult {
    pub page: usize,
    pub text: String,
    pub bounding_boxes: Vec<Rect>,  // Voor highlighting
    pub score: f32,
}
```

**Character-level search met bounding boxes (~800 LOC extra):**
- Parse content stream text operators (Tj, TJ, Tm, etc.)
- Track text matrix voor positie
- Map characters → rectangles op pagina

**Geschatte totaal Fase N:** ~12K LOC (waarvan ~3K met OCR feature flag)

---

## 12. Fase O: Data Exchange + EU Compliance {#12-fase-o}

### Crate Stack

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| **`quick-xml`** | 15M | MIT | XML generatie/parsing |
| **`roxmltree`** | 8M | MIT/Apache-2.0 | XML parsing (al in use) |
| **`zugferd-code-lists`** | — | MIT | ZUGFeRD code lijsten |
| **`chrono`** | 35M | MIT/Apache-2.0 | Datum/tijd formatting |

### O.1: FDF/XFDF Form Data Exchange (~800 LOC)

```rust
/// FDF (Forms Data Format) — binair
pub struct FdfDocument {
    pub fields: Vec<FdfField>,
    pub file_spec: Option<String>,  // Bron-PDF
}

/// XFDF (XML Forms Data Format) — XML-based
pub struct XfdfDocument {
    pub fields: Vec<XfdfField>,
    pub annotations: Vec<XfdfAnnotation>,
}

impl FdfDocument {
    pub fn import_into(&self, doc: &mut Document) -> Result<()> { ... }
    pub fn export_from(doc: &Document) -> Result<Self> { ... }
}

impl XfdfDocument {
    pub fn import_into(&self, doc: &mut Document) -> Result<()> { ... }
    pub fn export_from(doc: &Document) -> Result<Self> { ... }
    pub fn to_xml(&self) -> String { ... }
    pub fn from_xml(xml: &str) -> Result<Self> { ... }
}
```

### O.2: XML Form Data (~1.5K LOC)

- XFA datasets export/import (al grotendeels in Epic 5)
- AcroForm XML export/import
- XDP (XML Data Package) generatie

### O.3: ZUGFeRD / Factur-X E-invoicing (~2.5K LOC)

**Dit is een sterke EU differentiator — slechts 1/10 concurrenten biedt dit.**

ZUGFeRD 2.3 / Factur-X = Cross-Industry Invoice (CII) XML embedded in PDF/A-3.

```rust
pub enum ZugferdProfile {
    Minimum,     // Minste velden
    BasicWL,     // Basis zonder line items
    Basic,       // Basis met line items
    EN16931,     // EU standaard (meest gebruikt)
    Extended,    // Uitgebreid
}

pub struct ZugferdInvoice {
    pub profile: ZugferdProfile,
    pub invoice_number: String,
    pub issue_date: NaiveDate,
    pub seller: TradeParty,
    pub buyer: TradeParty,
    pub line_items: Vec<LineItem>,
    pub tax_total: Decimal,
    pub grand_total: Decimal,
    pub currency: String,       // ISO 4217
    pub payment_terms: Option<PaymentTerms>,
}

impl ZugferdInvoice {
    /// Genereer CII XML
    pub fn to_xml(&self) -> Result<String> {
        // Gebruik quick-xml voor XML generatie
        // Volg UN/CEFACT Cross-Industry Invoice schema
        // Valideer tegen profiel constraints
    }

    /// Embed in bestaande PDF als PDF/A-3 bijlage
    pub fn embed_in_pdf(&self, doc: &mut Document) -> Result<()> {
        // 1. Genereer CII XML
        let xml = self.to_xml()?;
        // 2. Converteer PDF naar PDF/A-3 (via Fase H)
        PdfAConverter::new(PdfAConformance::A3b).convert(doc)?;
        // 3. Embed XML als Associated File
        embed_xml_attachment(doc, "factur-x.xml", &xml, AfRelationship::Data)?;
        // 4. Voeg XMP metadata toe met ZUGFeRD conformance
        add_zugferd_xmp(doc, self.profile)?;
    }

    /// Extract ZUGFeRD data uit bestaande PDF
    pub fn extract_from_pdf(doc: &Document) -> Result<Option<Self>> {
        // Zoek embedded "factur-x.xml" of "zugferd-invoice.xml"
        // Parse CII XML
        // Map naar ZugferdInvoice struct
    }
}

pub struct TradeParty {
    pub name: String,
    pub address: Address,
    pub tax_id: Option<String>,     // BTW nummer
    pub registration: Option<String>, // KvK nummer
}

pub struct LineItem {
    pub description: String,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub tax_rate: Decimal,
    pub tax_category: TaxCategory,
}
```

**zugferd-code-lists crate** biedt: ISO valuta codes, land codes, tax category codes, unit codes. Scheelt ~500 LOC aan handmatige code lists.

**Afhankelijkheid:** Fase H (PDF/A-3) moet klaar zijn voor ZUGFeRD embedding.

**Geschatte totaal Fase O:** ~5K LOC (minder dan eerder geschat dankzij hergebruik)

---

## 13. Fase I: C-compatible API {#13-fase-i}

### Crate Stack

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| **`cbindgen`** | 2M | MPL-2.0 | C header generatie |
| **`cargo-c`** | — | MIT | Build tool voor C-compatible libraries |
| **`libc`** | 110M | MIT/Apache-2.0 | C types |

### Referentie-implementatie: resvg

resvg (SVG rendering library) heeft het beste C API patroon in het Rust ecosystem:
- `resvg_capi` crate met `#[no_mangle] extern "C"` functies
- Opaque pointers voor alle objecten
- Consistent error handling via return codes
- Header generatie via cbindgen

### Implementatieaanpak

```rust
// pdf-capi/src/lib.rs

/// Opaque handle types
pub struct PdfDocument(Box<crate::Document>);
pub struct PdfPage(/* ... */);

/// Open een PDF document
#[no_mangle]
pub extern "C" fn pdf_document_open(
    path: *const c_char,
    password: *const c_char, // nullable
    out: *mut *mut PdfDocument,
) -> PdfStatus {
    // ...
}

/// Render pagina naar RGBA pixels
#[no_mangle]
pub extern "C" fn pdf_page_render(
    page: *const PdfPage,
    dpi: f32,
    width: *mut u32,
    height: *mut u32,
    pixels: *mut *mut u8,
) -> PdfStatus {
    // ...
}

/// Free document
#[no_mangle]
pub extern "C" fn pdf_document_free(doc: *mut PdfDocument) {
    if !doc.is_null() {
        unsafe { drop(Box::from_raw(doc)) };
    }
}

/// Status codes
#[repr(C)]
pub enum PdfStatus {
    Ok = 0,
    ErrorInvalidArgument = 1,
    ErrorFileNotFound = 2,
    ErrorInvalidPassword = 3,
    ErrorCorruptPdf = 4,
    // ...
}
```

**cbindgen** genereert automatisch `pdf_engine.h` uit deze Rust code.
**cargo-c** produceert `.so`/`.dylib`/`.dll` + `.a` + `.h` + `.pc` (pkg-config).

**Geschatte omvang:** ~4K LOC

---

## 14. Fase J: Python Bindings {#14-fase-j}

### Crate Stack

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| **`pyo3`** | 15M | MIT/Apache-2.0 | Python FFI framework |
| **`maturin`** | — | MIT/Apache-2.0 | Build/publish tool |
| **`numpy`** (pyo3) | 3M | MIT | NumPy interop voor pixel data |
| **`pyo3-stub-gen`** | — | MIT | `.pyi` type stub generatie |

### Referentie-implementatie: Polars

Polars (DataFrame library) is het referentieproject voor PyO3 bindings:
- Thin Python wrapper rond Rust core
- Zero-copy waar mogelijk (NumPy arrays)
- Type stubs voor IDE support
- maturin voor build/publish naar PyPI

### Implementatieaanpak

```rust
// pdf-python/src/lib.rs
use pyo3::prelude::*;

#[pyclass]
struct PdfDocument {
    inner: crate::Document,
}

#[pymethods]
impl PdfDocument {
    #[staticmethod]
    fn open(path: &str, password: Option<&str>) -> PyResult<Self> { ... }

    fn page_count(&self) -> usize { ... }

    fn get_field(&self, name: &str) -> PyResult<Option<String>> { ... }

    fn set_field(&mut self, name: &str, value: &str) -> PyResult<()> { ... }

    fn render_page<'py>(&self, py: Python<'py>, page: usize, dpi: f32) -> PyResult<Bound<'py, numpy::PyArray3<u8>>> {
        let image = self.inner.render_page(page, dpi)?;
        let (w, h) = image.dimensions();
        // Zero-copy RGBA → NumPy array
        Ok(numpy::PyArray3::from_vec3(py, &image_to_vec3(&image))?)
    }

    fn save(&self, path: &str) -> PyResult<()> { ... }

    fn flatten_forms(&mut self) -> PyResult<()> { ... }
}

#[pymodule]
fn pdf_engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PdfDocument>()?;
    Ok(())
}
```

**pyo3-stub-gen** genereert `.pyi` files zodat IDE's (VS Code, PyCharm) autocomplete bieden:
```python
# pdf_engine.pyi (auto-generated)
class PdfDocument:
    @staticmethod
    def open(path: str, password: str | None = None) -> PdfDocument: ...
    def page_count(self) -> int: ...
    def get_field(self, name: str) -> str | None: ...
    def render_page(self, page: int, dpi: float = 150.0) -> numpy.ndarray: ...
```

**Geschatte omvang:** ~3K LOC

---

## 15. Fase K: WASM + CLI {#15-fase-k}

### Crate Stack — WASM

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| **`wasm-bindgen`** | 20M | MIT/Apache-2.0 | Rust ↔ JS interop |
| **`js-sys`** | 18M | MIT/Apache-2.0 | JavaScript standard library bindings |
| **`web-sys`** | 10M | MIT/Apache-2.0 | Web API bindings |
| **`wasm-pack`** | — | MIT/Apache-2.0 | Build tool (npm publish) |
| **`tsify`** | 0.5M | MIT | TypeScript type generation |
| **`serde-wasm-bindgen`** | 2M | MIT | Serde ↔ JsValue conversie |

### Crate Stack — CLI

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| **`clap`** | 55M | MIT/Apache-2.0 | CLI argument parsing |
| **`indicatif`** | 15M | MIT | Progress bars |
| **`rayon`** | 30M | MIT/Apache-2.0 | Parallel processing |
| **`color-print`** | 1M | MIT/Apache-2.0 | Colored terminal output |

### WASM Implementatie

```rust
// pdf-wasm/src/lib.rs
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct PdfDocument { /* ... */ }

#[wasm_bindgen]
impl PdfDocument {
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<PdfDocument, JsError> {
        // Parse PDF from bytes
    }

    pub fn render_page(&self, page: usize, dpi: f32) -> Result<Vec<u8>, JsError> {
        // Render to PNG bytes
    }

    pub fn get_field(&self, name: &str) -> Option<String> { ... }

    pub fn set_field(&mut self, name: &str, value: &str) -> Result<(), JsError> { ... }

    pub fn flatten(&mut self) -> Result<(), JsError> { ... }

    pub fn save(&self) -> Result<Vec<u8>, JsError> {
        // Return modified PDF as bytes
    }
}
```

**WASM-specifieke aandachtspunten:**
- Geen `tesseract-sys` (C dependency) → OCR niet beschikbaar in WASM
- Geen filesystem → alles via `&[u8]` / `Vec<u8>`
- Memory: WASM heeft 4GB limiet → streaming voor grote PDFs
- **tsify** genereert TypeScript types voor npm package

### CLI Implementatie

```rust
// pdf-cli/src/main.rs
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pdf-engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Render PDF pagina's naar afbeeldingen
    Render {
        input: PathBuf,
        #[arg(short, long, default_value = "150")]
        dpi: f32,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Merge meerdere PDFs
    Merge {
        inputs: Vec<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Fill form fields
    Fill {
        input: PathBuf,
        #[arg(short, long)]
        data: PathBuf,  // JSON/FDF/XFDF
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        flatten: bool,
    },
    /// Extract text
    Text {
        input: PathBuf,
        #[arg(long)]
        pages: Option<String>,
    },
    /// Encrypt/decrypt
    Encrypt { /* ... */ },
    Decrypt { /* ... */ },
    /// PDF/A conversion
    PdfA {
        input: PathBuf,
        #[arg(long, default_value = "a2b")]
        level: String,
        output: PathBuf,
    },
    /// ZUGFeRD e-invoice
    Invoice {
        pdf: PathBuf,
        xml: PathBuf,
        #[arg(long, default_value = "en16931")]
        profile: String,
        output: PathBuf,
    },
}
```

**Geschatte omvang:** ~5K LOC (3K WASM + 2K CLI)

---

## 16. Fase L: Node.js Bindings {#16-fase-l}

### Crate Stack

| Crate | Downloads | Licentie | Rol |
|-------|-----------|----------|-----|
| **`napi`** | 5M | MIT | Node.js N-API framework |
| **`napi-derive`** | 5M | MIT | Proc macro's voor bindings |
| **`napi-build`** | 5M | MIT | Build script |

### Referentie-implementatie: swc

swc (JavaScript/TypeScript compiler) is de referentie voor napi-rs:
- Native performance in Node.js
- TypeScript definities auto-gegenereerd
- Pre-built binaries voor alle platforms
- npm publish workflow

### Implementatieaanpak

```rust
// pdf-node/src/lib.rs
use napi_derive::napi;
use napi::bindgen_prelude::*;

#[napi]
pub struct PdfDocument { /* ... */ }

#[napi]
impl PdfDocument {
    #[napi(factory)]
    pub fn open(path: String, password: Option<String>) -> Result<Self> { ... }

    #[napi(factory)]
    pub fn from_buffer(data: Buffer) -> Result<Self> { ... }

    #[napi(getter)]
    pub fn page_count(&self) -> u32 { ... }

    #[napi]
    pub fn render_page(&self, page: u32, dpi: Option<f64>) -> Result<Buffer> {
        // Render to PNG, return as Node.js Buffer
    }

    #[napi]
    pub fn get_field(&self, name: String) -> Option<String> { ... }

    #[napi]
    pub fn set_field(&mut self, name: String, value: String) -> Result<()> { ... }

    #[napi]
    pub fn save(&self, path: String) -> Result<()> { ... }

    #[napi]
    pub fn to_buffer(&self) -> Result<Buffer> { ... }
}
```

**napi-rs voordelen:**
- Auto TypeScript `.d.ts` generatie
- Pre-built binaries via `@napi-rs/cli` (geen node-gyp nodig bij installatie)
- Async support via `#[napi(ts_return_type = "Promise<Buffer>")]`

**Geschatte omvang:** ~3K LOC

---

## 17. Dependency Matrix {#17-dependency-matrix}

### Alle externe Rust crates, per fase

| Crate | Fase(s) | Type | Downloads | Licentie |
|-------|---------|------|-----------|----------|
| hayro-* (9 crates) | A, D, F, N | Fork | — | MIT+Apache-2.0 |
| lopdf | B, C, M | Dep | 4.6M | MIT |
| tiny-skia | F | Dep | 21.8M | BSD-3-Clause |
| kurbo | F | Dep | 9M | MIT/Apache-2.0 |
| rustybuzz | F | Dep | 4M | MIT |
| skrifa | F | Dep | 2M | MIT/Apache-2.0 |
| fontdb | F | Dep | 5M | MIT |
| cosmic-text | F | Dep | 0.5M | MIT/Apache-2.0 |
| moxcms | F | Dep | — | MIT |
| image | E, F, M, N | Dep | 48M | MIT/Apache-2.0 |
| zune-jpeg | F | Dep | 4M | MIT/Apache-2.0/Zlib |
| png | F | Dep | 20M | MIT/Apache-2.0 |
| fast_image_resize | F | Dep | 1M | MIT/Apache-2.0 |
| aes | M | Dep | 26M | MIT/Apache-2.0 |
| cbc | M | Dep | 8M | MIT/Apache-2.0 |
| rc4 | M | Dep | 0.4M | MIT/Apache-2.0 |
| md5 | M | Dep | 15M | MIT/Apache-2.0 |
| sha2 | G, M | Dep | 28M | MIT/Apache-2.0 |
| flate2 | M | Dep | 85M | MIT/Apache-2.0 |
| weezl | M | Dep | 3M | MIT/Apache-2.0 |
| subsetter | H | Dep | 0.5M | MIT/Apache-2.0 |
| pdf-writer | H | Dep | 1M | MIT/Apache-2.0 |
| xmp_writer | H | Dep | — | MIT/Apache-2.0 |
| underskrift | G | Eval | — | BSD-2-Clause |
| cms | G | Dep | 0.5M | MIT/Apache-2.0 |
| der | G | Dep | 5M | MIT/Apache-2.0 |
| x509-cert | G | Dep | 2M | MIT/Apache-2.0 |
| rsa | G | Dep | 5M | MIT/Apache-2.0 |
| ecdsa | G | Dep | 4M | MIT/Apache-2.0 |
| p256 / p384 | G | Dep | 3M | MIT/Apache-2.0 |
| reqwest | G | Dep | 45M | MIT/Apache-2.0 |
| tesseract-sys | N | Opt dep | 0.2M | MIT |
| tantivy | N | Dep | 10.6M | MIT |
| fax | N | Dep | 0.1M | MIT |
| quick-xml | O | Dep | 15M | MIT |
| roxmltree | O | Dep | 8M | MIT/Apache-2.0 |
| zugferd-code-lists | O | Dep | — | MIT |
| chrono | O | Dep | 35M | MIT/Apache-2.0 |
| cbindgen | I | Build | 2M | MPL-2.0 |
| pyo3 | J | Dep | 15M | MIT/Apache-2.0 |
| maturin | J | Build | — | MIT/Apache-2.0 |
| numpy (pyo3) | J | Dep | 3M | MIT |
| wasm-bindgen | K | Dep | 20M | MIT/Apache-2.0 |
| clap | K | Dep | 55M | MIT/Apache-2.0 |
| indicatif | K | Dep | 15M | MIT |
| rayon | K | Dep | 30M | MIT/Apache-2.0 |
| napi | L | Dep | 5M | MIT |
| napi-derive | L | Dep | 5M | MIT |
| criterion | E | Dev dep | 8M | Apache-2.0/MIT |
| proptest | E | Dev dep | 5M | MIT/Apache-2.0 |
| insta | E | Dev dep | 13M | Apache-2.0 |

**Totaal: ~50 directe dependencies, allemaal commercieel compatibel.**

---

## 18. Licentie Compliance {#18-licentie-compliance}

### Alle gebruikte licenties

| Licentie | Compatibel met closed-source? | Vereisten | Crates |
|----------|-------------------------------|-----------|--------|
| MIT | Ja | Copyright notice behouden | 80%+ van onze deps |
| Apache-2.0 | Ja | Notice + patent grant | Veel dual MIT/Apache |
| BSD-2-Clause | Ja | Copyright notice | underskrift |
| BSD-3-Clause | Ja | Copyright notice + no-endorsement | tiny-skia |
| MPL-2.0 | Ja (file-level copyleft) | Gewijzigde MPL-bestanden openbaar | cbindgen (build tool) |
| Zlib | Ja | Minimale vereisten | zune-jpeg |
| ISC | Ja | Copyright notice | pixelmatch (dev-dep) |

### Aandachtspunten

1. **AGPL-3.0 (dssim):** Gebruik ALLEEN als dev-dependency voor visuele tests. Mag NIET in productie binary.
2. **MPL-2.0 (cbindgen):** Is een build tool, niet gelinkt in binary. Geen impact op onze licentie.
3. **tesseract-sys → Tesseract (Apache-2.0):** De C++ Tesseract library zelf is Apache-2.0, prima.
4. **hayro fork:** MIT+Apache-2.0, vereist copyright notice in `THIRD_PARTY_LICENSES.txt`.

### THIRD_PARTY_LICENSES.txt template

```
This software includes code from the following open source projects:

hayro (https://github.com/LaurenzV/hayro)
Copyright (c) Laurenz Stampfl
Licensed under MIT and Apache-2.0

tiny-skia (https://github.com/nickel-org/tiny-skia)
Copyright (c) Yevhenii Reizner
Licensed under BSD-3-Clause

[... alle andere dependencies ...]
```

---

## 19. Build-vs-Buy Samenvatting {#19-build-vs-buy}

### Legende
- **Fork:** Neem broncode, pas aan, onderhoud zelf
- **Dependency:** Gebruik als Cargo dependency (geen broncode wijzigingen)
- **Referentie:** Bestudeer voor patronen/architectuur, bouw eigen implementatie
- **Custom:** Volledig zelf bouwen (geen bruikbare bestaande code)

### Per Fase

| Fase | Custom (LOC) | Dependencies | Forks | Referenties | Besparing |
|------|-------------|-------------|-------|------------|-----------|
| A: Workspace | ~3K | — | hayro (9 crates, ~48K LOC) | — | ~48K LOC parser/decoder |
| B: AcroForm | ~12K | lopdf | — | acroform, PDFBox, pdf.js | ~3K (PDF I/O via hayro) |
| C: Annotaties | ~8K | lopdf | — | printpdf, pdf-writer | ~2K (PDF I/O) |
| D: XFA Bridge | ~6K | — | — | — | ~0 (pure integratie) |
| E: Tests | ~4K | image, criterion, proptest | — | — | ~1K (test utilities) |
| M: Manipulatie | ~10K | lopdf, aes, cbc, rc4, flate2 | — | — | ~4K (crypto primitieven) |
| F: Rendering | ~10K | tiny-skia, rustybuzz, skrifa, etc. | — | hayro renderer | ~20K (rendering engine) |
| G: Signatures | ~2-8K | cms, x509-cert, rsa, ecdsa | — | underskrift | ~6K (als underskrift werkt) |
| H: PDF/A | ~7K | pdf-writer, xmp_writer, subsetter | — | krilla | ~5K (PDF/A patterns) |
| N: Content | ~12K | tantivy, tesseract-sys, image | — | — | ~3K (search engine) |
| O: Data Exchange | ~5K | quick-xml, chrono | — | zugferd-code-lists | ~1K (code lists) |
| I: C API | ~4K | cbindgen, libc | — | resvg | ~1K (patterns) |
| J: Python | ~3K | pyo3, maturin, numpy | — | polars | ~1K (patterns) |
| K: WASM+CLI | ~5K | wasm-bindgen, clap | — | — | ~0 |
| L: Node.js | ~3K | napi, napi-derive | — | swc | ~1K (patterns) |

### Totalen

| Categorie | LOC |
|-----------|-----|
| **Onze custom code** | **~80-94K LOC** |
| **Geforkte hayro code** | **~48K LOC** |
| **Bespaard door dependencies** | **~96K LOC** (geschat) |
| **Bespaard door underskrift** (als geschikt) | **~6K LOC** |

**Totale "effort equivalentie":** ~230K LOC aan functionaliteit, waarvan wij ~90K schrijven.
**Hergebruikratio:** ~60% van de totale functionaliteit komt uit het Rust ecosystem.

---

## Appendix: Risico's per Fase

| Fase | Risico | Mitigatie |
|------|--------|----------|
| A | hayro API wijzigingen upstream | Fork isolates ons; cherry-pick selectief |
| B | AcroForm edge cases (duizenden PDFs in het wild) | Uitgebreide test corpus, fuzzing |
| C | Annotatie appearance complexiteit | Focus op top-10 types eerst |
| D | XFA ↔ hayro performance overhead | Profiling, lazy evaluation |
| F | Font rendering accuracy | Vergelijk pixel-by-pixel met Adobe Reader |
| G | underskrift stabiliteit (v0.1.1) | Evaluatie + fallback naar RustCrypto |
| H | PDF/A validatie strictheid | veraPDF als externe validator |
| M | PDF linearisatie complexiteit | Studie van qpdf broncode als referentie |
| N | Tesseract C dependency in WASM | Feature flag, OCR alleen in native builds |
| N | Redactie betrouwbaarheid | Forensische tests met hex editors |
| O | ZUGFeRD schema updates | Schema versioning, automatische tests |

---

## Architectuurbeslissingen Fase A-E

### Beslissing 1: vello_cpu i.p.v. tiny-skia
**Motivatie:** Hayro fork bracht vello_cpu mee. Analytisch path rendering (beter dan scanline). Actief maintained door linebender project (Google-gesponsord). Beter alignment met hayro's Device trait rendering pipeline.
**Impact:** Positief — betere kwaliteit rendering.
**Actie:** Geen bijsturing nodig.

### Beslissing 2: Dual PDF I/O (pdf-syntax + lopdf)
**Motivatie:** pdf-syntax (hayro fork) is read-only by design. Eigen PDF writer bouwen = ~10K LOC effort. lopdf (4.6M downloads) is battle-tested voor PDF mutatie.
**Impact:** Risico bij complexe PDF mutatie (Fase M). Dubbele parsing, vertaling tussen object models.
**Actie:** Evalueer performance bij Fase M implementatie (merge/split 1000+ pagina's, encryptie).

### Beslissing 3: Raw PDF operators voor XFA (tijdelijk)
**Motivatie:** `Color::new()` was `pub(crate)` in hayro fork, waardoor Device trait onbruikbaar was vanuit pdf-xfa. Pragmatische keuze: direct PDF content stream operators schrijven.
**Impact:** Negatief — mist transparency, gradients, blend modes, kleurmanagement.
**Actie:** Paint bridge gebouwd (Issue #210). Na volledige Device trait integratie kan render_bridge.rs vervangen worden.

### Beslissing 4: Pragmatische traits i.p.v. upfront design
**Motivatie:** Snel bouwen, traits later definiëren op basis van werkelijke behoeften. Oorspronkelijk plan had 4 traits (FormEngine, RenderTarget, AnnotationHandler, SignatureHandler) die te breed waren.
**Impact:** Negatief voor Fase I-L bindings als traits ontbreken.
**Actie:** Unified facade traits (`FormAccess` + `DocumentOps`) gedefinieerd in Issue #211.

### Beslissing 5: Arena-based FieldTree (compacter dan gepland)
**Motivatie:** `Vec<FieldNode>` + `FieldId(usize)` — eenvoudiger ontwerp bleek voldoende. 2.1K LOC i.p.v. geschatte 12K LOC.
**Impact:** Positief — minder code, sneller, cache-friendly, makkelijker te onderhouden.
**Actie:** Geen bijsturing nodig.

---

## Evaluatiepunten voor Fase M

Bij implementatie van Fase M (PDF Manipulation + Security) moeten de volgende metingen uitgevoerd worden om de dual I/O strategie te evalueren:

| Meting | Acceptabel | Actie bij overschrijding |
|--------|-----------|--------------------------|
| Merge 1000+ pagina PDFs | < 2x single-library | Overweeg unified writer |
| Split 100+ splits | < 2x single-library | Profiling + optimalisatie |
| Encryption memory | < 1.5x single-library | Object model dedup |
| Round-trip fidelity | 100% pixel-identical | Bug fixing |

**Besliscriterium:** Als performance < 2x vergeleken met een hypothetische single-library oplossing → dual I/O is acceptabel en geen actie nodig.
