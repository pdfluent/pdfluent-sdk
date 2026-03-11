//! License error types.

use thiserror::Error;

/// License operation errors.
#[derive(Debug, Error)]
pub enum LicenseError {
    /// The license file is malformed.
    #[error("malformed license: {0}")]
    MalformedToken(String),

    /// The Ed25519 signature does not match.
    #[error("invalid signature")]
    InvalidSignature,

    /// The public key is invalid (not 32 bytes).
    #[error("invalid public key")]
    InvalidPublicKey,

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

    /// IO error (file loading).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Convenience result type.
pub type Result<T> = std::result::Result<T, LicenseError>;
