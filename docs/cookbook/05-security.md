# Security Recipes

## Sign a PDF with PAdES

```rust
use pdfluent::{Sdk, Signer, SignatureOptions};

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("contract.pdf")?;

let signer = Signer::from_pkcs12("cert.p12", "password")?;

let signed = doc.sign(signer, SignatureOptions {
    page: 0,
    reason: "Approved by legal department".into(),
    location: "Amsterdam, Netherlands".into(),
    timestamp_url: Some("http://timestamp.sectigo.com".into()),
    ..Default::default()
})?;

signed.save("contract_signed.pdf")?;
```

---

## Verify a Signature

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("signed_document.pdf")?;

let result = doc.verify_signatures().pop();

match result {
    Some(sig) => {
        if sig.is_valid() {
            println!("Signature is valid");
            println!("  Signed by: {}", sig.signer());
            println!("  At: {}", sig.timestamp().unwrap_or_default());
        } else {
            println!("INVALID SIGNATURE: {}", sig.error());
        }
    }
    None => println!("No signatures found"),
}
```

---

## Encrypt a PDF

```rust
use pdfluent::{Sdk, EncryptionOptions, Permission};

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("document.pdf")?;

let encryption = EncryptionOptions::new()
    .user_password("user123")
    .owner_password("owner456")
    .permissions(Permission::PRINT | Permission::EXTRACT)
    .algorithm(pdfluent::EncryptionAlgorithm::AES256)?;

let encrypted = doc.encrypt(encryption)?;
encrypted.save("document_encrypted.pdf")?;
```

---

## Decrypt a PDF

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open_with_password("document_encrypted.pdf", "user123")?;

doc.save("document_decrypted.pdf")?;
println!("PDF decrypted successfully");
```
