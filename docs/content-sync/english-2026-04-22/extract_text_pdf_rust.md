# extract-text-pdf-rust — content sync

**URL:** <https://pdfluent.com/how-to/extract-text-pdf-rust>
**Status:** NEEDS_ENGLISH_UPDATE
**SDK pin:** master `e891ffb0d`

## Current published snippet

```rust
use pdfluent::PdfDocument;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PdfDocument::open("document.pdf")?;

    for (i, page) in doc.pages().enumerate() {
        let text = page.extract_text()?;
        println!("--- Page {} ---", i + 1);
        println!("{}", text);
    }
    Ok(())
}
```

## Problems

1. **`page.extract_text()` is not the SDK API.** SDK method is `Page::text()`.

Everything else compiles.

## Canonical SDK-truth snippet

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("document.pdf")?;

    for (i, page) in doc.pages().enumerate() {
        let text = page.text()?;
        println!("--- Page {} ---", i + 1);
        println!("{}", text);
    }
    Ok(())
}
```

## If the article also documents structured extraction

The SDK exposes **document-level** structured text via
`PdfDocument::text_with_layout() -> Result<Vec<TextBlock>>` (RFC §1
per-method contract; v1.1 renamed it from `structured_text`). There
is no per-page structured extractor today — that's a 1.1 follow-up.

If the website has a second snippet showing per-page layout
extraction, replace it with the document-level variant:

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("document.pdf")?;
    for block in doc.text_with_layout()? {
        println!(
            "[page {}] [{:.1},{:.1}] {:?}",
            block.page, block.x, block.y, block.text,
        );
    }
    Ok(())
}
```

## Related prose updates

- The page may claim "per-page structured text via
  `page.structured_text()`". Remove — the SDK doesn't ship that.
  Mention `text_with_layout()` at the document level instead.

## What stays the same

- SEO title, meta.
- Intro paragraphs.
- Closing "see also" section.
