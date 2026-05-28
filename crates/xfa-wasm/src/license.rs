//! License activation surface for the WASM binding.
//!
//! The browser has no synchronous filesystem access, so file-based
//! activation is intentionally omitted. JavaScript consumers can fetch the
//! file themselves and pass the string to [`activate_license_key`].

use std::sync::atomic::{AtomicU8, Ordering};

use wasm_bindgen::prelude::*;

use pdfluent::Tier;

fn tier_str(t: Tier) -> &'static str {
    match t {
        Tier::Trial => "Trial",
        Tier::Developer => "Developer",
        Tier::Team => "Team",
        Tier::Business => "Business",
        Tier::Enterprise => "Enterprise",
        _ => "Unknown",
    }
}

static LICENSE_SOURCE: AtomicU8 = AtomicU8::new(0);

fn record_explicit() {
    LICENSE_SOURCE.store(2, Ordering::Relaxed);
}

fn current_source_str() -> &'static str {
    match LICENSE_SOURCE.load(Ordering::Relaxed) {
        2 => "Explicit",
        _ => {
            // Browsers and Workers do not expose env vars; std::env::var
            // returns Err on wasm32. The fallback below handles native
            // (wasm-bindgen-test) and any embedder that does expose vars.
            if let Ok(key) = std::env::var("PDFLUENT_LICENSE_KEY") {
                if !key.is_empty() && pdfluent::license_info().tier != Tier::Trial {
                    return "EnvVar";
                }
            }
            "Default"
        }
    }
}

/// Status of the currently-active license.
#[wasm_bindgen]
#[derive(Clone)]
pub struct LicenseStatus {
    tier: String,
    source: String,
    output_is_marked: bool,
}

#[wasm_bindgen]
impl LicenseStatus {
    /// Tier name: `"Trial" | "Developer" | "Team" | "Business" | "Enterprise"`.
    #[wasm_bindgen(getter)]
    pub fn tier(&self) -> String {
        self.tier.clone()
    }

    /// Source: `"Default" | "EnvVar" | "Explicit"`.
    #[wasm_bindgen(getter)]
    pub fn source(&self) -> String {
        self.source.clone()
    }

    /// Whether output is marked via `/Producer` metadata (Trial tier).
    #[wasm_bindgen(getter, js_name = "outputIsMarked")]
    pub fn output_is_marked(&self) -> bool {
        self.output_is_marked
    }
}

fn map_license_error(e: pdfluent::Error) -> JsValue {
    use crate::pdfluent_error::{code, legacy_code, pdfluent_error};
    let c8 = e.code();
    let message = e.to_string();
    let operation = "license.activate";
    match e {
        pdfluent::Error::InvalidLicense { reason } => {
            if reason.contains("already set") {
                pdfluent_error(
                    operation,
                    c8,
                    legacy_code::LICENSE_ALREADY_SET,
                    &format!("license already set: {reason}"),
                    "License has already been activated this process — restart the worker/page to switch tiers.",
                )
            } else {
                pdfluent_error(
                    operation,
                    c8,
                    legacy_code::LICENSE_ERROR,
                    &format!("invalid license: {reason}"),
                    "Verify the license key string was copied correctly and matches the issued payload.",
                )
            }
        }
        pdfluent::Error::FeatureNotInTier { .. } => pdfluent_error(
            operation,
            code::LICENSE_FEATURE_NOT_IN_TIER,
            legacy_code::LICENSE_ERROR,
            &message,
            "Upgrade your tier — see https://pdfluent.com/pricing.",
        ),
        pdfluent::Error::CapabilityNotCompiled { feature_flag, .. } => pdfluent_error(
            operation,
            code::LICENSE_CAPABILITY_NOT_COMPILED,
            legacy_code::LICENSE_ERROR,
            &message,
            &format!(
                "Rebuild with the `{feature_flag}` Cargo feature enabled, or use a pre-built binary that includes the capability."
            ),
        ),
        pdfluent::Error::LicenseExpired { expires_at } => pdfluent_error(
            operation,
            code::LICENSE_EXPIRED,
            legacy_code::LICENSE_ERROR,
            &format!("license expired at unix timestamp {expires_at}"),
            "Renew the license — visit https://pdfluent.com/pricing or contact sales.",
        ),
        pdfluent::Error::LicenseInvalidSignature => pdfluent_error(
            operation,
            code::LICENSE_INVALID_SIGNATURE,
            legacy_code::LICENSE_ERROR,
            "license signature does not verify against the configured public key",
            "Re-download the license file from PDFluent; if the issue persists, contact support.",
        ),
        pdfluent::Error::LicenseRateLimited {
            resource,
            used,
            limit,
        } => pdfluent_error(
            operation,
            code::LICENSE_RATE_LIMITED,
            legacy_code::LICENSE_ERROR,
            &format!("rate limit exceeded: {used}/{limit} {resource}"),
            "Either upgrade the tier or wait for the metering window to reset.",
        ),
        _ => pdfluent_error(
            operation,
            c8,
            legacy_code::LICENSE_ERROR,
            &format!("license error: {message}"),
            "",
        ),
    }
}

/// Activate the process-global license from a key string.
///
/// Throws on invalid key or if the process has already been activated to a
/// different tier this session.
#[wasm_bindgen(js_name = "activateLicenseKey")]
pub fn activate_license_key(key: &str) -> Result<(), JsValue> {
    pdfluent::set_license_key(key).map_err(map_license_error)?;
    record_explicit();
    Ok(())
}

/// Return the current license status. Always succeeds; defaults to Trial.
#[wasm_bindgen(js_name = "licenseStatus")]
pub fn license_status() -> LicenseStatus {
    let info = pdfluent::license_info();
    LicenseStatus {
        tier: tier_str(info.tier).to_string(),
        source: current_source_str().to_string(),
        output_is_marked: info.output_is_marked,
    }
}
