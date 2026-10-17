# pdfluent-sign

PDF digital signatures — PAdES B-B, B-T and B-LT, CMS / PKCS#7, certificate-chain validation, DocMDP / FieldMDP, LTV.

This crate is part of [PDFluent](https://pdfluent.com), a pure-Rust PDF SDK. Source: <https://github.com/pdfluent/pdfluent-sdk>.

**AGPL-3.0 or a commercial licence**, at your option — see the Licence section below.

## What it does

Signs and validates PDFs to PAdES baseline profiles, and validates signatures of any profile it can parse.

| Profile | Signing | What goes in the file |
|---|---|---|
| B-B | yes | Detached CMS/PKCS#7 under SubFilter `ETSI.CAdES.detached` |
| B-T | yes | B-B plus an RFC 3161 timestamp token as an unsigned CMS attribute |
| B-LT | yes | B-T plus a Document Security Store carrying the certificate chain |
| B-LTA | **no** | Needs a document timestamp (`/Type /DocTimeStamp`, SubFilter `ETSI.RFC3161`) over the whole file. Not implemented; asking for it is an error |

B-T and B-LT are signed in two phases, because an authority stamps a signature and the signature has to exist first: `sign_pdf_pending_timestamp` signs and hands back a `PendingTimestamp`, whose `signature_value()` is what the RFC 3161 request covers, and `timestamped()` / `long_term()` embed the token that comes back. A token whose message imprint is not the digest of that signature is refused rather than embedded — including one obtained before signing, which cannot be about a signature that did not exist yet.

Fetching a token means an HTTP request to a timestamp authority, and the default build has no HTTP client anywhere in the signing path; enable the `tsa` feature for `sign_pdf_ltv`, which does the fetch for you. `sign_pdf_timestamped` and `sign_pdf_ltv_with_token` take a token you already hold, and apply the same imprint check.

Also supports incremental update signing, signature appearances, and DocMDP enforcement.

## Status

Beta. The sign and verify round trip is covered by the crate's own test suite, and an appended revision is checked against qpdf rather than against our own reader.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade with the `signing` feature (enabled by default):

```rust
use pdfluent::prelude::*;
```

For low-level access, see <https://pdfluent.com/docs>.

## Licence

AGPL-3.0 or a commercial licence, at your option. The AGPL is the default and
the complete product: there is no licence key, no activation call and no tier,
and every feature works in every build. If you cannot accept the copyleft
obligation, the commercial licence is sold yearly and self-service at
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing), in four options: Commercial (per
organisation), OEM Startup and OEM (per product), and Priority support (an
add-on). The two texts are `LICENSE` and `LICENSE-COMMERCIAL` in this crate.

## Links

- Main crate: <https://crates.io/crates/pdfluent>
- Documentation: <https://pdfluent.com/docs>
- Pricing: <https://pdfluent.com/sdk/pricing>
