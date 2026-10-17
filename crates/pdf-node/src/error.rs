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
//! recovered by the typed-error layer (`scripts/build/typed-error-layer.js`,
//! appended to `index.js`), which throws a `PdfluentError` with `.code`,
//! `.message`, `.operation` and `.cause` attached as own properties.
//!
//! Wire format:
//!
//! ```json
//! {
//!   "code": "E-PARSE-INVALID-PDF",
//!   "message": "invalid PDF: xref table is corrupt at byte 4096",
//!   "operation": "open",
//!   "cause": null
//! }
//! ```
//!
//! All four fields are always present (`cause` may be `null`).  The `code`
//! string matches the C8 error catalogue at `docs/error_catalogue.md`, and it
//! is read from `pdfluent::Error::code()` rather than spelled out here, so the
//! binding cannot drift from the catalogue.
//!
//! Every fallible call in this crate goes through one of the constructors
//! below. Consumers must NOT pattern-match on `message`.  Use `.code` instead.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use napi::Status;
use pdf_engine::model::ModelErrorKind;
use pdfluent::Error as Catalogue;
use std::path::PathBuf;

/// Build the structured JSON payload that the JavaScript wrapper unmarshals
/// into a `PdfluentError`.
///
/// `operation` is the JavaScript name of the call that failed
/// (e.g. `"open"`, `"save"`, `"renderPage"`).
///
/// `cause` is an optional, safe-to-expose underlying message.  It must NOT
/// contain passwords or any other sensitive data.
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

fn raise(sample: Catalogue, operation: &str, message: impl Into<String>) -> napi::Error {
    structured_error(sample.code(), &message.into(), operation, None)
}

/// `E-IO-FILE-NOT-FOUND` when the OS said the path is not there, else
/// `E-IO-GENERIC`.
pub(crate) fn io(operation: &str, message: impl Into<String>, err: &std::io::Error) -> napi::Error {
    let sample = if err.kind() == std::io::ErrorKind::NotFound {
        Catalogue::FileNotFound {
            path: PathBuf::new(),
        }
    } else {
        Catalogue::Io {
            source: std::io::Error::new(err.kind(), ""),
            path: None,
        }
    };
    raise(sample, operation, message)
}

/// `E-PARSE-INVALID-PDF`.
pub(crate) fn invalid_pdf(operation: &str, message: impl Into<String>) -> napi::Error {
    let sample = Catalogue::InvalidPdf {
        byte_offset: None,
        reason: String::new(),
    };
    raise(sample, operation, message)
}

/// `E-PAGE-OUT-OF-RANGE`. The message keeps this binding's 0-based wording.
pub(crate) fn page_out_of_range(operation: &str, message: impl Into<String>) -> napi::Error {
    let sample = Catalogue::PageOutOfRange {
        requested: 0,
        page_count: 0,
    };
    raise(sample, operation, message)
}

/// `E-UNSUPPORTED`: a parameter or request this binding does not accept.
pub(crate) fn unsupported(operation: &str, message: impl Into<String>) -> napi::Error {
    raise(Catalogue::Unsupported(String::new()), operation, message)
}

/// `E-INTERNAL`: a fault in this crate, never a mistake in the call.
pub(crate) fn internal(operation: &str, message: impl Into<String>) -> napi::Error {
    let sample = Catalogue::Internal {
        message: String::new(),
        crate_version: "",
    };
    raise(sample, operation, message)
}

/// A worker thread that died is a fault here, not in the caller's input.
pub(crate) fn join(operation: &str) -> impl FnOnce(tokio::task::JoinError) -> napi::Error + '_ {
    move |e| internal(operation, format!("join error: {e}"))
}

