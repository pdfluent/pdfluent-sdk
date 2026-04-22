# render-pdf-to-png-rust — content sync

**URL:** <https://pdfluent.com/how-to/render-pdf-to-png-rust>
**Status:** WEBSITE_PROMISES_UNSUPPORTED_BEHAVIOR
**SDK pin:** master `e891ffb0d`

## Current published snippet

```rust
use pdfluent::{PdfDocument, RenderOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PdfDocument::open("document.pdf")?;
    let page = doc.page(0)?;

    let opts = RenderOptions::default().dpi(150);
    let image = page.render(&opts)?;
    image.save_png("page_1.png")?;

    println!("{}x{} pixels", image.width, image.height);
    Ok(())
}
```

## Problems

1. **`pdfluent::RenderOptions` doesn't exist.** The similarly-named
   `RenderOptions` lives in `pdf_engine` and is an internal type.
   The public render-options type is `ToImagesOptions`.
2. **Per-page `page.render()` / `image.save_png()` don't exist.**
   The public API is document-level: `PdfDocument::to_images(pattern, opts)`.
3. **`doc.page(0)` is 0-indexed.** SDK is 1-based: `doc.page(1)`.

## Canonical SDK-truth snippet (single page)

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("document.pdf")?;

    // Render page 1 (1-based indexing, per RFC §1) at 150 DPI.
    let report = doc.to_images(
        "page_{page}.png",
        ToImagesOptions::new()
            .with_dpi(150)
            .with_format(ImageFormat::Png)
            .with_pages(1, 1),
    )?;

    println!("wrote {}", report.paths[0].display());
    Ok(())
}
```

## Alternative canonical snippet (whole document)

If the tutorial's goal is "render every page", this is the cleaner
shape:

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("document.pdf")?;

    let report = doc.to_images(
        "page_{page}.png",
        ToImagesOptions::new().with_dpi(150),
    )?;

    println!("rendered {} pages", report.paths.len());
    Ok(())
}
```

## Related prose updates

- Any paragraph talking about per-page render calls → rewrite to
  say "the SDK renders in bulk via `to_images`; the `pattern`
  string supports `{page}` substitution for per-page filenames".
- Any claim about width/height on the returned image → remove.
  The SDK reports written file paths, not pixel dimensions; DPI
  plus page size controls resolution.
- Available formats: `ImageFormat::Png` and `ImageFormat::Jpeg`.
  CMYK export is not on the 1.0 roadmap.

## Platform notes to add

- **wasm32:** `to_images` is native-only. On wasm32 targets it
  returns `Error::UnsupportedOnWasm`. See [WASM_SUPPORT.md §2.5](../../../WASM_SUPPORT.md).
  If the article claims wasm support, add the caveat.

## What stays the same

- Intro.
- SEO title, meta.
- Closing "see also" pointing at the docx / compress how-tos.
