# Document Manipulation and Creation

PDFluent allows you to create new document versions by manipulating existing ones. This tutorial covers merging, splitting, rotating, and watermarking.

## Merging Documents

You can merge two or more documents into a single PDF.

```rust
use pdf_engine::api;

fn main() -> Result<(), api::Error> {
    let doc1 = api::read("document1.pdf")?;
    let doc2 = api::read("document2.pdf")?;

    // Merges doc2 into doc1
    doc1.merge(&doc2)?;

    doc1.save("merged.pdf")?;
    Ok(())
}
```

## Splitting Pages

To extract individual pages from a document, use the `split_pages` method. This returns a vector of single-page `Document` handles.

```rust
let pages = doc1.split_pages()?;
println!("Document split into {} individual pages", pages.len());

// Save the first page as a separate file
pages[0].save("page_1.pdf")?;
```

## Rotating Pages

Pages can be rotated by a specific angle (e.g., 90, 180, 270 degrees).

```rust
// Rotate the first page by 90 degrees clockwise
doc1.rotate_page(1, 90)?;
```

## Adding a Watermark

PDFluent provides a high-level API for adding text watermarks across all pages.

```rust
use pdf_engine::api::WatermarkOptions;

let options = WatermarkOptions {
    opacity: 0.5,
    rotation: 45.0,
};

doc1.add_watermark("CONFIDENTIAL", options)?;
doc1.save("watermarked.pdf")?;
```

## Summary

In this tutorial, you learned how to:
1. Merge multiple PDFs.
2. Split a PDF into individual pages.
3. Rotate specific pages.
4. Add transparent watermarks.
