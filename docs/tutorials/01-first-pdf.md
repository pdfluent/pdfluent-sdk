# Your First PDF with PDFluent

Welcome to PDFluent! This tutorial will guide you through the basics of loading a PDF document and extracting its text.

## Introduction

PDFluent follows the "Zero-Config First Success" design principle. You can start working with PDFs with just a few lines of code.

## Loading a PDF

The simplest way to load a PDF is using the `read` function. It accepts a file path or a byte slice.

```rust
use pdf_engine::api;

fn main() -> Result<(), api::Error> {
    // Load a PDF from a file path
    let doc = api::read("example.pdf")?;

    // Get the total number of pages
    let count = doc.page_count();
    println!("Document has {} pages", count);

    Ok(())
}
```

## Extracting Text

You can extract all the text from a document or from a specific page.

```rust
// Extract all text from the entire document
let full_text = doc.text();
println!("Full text length: {}", full_text.len());

// Access a specific page (1-based index) and extract its text
let first_page = doc.page(1);
let page_text = first_page.text();
println!("First page text: {}", page_text);
```

## Saving the Document

To save your document to a new file:

```rust
doc.save("output.pdf")?;
```

## Summary

In this tutorial, you learned how to:
1. Load a PDF from a path.
2. Get the page count.
3. Extract plain text from the entire document or a single page.
4. Save the document.
