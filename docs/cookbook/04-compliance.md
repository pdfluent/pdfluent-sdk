# Compliance Recipes

> All snippets use the canonical, RFC 0001-frozen public API
> (`use pdfluent::prelude::*;` + `PdfDocument::open(...)`). PDF/A
> validation is gated behind the `pdfa` feature (enabled by default
> in the `pdfluent` crate).

## Validate PDF/A Compliance

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("invoice.pdf")?;
    let report = doc.validate_pdfa(PdfAProfile::A2b)?;

    if report.is_compliant() {
        println!("PDF/A-2B compliant ✓");
    } else {
        println!("{} violation(s):", report.violations.len());
        for v in &report.violations {
            println!("  [{:?}] {} — {}", v.severity, v.rule, v.message);
        }
    }
    Ok(())
}
```

Supported profiles: `PdfAProfile::{A1b, A2b, A3b}`.

`Violation { rule: String, message: String, severity: Severity }`.
`Severity::{Error, Warning}` — info-level findings are filtered out
of the report.

---

## Inspect Embedded Signatures

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("signed.pdf")?;

    for sig in doc.signatures()? {
        println!(
            "- field={} signer={} profile={:?}",
            sig.field_name, sig.signer_name, sig.profile
        );
    }

    let report = doc.verify_signatures()?;
    if report.is_signed() && report.all_valid() {
        println!("All {} signatures pass.", report.validations().len());
    } else {
        for v in report.validations() {
            println!(
                "{}: {:?}",
                v.info.field_name, v.status
            );
        }
    }
    Ok(())
}
```

`SignatureValidationReport::all_valid()` is **vacuously true** for
an unsigned document; combine with `is_signed()` if presence is
required.

---

## Read Document Metadata for Audit

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("evidence.pdf")?;
    let meta = doc.metadata();

    println!("Title:     {}", meta.title.as_deref().unwrap_or(""));
    println!("Author:    {}", meta.author.as_deref().unwrap_or(""));
    println!("Producer:  {}", meta.producer.as_deref().unwrap_or(""));
    println!("Created:   {}", meta.creation_date.as_deref().unwrap_or(""));
    println!("Modified:  {}", meta.modification_date.as_deref().unwrap_or(""));
    Ok(())
}
```

The `producer` field is automatically marked by the SDK in
[`Tier::Trial`](https://pdfluent.com/pricing) mode; activate
a commercial license via [`set_license_key`](https://pdfluent.com/docs)
to remove the trial mark.

---

## Strict Open for Audit / E-Invoicing Flows

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open_with(
        "incoming.pdf",
        OpenOptions::new().with_repair(false),
    )?;

    // Continue with validate_pdfa, metadata audit, signature
    // verification, etc.
    let _ = doc.validate_pdfa(PdfAProfile::A3b)?;
    Ok(())
}
```
