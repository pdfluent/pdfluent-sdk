# Validation Pass 02 — SDK Core Facade RFC v1.1

**Onderwerp:** PR #1247 na commit `945173e` (validation pass 01 toegepast)
**Datum:** 2026-04-21
**Rol:** tweede externe-developer validatie — fresh eyes op revisie v1.1

---

## Nieuwe bevindingen

### Blockers (regressies uit v1.1)

**R1 — `redact(text)` method verdwenen uit `PdfDocument`.**
De oorspronkelijke RFC §3 en het design-story §3 gap-analyse noemen `redact` als publieke methode. In v1.1 heb ik bij het herschrijven van `document.rs` de redact-methoden per ongeluk weggelaten. `pdf-redact` crate en `Capability::Redaction` bestaan wel, maar er is geen publieke methode om het aan te roepen.

**R2 — `redact_region(page, rect)` method verdwenen.** Zelfde oorzaak.

**R3 — `split_pages() -> Result<Vec<PdfDocument>>` method verdwenen.** Idem. De website heeft drie how-to's (`split-pdf-by-page-range`, `split-pdf-by-bookmark`, `extract-pdf-pages`) die dit adresseren. Zonder deze methode kan geen enkele van die snippets compileren.

**R4 — `form_fields() -> Vec<FormField>` mist `Result`.**
In v1.1 heb ik dit veranderd van `Result<Vec<FormField>>` naar `Vec<FormField>` — verkeerd. Form-read kan falen op corrupte form-dict of encrypted doc zonder wachtwoord. Moet terug naar `Result`. De v1.1 wijziging was over-eager; mijn rationale ("errors land op setter calls") geldt alleen voor mutations.

### Nieuwe bevindingen (niet in pass 01)

**N1 — `async-tokio` feature + `r#async` module = lege waffle.**
RFC §4 zegt "beta in 1.0" maar het scaffold heeft:
- Feature `async-tokio` ingeschakeld in Cargo.toml
- Optional dep op `tokio`
- Module-declaratie `pub mod r#async;` maar **zonder inhoud**

Een user die `cargo add pdfluent --features async-tokio` doet krijgt een lege module. Dat is erger dan "niet in 1.0": het is misleidend.

**Twee opties:**
- A: Implementeer minimale async surface (open, save, text). Dan locken we die namen wel definitief.
- B: Defer volledig naar 1.1. Remove feature + module + dep. Documenteer in §13.

**Aanbeveling: B.** Commitment tot stable async is zwaar (tokio vs async-std, Send bounds, spawn_blocking patterns). Beter niets shippen dan half shippen.

**N2 — `Permissions` heeft alleen presets, geen builder.**
```rust
Permissions::print_only()
Permissions::read_only()
Permissions::annotate()
```

Maar wat als een user print+copy wil, maar niet annotate? Geen pad. Fields zijn `pub(crate)`. User moet ofwel forken of een preset accepteren die niet past.

**Fix:** voeg `with_*()` builder-methoden toe (`with_print(bool)`, `with_modify(bool)`, etc.). Matcht het patroon van `EncryptOptions::with_user_password` etc.

**N3 — `SignOptions::visible_rect(page: u32, rect: [f64; 4])` — `page: u32` afwijkend.**
Elders in de API is page een `usize`: `page(n: usize)`, `rotate_page(page: usize, ...)`, `page_count() -> usize`, `TextBlock::page: usize`. Alleen hier `u32`. Cross-module inconsistentie.

**Fix:** verander naar `usize`.

**N4 — `RedactOptions` bestaat maar wordt nergens gebruikt.**
`RedactOptions` heeft `case_sensitive` en `regex` velden, een `new()` en builder-methods. Maar geen enkele methode heeft `RedactOptions` in signature. Dead type.

**Fix:** zodra we R1 oplossen, neemt `redact` een `RedactOptions`-parameter. Voeg `RedactOptions::on_pages(&[usize])` toe voor page-scope (website how-to "Redact on specific pages").

### Non-blockers (minor polish)

**P1 — §13 "Not in 1.0" mist image-watermarks.**
Users die `add_image_watermark` zoeken krijgen compile-error zonder verwijzing. §13 noemt wel HTML→PDF maar niet deze andere gaps. Een-regel toevoeging.

**P2 — `doc.license_info()` per-document accessor ontbreekt.**
Users kunnen `pdfluent::license_info()` aanroepen (global), maar als een doc een per-doc override heeft via `OpenOptions::with_license_key`, is dat niet makkelijk te inspecteren. Nice-to-have, niet blocker — `Error::FeatureNotInTier` toont `current_tier` al duidelijk als iets faalt.

**Verdict P2:** skip, YAGNI.

---

## Beslissing

**Freeze is niet veilig in huidige vorm.** 8 issues (4 regressies + 4 nieuwe):

| # | Soort | Fix |
|---|---|---|
| R1 | regression — `redact(text)` | Restore + signature met `RedactOptions` |
| R2 | regression — `redact_region(page, rect)` | Restore |
| R3 | regression — `split_pages()` | Restore + toevoegen `extract_pages(range)` |
| R4 | regression — `form_fields` zonder Result | Restore Result |
| N1 | async waffle | Defer naar 1.1; verwijder feature + module + dep |
| N2 | `Permissions` geen builder | Voeg `with_*()` methoden toe |
| N3 | `visible_rect page: u32` | → `usize` |
| N4 | `RedactOptions` dead | Gebruik in redact signature + `on_pages` |
| P1 | §13 image watermarks | Regel toevoegen |

Na toepassing van deze fixes is de API veilig voor freeze.

---

## Lesson learned

Validation pass 01 introduceerde 4 regressies (R1-R4) door herschrijven van `document.rs`. Als ik toen alleen `git diff` had gelezen ipv fresh-eyes-review, was dit ontdekt. Pass 02 bevestigt dat fresh-eyes-review de juiste methode is — automatische merge zonder 2e pass zou deze regressies hebben laten ontsnappen.

**Regel voor de toekomst:** na elke niet-triviale API-wijziging een fresh-eyes-pass die ALLE public API methoden inventariseert t.o.v. het vorige spec — niet alleen de gewijzigde delen.
