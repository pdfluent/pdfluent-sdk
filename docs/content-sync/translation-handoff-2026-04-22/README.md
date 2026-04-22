# Translation handoff — 2026-04-22

**Target:** Minimax translation pass.
**Source of truth:** English update package at
[`../english-2026-04-22/`](../english-2026-04-22/).
**Prerequisite:** the English website updates in that package must be
**applied and verified live** before translations begin. Do not
translate content that still needs to be corrected upstream.

## Scope

Five how-to pages, one solution page category, plus the shared
error-reference header. See `sources/` for the per-file source
bundle Minimax consumes.

| File | Language status | Estimated words | Priority |
|---|---|---|---|
| `encrypt_pdf_rust.md` | EN source ready | ~650 | P0 |
| `extract_text_pdf_rust.md` | EN source ready | ~480 | P0 |
| `fill_pdf_form_rust.md` | EN source ready | ~700 | P0 |
| `merge_pdfs_rust.md` | EN source ready | ~580 | P0 |
| `render_pdf_to_png_rust.md` | EN source ready | ~640 | P0 |
| `stability-truth-gaps.md` (shared header) | EN source ready | ~420 | P0 |

Total: ~3,470 English source words per target language.

## Target languages

Confirm with maintainer before handoff — the initial set per the
MEMORY.md PDFluent deploy pipeline is:

| Code | Language | QA required |
|---|---|---|
| `nl` | Dutch | Light (native speaker review) |
| `de` | German | Light |
| `fr` | French | Light |
| `es` | Spanish | Light |
| `it` | Italian | Light |
| `pt` | Portuguese (pt-PT) | Light |
| `ja` | Japanese | **Heavy** (technical terminology in CJK is higher-risk) |
| `zh` | Chinese (simplified) | **Heavy** |

If the maintainer has a shorter launch list, cut from the bottom
(keep NL/DE/FR/ES as the European core).

## Do-not-translate list

These tokens MUST appear verbatim in every translation. Minimax
must preserve case and surrounding punctuation.

### Product + brand names
- `PDFluent`
- `pdfluent`
- `pdfluent.com`
- `Anthropic` (where cited)
- `ISO 32000-1`, `ISO 32000-2` (standards tags)

### Rust identifiers (types, traits, functions, modules)
Every symbol exported from `pdfluent::*`. Representative list (not
exhaustive — any identifier in a code block is a do-not-translate):

- `PdfDocument`, `PdfMerger`, `PdfFormMut`, `PdfAProfile`, `PdfVersion`
- `OpenOptions`, `SaveOptions`, `EncryptOptions`, `MergeOptions`,
  `SignOptions`, `RedactOptions`, `CompressOptions`,
  `ToImagesOptions`, `WatermarkOptions`
- `Permissions`, `EncryptionAlgorithm`, `PageDecoration`,
  `ImageFormat`, `InsertImageFormat`, `ImageInsert`, `FormField`,
  `FieldType`, `TextBlock`, `Metadata`, `MetadataMut`,
  `PadesProfile`, `SignatureInfo`, `SignatureValidation`,
  `SignatureValidationReport`, `SignatureStatus`, `LicenseInfo`
- `Tier`, `Capability`, `CapabilitySet`, `Error`, `Result`
- `Rotation`, `Position`, `Layer`, `BookmarkMergeStrategy`
- Every method name: `open`, `open_with`, `from_bytes`,
  `from_bytes_with`, `from_reader`, `create`, `save`, `save_with`,
  `to_bytes`, `write_to`, `page_count`, `version`, `text`,
  `text_with_layout`, `page`, `pages`, `metadata`,
  `metadata_mut`, `form_fields`, `form_mut`, `flatten_forms`,
  `add_decoration`, `add_watermark`, `rotate_page`,
  `split_pages`, `extract_pages`, `encrypt`, `decrypt`, `sign`,
  `signatures`, `verify_signatures`, `redact`, `redact_region`,
  `to_docx`, `to_images`, `compress`, `linearize`, `subset_fonts`,
  `embed_font`, `insert_image`, `set_text`, `set_checkbox`,
  `set_radio`, `set_dropdown`, `set_title`, `set_author`,
  `commit`

### Error codes
Every `E-…` code from STABILITY.md §8. Representative:
- `E-SECURITY-DECRYPTION-FAILED`
- `E-ENV-MISSING-DEPENDENCY`
- `E-ENV-UNSUPPORTED-ON-WASM`
- `E-INTERNAL`
- `E-IO-FILE-NOT-FOUND`
- `E-LICENSE-FEATURE-NOT-IN-TIER`
- `E-LICENSE-INVALID`
- `E-MEMORY-BUDGET-EXCEEDED`
- (etc. — the full list is in `sources/error-codes-glossary.md`)

### File-format / protocol names
- `PDF`, `PDF/A`, `PDF/UA`, `DOCX`, `PNG`, `JPEG`, `WebP`
- `AES-128`, `AES-256`, `RC4-128`
- `AcroForm`, `XFA`
- `PAdES`, `B-B`, `B-T`, `LT`, `LTA` (signature profiles)
- `UTF-8`, `UTF-16BE`, `PDFDocEncoding`
- `RGBA`, `RGB`, `CMYK`
- `ZUGFeRD`, `Factur-X`, `XRechnung`

