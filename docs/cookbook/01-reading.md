# Reading Recipes

> All snippets use the canonical, RFC 0001-frozen public API
> (`use pdfluent::prelude::*;` + `PdfDocument::open(...)`). Nothing here
> needs a licence key: there is none (#226).

## Extract Text from PDF

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("document.pdf")?;

    // Whole-document text:
    let text = doc.extract_text()?;
    println!("{}", text);

    // Per-page text with layout. Each TextBlock has `text`, a
    // `bbox: [x_min, y_min, x_max, y_max]` in PDF points, and a
    // 0-based `page` index.
    for block in doc.text_with_layout()? {
        println!(
            "[p{} {:.0},{:.0}] {}",
            block.page, block.bbox[0], block.bbox[1], block.text
        );
    }
    Ok(())
}
```

**Output:**

```
[p0 72,720] Invoice #12345
[p0 72,680] Date: 2026-04-02
[p0 72,640] Total: EUR 199.99
```

---

## Render a Page to PNG

```rust
use pdfluent::prelude::*;
use std::fs::write;

fn main() -> Result<()> {
    let doc = PdfDocument::open("document.pdf")?;

    // Render page 0 at 150 DPI as PNG bytes:
    let png = doc.render_page(0, 150, ImageFormat::Png)?;
    write("page_0.png", &png)?;
    println!("Saved page_0.png ({} bytes)", png.len());
    Ok(())
}
```

For bulk rendering of every page to disk, use
[`PdfDocument::to_images`](https://pdfluent.com/docs).

---

## Get PDF Metadata

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("document.pdf")?;
    let meta = doc.metadata();

    println!("Title:   {}", meta.title.as_deref().unwrap_or("(none)"));
    println!("Author:  {}", meta.author.as_deref().unwrap_or("(none)"));
    println!("Pages:   {}", doc.page_count());
    println!("Version: {}", doc.version());
    Ok(())
}
```

---

## List All Pages

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("document.pdf")?;

    for (idx, page) in doc.pages().enumerate() {
        let (w, h) = page.dimensions();
        println!("Page {}: {:.0} x {:.0} pts", idx, w, h);
    }
    Ok(())
}
```

**Output:**

```
Page 0: 612 x 792 pts (Letter)
Page 1: 612 x 792 pts (Letter)
Page 2: 595 x 842 pts (A4)
```

---

## Read AcroForm Fields

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("form.pdf")?;

    for field in doc.form_fields()? {
        println!(
            "{:?}: {} = {:?}",
            field.field_type, field.name, field.value
        );
    }
    Ok(())
}
```
