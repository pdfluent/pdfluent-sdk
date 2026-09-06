# API Reference

`pdfluent` exposes a single owning `PdfDocument` facade plus a small set of
focused helper types. This page documents the current public surface in this
branch and calls out the places where the facade exists in name but is not yet
fully wired.

## 1. Open, Create, And Save Documents

The document lifecycle starts with `PdfDocument` constructors:

- `PdfDocument::open(path)`
- `PdfDocument::open_with(path, OpenOptions)`
- `PdfDocument::from_bytes(bytes)`
- `PdfDocument::from_bytes_with(bytes, OpenOptions)`
- `PdfDocument::from_reader(reader)`
- `PdfDocument::create()`

The persistence surface is:

- `PdfDocument::save(path)`
- `PdfDocument::save_with(path, SaveOptions)`
- `PdfDocument::to_bytes()`
- `PdfDocument::write_to(writer)`

`SaveOptions` currently exposes:

- `SaveOptions::new()`
- `SaveOptions::with_linearize(bool)` — accepted, but currently a no-op in
  this branch
- `SaveOptions::with_overwrite(bool)` — opt in to overwriting an existing path

```rust,no_run
use pdfluent::prelude::*;

fn roundtrip(bytes: &[u8]) -> Result<()> {
    let doc = PdfDocument::from_bytes(bytes)?;
    let reopened = PdfDocument::open_with(
        "input.pdf",
        OpenOptions::new().with_repair(true),
    )?;

    assert!(doc.page_count() >= 1);
    assert!(reopened.page_count() >= 1);

    doc.save_with(
        "output.pdf",
        SaveOptions::new()
            .with_overwrite(true)
            .with_linearize(true),
    )?;

    let copy = doc.to_bytes()?;
    let _again = PdfDocument::from_bytes(&copy)?;
    Ok(())
}
```

## 2. Metadata Read And Write

`PdfDocument::metadata()` returns a `Metadata` snapshot with title, author,
subject, keywords, producer, creator, creation date, and modification date.

`PdfDocument::metadata_mut()` returns `MetadataMut`, which exposes:

- `MetadataMut::set_title(...)`
- `MetadataMut::set_author(...)`
- `MetadataMut::set_subject(...)`
- `MetadataMut::set_keywords(...)`
- `MetadataMut::commit()`

`MetadataMut` is chainable and auto-commits on drop, but explicit `commit()`
is the safer path because it surfaces errors. The write goes into the document
metadata dictionary; if you need `metadata()` to reflect the new values on the
same handle, save and reopen the document.

```rust,no_run
use pdfluent::prelude::*;

fn rewrite_metadata() -> Result<()> {
    let mut doc = PdfDocument::open("report.pdf")?;

    doc.metadata_mut()
        .set_title("Quarterly Report")
        .set_author("Finance")
        .set_subject("Q2 close")
        .set_keywords(&["quarterly", "finance", "internal"])
        .commit()?;

    doc.save_with(
        "report-updated.pdf",
        SaveOptions::new().with_overwrite(true),
    )?;
    Ok(())
}
```

## 3. Pages, Text, And Page-Level Helpers

The read surface for document structure and text includes:

- `PdfDocument::page_count()`
- `PdfDocument::version()`
- `PdfDocument::text()`
- `PdfDocument::text_with_layout()`
- `PdfDocument::page(page_number)`
- `PdfDocument::pages()`

The current page-level helpers also include:

- `PdfDocument::rotate_page(page, rotation)`
- `PdfDocument::split_pages()`
- `PdfDocument::extract_pages(range)`

Per-page access uses the borrowed `Page` handle:

- `Page::number()`
- `Page::dimensions()`
- `Page::text()`

```rust,no_run
use pdfluent::prelude::*;

fn inspect_pages() -> Result<()> {
    let doc = PdfDocument::open("input.pdf")?;

    println!("PDF version {}", doc.version());
    println!("pages: {}", doc.page_count());

    for page in doc.pages() {
        let (width, height) = page.dimensions();
        println!("page {}: {} x {}", page.number(), width, height);
        println!("{}", page.text()?);
    }

    let first_ten = doc.extract_pages(1..=10)?;
    first_ten.save_with(
        "chapter-1.pdf",
        SaveOptions::new().with_overwrite(true),
    )?;
    Ok(())
}
```

## 4. Forms

Read-only form access is available today:

- `PdfDocument::form_fields() -> Result<Vec<FormField>>`
- `FormField` exposes `name`, `field_type`, `value`, `required`, and
  `read_only`
- `FieldType` distinguishes text, checkbox, radio, dropdown, signature, and
  related field kinds

```rust,no_run
use pdfluent::prelude::*;

fn list_fields() -> Result<()> {
    let doc = PdfDocument::open("form.pdf")?;

    for field in doc.form_fields()? {
        println!(
            "{} {:?} value={:?} required={} read_only={}",
            field.name,
            field.field_type,
            field.value,
            field.required,
            field.read_only,
        );
    }

    Ok(())
}
```

The write surface is fully implemented:

