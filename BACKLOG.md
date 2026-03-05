# XFA-Native-Rust — Product Backlog

## Dependency Graph

```
Epic 0 (Fundament)
  ├── Epic 1 (DOM/SOM) ──────────────┐
  │                                   ├── Epic 3 (Layout Engine)
  ├── Epic 2 (FormCalc) ─────────────┘         │
  │                                             │
  ├── Epic 4 (Native PDF I/O & Rendering) ─────┤
  │                                             │
  └── Epic 5 (Persistence) ────────────────────→├── Epic 6 (Validatie)
```

**Kritisch pad:** 0 → 1 → 3 → 4 → 6
**Parallel pad:** Epic 2 (FormCalc) kan gelijktijdig met Epic 1

**Architectuurkeuze:** 100% pure Rust, geen C/C++ dependencies.
PDFium wordt alleen optioneel gebruikt voor visuele vergelijking tijdens development.
Dit maakt WASM-compilatie mogelijk en versterkt de "memory safe" USP.

---

## Epic 0: Project Fundament

| #   | Issue                  | Beschrijving                                                                 | Status  |
| --- | ---------------------- | ---------------------------------------------------------------------------- | ------- |
| 0.1 | Project scaffolding    | Cargo workspace met modules A-D, CI setup, `CLAUDE.md`                       | ✅ Done |
| 0.2 | SPEC.md opstellen      | Architectuurdocument op basis van XFA 3.3 §4 (Box Model) + §3 (SOM)         | ✅ Done |
| 0.3 | Dependency setup       | `roxmltree`, `pdfium-render`, test framework configureren                    | ✅ Done |
| 0.4 | Test infrastructure    | Golden render pipeline: render → PNG → pixel diff → rapport                  | ✅ Done |
| 0.5 | CI/CD pipeline         | GitHub Actions: `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt`     | ✅ Done |

---

## Epic 1: Module A — `xfa-dom-resolver` (SOM Paden)

**Spec referentie:** XFA 3.3 §3 (Object Models in XFA)

| #   | Issue                    | Beschrijving                                                        | Status  |
| --- | ------------------------ | ------------------------------------------------------------------- | ------- |
| 1.1 | XFA DOM parser           | Parse XFA XML packets uit PDF via `roxmltree`, bouw Template + Data DOM | ✅ Done |
| 1.2 | SOM path resolver        | Implementeer `xfa.form.subform[3].field[*]` pad-resolutie           | ✅ Done |
| 1.3 | SOM expression evaluator | Wildcards, predicates, named references in SOM paden                | ✅ Done |
| 1.4 | DOM manipulation API     | CRUD operaties op Template/Data DOM nodes                           | ✅ Done |
| 1.5 | Unit tests SOM           | Volledige test coverage voor padresolutie edge cases                 | ✅ Done |

---

## Epic 2: Module C — `formcalc-interpreter` (Scripting)

**Spec referentie:** XFA 3.3 §25 (FormCalc Specification)

| #    | Issue                           | Beschrijving                                                           | Status  |
| ---- | ------------------------------- | ---------------------------------------------------------------------- | ------- |
| 2.1  | FormCalc lexer                  | Tokenizer voor literals, keywords, operators                           | ✅ Done |
| 2.2  | FormCalc parser → AST           | Recursive descent parser voor volledige grammatica                     | ✅ Done |
| 2.3  | AST interpreter                 | Expression evaluator met type coercion                                 | ✅ Done |
| 2.4  | Built-in: Arithmetic            | `Abs`, `Avg`, `Ceil`, `Count`, `Floor`, `Max`, `Min`, `Mod`, `Round`, `Sum` | ✅ Done |
| 2.5  | Built-in: Date/Time             | `Date`, `Date2Num`, `Num2Date`, `Time`, `Time2Num` + 7 anderen        | ✅ Done |
| 2.6  | Built-in: String                | `At`, `Concat`, `Left`, `Len`, `Replace`, `Substr` + 13 anderen       | ✅ Done |
| 2.7  | Built-in: Financial             | `Apr`, `Pmt`, `Pv`, `Rate`, `Term` + 5 anderen                        | ✅ Done |
| 2.8  | Built-in: Logical + Misc        | `If`, `Choose`, `Oneof`, `Within`, `Eval`, `Null`                     | ✅ Done |
| 2.9  | SOM-integratie                  | FormCalc scripts resolven en muteren DOM nodes via Module A            | ✅ Done |
| 2.10 | Conformance tests               | Test suite tegen FormCalc spec voorbeelden                             | ✅ Done |

---

