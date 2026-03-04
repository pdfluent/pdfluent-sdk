# XFA-Native-Rust — Product Backlog

## Dependency Graph

```
Epic 0 (Fundament)
  ├── Epic 1 (DOM/SOM) ──────────────┐
  │                                   ├── Epic 3 (Layout Engine)
  ├── Epic 2 (FormCalc) ─────────────┘         │
  │                                             │
  ├── Epic 4 (PDFium Bridge) ───────────────────┤
  │                                             │
  └── Epic 5 (Persistence) ────────────────────→├── Epic 6 (Validatie)
```

**Kritisch pad:** 0 → 1 → 3 → 4 → 6
**Parallel pad:** Epic 2 (FormCalc) kan gelijktijdig met Epic 1

---

## Epic 0: Project Fundament

| #   | Issue                  | Beschrijving                                                                 | Status  |
| --- | ---------------------- | ---------------------------------------------------------------------------- | ------- |
| 0.1 | Project scaffolding    | Cargo workspace met modules A-D, CI setup, `CLAUDE.md`                       | ✅ Done |
| 0.2 | SPEC.md opstellen      | Architectuurdocument op basis van XFA 3.3 §4 (Box Model) + §3 (SOM)         | Pending |
| 0.3 | Dependency setup       | `roxmltree`, `pdfium-render`, test framework configureren                    | Pending |
| 0.4 | Test infrastructure    | Golden render pipeline: render → PNG → pixel diff → rapport                  | Pending |
| 0.5 | CI/CD pipeline         | GitHub Actions: `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt`     | Pending |

---

## Epic 1: Module A — `xfa-dom-resolver` (SOM Paden)

**Spec referentie:** XFA 3.3 §3 (Object Models in XFA)

| #   | Issue                    | Beschrijving                                                        | Status  |
| --- | ------------------------ | ------------------------------------------------------------------- | ------- |
| 1.1 | XFA DOM parser           | Parse XFA XML packets uit PDF via `roxmltree`, bouw Template + Data DOM | Pending |
| 1.2 | SOM path resolver        | Implementeer `xfa.form.subform[3].field[*]` pad-resolutie           | Pending |
| 1.3 | SOM expression evaluator | Wildcards, predicates, named references in SOM paden                | Pending |
| 1.4 | DOM manipulation API     | CRUD operaties op Template/Data DOM nodes                           | Pending |
| 1.5 | Unit tests SOM           | Volledige test coverage voor padresolutie edge cases                 | Pending |

---

## Epic 2: Module C — `formcalc-interpreter` (Scripting)

**Spec referentie:** XFA 3.3 §25 (FormCalc Specification)

| #    | Issue                           | Beschrijving                                                           | Status  |
| ---- | ------------------------------- | ---------------------------------------------------------------------- | ------- |
| 2.1  | FormCalc lexer                  | Tokenizer voor literals, keywords, operators                           | Pending |
| 2.2  | FormCalc parser → AST           | Recursive descent parser voor volledige grammatica                     | Pending |
| 2.3  | AST interpreter                 | Expression evaluator met type coercion                                 | Pending |
| 2.4  | Built-in: Arithmetic            | `Abs`, `Avg`, `Ceil`, `Count`, `Floor`, `Max`, `Min`, `Mod`, `Round`, `Sum` | Pending |
| 2.5  | Built-in: Date/Time             | `Date`, `Date2Num`, `Num2Date`, `Time`, `Time2Num` + 7 anderen        | Pending |
| 2.6  | Built-in: String                | `At`, `Concat`, `Left`, `Len`, `Replace`, `Substr` + 13 anderen       | Pending |
| 2.7  | Built-in: Financial             | `Apr`, `Pmt`, `Pv`, `Rate`, `Term` + 5 anderen                        | Pending |
| 2.8  | Built-in: Logical + Misc        | `If`, `Choose`, `Oneof`, `Within`, `Eval`, `Null`                     | Pending |
| 2.9  | SOM-integratie                  | FormCalc scripts resolven en muteren DOM nodes via Module A            | Pending |
| 2.10 | Conformance tests               | Test suite tegen FormCalc spec voorbeelden                             | Pending |