/// Map an engine error to its catalogue code.
///
/// `pdfluent`'s own `From<EngineError>` folds everything it does not name into
/// `E-PARSE-INVALID-PDF`, including a page past the end and an OS error. Both
/// have their own code in the catalogue, so they are sorted out here first.
pub(crate) fn engine_with(
    operation: &str,
    message: impl Into<String>,
    e: pdf_engine::EngineError,
) -> napi::Error {
    use pdf_engine::EngineError as E;
    // An OptionsError rides inside EngineError::Io: checked first, before
    // the generic Io arm below, or a mistake in the call reads as E-IO.
    if e.options().is_some() {
        return unsupported(operation, message);
    }
    // OCR asked for and not runnable is not a broken file: it keeps the
    // `E-UNSUPPORTED` the facade mapping gives it. OCR failures ride as `Io`
    // carrying a typed cause, read through `ocr_message`.
    if let Some(ocr) = e.ocr_message() {
        return unsupported(operation, format!("OCR error: {ocr}"));
    }
    // The table-structure weights are missing: the stage's own install text
    // names a manual route, but the addon's users install through the
    // product command, so the message names it instead. The code below is
    // unchanged -- still the I/O error a missing file is.
    let message = tatr_install_message(&e).unwrap_or_else(|| message.into());
    match e {
        E::PageOutOfRange { .. } => page_out_of_range(operation, message),
        E::Io(ref source) => io(operation, message, source),
        E::RenderError(_) => internal(operation, message),
        other => raise(Catalogue::from(other), operation, message),
    }
}

/// The missing table-structure weights, in the install command's words.
///
/// `Some` only for a [`ModelErrorKind::ModelNotAvailable`] naming the
/// `tatr-v1.1-all` package: the path looked at plus the product's tables
/// installer. Anything else -- another model, a checksum mismatch, an OS
/// error -- is `None` and keeps the engine's own message.
fn tatr_install_message(e: &pdf_engine::EngineError) -> Option<String> {
    let model = e.model_error()?;
    if model.kind != ModelErrorKind::ModelNotAvailable
        || model.model.as_deref() != Some("tatr-v1.1-all")
    {
        return None;
    }
    let path = model
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<model dir>/tatr-v1.1-all/tatr-v1.1-all.onnx".to_string());
    Some(format!(
        "model 'tatr-v1.1-all' is not available: {path} was not found. \
         Install it with: pdfluent models install tables"
    ))
}

/// [`engine_with`], keeping the engine's own message.
pub(crate) fn engine(operation: &str) -> impl FnOnce(pdf_engine::EngineError) -> napi::Error + '_ {
    move |e| {
        let message = e.to_string();
        engine_with(operation, message, e)
    }
}

/// Map a facade error: it already carries its catalogue code.
pub(crate) fn facade(operation: &str) -> impl FnOnce(pdfluent::Error) -> napi::Error + '_ {
    move |e| structured_error(e.code(), &e.to_string(), operation, None)
}

/// Map a lopdf error, keeping an OS error an I/O error.
pub(crate) fn lopdf(operation: &str, message: impl Into<String>, e: lopdf::Error) -> napi::Error {
    match e {
        lopdf::Error::IO(ref source) => io(operation, message, source),
        other => raise(Catalogue::from(other), operation, message),
    }
}

/// Map a manipulation error (merge, encrypt).
pub(crate) fn manip(
    operation: &str,
    message: impl Into<String>,
    e: pdf_manip::ManipError,
) -> napi::Error {
    use pdf_manip::ManipError as M;
    match e {
        M::Io(ref source) => io(operation, message, source),
        M::Pdf(inner) => lopdf(operation, message, inner),
        M::PageOutOfRange(..) => page_out_of_range(operation, message),
        M::EmptyPageRange => unsupported(operation, message),
        other => raise(Catalogue::from(other), operation, message),
    }
}

/// Map a redaction error.
pub(crate) fn redact(
    operation: &str,
    message: impl Into<String>,
    e: pdf_redact::RedactError,
) -> napi::Error {
    use pdf_redact::RedactError as R;
    match e {
        R::Io(ref source) => io(operation, message, source),
        R::Pdf(inner) => lopdf(operation, message, inner),
        R::PageOutOfRange(..) => page_out_of_range(operation, message),
        // A valid document this build cannot redact in place: not a broken PDF.
        R::UnsupportedImageFilter(_) | R::UnsupportedToUnicodeCMap { .. } => {
            unsupported(operation, message)
        }
        other => raise(Catalogue::from(other), operation, message),
    }
}

