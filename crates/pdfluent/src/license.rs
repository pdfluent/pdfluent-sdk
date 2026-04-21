//! License provisioning.
//!
//! Three precedence-ordered sources for a license key, highest first:
//!
//! 1. Per-document override via
//!    [`crate::OpenOptions::with_license_key`].
//! 2. Process-global key set via [`set_license_key`].
//! 3. Environment variable `PDFLUENT_LICENSE_KEY`.
//!
//! When no license is provided, the SDK runs in [`Tier::Trial`] mode: all
//! capabilities are accessible, but saved output is marked via the
//! `/Producer` metadata field.
//!
//! [`Tier::Trial`]: crate::tier::Tier::Trial

use crate::capability::CapabilitySet;
use crate::error::Result;
use crate::tier::Tier;

/// Summary of the currently-active license.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LicenseInfo {
    /// Tier granted by the license.
    pub tier: Tier,
    /// Expiration date (ISO 8601), if the license is time-bound.
    pub expires_at: Option<String>,
    /// Set of capabilities unlocked.
    pub capabilities: CapabilitySet,
    /// Whether output is currently being marked as trial output.
    pub output_is_marked: bool,
}

/// Set the process-global license key.
///
/// The key format is the signed payload produced by `xfa-license-gen`. This
/// function parses and validates the key; on failure it returns
/// [`crate::Error::InvalidLicense`].
///
/// The key applies to every [`crate::PdfDocument`] constructed after the
/// call. Per-document overrides via
/// [`crate::OpenOptions::with_license_key`] take precedence.
///
/// # Errors
///
/// - [`crate::Error::InvalidLicense`] if the payload is malformed or its
///   signature does not verify.
pub fn set_license_key(_key: &str) -> Result<()> {
    unimplemented!("Epic 3 #1227 wires this against xfa_license::claims");
}

/// Inspect the currently-active license.
///
/// Returns the trial license if no key has been set via
/// [`set_license_key`] and `PDFLUENT_LICENSE_KEY` is unset.
pub fn license_info() -> LicenseInfo {
    unimplemented!("Epic 3 #1227");
}
