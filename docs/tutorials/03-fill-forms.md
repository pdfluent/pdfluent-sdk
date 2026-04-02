# Working with Interactive Forms

PDFluent makes it easy to work with PDF forms (AcroForms or XFA). You can list form fields, fill them with values, and flatten them for final production.

## Listing Form Fields

To see what fields are available in a PDF, use the `form_fields` method. This returns a vector of `FormField` structs.

```rust
use pdf_engine::api;

fn main() -> Result<(), api::Error> {
    let doc = api::read("form.pdf")?;

    for field in doc.form_fields() {
        println!("Field Name: {}, Type: {}, Value: {}", 
            field.name, field.field_type, field.value);
    }

    Ok(())
}
```

## Filling a Form

Filling a form is as simple as providing a slice of name-value pairs.

```rust
// Fill form fields by name
doc.fill_form(&[
    ("first_name", "Jasper"),
    ("last_name", "de Winter"),
    ("email", "jasper@example.com"),
])?;
```

## Flattening Forms

Once you've filled a form, you might want to "flatten" it. Flattening converts interactive form fields into static content, making it impossible to edit the fields later and ensuring consistent display across all PDF viewers.

```rust
// Flatten the form, making the content static
doc.flatten_forms()?;

// Save the finalized document
doc.save("final_filled_form.pdf")?;
```

## Summary

In this tutorial, you learned how to:
1. List all interactive form fields in a document.
2. Fill fields with data.
3. Flatten form fields into static content.