- `PdfDocument::form_mut()` → `PdfFormMut`
- `PdfFormMut::set_text(name, value)` — text fields; hierarchical names
  (`"parent.child"`) resolved through `/Kids` recursion
- `PdfFormMut::set_checkbox(name, checked)`
- `PdfFormMut::set_radio(name, on_state)` — pass the export-value name
- `PdfFormMut::set_dropdown(name, option)` — pass the option value

Every write updates `/V`, per-widget `/AS`, and regenerates the `/AP`
appearance stream in one call — no `/NeedAppearances` required. Read-only
fields return an error at set-time.

- `PdfDocument::form_model() -> Result<Vec<FormFieldModel>>` — inspect field
  types, current/default values, widget rectangles, and kind-specific metadata
  (comb, multiline, options, on-states) before writing.
- `PdfDocument::regenerate_form_appearances()` — materialise appearance streams
  for PDFs filled by tools that only wrote `/V`.
- `PdfDocument::flatten_forms()` — convert interactive fields to static content.

```rust,no_run
use pdfluent::prelude::*;

fn fill_form() -> Result<()> {
    let mut doc = PdfDocument::open("form.pdf")?;

    // Inspect before writing
    for field in doc.form_model()? {
        println!("[{:?}] {} = {:?}", field.kind, field.name, field.value);
    }

    {
        let mut form = doc.form_mut();
        form.set_text("Address.Street", "123 Main St")?  // hierarchical name OK
            .set_checkbox("Agree", true)?
            .set_radio("Country", "NL")?
            .set_dropdown("Category", "Option2")?;
    }

    doc.save_with("filled.pdf", SaveOptions::new().with_overwrite(true))?;
    Ok(())
}
```

## 5. Redaction

Requires feature `redaction`.

The redaction surface includes:

- `PdfDocument::redact(text, RedactOptions)`
- `PdfDocument::redact_region(page, rect)`
- `RedactOptions::new()`
- `RedactOptions::case_sensitive(bool)`
- `RedactOptions::regex(bool)`
- `RedactOptions::on_pages(&[usize])`

Both redaction calls mutate the document in memory. Save afterwards to persist
the new bytes.

```rust,no_run
use pdfluent::prelude::*;

fn apply_redaction() -> Result<()> {
    let mut doc = PdfDocument::open("input.pdf")?;

    doc.redact(
        "SSN",
        RedactOptions::new()
            .case_sensitive(false)
            .on_pages(&[1, 2]),
    )?;
    doc.redact_region(1, [72.0, 72.0, 216.0, 108.0])?;

    doc.save_with(
        "redacted.pdf",
        SaveOptions::new().with_overwrite(true),
    )?;
    Ok(())
}
```

## 6. Signatures And PAdES Profiles

Requires feature `signing`.

Signing helpers:

- `Pkcs12Signer::from_pfx_file(...)`
- `Pkcs12Signer::from_pfx_bytes(...)`
- `PdfDocument::sign(...)`
- `PdfDocument::signatures()`
- `PdfDocument::verify_signatures()`

`SignOptions` exposes:

- `SignOptions::new()`
- `SignOptions::reason(...)`
- `SignOptions::location(...)`
- `SignOptions::contact_info(...)`
- `SignOptions::field_name(...)`
- `SignOptions::visible_rect(page, rect)`
- `SignOptions::profile(profile)`

`PadesProfile` maps to the common PAdES levels:

- `BasicSignature` = PAdES B-B
- `Timestamped` = PAdES B-T
- `LongTerm` = PAdES B-LT and is the current default
- `LongTermArchive` = PAdES B-LTA

```rust,no_run
use pdfluent::prelude::*;

fn sign_document() -> Result<()> {
    let mut doc = PdfDocument::open("contract.pdf")?;
    let signer = Pkcs12Signer::from_pfx_file("signer.p12", "secret")?;

    doc.sign(
        &signer,
        SignOptions::new()
            .reason("Approved")
            .location("Amsterdam")
            .field_name("Signature1")
            .profile(PadesProfile::LongTerm),
    )?;

    let report = doc.verify_signatures()?;
    assert!(report.is_signed());
    Ok(())
}
```

## 7. PDF/A Validation

Requires feature `pdfa`.

This branch exposes the PDF/A types:

- `PdfAProfile`
- `PdfAValidationReport`
- `Violation`

The missing piece is the actual facade entry point: there is no public
`PdfDocument` PDF/A validation method in the current branch. That is why this
page does not show a runnable `doc.validate_pdfa(...)` example: no such method
exists on the facade, and this reference does not invent APIs that are not
present.

```rust,no_run
use pdfluent::prelude::*;

fn profile_only() -> Result<()> {
    let profile = PdfAProfile::A2b;
    let _ = profile;
    Ok(())
}
```

## 8. Feature Gates

There is one gating layer, and it is a build-time one: Cargo features. The
current opt-in families are `signing`, `redaction`, `pdfa`, and several
additional export, OCR, and WASM-related flags. This crate enables `signing`,
`redaction`, and `pdfa` by default.

There is no second, runtime layer. Licence keys, tiers and capability checks
were removed in #226: nothing reads a key, nothing is withheld, and output is
never marked. A method that is compiled in is a method every caller may call.
