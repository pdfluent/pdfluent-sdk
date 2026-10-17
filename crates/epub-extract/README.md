# epub-extract

EPUB → structured blocks extraction.

This crate is part of [PDFluent](https://pdfluent.com), a pure-Rust PDF SDK. Source: <https://github.com/pdfluent/pdfluent-sdk>.

**AGPL-3.0 or a commercial licence**, at your option — see the Licence section below.

## What it does

Reads an `.epub` file (a ZIP): `container.xml`, then the package document's
spine order, then each XHTML chapter. Chapters become structured blocks in
spine order: `h1`–`h6` headings with levels, `p` paragraphs, `li` list items
with nesting, `td`/`th` table cells with `colspan`/`rowspan` as spans, `a`
links, `strong`/`em` as inline Markdown, and `img` image references. Every
block carries its chapter; there are no page numbers.

## Status

MVP. Headings, paragraphs, nested lists, tables with spans, links and images
are extracted; the navigation document, fixed-layout metadata and embedded
fonts are out of scope.

## Usage

Most users do not depend on this crate directly. For low-level access:

```rust
let bytes = std::fs::read("book.epub").unwrap();
let blocks = epub_extract::extract_epub(&bytes).unwrap();
```

For the engine's Markdown and JSON serializers instead, map the same book
onto `pdf-engine` blocks (spine index as the page):

```rust
let bytes = std::fs::read("book.epub").unwrap();
let markdown = epub_extract::extract_markdown(&bytes).unwrap();
```

For the full SDK, see <https://pdfluent.com/docs>.

## Licence

AGPL-3.0 or a commercial licence, at your option. The AGPL is the default and
the complete product: there is no licence key, no activation call and no tier,
and every feature works in every build. If you cannot accept the copyleft
obligation, the commercial licence is sold yearly and self-service at
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing), in four options: Commercial (per
organisation), OEM Startup and OEM (per product), and Priority support (an
add-on). The two texts are `LICENSE` and `LICENSE-COMMERCIAL` in this crate.

## Links

- Main crate: <https://crates.io/crates/pdfluent>
- Documentation: <https://pdfluent.com/docs>
- Pricing: <https://pdfluent.com/sdk/pricing>
