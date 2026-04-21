# Validation Pass 01 — SDK Core Facade RFC

**Onderwerp:** PR #1247 (RFC 0001 + pdfluent scaffold)
**Datum:** 2026-04-21
**Rol:** externe-developer validatie vóór API Design Freeze (#1239)

Dit rapport kijkt kritisch naar de API vanuit het perspectief van een developer die `pdfluent` voor het eerst gebruikt. Waar die developer moet nadenken, is er een ontwerpfout.

---

## STAP 1 — Drie verse examples zonder bestaande code te kopiëren

### 1A. HTML → PDF

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::from_html(
        "https://pdfluent.com",
        HtmlToPdfOptions::new()
            .page_size(PageSize::A4)
            .margins(Margins::inches(1.0))
            .wait_for_selector("main"),
    )?;
    doc.save("pdfluent.pdf")?;
    Ok(())
}
```

**Frictie gevonden:**
- `PdfDocument::from_html` staat niet in de RFC
- `HtmlToPdfOptions`, `PageSize`, `Margins` — bestaan niet
- Onduidelijk of ik Chromium zelf moet installeren

**Root cause:** HTML→PDF valt onder IronPDF Parity (#1206), niet onder milestone #52. Maar de RFC zegt dit niet expliciet en er is geen stub-methode die de user naar #1206 verwijst. Een developer die het probeert krijgt `error[E0599]: no method named \`from_html\`` zonder hint.

**Fix:** RFC voegt §13 toe "Not in 1.0" die HTML→PDF, DOCX-export, OCR etc. expliciet opsomt, verwijst naar milestone + planned release.

### 1B. PDF → extract text

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("report.pdf")?;

    // Alle tekst
    let full = doc.text()?;

    // Per page
    for (i, page) in doc.pages().enumerate() {
        println!("--- Page {} ---", i + 1);
        println!("{}", page.text()?);
    }

    // Met layout
    for block in doc.structured_text()? {
        println!("{} at [{:?}]", block.text, block.bbox);
    }
    Ok(())
}
```

**Frictie gevonden:**
- `structured_text` — ik wist niet zeker of dit de juiste naam was. Had ook kunnen zijn `text_with_layout`, `extract_text_blocks`, `text_blocks`, `layout`.
- De capability heet `TextExtractWithLayout`. De method heet `structured_text`. **Mismatch maakt discoverability slechter.**

**Fix:** hernoem `structured_text` → `text_with_layout()`. Matcht de capability-naam en is zelf-beschrijvend.

### 1C. Combine + watermark + save

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfMerger::new()
        .add(PdfDocument::open("cover.pdf")?)
        .add(PdfDocument::open("body.pdf")?)
        .add(PdfDocument::open("appendix.pdf")?)
        .build()?;

    doc.add_watermark("DRAFT", WatermarkOptions::centered().opacity(0.3))?;
    doc.save("final.pdf")?;
    Ok(())
}
```

**Frictie gevonden:**
- Ik heb `with_bookmarks(...)` niet gezet. Wat is het default gedrag? `Option<BookmarkMergeStrategy>` = `None`. Betekent dat discard? Concat? Ongedocumenteerd.
- `add_watermark(text, ...)` — wat als ik een image-watermark wil? Geen pad. Silently alleen tekst.

