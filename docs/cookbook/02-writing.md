# Writing Recipes

> All snippets use the canonical, RFC 0001-frozen public API
> (`use pdfluent::prelude::*;` + `PdfDocument::open(...)`).

## Create a New (Empty) PDF

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    // Create an empty document and persist it.
    let doc = PdfDocument::create();
    doc.save("hello.pdf")?;
    println!("Created hello.pdf ({} page)", doc.page_count());
    Ok(())
}
```

For page-content authoring (text + graphics primitives), see the
lower-level `pdf-syntax` / `pdf-content-stream` crates. The
canonical `pdfluent` facade is optimised for **manipulating
existing PDFs** (the dominant enterprise use case); authoring is
on the public roadmap.

---

## Merge Multiple PDFs

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let cover = PdfDocument::open("cover.pdf")?;
    let chap1 = PdfDocument::open("chapter1.pdf")?;
    let chap2 = PdfDocument::open("chapter2.pdf")?;

    let merged = PdfMerger::new()
        .add(cover)
        .add(chap1)
        .add(chap2)
        .with_bookmarks(BookmarkMergeStrategy::Preserve)
        .build()?;

    merged.save("book.pdf")?;
    println!("Merged into book.pdf");
    Ok(())
}
```

---

## Split a PDF Into Per-Page Documents

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("big_document.pdf")?;
    let total_pages = doc.page_count();

    // split_pages() returns one PdfDocument per source page.
    for (idx, page_doc) in doc.split_pages()?.into_iter().enumerate() {
        page_doc.save(format!("page_{:03}.pdf", idx))?;
    }
    println!("Split into {} files", total_pages);
    Ok(())
}
```

---

## Rotate a Page and Save

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("scan.pdf")?;
    doc.rotate_page(0, Rotation::Clockwise90)?;
    doc.save("scan-rotated.pdf")?;
    Ok(())
}
```

---

## Stream-Compress an Existing PDF

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("heavy.pdf")?;
    let report = doc.compress(CompressOptions::default())?;
    doc.save("heavy-compressed.pdf")?;
    println!(
        "Compressed: {} streams compressed, {} deduplicated, \
         {} unused objects removed",
        report.streams_compressed,
        report.streams_deduplicated,
        report.unused_removed
    );
    Ok(())
}
```

---

## Add a Diagonal Text Watermark

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("contract.pdf")?;
    doc.add_watermark(
        "CONFIDENTIAL",
        WatermarkOptions::centered()
            .rotated(45.0)
            .layer(Layer::Foreground)
            .opacity(0.25)
            .font_size(48.0),
    )?;
    doc.save("contract-marked.pdf")?;
    Ok(())
}
```
