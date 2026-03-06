# Fase A — Beslissingen en keuzes

Overzicht van autonome beslissingen genomen tijdens de implementatie van Fase A.

## 1. Projectnaam: behouden als xfa-native-rust

De roadmap suggereerde "oxide-pdf" als werkruimtenaam. Ik heb gekozen om de
bestaande repo (`xfa-native-rust`) te behouden en de nieuwe crates erin te
integreren, in plaats van een nieuw project/repo aan te maken. Reden:
- De XFA-engine is het bestaande product en de differentiator
- Een nieuwe repo zou de git-historie en issues verliezen
- De crate-namen (`pdf-syntax`, `pdf-render`, etc.) zijn al duidelijk genoeg
- Hernoeming van de repo kan later alsnog als dat commercieel wenselijk is

## 2. Rust edition: 2024 voor hayro-forks, 2021 voor eigen crates

De geforkte hayro crates gebruiken edition 2024 (hun originele edition). Onze
eigen nieuwe crates gebruiken edition 2021 zoals in de bestaande workspace.
Rust 1.93 ondersteunt beide editions naast elkaar in een workspace. Dit
voorkomt onnodige code-aanpassingen in de geforkte crates.

## 3. pdf-font: drie crates samengevoegd als submodules

Issue A.2 specificeerde: "hayro-font + hayro-cmap + hayro-postscript → pdf-font".
Geïmplementeerd als:
- `pdf_font::postscript` — PostScript scanner (was hayro-postscript)
- `pdf_font::cmap` — CMap parser (was hayro-cmap)
- `pdf_font::font` — CFF/Type1 parser (was hayro-font), ook re-exported via `pub use font::*`

De `lib.rs` files zijn omgezet naar `mod.rs` en interne verwijzingen
(`hayro_postscript::X`) zijn vervangen door crate-interne paden (`crate::postscript::X`).

## 4. hayro image decoders: namen behouden

`hayro-jbig2`, `hayro-jpeg2000`, `hayro-ccitt` zijn behouden met hun originele
namen, conform de issue-specificatie ("as-is"). Ze worden door `pdf-syntax`
gebruikt als optionele dependencies.

## 5. Test suites van hayro: asset-tests genegeerd

De originele hayro crates hadden asset-gebaseerde test suites (`[[test]]`
sections) die een externe testcorpus vereisen (jbig2/jpeg2000 testbestanden).
Deze test-entries zijn verwijderd omdat we die corpus niet hebben. De 171
inline unit tests van `pdf-syntax` zijn behouden (2 tests die hayro-tests
corpus nodig hebben zijn gemarkeerd met `#[ignore]`).

## 6. XFA-migratie: wrapper-crate in plaats van fysieke verplaatsing

Issue A.4 suggereerde "xfa-dom-resolver → pdf-xfa/dom-resolver of als subcrate".
Gekozen voor een wrapper-crate `pdf-xfa` die de bestaande crates re-exporteert:
```rust
pub use xfa_dom_resolver as dom_resolver;
pub use formcalc_interpreter as formcalc;
pub use xfa_layout_engine as layout;
pub use xfa_json as json;
```
Dit voorkomt pad-wijzigingen in alle bestaande code en houdt de bestaande
crate-namen werkend. De fysieke directory-structuur is ongewijzigd.

## 7. CI/CD: multi-platform matrix toegevoegd

De bestaande CI draaide alleen op ubuntu-latest. Per issue A.5 is een matrix
toegevoegd voor Linux, macOS en Windows (check + test jobs). Clippy en fmt
draaien alleen op Linux (platformonafhankelijk). Een WASM build check is
toegevoegd voor `xfa-wasm`.

## 8. Trait interfaces: object-safe waar mogelijk

De vier traits (FormEngine, AnnotationRenderer, SignatureValidator,
ComplianceChecker) zijn ontworpen met `&self`/`&mut self` receivers en
gebruiken `dyn`-compatible parameter types. De enige uitzondering is
`SignatureValidator::sign_document` die `Box<dyn Error>` returnt.

## Resultaat

- **Crates:** 20+ workspace members (7 geforkt, 7 nieuw, 10 bestaand)
- **Tests:** 658 passed, 0 failed, 13 ignored
- **Clippy:** 0 warnings
- **Format:** clean
- **LOC toegevoegd:** ~69.000 (waarvan ~67.000 uit hayro-fork)
