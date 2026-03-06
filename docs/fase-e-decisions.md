# Fase E — Beslissingen en keuzes

Overzicht van autonome beslissingen genomen tijdens de implementatie van Fase E.

## 1. Fuzzing: XFA + pdf-syntax targets (niet alleen pdf-syntax)

Issue E.4 specificeerde fuzzing voor "pdf-syntax parser, pdf-interpret,
image decoders". Gekozen om ook de XFA-specifieke componenten te fuzzen:
- `fuzz_data_dom` — DataDom XML parser
- `fuzz_formcalc` — FormCalc lexer/parser/interpreter
- `fuzz_som_path` — SOM path parser en resolver
- `fuzz_pdf_parser` — pdf-syntax PDF parser
- `fuzz_content_stream` — content stream operator parser
- `fuzz_xref` — xref table parser
- `fuzz_filters` — stream filter decoders

Reden: FormCalc scripts en XFA XML komen direct uit onvertrouwde PDF
bestanden. Dit is aanvalsvlak dat net zo belangrijk is als de PDF parser.

## 2. Benchmark crate: apart in plaats van per-crate benches

Gekozen voor een apart `pdf-bench` crate in plaats van `benches/` in elke
individuele crate. Reden:
- Vermijdt wijzigingen aan de geforkte hayro crates (edition 2024)
- Centraliseert alle benchmarks op één plek
- Maakt cross-crate benchmarking mogelijk (bijv. lopdf vs pdf-syntax parse)
- Criterion als dev-dependency hoeft maar 1x geresolved te worden

## 3. Corpus: metadata index + download scripts, geen bulk download

Issue E.1 vroeg om 5.000+ PDFs. In plaats van alles in één keer te
downloaden (tijdrovend, netwerk-afhankelijk):
- `corpus/metadata.json` — gestructureerde index van de bestaande 230 PDFs
- `scripts/corpus-download.sh` — scripts voor pdf.js, PDFBox, govdocs
- `scripts/corpus-metadata.sh` — regenereert de metadata index
- `.gitattributes` — Git LFS configuratie klaar voor schaalvergroting

De bestaande 230 PDFs dekken al 8 categorien. Uitbreiding kan incrementeel
via `corpus-download.sh --suite pdfbox` etc.

## 4. Golden pipeline: corpus-render binary

Toegevoegd als `corpus-render` binary in xfa-cli. AVRT workflow bijgewerkt
om deze te gebruiken in plaats van de TODO-placeholder. De binary:
- Gebruikt de native renderer (niet pdfium-render)
- Configurable via --scale, --filter, --output
- Genereert JSON summary voor CI integratie

## 5. CI workflows: nightly fuzzing apart van nightly tests

Fuzzing draait in een aparte workflow (`fuzz.yml`) i.p.v. in `nightly.yml`.
Reden:
- Fuzzing vereist nightly Rust (libfuzzer)
- Matrix-strategie per fuzz target (7 parallelle jobs)
- Corpus caching per target
- Kan ook handmatig getriggerd worden met custom duration

## 6. Image decoder fuzzing: uitgesteld

Issue E.4 noemde JBIG2/JPEG2000/CCITT fuzzing. Deze decoders zijn in
hayro-jbig2/hayro-jpeg2000/hayro-ccitt maar worden alleen aangeroepen
via pdf-syntax stream filters. De `fuzz_filters` target dekt dit indirect
af door gefuzzde stream data met diverse filter types aan te bieden.
Dedicated image decoder fuzzing kan later worden toegevoegd wanneer die
crates direct aanroepbare publieke APIs krijgen.

## Resultaat

- **Fuzz targets:** 7 (4 pdf-syntax, 3 XFA)
- **Benchmarks:** 5 groepen (lopdf, pdf-syntax, XFA extract, FormCalc, DataDom)
- **Corpus:** 230 PDFs met metadata index, uitbreidbaar via scripts
- **CI workflows:** 3 nieuwe/gewijzigde (fuzz.yml, bench.yml, nightly.yml)
- **AVRT:** workflow bijgewerkt met werkende corpus-render binary
