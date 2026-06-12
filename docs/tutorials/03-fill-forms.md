# Working with Interactive Forms

PDFluent makes it easy to work with PDF AcroForms. You can list form fields,
fill them with values, and flatten them for final production.

## Listing Form Fields

To see what fields are available in a PDF, use the `form_fields` method.
It returns a `Result<Vec<FormField>>`.

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("form.pdf")?;

    for field in doc.form_fields()? {
        println!(
            "Field Name: {}, Type: {:?}, Value: {}",
            field.name, field.field_type, field.value,
        );
    }

    Ok(())
}
```

## Filling a Form

Mutate the document via `form_mut()` and the four `set_*` setters on
`PdfFormMut`. The setters return `Result<&mut Self>`, so they chain:

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("form.pdf")?;

    {
        let mut form = doc.form_mut();
        form.set_text("first_name", "Jasper")?
            .set_text("last_name", "de Winter")?
            .set_checkbox("agree_terms", true)?
            .set_radio("preferred_contact", "Email")?
            .set_dropdown("country", "NL")?;
    }

    doc.save("form_filled.pdf")?;
    Ok(())
}
```

Setting a field that does not exist, or passing the wrong setter for a
field's type, returns an error — so `?` bubbles misuse out of the chain.

Hierarchical field names like `Address.Street` are resolved through `/Kids`
recursion — pass the fully-qualified name directly to any setter.

The writeback chain keeps `/V`, per-widget `/AS`, and `/AP` appearance streams
consistent in one call. No `/NeedAppearances` round-trip is required; the fill
is visible in every conforming viewer immediately.

## Flattening Forms

Once a form is filled you can *flatten* it — converting interactive
fields into static page content, preventing further edits and ensuring
consistent rendering across viewers.

```rust
doc.flatten_forms()?;
doc.save("final_filled_form.pdf")?;
```

> `flatten_forms` converts all interactive fields to static page content.

## Summary

In this tutorial you learned how to:

1. List the interactive form fields in a document via `form_fields()`.
2. Fill fields with data via `form_mut().set_text/set_checkbox/set_radio/set_dropdown`.
3. Flatten filled fields into static content via `flatten_forms()`.