/// Map an annotation-building error. A page that is not there has its own
/// code; the rest are a rectangle or an appearance this call cannot build.
pub(crate) fn annot(
    operation: &str,
    message: impl Into<String>,
    e: pdf_annot::error::AnnotBuildError,
) -> napi::Error {
    use pdf_annot::error::AnnotBuildError as A;
    match e {
        A::PageOutOfRange(..) => page_out_of_range(operation, message),
        _ => unsupported(operation, message),
    }
}

/// Map a form write-back error. A field that is missing, read-only, of the
/// wrong type or offered a value it does not list is a request this document
/// cannot honour; only a broken form structure is a broken PDF.
pub(crate) fn writeback(
    operation: &str,
    message: impl Into<String>,
    e: &pdf_forms::WritebackError,
) -> napi::Error {
    match e {
        pdf_forms::WritebackError::Malformed(_) => invalid_pdf(operation, message),
        _ => unsupported(operation, message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code_of(e: napi::Error) -> String {
        let payload: serde_json::Value = serde_json::from_str(&e.reason).expect("JSON payload");
        payload["code"].as_str().expect("code").to_string()
    }

    // A valid document this build cannot redact is not a broken one (#587,
    // codex round two): through `From<RedactError>` both used to read as
    // E-PARSE-INVALID-PDF.
    #[test]
    fn a_redaction_this_build_cannot_do_is_unsupported() {
        let e = pdf_redact::RedactError::UnsupportedImageFilter("JBIG2Decode".into());
        assert_eq!(code_of(redact("redactText", "x", e)), "E-UNSUPPORTED");
    }

    #[test]
    fn an_engine_page_past_the_end_keeps_its_own_code() {
        let e = pdf_engine::EngineError::PageOutOfRange { index: 9, count: 1 };
        assert_eq!(code_of(engine("renderPage")(e)), "E-PAGE-OUT-OF-RANGE");
    }

    #[test]
    fn an_options_error_keeps_the_unsupported_code() {
        // An OptionsError rides inside EngineError::Io: a mistake in the
        // call, not an OS failure, so it must read as E-UNSUPPORTED (#574).
        let e = pdf_engine::EngineError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            pdf_engine::OptionsError::ConflictingPageSelection,
        ));
        assert_eq!(code_of(engine("extractBlocks")(e)), "E-UNSUPPORTED");
    }

    #[test]
    fn a_plain_io_error_still_reads_as_io() {
        // The options() check above must not swallow a genuine OS error.
        let e = pdf_engine::EngineError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ));
        assert_eq!(code_of(engine("extractBlocks")(e)), "E-IO-GENERIC");
    }

    // OCR asked for and not runnable is not a broken file and not an OS
    // error: the engine carries the typed cause inside `EngineError::Io`,
    // and the mapping keeps `E-UNSUPPORTED` with the `OCR error: ` message.
    #[test]
    fn an_ocr_failure_is_unsupported_not_io() {
        let e = pdf_engine::EngineError::Io(std::io::Error::other(
            pdf_ocr::TesseractCliError::NoBinary("tesseract binary not found".to_string()),
        ));
        let mapped = engine("extractMarkdown")(e);
        let payload: serde_json::Value =
            serde_json::from_str(&mapped.reason).expect("JSON payload");
        assert_eq!(payload["code"].as_str().expect("code"), "E-UNSUPPORTED");
        assert_eq!(
            payload["message"].as_str().expect("message"),
            "OCR error: tesseract binary not found"
        );
    }

    #[test]
    fn a_plain_io_error_keeps_its_io_code() {
        let e = pdf_engine::EngineError::Io(std::io::Error::other("disk went away"));
        assert_eq!(code_of(engine("extractMarkdown")(e)), "E-IO-GENERIC");
    }
}
