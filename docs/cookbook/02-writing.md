# Writing Recipes

## Create a New PDF

```rust
use pdfluent::{Sdk, PageSize};
use pdfluent::document::DocumentBuilder;

let sdk = Sdk::init_with_license("license.json")?;

let doc = DocumentBuilder::new()
    .page_size(PageSize::A4)
    .add_text("Hello, PDFluent!", 72.0, 720.0)
    .add_text("This is a new PDF.", 72.0, 680.0)
    .build()?;

doc.save("hello.pdf")?;
println!("Created hello.pdf");
```

---

## Add Pages to PDF

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let mut doc = sdk.open("existing.pdf")?;

// Add a blank page
doc.add_page(pdfluent::PageSize::A4)?;

// Add page from another document
let mut other = sdk.open("source.pdf")?;
doc.append_pages(&other, &[0, 1, 2])?; // pages 0, 1, 2

doc.save("expanded.pdf")?;
```

---

## Remove Pages from PDF

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let mut doc = sdk.open("multi_page.pdf")?;

// Remove page 2 (0-indexed)
doc.remove_page(2)?;

// Remove pages 5 through 7
doc.remove_pages(5..=7)?;

doc.save("trimmed.pdf")?;
```

---

## Merge Multiple PDFs

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;

let mut merged = sdk.create_empty()?;

// Merge in order
for file in ["cover.pdf", "chapter1.pdf", "chapter2.pdf"] {
    let doc = sdk.open(file)?;
    merged.append_document(&doc)?;
}

merged.save("book.pdf")?;
println!("Merged 3 PDFs into book.pdf");
```

---

## Split a PDF

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("big_document.pdf")?;

let total_pages = doc.page_count();

// Split into chapters of 10 pages
for (i, chunk) in doc.pages().chunks(10).enumerate() {
    let mut new_doc = sdk.create_empty()?;
    for page in chunk {
        new_doc.append_page(&doc, page.number)?;
    }
    new_doc.save(format!("chapter_{}.pdf", i + 1))?;
}

println!("Split into {} files", (total_pages + 9) / 10);
```
