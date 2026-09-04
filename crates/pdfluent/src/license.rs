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

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::sync::{OnceLock, RwLock};

use crate::capability::{Capability, CapabilitySet};
use crate::error::{Error, Result};
use crate::tier::Tier;

/// Build-time injected public key for verifying signed license payloads.
///
/// Production deployments set this via [`set_license_public_key`] once at
/// startup. The test public key in
/// `crates/xfa-license/tests/fixtures/test_public.key` is used for the
/// repo's unit + integration tests.
static LICENSE_PUBLIC_KEY: OnceLock<[u8; 32]> = OnceLock::new();

/// Verified-payload metadata captured at activation time, exposed via
/// [`license_info`].
///
/// Held in a `RwLock` so the umbrella can update it on successful
/// signed-payload activation without changing the simpler
/// `OnceLock<Tier>` for the mock `tier:X` path.
static SIGNED_PAYLOAD_META: RwLock<Option<SignedPayloadMeta>> = RwLock::new(None);

#[derive(Debug, Clone)]
struct SignedPayloadMeta {
    expires_at: u64,
    licensee: String,
    company: String,
    features: Vec<String>,
}

/// Summary of the currently-active license.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LicenseInfo {
    /// Tier granted by the license.
    pub tier: Tier,
    /// Expiration date (ISO 8601), if the license is time-bound.
    ///
    /// `Some(ts)` when a signed payload was activated via
    /// [`set_license_payload`]; `None` for mock `tier:X` keys (which have
    /// no expiry concept).
    pub expires_at: Option<String>,
    /// Set of capabilities unlocked.
    pub capabilities: CapabilitySet,
    /// Whether output is currently being marked as trial output.
    pub output_is_marked: bool,
    /// Feature names from the signed payload's optional `features`
    /// override list, or empty when no signed payload is active.
    pub features: Vec<String>,
    /// Licensee name from the signed payload, if any.
    pub licensee: Option<String>,
    /// Company / organisation from the signed payload, if any.
    pub company: Option<String>,
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
    // Auto-detect: JSON payload (signed) vs mock tier:X format.
    // A leading `{` always indicates a signed payload — we route it
    // through the verifier and return the typed errors documented on
    // [`set_license_payload`]. No silent fallback if the public key
    // is not set.
    let trimmed = key.trim();
    if trimmed.starts_with('{') {
        return set_license_payload(trimmed);
    }

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
/// 1. Process-global tier set via [`set_license_key`] /
///    [`set_license_payload`].
/// 2. Env var `PDFLUENT_LICENSE_KEY` (parsed per the module doc).
/// 3. [`Tier::Trial`].
///
/// When a signed payload is the source of the active tier, the returned
/// `LicenseInfo` additionally carries `expires_at` (ISO 8601), `features`
/// (from the payload's `features` override), `licensee`, and `company`.
pub fn license_info() -> LicenseInfo {
    let tier = effective_tier();
    let meta = SIGNED_PAYLOAD_META.read().ok().and_then(|g| g.clone());
    let (expires_at, features, licensee, company) = match meta {
        Some(m) => (
            Some(unix_to_iso8601(m.expires_at)),
            m.features,
            Some(m.licensee),
            Some(m.company),
        ),
        None => (None, Vec::new(), None, None),
    };
    LicenseInfo {
        tier,
        expires_at,
        capabilities: tier.capabilities(),
        output_is_marked: tier.is_marked(),
        features,
        licensee,
        company,
    }
}

/// Inject the public Ed25519 verification key.
///
/// **Must be called before [`set_license_payload`].** A real production
/// deployment injects this once at startup with the operator's public
/// key bytes. Tests inject the test-only key from
/// `crates/xfa-license/tests/fixtures/test_public.key`.
///
/// Calling twice with the SAME key is idempotent. Calling with a
/// different key returns [`Error::InvalidLicense`] — restart the process
/// to swap keys.
pub fn set_license_public_key(public_key: &[u8]) -> Result<()> {
    let key_bytes: [u8; 32] = public_key.try_into().map_err(|_| Error::InvalidLicense {
        reason: "public key must be exactly 32 bytes".into(),
    })?;
    match LICENSE_PUBLIC_KEY.set(key_bytes) {
        Ok(()) => Ok(()),
        Err(_) => {
            let existing = *LICENSE_PUBLIC_KEY.get().expect("initialised");
            if existing == key_bytes {
                Ok(())
            } else {
                Err(Error::InvalidLicense {
                    reason: "public key already set; restart the process to swap keys".into(),
                })
            }
        }
    }
}