**Fix:**
1. `BookmarkMergeStrategy: Default = Concat` — meest verwachte gedrag als default. `MergeOptions::bookmarks` wordt non-optional `BookmarkMergeStrategy`.
2. `add_watermark(text: &str, ...)` wordt gedocumenteerd als tekst-only; image watermark als aparte methode `add_image_watermark(bytes, ...)` of fold beide in de `PageDecoration` consolidatie (#1225).

---

## STAP 2 — API Minimalism Check

### Dubbele paden voor hetzelfde doen:

**(a) `PdfDocumentBuilder` vs `OpenOptions`**

Beide doen hetzelfde:
```rust
// Path 1
let doc = PdfDocument::open_with("x.pdf", OpenOptions::new().with_password("pw"))?;
// Path 2
let doc = PdfDocumentBuilder::new().with_password("pw").open("x.pdf")?;
```

**Regel: liever minder API dan meer.**

**Fix:** verwijder `PdfDocumentBuilder`. `OpenOptions` + `open_with` is 1 pad, consistent met `SaveOptions` + `save_with`.

**(b) `permissions_mut()` builder vs `encrypt(EncryptOptions::with_permissions(...))`**

Beide stellen permissies in:
```rust
// Path 1
doc.permissions_mut() /* ...builder... */ .commit()?;
// Path 2
doc.encrypt(EncryptOptions::aes256().with_permissions(Permissions::print_only()))?;
```

Semantiek-overlap + onduidelijk wat `permissions_mut` doet op een niet-versleutelde doc.

**Fix:** verwijder `permissions_mut()` + `PermissionsBuilder`. Users gebruiken `encrypt()` voor set én change. Reduceert API-oppervlak met 2 types.

**(c) `signatures()` vs `verify_signatures()`**

Beide lijken signatures te retrieven maar doen verschillende dingen. Huidige `Signature` heeft `status: SignatureStatus` — dus `signatures()` lijkt al te valideren. Maar dan waarom `verify_signatures()`?

**Fix:** scheiding helder maken:
- `signatures() -> Vec<SignatureInfo>` — lichtgewicht metadata, GEEN validatie. `SignatureInfo` heeft geen `status` veld.
- `verify_signatures() -> SignatureValidationReport` — volledige validatie met per-handtekening status, cert chain, timestamp check, revocation.

### Opties die defaults moeten zijn:

**`MergeOptions::bookmarks: Option<BookmarkMergeStrategy>`** — Option met None-default is onduidelijk. Non-optional met `Default = Concat` is voorspelbaar.

### Dead types:

**`Alignment` enum** in watermark module is geëxporteerd via prelude maar wordt nergens gebruikt door `WatermarkOptions`. Weg.

---

## STAP 3 — Capability Friction Check

**Simulatie:** een Developer-tier user probeert te signen.

```rust
let mut doc = PdfDocument::open("contract.pdf")?;
doc.sign(&signer, SignOptions::new())?;
// Error: FeatureNotInTier
//   needed capability: DigitalSignatureSign
//   your tier: Developer
//   required tier: Team (€1,499/yr)
//   Upgrade: https://pdfluent.com/pricing
//   Docs: https://pdfluent.com/errors/E-LICENSE-FEATURE-NOT-IN-TIER
```

✅ Dat werkt goed. Zowel code als Display is duidelijk.

**MAAR — hoe zet ik mijn license key?**

```rust
let doc = PdfDocument::open("x.pdf")?;
// Waar kom ik mijn license-key binnen?
```

**Grote gap.** Nergens in de RFC staat hoe een user een license-key levert:
- Er is geen `PdfDocument::with_license`
- Er is geen `pdfluent::set_license_key(...)`
- Er is geen env var gedocumenteerd
- Er is geen bestand-locatie (`~/.config/pdfluent/license.key`) gedocumenteerd

Methoden doen intern `self.license.require(...)` maar er is geen constructor-pad voor `License`. Dit is een **kritieke gap** — zonder fix kan een betalende klant de SDK niet gebruiken.

**Fix:**

```rust
// Global (thread-safe set-once)
pdfluent::set_license_key("-----BEGIN PDFLUENT LICENSE-----...")?;

// Per-document override
let doc = PdfDocument::open_with(
    "x.pdf",
    OpenOptions::new().with_license_key(key),
)?;

// Env var automatisch: PDFLUENT_LICENSE_KEY

// Inspection
let info = pdfluent::license_info();
println!("Tier: {:?}, capabilities: {}", info.tier, info.capabilities);
```

---

## STAP 4 — Cross-Module Consistency

### Naming:
| Concept | Gebruikt | Opmerking |
|---|---|---|
| Sub-object builder | `_mut()` accessor | `form_mut`, `metadata_mut`, `pages_mut` — consistent ✓ |
| Option-veld op struct | `with_*()` | `with_password`, `with_linearize` — consistent ✓ |
| Voeg toe aan collectie | `add_*()` | `PdfMerger::add`, `add_watermark` — consistent ✓ |
| Setter op mutable view | `set_*()` | `form.set_text`, `metadata.set_title` — consistent ✓ |

**Mismatch gevonden:**
- `form_mut() -> Result<PdfFormMut>` — kan falen (geen form in doc)
- `metadata_mut() -> MetadataMut` — kan niet falen in RFC (geen Result)

Inconsistent. Een `doc.metadata_mut()?` hoort niet te werken zonder `?` als `form_mut()?` dat wel nodig heeft. Voor chainable DX moet ofwel beide Result-less zijn, ofwel beide Result.

**Fix:** `form_mut() -> PdfFormMut` altijd (no Result). Empty-form-in-doc retourneert een geldige builder; het zetten van een niet-bestaand field returnt `Error::FieldNotFound` bij de `set_text()` call. Dit matcht `metadata_mut()` en geeft betere DX.

### PAdES profile naming:

```rust
PadesProfile::BB    // wtf is BB?
PadesProfile::BT    // onduidelijk
PadesProfile::BLT   // OK als je PAdES kent
PadesProfile::BLTA  // moeilijker
```

- Non-idiomatic Rust (upper-case acronyms trigger clippy warnings)
- Niet zelf-documenterend voor niet-PAdES-experts

**Fix:** descriptive namen:
```rust
PadesProfile::BasicSignature       // B-B
PadesProfile::TimestampedSignature // B-T
PadesProfile::LongTerm             // B-LT (default)
PadesProfile::LongTermArchive      // B-LTA
```

Rustdoc-comment verwijst naar ETSI EN 319 142-1 voor mapping.

### Rotation naming:

```rust
Rotation::D90  // "D" prefix obscuur
Rotation::D180
Rotation::D270
```

**Fix:**
```rust
Rotation::Clockwise90
Rotation::Clockwise180
Rotation::Clockwise270
```

Expliciet richting + getal. `Clockwise` ipv `CounterClockwise` sluit aan bij PDF coordinate-system convention.

---

## STAP 5 — Website Parity Check

Alle 5 bootstrap-examples compileren tegen de huidige scaffold — website naming werkt. Echter, door bovenstaande renames:

- `PadesProfile::BLT` wordt `PadesProfile::LongTerm` → **geen website-snippet gebruikt dit nog** (signing niet in bootstrap).
- `Rotation::D90` wordt `Rotation::Clockwise90` → **geen bootstrap snippet gebruikt dit nog**.
- `structured_text` wordt `text_with_layout` → **extract_text_pdf_rust uses `doc.text()?`, niet structured** — geen impact.
- `permissions_mut` verwijderen → **geen bootstrap snippet gebruikt dit**.
- `PdfDocumentBuilder` verwijderen → **geen bootstrap snippet gebruikt dit**.

Conclusie: renames breken geen website-snippets. Scrape van pdfluent.com zal mogelijk wel nieuwe entries tonen (bijv. signature how-to's) — die moet Epic 6 valideren in #1237.

---

## STAP 6 — Beslismoment

**API Freeze is NIET veilig in huidige vorm.** Minimaal 12 verbeterpunten gevonden waarvan 4 blockers.

### Blockers (moeten gefixt vóór freeze)

1. **License provisioning ontbreekt volledig** — geen pad voor users om license key te leveren. SDK onbruikbaar voor betalende klanten in huidige vorm.
2. **`PdfDocumentBuilder` + `OpenOptions` dubbel** — schendt API minimalism. Verwijder builder.
3. **`permissions_mut()` + `encrypt()` dubbel** — schendt API minimalism. Verwijder `permissions_mut`.
4. **`signatures()` / `verify_signatures()` semantiek-overlap** — splits `Signature` → `SignatureInfo` (metadata) + `SignatureValidation` (in report).

### Must-fix naming (vóór freeze)

5. `structured_text` → `text_with_layout` (consistent met Capability-naam).
6. `PadesProfile::BB/BT/BLT/BLTA` → `BasicSignature/Timestamped/LongTerm/LongTermArchive`.
7. `Rotation::D90/D180/D270` → `Clockwise90/Clockwise180/Clockwise270`.

### Consistency fixes

8. `metadata_mut()` en `form_mut()` allebei `Result`-less. Errors landen op de setter-methoden.
9. `BookmarkMergeStrategy: Default = Concat`, `MergeOptions::bookmarks` non-optional.
10. `Alignment` dead type uit watermark module weg.

### Contract fixes

11. `Error::docs_url() -> &'static str` (niet `String`). Per-variant const string.
12. `PdfDocument: !Clone` in 1.0. Users dupliceren via `PdfDocument::from_bytes(doc.to_bytes()?)`.

### Non-blocker documentatie

13. RFC §13 "Not in 1.0" — expliciet lijst met HTML→PDF, DOCX-export, OCR, etc. + verwijzing naar milestones die deze leveren. Voorkomt dat gebruikers onverklaarbare compile-errors krijgen.

---

## STAP 7 — Actieplan

1. Update RFC (`docs/rfc/0001-sdk-core-api.md`) met wijzigingen 1-13.
2. Update scaffold code in `crates/pdfluent/src/` om RFC te matchen.
3. Voeg `crates/pdfluent/src/license.rs` toe met `set_license_key`, `license_info`, `LicenseInfo`.
4. Re-run `cargo test -p pdfluent --tests` — moet nog steeds 5 passed / 5 ignored / 0 failed zijn.
5. Amend commit op branch, push.
6. Update PR #1247 beschrijving.
7. Tweede validation pass door deze checklist.
8. Als alles groen: sluit #1239.

**Tussenconclusie:** de validation pas heeft 12 concrete verbeteringen gevonden. Door ze NU op te lossen voorkomen we dat elke verbetering later een 2.0 breaking change wordt. De moeite waard.
