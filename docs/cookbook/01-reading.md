# Reading Recipes

## Extract Text from PDF

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("document.pdf")?;

// Extract all text
let text = doc.extract_text()?;
println!("{}", text);

// Per-page with bounding boxes
for page in doc.pages() {
    for block in page.extract_text_blocks()? {
        println!("[p{} {:.0},{:.0}] {}",
            block.page, block.x, block.y, block.text);
    }
}
```

**Output:**
```
[p1 72,720] Invoice #12345
[p1 72,680] Date: 2026-04-02
[p1 72,640] Total: €199.99
```

---

## Extract Images from PDF

```rust
use pdfluent::Sdk;
use std::fs::write;

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("document_with_images.pdf")?;

for (idx, image) in doc.images().enumerate() {
    let ext = match image.format {
        pdfluent::ImageFormat::JPEG => "jpg",
        pdfluent::ImageFormat::PNG => "png",
        pdfluent::ImageFormat::TIFF => "tiff",
        _ => "bin",
    };
    let path = format!("image_{}.{}", idx, ext);
    write(&path, &image.data)?;
    println!("Saved: {}", path);
}
```

---

## Extract Tables from PDF

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("invoice.pdf")?;

let tables = doc.extract_tables(0)?; // page 0
for table in tables {
    println!("Table found at ({:.0}, {:.0})", table.x, table.y);
    for row in table.rows {
        println!("  | {} |", row.join(" | "));
    }
}
```

---

## Get PDF Metadata

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("document.pdf")?;

let meta = doc.metadata()?;
println!("Title: {}", meta.title.unwrap_or_default());
println!("Author: {}", meta.author.unwrap_or_default());
println!("Pages: {}", doc.page_count());
println!("PDF Version: {}", meta.pdf_version);
```

---

## List All Pages

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("document.pdf")?;

for page in doc.pages() {
    println!("Page {}: {:.0}x{:.0} pts",
        page.number,
        page.width,
        page.height);
}
```

**Output:**
```
Page 1: 612x792 pts (Letter)
Page 2: 612x792 pts (Letter)
Page 3: 595x842 pts (A4)
```
