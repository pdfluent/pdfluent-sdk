# Compliance Recipes

## Validate PDF/A Compliance

```rust
use pdfluent::{Sdk, PdfaLevel};

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("document.pdf")?;

let report = doc.validate_pdfa(PdfaLevel::A2b)?;

if report.is_compliant() {
    println!("Document is PDF/A-2b compliant");
} else {
    println!("Violations found: {}", report.violation_count());
    for v in report.violations() {
        eprintln!("[§{}] {}", v.clause, v.message);
    }
}
```

---

## Convert to PDF/A

```rust
use pdfluent::{Sdk, PdfaLevel};

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("legacy_invoice.pdf")?;

let archived = doc.convert_to_pdfa(PdfaLevel::A2b)?;
archived.save("invoice_pdfa.pdf")?;

println!("Converted to PDF/A-2b");
```

---

## Create ZUGFeRD Invoice

```rust
use pdfluent::{Sdk, ZugferdProfile};
use pdfluent::invoice::InvoiceBuilder;

let sdk = Sdk::init_with_license("license.json")?;

let invoice = InvoiceBuilder::new()
    .seller("Acme Corp", "BE0123456789")
    .buyer("Customer BV", "NL123456789B01")
    .line_item("Consulting", 1500.00, 21) // VAT %
    .line_item("Software license", 500.00, 21)
    .build()?;

let pdf = sdk.create_zugferd_invoice(&invoice, ZugferdProfile::Extended)?;
pdf.save("invoice_zugferd.pdf")?;
```

---

## Verify Digital Signature

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("signed_contract.pdf")?;

let signatures = doc.verify_signatures()?;

for sig in signatures {
    println!("Signer: {}", sig.signer());
    println!("  Valid: {}", sig.is_valid());
    println!("  Timestamp: {}", sig.timestamp().unwrap_or_default());
    println!("  Hash: {}", sig.certificate_hash());
}
```
