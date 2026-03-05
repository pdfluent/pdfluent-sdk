//! API error types and HTTP response mapping.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// API error type with HTTP status code mapping.
#[derive(Debug)]
pub enum ApiError {
    /// Invalid or unreadable PDF input.
    BadRequest(String),
    /// Resource not found (e.g., unknown form ID).
    NotFound(String),
    /// Internal processing error.
    Internal(String),
}

/// JSON error response body.
#[derive(Serialize)]
struct ErrorBody {
    error: String,
    detail: Option<String>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, detail) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", Some(msg)),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", Some(msg)),
            ApiError::Internal(msg) => {
                tracing::error!("Internal error: {msg}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    Some(msg),
                )
            }
        };

        let body = ErrorBody {
            error: error.to_string(),
            detail,
        };

        (status, axum::Json(body)).into_response()
    }
}

impl From<pdfium_ffi_bridge::error::PdfError> for ApiError {
    fn from(e: pdfium_ffi_bridge::error::PdfError) -> Self {
        ApiError::BadRequest(e.to_string())
    }
}
