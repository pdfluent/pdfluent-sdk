//! License provisioning and capability enforcement.
//!
//! Three precedence-ordered sources for a license key, highest first:
//!
//! 1. Per-document override via
//!    [`crate::OpenOptions::with_license_key`] (wired through
//!    `PdfDocument`-local state — see that type's docs).
//! 2. Process-global key set via [`set_license_key`].
//! 3. Environment variable `PDFLUENT_LICENSE_KEY`.
//!
//! When no license is provided, the SDK runs in [`Tier::Trial`] mode: all
//! **technical** capabilities are accessible (minus deployment-rights
//! capabilities like `AirGapped` and `OemRedistribution` which stay
//! Enterprise-only per RFC §6.3), but saved output is marked via the
//! `/Producer` metadata field.
//!
//! # License key format in 1.0
//!
//! The 1.0 GA release accepts a **simple evaluation format** so users can
//! test tier-based capability enforcement without a full signed-license
//! pipeline:
//!
//! ```text
//! tier:trial
//! tier:developer
//! tier:team
//! tier:business
//! tier:enterprise
//! ```
//!
//! Full signed-payload verification (cryptographic integrity check against
//! the production signing key) is tracked as a post-1.0 integration with
//! the `xfa-license` / `xfa-license-gen` crates. Until that lands, using
//! the simple format in production is equivalent to the honour-system;
//! real signed keys will start being accepted in 1.1 without breaking
//! the existing API.
//!
//! [`Tier::Trial`]: crate::tier::Tier::Trial

use std::sync::OnceLock;

use crate::capability::{Capability, CapabilitySet};
use crate::error::{Error, Result};
use crate::tier::Tier;

/// Summary of the currently-active license.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LicenseInfo {
    /// Tier granted by the license.
    pub tier: Tier,
    /// Expiration date (ISO 8601), if the license is time-bound.
    ///
    /// Always `None` in 1.0 — time-bound keys ship with the signed-payload
    /// format in 1.1.
    pub expires_at: Option<String>,
    /// Set of capabilities unlocked.
    pub capabilities: CapabilitySet,
    /// Whether output is currently being marked as trial output.
    pub output_is_marked: bool,
}

/// Process-global tier, set once by [`set_license_key`] or resolved from
/// the environment at first access.
///
/// We use `OnceLock<Tier>` rather than `Mutex<Option<Tier>>` because:
/// - Calling `set_license_key` twice with different values is not a
///   supported use-case (the second call fails; restart the process for
///   a different license).
/// - Reading is hot (every gated method calls `require_capability`); lock
///   contention is undesirable.
static GLOBAL_TIER: OnceLock<Tier> = OnceLock::new();

/// Set the process-global license key.
///
/// The key format is described in the module-level documentation. On
/// failure this returns [`Error::InvalidLicense`].
///
/// This function may be called at most once per process. Subsequent calls
/// with the same resolved tier are idempotent no-ops; calls that resolve
/// to a different tier return [`Error::InvalidLicense`] with a
/// `reason: "license already set"` message. Restart the process to switch
/// tiers.
pub fn set_license_key(key: &str) -> Result<()> {
    let tier = parse_key_to_tier(key)?;
    match GLOBAL_TIER.set(tier) {
        Ok(()) => Ok(()),
        Err(_existing) => {
            let existing = *GLOBAL_TIER.get().expect("initialised");
            if existing == tier {
                Ok(())
            } else {
                Err(Error::InvalidLicense {
                    reason: format!(
                        "license already set to {existing:?}; restart the process to switch to {tier:?}",
                    ),
                })
            }
        }
    }
}

/// Inspect the currently-active license.
///
/// Falls back to:
/// 1. Process-global tier set via [`set_license_key`].
/// 2. Env var `PDFLUENT_LICENSE_KEY` (parsed per the module doc).
/// 3. [`Tier::Trial`].
pub fn license_info() -> LicenseInfo {
    let tier = effective_tier();
    LicenseInfo {
        tier,
        expires_at: None,
        capabilities: tier.capabilities(),
        output_is_marked: tier.is_marked(),
    }
}

/// Resolve the effective tier for this process.
pub(crate) fn effective_tier() -> Tier {
    if let Some(&t) = GLOBAL_TIER.get() {
        return t;
    }
    if let Ok(key) = std::env::var("PDFLUENT_LICENSE_KEY") {
        if let Ok(t) = parse_key_to_tier(&key) {
            return t;
        }
        // Malformed env key → fall through to Trial rather than panicking.
    }
    Tier::Trial
}

/// Parse a license key string into a [`Tier`].
fn parse_key_to_tier(key: &str) -> Result<Tier> {
    let trimmed = key.trim();
    let lowered = trimmed.to_ascii_lowercase();
    let after_prefix = lowered
        .strip_prefix("tier:")
        .ok_or_else(|| Error::InvalidLicense {
            reason: format!("expected `tier:<name>` format, got {trimmed:?}"),
        })?;
    match after_prefix.trim() {
        "trial" => Ok(Tier::Trial),
        "developer" => Ok(Tier::Developer),
        "team" => Ok(Tier::Team),
        "business" => Ok(Tier::Business),
        "enterprise" => Ok(Tier::Enterprise),
        other => Err(Error::InvalidLicense {
            reason: format!(
                "unknown tier {other:?}; expected trial/developer/team/business/enterprise"
            ),
        }),
    }
}

// ---------------------------------------------------------------------------
// Internal capability enforcement
// ---------------------------------------------------------------------------

/// Check that the active license grants the given capability.
///
/// Reads the effective tier (see [`effective_tier`]) and tests the
/// capability-set. Returns [`Error::FeatureNotInTier`] on denial, with the
/// fields populated so the Elm-style `Display` impl produces a complete
/// upgrade hint.
pub(crate) fn require_capability(cap: Capability) -> Result<()> {
    let tier = effective_tier();
    let caps = tier.capabilities();
    if caps.contains(cap) {
        return Ok(());
    }

    // Find the minimum tier that grants this capability for the error's
    // `required_tier` field. Walk in ascending order.
    let required = [
        Tier::Trial,
        Tier::Developer,
        Tier::Team,
        Tier::Business,
        Tier::Enterprise,
    ]
    .iter()
    .copied()
    .find(|t| t.capabilities().contains(cap))
    .unwrap_or(Tier::Enterprise);

    Err(Error::FeatureNotInTier {
        capability: cap,
        current_tier: tier,
        required_tier: required,
    })
}