/// Activate a signed license payload (Ed25519-verified JSON).
///
/// Verifies the signature against the public key set via
/// [`set_license_public_key`], parses the payload, checks
/// `expires_at > now`, maps the payload tier to a `pdfluent::Tier`, and
/// stores the resulting tier + payload metadata in process-global state
/// so [`license_info`] surfaces it.
///
/// # Errors
/// - [`Error::InvalidLicense`] (`E-LICENSE-INVALID`) — public key
///   not yet set; malformed JSON; tier name not recognised; already-set
///   with a different resolved tier.
/// - [`Error::LicenseInvalidSignature`] (`E-LICENSE-INVALID-SIGNATURE`) —
///   signature does not verify (tampered or wrong-key payload).
/// - [`Error::LicenseExpired`] (`E-LICENSE-EXPIRED`) — `expires_at` is
///   in the past relative to the system clock.
///
/// On any error the process-global tier is **NOT** modified — there is
/// no silent fallback to Trial.
pub fn set_license_payload(license_json: &str) -> Result<()> {
    let Some(public_key) = LICENSE_PUBLIC_KEY.get() else {
        return Err(Error::InvalidLicense {
            reason: "no public key configured — call set_license_public_key first".into(),
        });
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let license_file = xfa_license::token::verify_license(public_key, license_json)
        .map_err(map_xfa_license_error)?;

    if license_file.payload.is_expired(now) {
        return Err(Error::LicenseExpired {
            expires_at: license_file.payload.expires_at,
        });
    }

    let tier = map_xfa_tier(license_file.payload.tier)?;

    // Reuse the same set-once contract as set_license_key.
    match GLOBAL_TIER.set(tier) {
        Ok(()) => {
            // Capture the payload metadata for license_info().
            if let Ok(mut guard) = SIGNED_PAYLOAD_META.write() {
                *guard = Some(SignedPayloadMeta {
                    expires_at: license_file.payload.expires_at,
                    licensee: license_file.payload.licensee.clone(),
                    company: license_file.payload.company.clone(),
                    features: license_file.payload.features.clone().unwrap_or_default(),
                });
            }
            Ok(())
        }
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

/// Map an xfa-license verification error to a pdfluent typed error.
///
/// Discriminates by variant — never by message-string parsing.
fn map_xfa_license_error(e: xfa_license::error::LicenseError) -> Error {
    use xfa_license::error::LicenseError as L;
    match e {
        L::InvalidSignature => Error::LicenseInvalidSignature,
        L::Expired(ts) => Error::LicenseExpired { expires_at: ts },
        L::RateLimitExceeded(limit) => Error::LicenseRateLimited {
            resource: "api_calls".into(),
            used: limit as u64,
            limit: limit as u64,
        },
        L::QuotaExceeded {
            resource,
            used,
            limit,
        } => Error::LicenseRateLimited {
            resource,
            used,
            limit,
        },
        L::FeatureNotAvailable(name) => Error::InvalidLicense {
            reason: format!("feature not available: {name}"),
        },
        L::InvalidPublicKey => Error::InvalidLicense {
            reason: "invalid public key (must be 32 bytes)".into(),
        },
        L::MalformedToken(detail) => Error::InvalidLicense {
            reason: format!("malformed signed-license token: {detail}"),
        },
        L::Json(err) => Error::InvalidLicense {
            reason: format!("license JSON parse failure: {err}"),
        },
        L::Io(err) => Error::InvalidLicense {
            reason: format!("license I/O error: {err}"),
        },
    }
}

/// Map an `xfa_license::Tier` to the umbrella `pdfluent::Tier`.
///
/// Conservative one-way mapping — see
/// `benchmarks/runs/ga_100_closure_v3/commercial_license_e2e/SIGNED_LICENSE_PAYLOAD_ARCHITECTURE.md`
/// Q5 for the rationale.
fn map_xfa_tier(t: xfa_license::Tier) -> Result<Tier> {
    use xfa_license::Tier as X;
    match t {
        X::Trial => Ok(Tier::Trial),
        X::Basic => Ok(Tier::Developer),
        X::Professional => Ok(Tier::Team),
        X::Enterprise => Ok(Tier::Enterprise),
        X::Archival => Ok(Tier::Business),
    }
}

/// Format a unix timestamp as ISO 8601 (UTC, no fractional seconds).
fn unix_to_iso8601(ts: u64) -> String {
    // Avoid pulling in chrono just for this — implement the small
    // calendar manually. The output is the de-facto standard
    // "YYYY-MM-DDTHH:MM:SSZ" form.
    let secs = ts as i64;
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let h = sod / 3600;
    let m = (sod / 60) % 60;
    let s = sod % 60;

    // Days since unix epoch 1970-01-01 → (year, month, day) via the
    // proleptic Gregorian algorithm from Howard Hinnant's date library.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = y + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
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

/// Check that the active license grants the given capability, optionally
/// overridden by a per-document license key.
///
/// Precedence when `override_key` is `Some`:
///
/// 1. Parse the per-document key into a Tier. If parsing succeeds, that
///    tier is the effective tier for this call.
/// 2. If parsing fails, the call returns `Error::InvalidLicense` —
///    malformed per-doc keys are always a hard error.
///
/// When `override_key` is `None`, the effective tier comes from
/// [`effective_tier`] (process-global → env → Trial).
///
/// `required_tier` is the minimum **paid** tier that grants the
/// capability (Trial is excluded so the upgrade hint never says
/// "upgrade to Trial").
/// The tier that applies to a document, honouring a per-document license
/// override. A malformed override degrades to Trial (the capability check
/// will already have surfaced the parse error to the caller).
pub(crate) fn effective_tier_with_override(override_key: Option<&str>) -> Tier {
    match override_key {
        Some(key) => parse_key_to_tier(key).unwrap_or(Tier::Trial),
        None => effective_tier(),
    }
}

pub(crate) fn require_capability_with_override(
    cap: Capability,
    override_key: Option<&str>,
) -> Result<()> {
    let tier = match override_key {
        Some(key) => parse_key_to_tier(key)?,
        None => effective_tier(),
    };
    if tier.capabilities().contains(cap) {
        return Ok(());
    }

    // Paid tiers only — suggesting "upgrade to Trial" is nonsensical.
    let required = [
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

/// Backwards-compatible entry for call-sites without a per-document
/// override (e.g. associated constructors that don't have `&self`).
pub(crate) fn require_capability(cap: Capability) -> Result<()> {
    require_capability_with_override(cap, None)
}
