# merge-pdfs-rust — content sync

**URL:** <https://pdfluent.com/how-to/merge-pdfs-rust>
**Status:** WEBSITE_PROMISES_UNSUPPORTED_BEHAVIOR
**SDK pin:** master `e891ffb0d`

## Current published snippet

```rust
use pdfluent::PdfMerger;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PdfMerger::new()
        .add_file("part1.pdf")?
        .add_file("part2.pdf")?
        .add_file("part3.pdf")?
        .merge("combined.pdf")?;

    println!("Merged {} pages into combined.pdf", output.page_count);
    Ok(())
}
```

## Problems

1. **`.add_file("…")?`** — SDK's `add_file` returns `&mut Self` (not
   `Result`); no `?` needed.
2. **`.merge("combined.pdf")`** — SDK's merge takes `MergeOptions`
   (not a path). It returns a `PdfDocument` you save separately.
3. **`output.page_count` field access** — SDK's page count is the
   `PdfDocument::page_count()` method; there is no bare field.

## Canonical SDK-truth snippet

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let combined = PdfMerger::new()
        .add_file("part1.pdf")
        .add_file("part2.pdf")
        .add_file("part3.pdf")
        .merge(MergeOptions::new())?;

    combined.save("combined.pdf")?;

    println!(
        "Merged {} pages into combined.pdf",
        combined.page_count(),
    );
    Ok(())
}
```

## Related prose updates

- The article may claim merging writes to disk inline. Clarify:
  **merging produces a `PdfDocument`; saving is a separate step.**
  This mirrors RFC §1.2 lifecycle (construct → modify → save).
- If there's a section on bookmark handling, point at
  `MergeOptions::with_bookmark_strategy(BookmarkMergeStrategy::Concat)`
  which is the documented default as of RFC v1.1.

## What stays the same

- Intro ("why merge").
- SEO title, meta description.
- Closing "see also" links.