## Epic 3: Module B — `xfa-layout-engine` (De Core)

**Spec referentie:** XFA 3.3 §4 (Box Model), §8 (Layout for Growable Objects)

| #    | Issue                    | Beschrijving                                                           | Status  |
| ---- | ------------------------ | ---------------------------------------------------------------------- | ------- |
| 3.1  | Box Model implementatie  | Margins, borders, padding, content areas (§4)                          | ✅ Done |
| 3.2  | Positioned layout        | Absolute positioning van elementen in containers                       | ✅ Done |
| 3.3  | Flowed layout            | Flow-based layout: `tb`, `lr-tb`, `rl-tb`                             | ✅ Done |
| 3.4  | Content areas & pages    | Page templates, master pages, content area definitie                   | ✅ Done |
| 3.5  | Dynamic sizing           | `minH`, `maxH`, `minW`, `maxW` constraints                            | ✅ Done |
| 3.6  | Occur rules              | Herhalende subforms op basis van `occur` (min/max/initial)             | ✅ Done |
| 3.7  | Pagination               | Content overflow, page breaks, multi-page reflow (§8)                  | ✅ Done |
| 3.8  | Content splitting        | Splitsen van content over pagina's                                     | ✅ Done |
| 3.9  | Tables layout            | Table, row, cell layout met spanning                                   | Pending |
| 3.10 | Leaders & trailers       | Header/footer elementen per pagina                                     | ✅ Done |
| 3.11 | Text placement           | Rich text rendering, text wrapping, font metrics                       | ✅ Done |
| 3.12 | Scripting-integratie     | Layout reageert op FormCalc calculations/validations                   | ✅ Done |
| 3.13 | Golden render tests      | Visuele vergelijking met Adobe Reader output                           | ✅ Done |

---

## Epic 4: Module D — Native PDF I/O & Rendering (100% Rust)

**Spec referentie:** XFA 3.3 §14 (User Experience), PDF 1.7 Reference

**Architectuur:** Pure Rust met `lopdf` voor PDF structuur. Geen C/C++ dependencies.
PDFium is optioneel (`#[cfg(feature = "pdfium")]`) alleen voor visuele vergelijking.

| #   | Issue                       | Beschrijving                                                        | Status  |
| --- | --------------------------- | ------------------------------------------------------------------- | ------- |
| 4.1 | ~~PDFium bindings setup~~   | ~~`pdfium-render` crate integratie~~ → verplaatst naar optionele feature | ✅ Done |
| 4.2 | XFA packet extractie        | XFA streams uit PDF lezen via `lopdf` (pure Rust)                   | ✅ Done |
| 4.3 | Native PDF parser           | PDF lezen/schrijven via `lopdf`, XFA extractie zonder PDFium        | ✅ Done |
| 4.4 | Native render pipeline      | Layout DOM → `image` crate rendering → pixel output (pure Rust)    | ✅ Done |
| 4.5 | Event handling              | Muisklikken, toetsaanslagen doorsturen naar Rust engine             | ✅ Done |
| 4.6 | Integratie tests            | End-to-end: PDF → XFA → layout → render → vergelijken              | ✅ Done |

---

## Epic 5: Persistence & Security

**Spec referentie:** XFA 3.3 §16 (Security and Reliability)

| #   | Issue                    | Beschrijving                                                         | Status  |
| --- | ------------------------ | -------------------------------------------------------------------- | ------- |
| 5.1 | Dataset sync             | Bij save: Data DOM → `<xfa:datasets>` packet terug in PDF           | ✅ Done |
| 5.2 | UR3 signature detectie   | Detecteer Usage Rights signatures in PDF                             | ✅ Done |
| 5.3 | UR3 veilige verwijdering | Verwijder UR3 signatures zonder PDF te corrumperen                   | ✅ Done |
| 5.4 | Round-trip tests         | Open → edit → save → reopen → verify integriteit                    | ✅ Done |

---

## Epic 6: Validatie & Polish

| #   | Issue                   | Beschrijving                                                          | Status  |
| --- | ----------------------- | --------------------------------------------------------------------- | ------- |
| 6.1 | Conformance test suite  | Tests tegen echte XFA PDFs uit het wild                               | ✅ Done |
| 6.2 | Performance benchmarks  | Parsing, layout, rendering tijdmetingen                               | ✅ Done |
| 6.3 | Edge case hardening     | Foutafhandeling, malformed XML, ontbrekende fonts                     | ✅ Done |
| 6.4 | Documentatie            | API docs, architectuur overview, usage guide                          | ✅ Done |
