# Extracting Data and Tables

PDFluent provides high-level text extraction with precise coordinates, enabling you to build complex data and table extraction logic. This tutorial covers how to work with structured text blocks.

## Structured Text Blocks

To get more than just plain text, use the `structured_text` method. This returns a vector of `TextBlock` structs, each containing both the text content and its bounding box (coordinates).

```rust
use pdf_engine::api;

fn main() -> Result<(), api::Error> {
    let doc = api::read("invoice.pdf")?;

    // Get all structured text blocks in the document
    let blocks = doc.structured_text();

    for block in blocks {
        println!("Text: {}", block.text);
        println!("Bounding Box: {:?}", block.bbox); // [x, y, width, height]
    }

    Ok(())
}
```

## Extracting a Table

Extracting a table involves grouping text blocks based on their vertical and horizontal positions.

```rust
// Basic example of filtering text blocks by a specific vertical region (Y-coordinate)
let table_top = 100.0;
let table_bottom = 200.0;

let table_rows: Vec<_> = blocks.iter()
    .filter(|b| b.bbox[1] >= table_top && b.bbox[1] <= table_bottom)
    .collect();

for row in table_rows {
    println!("Row data: {}", row.text);
}
```

## Advanced Extraction

For even more control, you can access specific pages and perform text extraction at the page level.

```rust
let page = doc.page(1);
let page_text = page.text();
```

## Summary

In this tutorial, you learned how to:
1. Extract structured text with bounding box coordinates.
2. Group text blocks by their position.
3. Access specific pages for targeted data extraction.
