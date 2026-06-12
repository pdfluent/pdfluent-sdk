# fill-pdf-form-rust — content sync

**URL:** <https://pdfluent.com/how-to/fill-pdf-form-rust>
**Status:** NEEDS_WEBSITE_UPDATE
**SDK pin:** acroform/sdk-foundation

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
2. All four setter methods are correct.
3. **Missing radio-group example.** The article covers only text/checkbox/dropdown;
   radio groups use `set_radio("field_name", "on_state_name")`.
4. **Missing hierarchical-name example.** Fully-qualified names like
   `Address.Street` work directly — no special handling needed.

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
            .set_radio("preferred_contact", "Email")?   // radio group
            .set_dropdown("country", "Netherlands")?;
    }

    doc.save("application_filled.pdf")?;
    Ok(())
}
```

Two cosmetic improvements vs the published snippet:

- The inner `{ ... }` scope drops the `PdfFormMut` borrow before
  `doc.save()`, which lets `save` take `&self`.
- Chained setters via `?.` use the `Result<&mut Self>` return shape
  for a single `?` per chain ending.

## Related prose updates

- Any paragraph claiming `form_mut()` can fail → remove. It doesn't.
- Remove any claim that hierarchical names (`Address.Street`) are not
  supported. They are resolved through `/Kids` recursion in the current SDK.
- Remove any claim that `/AS` appearance-state entries are not updated.
  The writeback chain (acroform/sdk-foundation) keeps `/V`, per-widget `/AS`,
  and a regenerated `/AP` appearance stream in sync in one call.

## What stays the same

- Everything else about the article.
