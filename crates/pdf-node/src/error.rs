//! Error conversion from Rust engine errors to napi errors.

use napi::Status;

/// Convert a pdf-engine error into a napi Error.
pub fn to_napi_error(err: pdf_engine::EngineError) -> napi::Error {
    napi::Error::new(Status::GenericFailure, format!("{err}"))
}
