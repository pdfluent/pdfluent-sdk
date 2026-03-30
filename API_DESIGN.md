# API Design Review — PDFluent SDK (#617)

Dit document beschrijft de design decisions voor de publieke `pdfluent` API, gebaseerd op de review uitgevoerd in #617.

## Huidige Problemen (Geïdentificeerd in Review)

1. **Te veel low-level kennis vereist:** De huidige API (zoals `PdfDocument::open`, `RenderOptions`, `validate_pdfa`) vereist dat de developer begrijpt in welke specifieke module of crate functionaliteit zit (`pdf-engine`, `pdf-compliance`, `pdf-redact`).
2. **Geen unificatie:** Conversie (`pdf-docx`), compliance, manipulatie en rendering leven in aparte namespaces. Developers moeten meerdere `use` statements en crates beheren.
3. **Inconsistente Error Handling:** Fouten variëren van `std::io::Error` en specifieke library errors tot strings, wat het afhandelen complex maakt. Errors missen context (welke file, welke pagina?).
4. **Mutabiliteit vs Immutabiliteit:** Inconsistente return types, soms wordt het document aangepast in place (mut), soms wordt een nieuw document of een byte-array geretourneerd (bijv. `pdf_to_docx(doc)`).

## DX Principes (Developer Experience)

De API is ontworpen rond drie kernprincipes:

### P1. Zero-Config First Success
De meest voorkomende acties vereisen geen configuratie of imports van diepe datastructuren.
* **Slecht:** `PdfDocument::open(Arc::new(std::fs::read("file.pdf").unwrap()))`
* **Goed:** `pdfluent::read("file.pdf")`

### P2. Pit of Success
De meest logische, makkelijk te vinden methode resulteert automatisch in veilig, performant en idiomatisch gebruik.
We verbergen footguns door defaults veilig in te stellen.
* Bijvoorbeeld: Automatische linearisatie is de default tenzij expliciet uitgezet.

### P3. Progressive Disclosure
Simpele use-cases gebruiken simpele methodes; complexe use-cases breiden dit uit (via de builder of closure patterns).
* **Simpel:** `doc.save("out.pdf")`
* **Complex:** `doc.save_with("out.pdf", |opts| opts.format(PdfFormat::PdfA2b))`

### P4. Errors Are Documentation
Fouten moeten informeren over: wat ging er mis, de context (b.v. een specifieke path string of page out of bounds index), een `Help` boodschap met suggesties hoe dit te verhelpen, en een link naar de online documentatie.

## Naamgeving Conventies
- Actiewoorden voor methodes: `text()`, `page()`, `save()`, `sign()`.
- Opties structureren in closures: `method_with(..., |opts| ...)` voor het fluent chainen van configuratie zonder lange argument lists.
- 1-based indexering voor pagina's in the top-level API (`page(1)` in plaats van `page(0)`) aangezien business users en UI developers PDF pagina's standaard als 1-based beschouwen.

## Error Handling Filosofie
Een centrale `pdfluent::Error` (gebaseerd op `thiserror`) groepeert alle failures.
Varianten bevatten duidelijke display strings met in-line help instructies en een `Docs:` link. Hierdoor kunnen developers problemen direct vanuit hun terminal/logs oplossen, zonder naar de manual te hoeven zoeken.

## Per-taal Aanpassingen
De basis is in Rust, maar de FFI / Bindings volgen de verwachtingen per ecosysteem:
* **Rust:** Idiomatisch met `Result<T, Error>`, lifetimes waar nuttig (maar geminimaliseerd op de facade laag).
* **Python:** Exceptions in plaats van Result types, duck-typing voor `pdfluent.read(file_like)`.
* **JS/TS:** Asynchrone methodes (Promises) voor i/o (`read`, `save`), camelCase methodes (`pageCount()`).
* **Java/C#:** Object-oriented classes, exceptions, standard getter conventions (`getPageCount()`).