### Code blocks (entire)
Anything between ` ```rust ` and ` ``` `, or ` ```bash `, or
`<pre>`. **Never translate inside code blocks.** Comments inside
Rust code blocks MAY be translated only if the surrounding doc
explicitly says so; default is DO NOT translate.

### CLI fragments
- `cargo run`, `cargo test`, `cargo bench`, `cargo fmt`,
  `cargo clippy`, `cargo doc`, `cargo publish`, `cargo yank`
- `RUST_LOG=…`, `PDFLUENT_LICENSE_KEY=…`
- `wasm32-unknown-unknown`

## Terminology glossary

Prefer these translations consistently. If Minimax proposes an
alternative, flag in the QA pass.

| English | nl | de | fr | es | Notes |
|---|---|---|---|---|---|
| document | document | Dokument | document | documento | |
| page | pagina | Seite | page | página | |
| form field | formulierveld | Formularfeld | champ de formulaire | campo de formulario | |
| watermark | watermerk | Wasserzeichen | filigrane | marca de agua | |
| signature | handtekening | Signatur | signature | firma | digital signature context |
| permissions | rechten | Berechtigungen | autorisations | permisos | |
| license / licensing | licentie | Lizenz | licence | licencia | |
| tier | tier | Tier | palier | nivel | keep English `tier` in nl (product-established) |
| capability | capability | Capability | capability | capability | keep English (product-established) |
| truth-gap | waarheidsgat | Wahrheitslücke | lacune de vérité | brecha de verdad | rare term; prefer parenthetical English |
| deferred | uitgesteld | aufgeschoben | différé | diferido | |
| stub | stub | Stub | stub | stub | keep English |
| panic (Rust sense) | paniek | Panic | panic | panic | keep English in parens |
| render | renderen | rendern | rendre | renderizar | |
| extract (text) | extraheren | extrahieren | extraire | extraer | |
| encrypt / decrypt | versleutelen / ontsleutelen | verschlüsseln / entschlüsseln | chiffrer / déchiffrer | cifrar / descifrar | |
| fallback | fallback | Fallback | solution de repli | alternativa | keep English in nl/de |
| drift guard | drift guard | Drift-Guard | garde de dérive | guardia de deriva | keep English |

For `ja` and `zh`: Minimax picks idiomatic translation; native
reviewer in QA checks terminology.

## Frontmatter + metadata rules

If a page file has YAML frontmatter (title, description, slug, ogTitle, ogDescription, seoTitle, seoDescription):

1. **Translate** `title`, `description`, `ogTitle`, `ogDescription`, `seoTitle`, `seoDescription`.
2. **Do NOT translate** `slug`, `date`, `lang` (will be rewritten per-language by the build), `canonical`, `author`.
3. Preserve YAML syntax exactly. No trailing whitespace, no quote-style changes.

## Placeholders

Any `{{ variable }}` or `{variable}` pattern in text is a
templating placeholder. Leave it verbatim.

## QA checklist (per language, post-translation)

Run before integration:

- [ ] All do-not-translate tokens preserved (grep for `PDFluent`,
      `pdfluent::`, error codes, product names).
- [ ] Code blocks unchanged (diff-check fenced blocks against English).
- [ ] Frontmatter YAML valid (pyyaml parse).
- [ ] No untranslated English paragraphs accidentally left in.
- [ ] No translated Rust identifiers in code blocks.
- [ ] Terminology glossary applied consistently (spot-check 5
      terms per language).
- [ ] Character encoding is UTF-8 without BOM.
- [ ] Length check: no language should be >2× or <0.5× English word
      count (catches catastrophic mistranslations).

CJK-specific (ja, zh):
- [ ] Full-width / half-width punctuation consistent within each file.
- [ ] No stray Japanese katakana for words that should be kanji (or vice versa).

## Delivery format to Cloud/Claude

Return one directory per language, mirroring `sources/`:

```
translation-handoff-2026-04-22/
├── README.md         # this file
├── sources/          # English source (input)
│   ├── encrypt_pdf_rust.md
│   ├── …
├── nl/               # Dutch translation (output)
│   ├── encrypt_pdf_rust.md
│   ├── …
├── de/               # German
├── fr/
├── es/
├── it/
├── pt/
├── ja/
└── zh/
```

Name the per-language subdirs using the ISO 639-1 code.

## Minimax-suitability marker

**Minimax-suitable:** all 6 source files. Low semantic risk
because:
- Snippets are code (not translated)
- Prose is how-to / reference (no rhetorical nuance)
- Terminology is finite and glossary-bounded
- QA checklist is mechanical

**Requires light human QA:** terminology consistency + CJK
character-set check. Estimate 10-15 min per language.

## Handoff status

- **Prerequisite (website update applied live):** PENDING — FASE C
  produced the content package, maintainer must apply upstream
  first.
- **Handoff files:** STAGED in this repo under
  `docs/content-sync/translation-handoff-2026-04-22/`.
- **Minimax can start once:** the English source pages on
  pdfluent.com are refreshed from the canonical package and the
  drift-guard cache is re-committed (closing the loop).

Do NOT start translations on stale English content.
