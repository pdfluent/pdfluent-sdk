//! Node.js license activation surface.
//!
//! Mirrors the canonical Rust API ([`pdfluent::set_license_key`],
//! [`pdfluent::license_info`]) and the C ABI / Python license surfaces.
//!
//! The license tier is process-global, set once. Re-activating with the
//! same tier is idempotent; re-activating with a different tier raises a
//! typed [`PdfluentError`] whose `code` is `E-LICENSE-INVALID`.
//!
//! Key format (1.0 GA): `"tier:<name>"` where `<name>` is one of
//! `trial`, `developer`, `team`, `business`, `enterprise`. Cryptographically-
//! signed payloads are accepted by the same function from release 1.1
//! onward without breaking the API.
//!
//! Example (TypeScript):
//!
//! ```ts
//! import { activate, status, PdfluentError } from '@pdfluent/node';
//!
//! try {
//!   activate('tier:developer');
//! } catch (e) {
//!   if (e instanceof PdfluentError && e.code === 'E-LICENSE-INVALID') {
//!     console.error('bad key:', e.message);
//!   }
//!   throw e;
//! }
//!
//! const s = status();
//! console.log(s.active, s.tier);
//! ```

#![allow(dead_code)]

use std::sync::atomic::{AtomicU8, Ordering};

use napi::bindgen_prelude::*;
use napi_derive::napi;
use pdfluent::{license_info as pdfl_license_info, set_license_key as pdfl_set_license_key, Tier};

use crate::error::pdfluent_err_to_napi;

/// Activation source mirroring the C ABI [`pdf_capi::license::SourceTag`].
///
/// - `0` — Default: no key supplied; active tier is Trial.
/// - `1` — EnvVar: tier resolved from the `PDFLUENT_LICENSE_KEY` environment
///   variable.
/// - `2` — Explicit: tier set via [`activate`] / [`set_license_key`].
static ACTIVATION_SOURCE: AtomicU8 = AtomicU8::new(0);

fn record_source_explicit() {
    ACTIVATION_SOURCE.store(2, Ordering::Relaxed);
}

fn current_source_label() -> &'static str {
    match ACTIVATION_SOURCE.load(Ordering::Relaxed) {
        2 => "explicit",
        1 => "env",
        _ => {
            if let Ok(key) = std::env::var("PDFLUENT_LICENSE_KEY") {
                if !key.is_empty() && pdfl_license_info().tier != Tier::Trial {
                    return "env";
                }
            }
            "default"
        }
    }
}

fn tier_to_str(tier: Tier) -> &'static str {
    match tier {
        Tier::Trial => "trial",
        Tier::Developer => "developer",
        Tier::Team => "team",
        Tier::Business => "business",
        Tier::Enterprise => "enterprise",
        _ => "unknown",
    }
}

/// License state snapshot returned by [`status`].
#[napi(object)]
pub struct LicenseStatus {
    /// `true` when a paid tier (Developer or higher) is active.  `false`
    /// when the SDK is running in Trial mode (default).
    pub active: bool,
    /// Effective tier name. One of `"trial"`, `"developer"`, `"team"`,
    /// `"business"`, `"enterprise"`, or `"unknown"` for a future tier
    /// not yet mapped by this binding.
    pub tier: String,
    /// Activation source: `"default"`, `"env"`, or `"explicit"`.
    pub source: String,
    /// `true` when the active tier marks PDF output via the `/Producer`
    /// metadata field (Trial only).
    pub output_is_marked: bool,
    /// ISO-8601 expiry timestamp, or `None` for non-time-bound keys.
    ///
    /// Always `None` in 1.0 — time-bound keys ship in 1.1 with signed
    /// payloads. The field is present here so callers can write
    /// forward-compatible code today.
    pub expires_at: Option<String>,
}

/// Activate the process-global license from a key string.
///
/// The first successful call locks the resolved tier for the process
/// lifetime.  Subsequent calls with the **same** tier are idempotent
/// no-ops; calls with a **different** tier throw a `PdfluentError` with
/// `code === "E-LICENSE-INVALID"`.
///
/// The key is consumed immediately and never stored locally.  On failure
/// the engine surfaces a typed error — JavaScript callers must branch on
/// `err.code`, not on `err.message`.
///
/// Throws a `PdfluentError` with one of:
///   - `code === "E-LICENSE-INVALID"` — key is malformed, tier unknown,
///     or process already activated to a different tier.
#[napi]
pub fn activate(license_key: String) -> Result<()> {
    pdfl_set_license_key(&license_key).map_err(|e| pdfluent_err_to_napi(e, "activate"))?;
    record_source_explicit();
    Ok(())
}

/// Alias for [`activate`].  Provided to match the Python / C ABI naming
/// (`set_license_key` in Rust + Python; `pdfluent_license_activate_key` in
/// the C ABI).
#[napi]
pub fn set_license_key(license_key: String) -> Result<()> {
    activate(license_key)
}

/// Return the current canonical license state.
///
/// Always succeeds.  Reads are lock-free.
#[napi]
pub fn status() -> LicenseStatus {
    let info = pdfl_license_info();
    let tier = info.tier;
    LicenseStatus {
        active: !matches!(tier, Tier::Trial),
        tier: tier_to_str(tier).to_owned(),
        source: current_source_label().to_owned(),
        output_is_marked: info.output_is_marked,
        expires_at: info.expires_at,
    }
}

/// Alias for [`status`].  Provided to match the C ABI naming
/// (`pdfluent_license_status`).
#[napi]
pub fn license_status() -> LicenseStatus {
    status()
}
