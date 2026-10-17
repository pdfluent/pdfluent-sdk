# Security Recipes

> All snippets use the canonical, RFC 0001-frozen public API
> (`use pdfluent::prelude::*;` + `PdfDocument::open(...)`).
> Signing requires the `sign` capability and a PKCS#12 identity.

## Sign a PDF (PAdES B-B)

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
            .field_name("Signature1"),
    )?;

    doc.save("contract_signed.pdf")?;
    Ok(())
}
```

`SignOptions::new()` is PAdES B-B: a signature with no timestamp and no
validation material.

## Sign for the long term (PAdES B-LT)

A contract that has to survive the signing certificate's expiry needs B-LT: a
trusted timestamp, and a document security store holding the certificate chain
so a verifier does not have to fetch it years later.

The timestamp comes from a timestamp authority over HTTP, and the engine has no
HTTP client of its own, which is what keeps document content off the network.
So the request is yours to make — but it has to be made in the right order. An
authority stamps a *signature*, so the signature must exist first. `sign_timestamped`
does the two steps in that order and hands your callback the bytes to have
stamped:

```rust,ignore
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("contract.pdf")?;
    let signer = Pkcs12Signer::from_pfx_file("cert.p12", "password")?;

    doc.sign_timestamped(
        &signer,
        SignOptions::new()
            .profile(PadesProfile::LongTerm)
            .reason("Approved by legal department")
            // B-LT is the profile a verifier can check offline years later, and
            // that needs the proof the certificates were good, not just which
            // they were. Without it LongTerm is refused rather than written.
            .revocation_data(ocsp_responses, crls),
        // Your own request to your own timestamp authority, over the signature
        // that was just made. Return the DER-encoded RFC 3161 token.
        |signature| fetch_timestamp_token(signature),
    )?;

    doc.save("contract_signed.pdf")?;
    Ok(())
}
```

Use `sign` for B-T or B-LT and it fails, and so does a token that stamps
anything other than this signature — including one fetched before signing, which
cannot be about a signature that did not exist yet. That check is the whole
difference between a B-LT claim and a B-LT signature: the token travels into the
CMS as opaque bytes, so a document with the timestamp attribute in the right
place and the wrong imprint looks correct until the first verifier recomputes it.

Signing without a timestamp at all fails too. It does not fall back to B-B: a
signature that claims long-term validity while carrying no validation material
is a claim that only breaks years later, when nobody is looking.

`PadesProfile::LongTermArchive` (B-LTA) is not implemented. It needs an archive
timestamp written over the whole file, and asking for it returns an error that
says so.

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
