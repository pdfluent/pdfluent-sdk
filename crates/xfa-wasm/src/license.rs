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
    match e {
        pdfluent::Error::InvalidLicense { reason } => {
            if reason.contains("already set") {
                JsValue::from_str(&format!("license already set: {reason}"))
            } else {
                JsValue::from_str(&format!("invalid license: {reason}"))
            }
        }
        other => JsValue::from_str(&format!("license error: {other}")),
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
