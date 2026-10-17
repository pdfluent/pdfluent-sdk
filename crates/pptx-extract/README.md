# pptx-extract

PowerPoint PPTX extraction onto the `pdf-engine` block model.

```rust
let pptx = std::fs::read("deck.pptx").unwrap();
let markdown = pptx_extract::extract_markdown(&pptx).unwrap();
```

Slides become heading-led block runs: title (or `Slide N`), text frames in
reading order, bullets as list items, `a:tbl` tables as cells, pictures as
image references, speaker notes as a trailing `Notes: ` paragraph. Slide
numbers travel as block metadata only (`Block::page`).

See the crate documentation for the full mapping and the known limitations
(partial SmartArt/chart coverage, no rendering).

- Documentation: <https://pdfluent.com/docs>
- Pricing: <https://pdfluent.com/sdk/pricing>
