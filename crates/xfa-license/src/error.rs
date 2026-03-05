//! License error types.

use thiserror::Error;

/// License operation errors.
#[derive(Debug, Error)]
pub enum LicenseError {
    /// The license token is malformed (not 3 base64 parts).
    #[error("malformed token: {0}")]
    MalformedToken(String),

    /// The HMAC signature does not match.
    #[error("invalid signature")]
    InvalidSignature,

    /// The license has expired.
    #[error("license expired at {0}")]
    Expired(u64),

    /// A required feature is not included in this license tier.
    #[error("feature not available: {0}")]
    FeatureNotAvailable(String),

    /// The usage quota has been exceeded.
    #[error("quota exceeded: {used}/{limit} {resource}")]
    QuotaExceeded {
        resource: String,
        used: u64,
        limit: u64,
    },

    /// Rate limit exceeded (too many requests per time window).
    #[error("rate limit exceeded: {0} requests/minute")]
    RateLimitExceeded(u32),

    /// JSON serialization/deserialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Convenience result type.
pub type Result<T> = std::result::Result<T, LicenseError>;
