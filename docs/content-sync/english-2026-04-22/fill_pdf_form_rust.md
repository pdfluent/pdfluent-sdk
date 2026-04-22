# fill-pdf-form-rust — content sync

**URL:** <https://pdfluent.com/how-to/fill-pdf-form-rust>
**Status:** WEBSITE_OLDER_THAN_SDK
**SDK pin:** master `e891ffb0d`

## Current published snippet

```rust
use pdfluent::PdfDocument;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = PdfDocument::open("application.pdf")?;
    let mut form = doc.form_mut()?;

    form.set_text("first_name", "Jane")?;
    form.set_text("last_name", "Smith")?;
    form.set_checkbox("agree_terms", true)?;
    form.set_dropdown("country", "Netherlands")?;

    doc.save("application_filled.pdf")?;
    Ok(())
}
```

## Problems

1. **`doc.form_mut()?`** — RFC v1.1 removed the `Result` wrapper
   (see RFC §14 revision log). The accessor is infallible;
   field-not-found errors surface on the individual `set_*` calls.
2. All four setter methods are correct and live on master.

## Canonical SDK-truth snippet

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("application.pdf")?;

    {
        let mut form = doc.form_mut();
        form.set_text("first_name", "Jane")?
            .set_text("last_name", "Smith")?
            .set_checkbox("agree_terms", true)?
            .set_dropdown("country", "Netherlands")?;
    }

    doc.save("application_filled.pdf")?;
    Ok(())
}
```

Two cosmetic improvements vs a minimal rewrite:

- The inner `{ ... }` scope drops the `PdfFormMut` borrow before
  `doc.save()`, which lets `save` take `&self`.
- Chained setters via `?.` use the `Result<&mut Self>` return shape
  for a single `?` per chain ending.

## Related prose updates

- Any paragraph claiming `form_mut()` can fail → remove. It doesn't.
- If the article mentions hierarchical field names like
  `Address.Street`: note the 1.0 scope caveat from `form.rs`
  rustdoc — **only top-level `/AcroForm/Fields` are addressable
  in 1.0**. Nested fields via `/Kids` are tracked for 1.1.

## Truth-gap to keep visible

- **Checkbox / radio kid-widget appearance `/AS` sync.** SDK 1.0
  writes `/V` correctly, but does **not** update per-kid `/AS`
  entries. Viewers that honour `/AS` may show stale visual state
  after filling. Registered in STABILITY.md §3.3 as 1.1 follow-up
  (#1264 P1 audit items). If the article has screenshots of a
  filled form, they should note that visual confirmation may
  require opening in a viewer that rebuilds appearance streams
  from `/V`.

## What stays the same

- Everything else about the article.
