//! XFA License — key generation, validation, metering, and rate limiting.
//!
//! Provides a JWT-like license token system with HMAC-SHA256 signatures
//! for offline validation. Includes tier-based feature flags, quota
//! enforcement, and sliding-window rate limiting.
//!
//! # Token Format
//!
//! Tokens are three base64url-encoded parts separated by dots:
//! `header.payload.signature`
//!
//! # Usage
//!
//! ```
//! use xfa_license::{LicenseGuard, LicenseClaims, Tier, token};
//!
//! let secret = b"my-secret-key";
//!
//! // Issue a license.
//! let claims = LicenseClaims::new("customer-1", Tier::Professional, 1700000000, 1730000000);
//! let token_str = token::sign(&claims, secret).unwrap();
//!
//! // Validate and create a guard.
//! let now = 1710000000;
//! let mut guard = LicenseGuard::from_token(&token_str, secret, now).unwrap();
//!
//! // Check features and record usage.
//! assert!(guard.has_feature("flatten"));
//! guard.record_api_call(now).unwrap();
//! ```

pub mod claims;
pub mod error;
pub mod metering;
pub mod token;

pub use claims::{FeatureFlags, LicenseClaims, Quotas, Tier};
pub use error::{LicenseError, Result};
pub use metering::{UsageCounters, UsageMeter};

/// High-level license guard combining validation, feature gating, and metering.
///
/// Created from a token string and a signing secret. Validates the token,
/// checks expiry, and provides methods to check features and record usage.
#[derive(Debug)]
pub struct LicenseGuard {
    claims: LicenseClaims,
    meter: UsageMeter,
}

impl LicenseGuard {
    /// Validate a token and create a guard.
    ///
    /// Verifies the HMAC signature and checks that the license has not expired.
    pub fn from_token(token_str: &str, secret: &[u8], now: u64) -> Result<Self> {
        let claims = token::verify_and_check_expiry(token_str, secret, now)?;
        let meter = UsageMeter::from_claims(&claims);
        Ok(Self { claims, meter })
    }

    /// The validated license claims.
    pub fn claims(&self) -> &LicenseClaims {
        &self.claims
    }

    /// The license tier.
    pub fn tier(&self) -> Tier {
        self.claims.tier
    }

    /// The customer ID.
    pub fn customer_id(&self) -> &str {
        &self.claims.customer_id
    }

    /// Check if a named feature is available in this license.
    pub fn has_feature(&self, name: &str) -> bool {
        self.claims.features.has_feature(name)
    }

    /// Require a feature, returning an error if unavailable.
    pub fn require_feature(&self, name: &str) -> Result<()> {
        if self.has_feature(name) {
            Ok(())
        } else {
            Err(LicenseError::FeatureNotAvailable(name.to_string()))
        }
    }

    /// Re-check whether the license has expired at the given timestamp.
    ///
    /// Call this periodically for long-lived guards to detect post-construction expiry.
    pub fn check_expiry(&self, now: u64) -> Result<()> {
        if now > self.claims.expires_at {
            Err(LicenseError::Expired(self.claims.expires_at))
        } else {
            Ok(())
        }
    }

    /// Record an API call (rate limit + quota check).
    pub fn record_api_call(&mut self, now: u64) -> Result<()> {
        self.meter.record_api_call(now)
    }

    /// Record pages rendered.
    pub fn record_pages(&mut self, count: u64) -> Result<()> {
        self.meter.record_pages(count)
    }

    /// Record a form processed.
    pub fn record_form(&mut self) -> Result<()> {
        self.meter.record_form()
    }

    /// Current usage counters.
    pub fn usage(&self) -> &UsageCounters {
        self.meter.counters()
    }

    /// Reset usage counters for a new billing period.
    pub fn reset_usage(&mut self) {
        self.meter.reset_counters();
    }

    /// Whether output should be watermarked (trial tier).
    pub fn should_watermark(&self) -> bool {
        self.claims.tier == Tier::Trial
    }
}

/// Create a guard for unlicensed/trial mode (no token needed).
///
/// Returns a guard with Trial tier restrictions and watermarking enabled.
pub fn unlicensed_guard() -> LicenseGuard {
    let claims = LicenseClaims::new("unlicensed", Tier::Trial, 0, u64::MAX);
    let meter = UsageMeter::from_claims(&claims);
    LicenseGuard { claims, meter }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"integration-test-key";

    #[test]
    fn full_lifecycle() {
        let claims = LicenseClaims::new("cust-1", Tier::Professional, 1700000000, 1730000000);
        let token_str = token::sign(&claims, SECRET).unwrap();

        let now = 1710000000;
        let mut guard = LicenseGuard::from_token(&token_str, SECRET, now).unwrap();

        assert_eq!(guard.tier(), Tier::Professional);
        assert_eq!(guard.customer_id(), "cust-1");
        assert!(guard.has_feature("flatten"));
        assert!(guard.has_feature("signatures"));
        assert!(!guard.has_feature("pdfa"));
        assert!(!guard.should_watermark());

        guard.record_api_call(now).unwrap();
        guard.record_pages(5).unwrap();
        guard.record_form().unwrap();

        assert_eq!(guard.usage().api_calls, 1);
        assert_eq!(guard.usage().pages_rendered, 5);
        assert_eq!(guard.usage().forms_processed, 1);
    }

    #[test]
    fn expired_token_rejected() {
        let claims = LicenseClaims::new("c", Tier::Basic, 1000, 2000);
        let token_str = token::sign(&claims, SECRET).unwrap();

        let result = LicenseGuard::from_token(&token_str, SECRET, 3000);
        assert!(matches!(result, Err(LicenseError::Expired(2000))));
    }

    #[test]
    fn require_feature_gate() {
        let claims = LicenseClaims::new("c", Tier::Basic, 1000, 2000);
        let token_str = token::sign(&claims, SECRET).unwrap();
        let guard = LicenseGuard::from_token(&token_str, SECRET, 1500).unwrap();

        assert!(guard.require_feature("xfa_parse").is_ok());
        let err = guard.require_feature("flatten").unwrap_err();
        assert!(matches!(err, LicenseError::FeatureNotAvailable(f) if f == "flatten"));
    }

    #[test]
    fn unlicensed_mode() {
        let guard = unlicensed_guard();
        assert_eq!(guard.tier(), Tier::Trial);
        assert!(guard.should_watermark());
        assert!(guard.has_feature("xfa_parse"));
        assert!(!guard.has_feature("flatten"));
    }

    #[test]
    fn reset_usage_clears_counters() {
        let claims = LicenseClaims::new("c", Tier::Enterprise, 1000, u64::MAX);
        let token_str = token::sign(&claims, SECRET).unwrap();
        let mut guard = LicenseGuard::from_token(&token_str, SECRET, 1500).unwrap();

        guard.record_api_call(1500).unwrap();
        guard.record_pages(100).unwrap();
        assert_eq!(guard.usage().api_calls, 1);

        guard.reset_usage();
        assert_eq!(guard.usage().api_calls, 0);
        assert_eq!(guard.usage().pages_rendered, 0);
    }
}
