//! XFA License — Ed25519 signed license files with metering and rate limiting.
//!
//! Provides an asymmetric (Ed25519) license file system for offline validation.
//! Includes tier-based feature flags, quota enforcement, and sliding-window
//! rate limiting.
//!
//! # License File Format
//!
//! License files are JSON with a `signature` field containing a base64-encoded
//! Ed25519 signature over the payload fields. The public key is embedded in the
//! binary; the private key is only used by the license generation tool.
//!
//! # Usage
//!
//! ```ignore
//! use xfa_license::{LicenseGuard, token};
//!
//! let public_key = include_bytes!("../keys/public.key");
//! let license_json = std::fs::read_to_string("license.json").unwrap();
//!
//! let now = 1710000000;
//! let mut guard = LicenseGuard::from_license(public_key, &license_json, now).unwrap();
//!
//! assert!(guard.has_feature("flatten"));
//! guard.record_api_call(now).unwrap();
//! ```

pub mod claims;
pub mod error;
pub mod metering;
pub mod token;

pub use claims::{FeatureFlags, LicenseClaims, LicenseFile, LicensePayload, Quotas, Tier};
pub use error::{LicenseError, Result};
pub use metering::{UsageCounters, UsageMeter};

/// High-level license guard combining validation, feature gating, and metering.
///
/// Created from a license file and a public key. Validates the Ed25519 signature,
/// checks expiry, and provides methods to check features and record usage.
#[derive(Debug)]
pub struct LicenseGuard {
    claims: LicenseClaims,
    payload: LicensePayload,
    meter: UsageMeter,
}

impl LicenseGuard {
    /// Validate a license file and create a guard.
    ///
    /// Verifies the Ed25519 signature and checks that the license has not expired.
    pub fn from_license(public_key: &[u8], license_json: &str, now: u64) -> Result<Self> {
        let license = token::verify_and_check_expiry(public_key, license_json, now)?;
        let claims = license.payload.to_claims();
        let meter = UsageMeter::from_claims(&claims);
        Ok(Self {
            claims,
            payload: license.payload,
            meter,
        })
    }

    /// Load a license from a file path and validate it.
    pub fn from_license_path(public_key: &[u8], path: &std::path::Path, now: u64) -> Result<Self> {
        let license_json = std::fs::read_to_string(path)?;
        Self::from_license(public_key, &license_json, now)
    }

    /// The validated license claims.
    pub fn claims(&self) -> &LicenseClaims {
        &self.claims
    }

    /// The license payload.
    pub fn payload(&self) -> &LicensePayload {
        &self.payload
    }

    /// The license tier.
    pub fn tier(&self) -> Tier {
        self.claims.tier
    }

    /// The licensee name.
    pub fn licensee(&self) -> &str {
        &self.payload.licensee
    }

    /// The company name.
    pub fn company(&self) -> &str {
        &self.payload.company
    }

    /// Number of seats.
    pub fn seats(&self) -> u32 {
        self.payload.seats
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

/// Create a guard for unlicensed/personal mode (no license file needed).
///
/// Returns a guard with Trial tier restrictions and watermarking enabled.
pub fn unlicensed_guard() -> LicenseGuard {
    let claims = LicenseClaims::new("personal", Tier::Trial, 0, u64::MAX);
    let meter = UsageMeter::from_claims(&claims);
    let payload = LicensePayload {
        licensee: "Personal Use".into(),
        email: String::new(),
        company: String::new(),
        tier: Tier::Trial,
        seats: 1,
        issued_at: 0,
        expires_at: u64::MAX,
        features: None,
    };
    LicenseGuard {
        claims,
        payload,
        meter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "signing")]
    #[test]
    fn full_lifecycle() {
        let (private_key, public_key) = token::generate_keypair();
        let payload = LicensePayload {
            licensee: "Acme Corp".into(),
            email: "admin@acme.com".into(),
            company: "Acme Corporation".into(),
            tier: Tier::Professional,
            seats: 5,
            issued_at: 1700000000,
            expires_at: 1730000000,
            features: None,
        };
        let license_json = token::sign_license(&private_key, &payload).unwrap();

        let now = 1710000000;
        let mut guard = LicenseGuard::from_license(&public_key, &license_json, now).unwrap();

        assert_eq!(guard.tier(), Tier::Professional);
        assert_eq!(guard.licensee(), "Acme Corp");
        assert_eq!(guard.company(), "Acme Corporation");
        assert_eq!(guard.seats(), 5);
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

    #[cfg(feature = "signing")]
    #[test]
    fn expired_license_rejected() {
        let (private_key, public_key) = token::generate_keypair();
        let payload = LicensePayload {
            licensee: "Test".into(),
            email: "t@t.com".into(),
            company: "T".into(),
            tier: Tier::Basic,
            seats: 1,
            issued_at: 1000,
            expires_at: 2000,
            features: None,
        };
        let license_json = token::sign_license(&private_key, &payload).unwrap();

        let result = LicenseGuard::from_license(&public_key, &license_json, 3000);
        assert!(matches!(result, Err(LicenseError::Expired(2000))));
    }

    #[test]
    fn unlicensed_mode() {
        let guard = unlicensed_guard();
        assert_eq!(guard.tier(), Tier::Trial);
        assert!(guard.should_watermark());
        assert!(guard.has_feature("xfa_parse"));
        assert!(!guard.has_feature("flatten"));
        assert_eq!(guard.licensee(), "Personal Use");
    }
}
