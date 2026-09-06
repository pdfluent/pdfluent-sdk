<!--
Copyright (c) 2026 Innovation Trigger B.V.

PDFluent is available under two licences, at your option: the GNU AGPLv3, or
the PDFluent Commercial Licence. See the LICENSE file in this repository --
that file travels with the copy you received, which a URL does not.
-->

# The public API of `pdfluent`

This says what the crate's public surface is and, more usefully, what it is not.
It exists because on 25 August 2026 pdfluent.com documented a different API from
the one we ship — not a few wrong names, a different shape — and there was
nothing that would have caught it. Anyone writing an example, a README or a page
of marketing copy should read this first.

`crates/pdfluent/tests/the_api_contract_holds.rs` is this document as code: it
compiles the paths promised below, so a re-export dropped by accident turns a
test red instead of the website.

## One entry point

```rust
let mut doc = PdfDocument::open("invoice.pdf")?;
```

`PdfDocument` is the API. It carries around ninety methods: `extract_text`,
`find_text`, `replace_text`, `validate_pdfa`, `convert_to_pdfa`, `sign`,
`verify_signatures`, `redact`, `encrypt`, `attach_zugferd_xml`, `to_docx`,
`to_images`, `annotations`, `outlines`, `compress`, `add_watermark`, `save`.

There is no `Sdk`, no session object, no handle to construct first. The site
described `Sdk::new()?` followed by `sdk.open(path)`, and that was never real —
but the more important point is that it should not become real. It is two steps
where one is needed, and the extra step carries nothing: no configuration, no
lifetime, no shared cache that `PdfDocument` does not already own.

Per-call configuration goes in an options struct, not in a session:

```rust
let doc = PdfDocument::open_with("scan.pdf", OpenOptions::new())?;
```

## What a module is for

A module exists for something that is not a document.

| module | why it is not a method |
|---|---|
| `error` | `Error`, `Result` — the vocabulary every signature uses |
| `prelude` | the import line, so examples stay one line |
| `xfa` | the XFA form model is its own tree, not a property of the page |
| `form` | AcroForm fields, same reason |
| `structure` | the outline and tag tree |
| `compliance` / `pdfa` | the profiles and the report type; the *verb* is `doc.validate_pdfa()` |
| `signer` | keys and certificates exist before any document does |
| `merger` | takes many documents and returns one, so it cannot hang off one |
| `annotation` | the types `doc.annotations()` returns |
| `ocr` | the backend trait; a backend exists without a document |
| `parity`, `diagnostics` | about the engine, not about a file |

That is the test. **If the answer needs a document, it is a method.** The site
invented `pdfluent::color`, `pdfluent::content`, `pdfluent::digest`,
`pdfluent::nup`, `pdfluent::stamp` and `pdfluent::text` — every one of those
describes something you do *to a document*, so every one of them is a method or
should be. That rule alone rules out six of the site's twelve invented module
paths.

## The re-exports this contract adds

Three real gaps, none of them new implementation:

- **`pdfluent::ocr`** — the OCR trait and its error types live in
  `pdf_engine::ocr`, and `pdf-engine` is already a dependency. Re-exporting the
  trait costs a line and pulls in nothing: the facade wires up no backend, so a
  plain `pdfluent` dependency still drags in no system libraries and downloads
  no models (decision of 19-08-2026).
- **`pdfluent::annotation`** — `PdfDocument::annotations()` already returned
  `AnnotationInfo`; the type had no name in the facade, so an example could not
  spell out what it got back.
- **`pdfluent::pdfa`** — an alias for `compliance`. Our own documentation says
  `pdfa` throughout and it is the better name for what it holds; `compliance`
  stays, because removing it would break every caller.

## What this contract refuses

`HtmlToPdf`, `NUpLayout`, `TrustStore`, `PeppolClient`, `DependencyGraph` and
`pdfluent::portfolio` are not paths that need fixing. They are capabilities that
do not exist. HTML→PDF is a standing decision not to build: point people at
headless Chrome, PDFluent does everything after that.

Adding a re-export because a page of copy mentions it is how this happened.

## How this is enforced

- `crates/pdfluent/tests/the_api_contract_holds.rs` compiles the promises above.
  Remove the `ocr` re-export and it stops compiling.
- `scripts/ci/export_public_api.py` writes `docs/PUBLIC_API.json` — modules,
  re-exported names, every public type in the workspace, and the promises list.
  It fails on `--check` when the committed copy has drifted from the code.
- pdfluent.com checks every Rust snippet against that file as the first step of
  its own build, so an example naming something that does not exist does not
  reach a deploy.
- `crates/pdfluent/examples/site_snippets.rs` holds the site's examples as code
  that compiles in CI. A name check cannot tell you that
  `PdfDocument::open("x")?.ocr()` will not build; a compiler can.
- `scripts/ci/site_blocks_compile.py` builds every Rust block the site carries,
  not only the ones in `site_snippets.rs`. A block either compiles or it is
  named in the register with one of four reasons why it cannot. `api-missing` is
  deliberately not one of the four: a block naming an API that does not exist is
  the thing being measured, and moving it into the register would be moving the
  goalposts.
