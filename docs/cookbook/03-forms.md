# Forms Recipes

> All snippets use the canonical, RFC 0001-frozen public API
> (`use pdfluent::prelude::*;` + `PdfDocument::open(...)`).

## Fill AcroForm Fields

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("form.pdf")?;

    {
        // Pending-changes pattern: a single mutable borrow that
        // holds the form open while we set fields, dropped at the
        // end of this scope.
        let mut form = doc.form_mut();
        form.set_text("name",   "John Doe")?
            .set_text("email",  "john@example.com")?
            .set_text("amount", "199.99")?
            .set_checkbox("paid", true)?;
    }

    doc.save("form_filled.pdf")?;
    Ok(())
}
```

`PdfFormMut` exposes typed setters per field kind:
`set_text`, `set_checkbox`, `set_radio`, `set_dropdown`. The chain
returns `&mut Self` so multiple setters compose naturally.

---

## Read Form Fields

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

`FormField { name, field_type: FieldType, value: String }`.
`FieldType` covers text, checkbox, radio, dropdown, list-box,
signature, and button.

---

## Flatten All Forms (AcroForm + XFA)

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("form.pdf")?;

    // Fill any values first, then flatten:
    {
        let mut form = doc.form_mut();
        form.set_text("name", "John Doe")?;
    }

    doc.flatten_forms()?;
    doc.save("form_flat.pdf")?;
    Ok(())
}
```

`flatten_forms()` finalises field appearances and removes the
interactive form dictionary so the output is a fixed PDF.

For XFA-specific extraction (`xfa-json` round-trip) and FormCalc
evaluation, see the lower-level workspace crates
`xfa-json`, `xfa-dom-resolver`, and `formcalc-interpreter`.
The high-level `pdfluent` facade exposes the common case
(fill + flatten); XFA-specific scripting hooks remain in the
specialised crates.