---

## Epic 3: Module B — `xfa-layout-engine` (De Core)

**Spec referentie:** XFA 3.3 §4 (Box Model), §8 (Layout for Growable Objects)

| #    | Issue                    | Beschrijving                                                           | Status  |
| ---- | ------------------------ | ---------------------------------------------------------------------- | ------- |
| 3.1  | Box Model implementatie  | Margins, borders, padding, content areas (§4)                          | Pending |
| 3.2  | Positioned layout        | Absolute positioning van elementen in containers                       | Pending |
| 3.3  | Flowed layout            | Flow-based layout: `tb`, `lr-tb`, `rl-tb`                             | Pending |
| 3.4  | Content areas & pages    | Page templates, master pages, content area definitie                   | Pending |
| 3.5  | Dynamic sizing           | `minH`, `maxH`, `minW`, `maxW` constraints                            | Pending |
| 3.6  | Occur rules              | Herhalende subforms op basis van `occur` (min/max/initial)             | Pending |
| 3.7  | Pagination               | Content overflow, page breaks, multi-page reflow (§8)                  | Pending |
| 3.8  | Content splitting        | Splitsen van content over pagina's                                     | Pending |
| 3.9  | Tables layout            | Table, row, cell layout met spanning                                   | Pending |
| 3.10 | Leaders & trailers       | Header/footer elementen per pagina                                     | Pending |
| 3.11 | Text placement           | Rich text rendering, text wrapping, font metrics                       | Pending |
| 3.12 | Scripting-integratie     | Layout reageert op FormCalc calculations/validations                   | Pending |
| 3.13 | Golden render tests      | Visuele vergelijking met Adobe Reader output                           | Pending |

---

## Epic 4: Module D — `pdfium-ffi-bridge`

**Spec referentie:** PDFium API + XFA 3.3 §14 (User Experience)

| #   | Issue                       | Beschrijving                                                        | Status  |
| --- | --------------------------- | ------------------------------------------------------------------- | ------- |
| 4.1 | PDFium bindings setup       | `pdfium-render` crate integratie, library linking                   | Pending |
| 4.2 | XFA packet extractie        | XFA streams uit PDF lezen via PDFium                                | Pending |
| 4.3 | FPDF_FORMFILLINFO callbacks | Implementeer form fill interface voor UI events                     | Pending |
| 4.4 | Render pipeline             | XFA layout → PDFium rendering → pixel output                       | Pending |
| 4.5 | Event handling              | Muisklikken, toetsaanslagen doorsturen naar Rust engine             | Pending |
| 4.6 | Integratie tests            | End-to-end: PDF → XFA → render → vergelijken                       | Pending |

---

## Epic 5: Persistence & Security

**Spec referentie:** XFA 3.3 §16 (Security and Reliability)

| #   | Issue                    | Beschrijving                                                         | Status  |
| --- | ------------------------ | -------------------------------------------------------------------- | ------- |
| 5.1 | Dataset sync             | Bij save: Data DOM → `<xfa:datasets>` packet terug in PDF           | Pending |
| 5.2 | UR3 signature detectie   | Detecteer Usage Rights signatures in PDF                             | Pending |
| 5.3 | UR3 veilige verwijdering | Verwijder UR3 signatures zonder PDF te corrumperen                   | Pending |
| 5.4 | Round-trip tests         | Open → edit → save → reopen → verify integriteit                    | Pending |

---

## Epic 6: Validatie & Polish

| #   | Issue                   | Beschrijving                                                          | Status  |
| --- | ----------------------- | --------------------------------------------------------------------- | ------- |
| 6.1 | Conformance test suite  | Tests tegen echte XFA PDFs uit het wild                               | Pending |
| 6.2 | Performance benchmarks  | Parsing, layout, rendering tijdmetingen                               | Pending |
| 6.3 | Edge case hardening     | Foutafhandeling, malformed XML, ontbrekende fonts                     | Pending |
| 6.4 | Documentatie            | API docs, architectuur overview, usage guide                          | Pending |
