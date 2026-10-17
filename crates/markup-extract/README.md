# markup-extract

HTML, Markdown and plain-text extraction onto the `pdf-engine` block model.

```rust
let html = std::fs::read_to_string("report.html").unwrap();
let markdown = markup_extract::html::extract_markdown(&html);
```

Headings, paragraphs, lists, tables, links, inline formatting, code blocks
and image references become `pdf_engine::{Block, BlockKind}`, so markup
serializes through the engine's Markdown like PDFs do. Markup has no pages:
every block carries `page: 0`.

See the crate documentation for the full per-format mapping and the known
limitations (flattened nested tables in cells, skipped `script`/`style`).

- Documentation: <https://pdfluent.com/docs>
- Pricing: <https://pdfluent.com/sdk/pricing>
