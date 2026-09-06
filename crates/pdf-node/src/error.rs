//! Error conversion from Rust engine errors to napi errors.
//!
//! ## Wire format
//!
//! The Node binding uses napi-rs `Error<Status>` for the on-the-wire error
//! type. NAPI's `Status` enum is fixed, so there is no native NAPI surface
//! for a stable, machine-readable error `code` distinct from the napi
//! status name.
//!
//! To deliver a typed error shape on the JavaScript side, the structured
//! error payload is serialised as a JSON object in the `reason` string and
//! recovered in the `index.js` wrapper, which constructs a `PdfluentError`
//! subclass with `.code`, `.message`, `.operation`, and `.cause` attached as
//! own properties.
//!
//! Wire format:
//!
//! ```json
//! {
//!   "code": "E-PARSE-INVALID-PDF",
//!   "message": "operation failed",
//!   "operation": "open",
//!   "cause": "xref table is corrupt at byte 4096"
//! }
//! ```
//!
//! All four fields are always present (`cause` may be `null`).  The `code`
//! string matches the C8 error catalogue at `docs/error_catalogue.md` —
//! see `pdfluent::Error::code()` for the canonical source.
//!
//! Consumers must NOT pattern-match on `message`.  Use `.code` instead.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use napi::Status;

/// Build the structured JSON payload that the JavaScript wrapper unmarshals
/// into a `PdfluentError`.
///
/// `operation` is a short verb describing what the caller was attempting
/// (e.g. `"open"`, `"save"`, `"flatten"`).
///
/// `cause` is an optional, safe-to-expose underlying message.  It must NOT
/// contain passwords or any other sensitive data.
///
/// Unused since the licence surface was removed (#226): that surface was the
/// only caller. It stays because the JS wrapper and `docs/error_catalogue.md`
/// both describe this shape as the Node error contract, and the next surface
/// to adopt a typed code needs the encoder to exist rather than to be
/// reinvented one call-site at a time.
#[allow(dead_code)]
pub fn structured_error(
    code: &str,
    message: &str,
    operation: &str,
    cause: Option<&str>,
) -> napi::Error {
    let payload = serde_json::json!({
        "code": code,
        "message": message,
        "operation": operation,
        "cause": cause,
    });
    napi::Error::new(Status::GenericFailure, payload.to_string())
}

/// Convert a `pdf-engine` error into a (non-structured) napi error.
///
/// Existing call-sites use this to preserve historical behaviour.  Newer
/// surfaces should prefer [`structured_error`] so JavaScript callers can
/// branch on `.code`.
pub fn to_napi_error(err: pdf_engine::EngineError) -> napi::Error {
    napi::Error::new(Status::GenericFailure, format!("{err}"))
}
