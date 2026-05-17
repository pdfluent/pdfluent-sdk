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
//!   "code": "E-LICENSE-INVALID",
//!   "message": "license key is invalid",
//!   "operation": "activate",
//!   "cause": "unknown tier \"platinum\"; expected trial/developer/team/business/enterprise"
//! }
//! ```
//!
//! All four fields are always present (`cause` may be `null`).  The `code`
//! string matches the C8 error catalogue at `docs/error_catalogue.md` —
//! see `pdfluent::Error::code()` for the canonical source.
//!
//! Consumers must NOT pattern-match on `message`.  Use `.code` instead.

use napi::Status;

/// Build the structured JSON payload that the JavaScript wrapper unmarshals
/// into a `PdfluentError`.
///
/// `operation` is a short verb describing what the caller was attempting
/// (e.g. `"activate"`, `"status"`, `"open"`).
///
/// `cause` is an optional, safe-to-expose underlying message.  It must NOT
/// contain license-key material, passwords, or any other sensitive data.
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

/// Convert a [`pdfluent::Error`] (license activation, capability checks) into
/// a structured napi error whose JSON payload carries a stable `.code`
/// drawn from [`pdfluent::Error::code`].
pub fn pdfluent_err_to_napi(err: pdfluent::Error, operation: &str) -> napi::Error {
    let code = err.code();
    // Top-level message is intentionally short and stable per code; the
    // detailed human-readable form goes in `cause` so callers that want it
    // can render it but typed callers branch on `code` instead.
    let (message, cause): (&'static str, String) = match &err {
        pdfluent::Error::InvalidLicense { reason } => {
            ("license key is invalid", reason.clone())
        }
        pdfluent::Error::FeatureNotInTier {
            capability,
            current_tier,
            required_tier,
        } => (
            "feature not available in current license tier",
            format!(
                "{capability:?} requires {required_tier:?}; current tier is {current_tier:?}"
            ),
        ),
        pdfluent::Error::CapabilityNotCompiled {
            capability,
            feature_flag,
        } => (
            "capability not compiled into this build",
            format!("{capability:?} requires Cargo feature {feature_flag:?}"),
        ),
        other => ("operation failed", other.to_string()),
    };
    structured_error(code, message, operation, Some(&cause))
}

/// Convert a `pdf-engine` error into a (non-structured) napi error.
///
/// Existing call-sites use this to preserve historical behaviour.  Newer
/// surfaces (e.g. the license activation entry points) should prefer
/// [`structured_error`] / [`pdfluent_err_to_napi`] so JavaScript callers can
/// branch on `.code`.
pub fn to_napi_error(err: pdf_engine::EngineError) -> napi::Error {
    napi::Error::new(Status::GenericFailure, format!("{err}"))
}
