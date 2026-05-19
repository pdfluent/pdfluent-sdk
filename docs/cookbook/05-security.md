# Security Recipes

> All snippets use the canonical, RFC 0001-frozen public API
> (`use pdfluent::prelude::*;` + `PdfDocument::open(...)`).
> Signing requires the `sign` capability and a PKCS#12 identity.

## Sign a PDF with PAdES B-LT (long-term validation)

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("contract.pdf")?;

    let signer = Pkcs12Signer::from_pfx_file("cert.p12", "password")?;

    doc.sign(
        &signer,
        SignOptions::new()
            .reason("Approved by legal department")
            .location("Amsterdam, Netherlands")
            .field_name("Signature1")
            .profile(PadesProfile::LongTerm),
    )?;

    doc.save("contract_signed.pdf")?;
    Ok(())
}
```

`PadesProfile::{Baseline, ShortTerm, LongTerm}` — pick LongTerm
for archival contracts that need to survive certificate expiry.

---

## Verify Signatures

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("signed_document.pdf")?;
    let report = doc.verify_signatures()?;

    if !report.is_signed() {
        println!("Document is not signed.");
        return Ok(());
    }

    for v in report.validations() {
        match &v.status {
            SignatureStatus::Valid => {
                println!("✓ {} — valid", v.info.field_name);
            }
            SignatureStatus::Invalid { reason } => {
                println!("✗ {} — invalid: {}", v.info.field_name, reason);
            }
            SignatureStatus::Unknown { reason } => {
                println!("? {} — unknown: {}", v.info.field_name, reason);
            }
        }
    }
    Ok(())
}
```

`SignatureValidationReport::all_valid()` is **vacuously true** for
an unsigned document; pair with `is_signed()` if presence matters.

---

## Encrypt a PDF (AES-256, print-only)

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("document.pdf")?;

    doc.encrypt(
        EncryptOptions::aes256()
            .with_user_password("user123")
            .with_owner_password("owner456")
            .with_permissions(Permissions::print_only()),
    )?;
    doc.save("document_encrypted.pdf")?;
    Ok(())
}
```

Permission presets: `Permissions::{full_access, print_only,
read_only}`. The full builder lets you tune individual flags
(print, modify, copy, annotate, fill_forms, extract_accessibility,
assemble, print_high_quality).

---

## Decrypt a PDF

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    // Either open with the password directly:
    let mut doc = PdfDocument::open_with(
        "document_encrypted.pdf",
        OpenOptions::new().with_password("user123"),
    )?;

    // Or decrypt an already-open document in place:
    doc.decrypt("user123")?;

    doc.save("document_decrypted.pdf")?;
    Ok(())
}
```

---

## Redact a Search Term Across the Document

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("invoice.pdf")?;

    // Redact every occurrence of "IBAN: NL91 ABNA …" — GDPR-safe
    // permanent removal, not just visual covering.
    doc.redact(
        "IBAN: NL",
        RedactOptions::new().case_sensitive(false),
    )?;
    doc.save("invoice-redacted.pdf")?;
    Ok(())
}
```

For region-based redaction (rectangle on a specific page), use
`PdfDocument::redact_region(page, [x0, y0, x1, y1])`.
