//! Typed `PdfluentError` for the WASM binding.
//!
//! Every fallible public WASM method throws a JavaScript `Error` instance whose
//! `.name` is `"PdfluentError"`. The thrown value carries machine-inspectable
//! properties:
//!
//! - `code` — stable C8 catalogue code in the `E-<CATEGORY>-<SPECIFIC>`
//!   format (e.g. `"E-PARSE-INVALID-PDF"`). Matches the codes returned by the
//!   Rust `pdfluent::Error::code()` method and the C ABI / Python bindings.
//! - `message` — human-readable string, kept stable for backward compatibility.
//! - `operation` — short identifier of the API call that failed.
//! - `help` — optional actionable hint (may be empty string).
//! - `docsUrl` — deep-link into `https://pdfluent.com/errors/<code>`.
//! - `legacyCode` — the original SCREAMING_SNAKE_CASE identifier from the
//!   pre-1.0 WASM binding, kept stable for existing user code.

use js_sys::Reflect;
use wasm_bindgen::prelude::*;

use pdf_engine::api_error::PdfError;

/// JS-side `PdfluentError` class declaration. Cached on `globalThis` so all
/// errors thrown from this WASM module share the same prototype and
/// `instanceof PdfluentError` works regardless of bundling.
#[wasm_bindgen(inline_js = r#"
export function __pdfluent_error_ctor() {
  if (globalThis.__PdfluentError) {
    return globalThis.__PdfluentError;
  }
  class PdfluentError extends Error {
    constructor(message, code, operation, help, docsUrl, legacyCode) {
      super(message);
      this.name = "PdfluentError";
      this.code = code;
      this.operation = operation;
      this.help = help;
      this.docsUrl = docsUrl;
      this.legacyCode = legacyCode;
    }
  }
  globalThis.__PdfluentError = PdfluentError;
  return PdfluentError;
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = "__pdfluent_error_ctor")]
    fn pdfluent_error_ctor() -> JsValue;
}

/// C8 catalogue codes (`E-<CATEGORY>-<SPECIFIC>`).
pub mod code {
    /// PDF is structurally invalid.
    pub const PARSE_INVALID_PDF: &str = "E-PARSE-INVALID-PDF";
    /// Generic I/O failure.
    pub const IO_GENERIC: &str = "E-IO-GENERIC";
    /// PDF version is newer than supported.
    pub const PARSE_UNSUPPORTED_VERSION: &str = "E-PARSE-UNSUPPORTED-VERSION";
    /// PDF/A validation failed.
    pub const COMPLIANCE_PDFA_INVALID: &str = "E-COMPLIANCE-PDFA-INVALID";
    /// License key invalid or malformed.
    pub const LICENSE_INVALID: &str = "E-LICENSE-INVALID";
    /// Required feature not in tier.
    pub const LICENSE_FEATURE_NOT_IN_TIER: &str = "E-LICENSE-FEATURE-NOT-IN-TIER";
    /// Signed payload has expired.
    pub const LICENSE_EXPIRED: &str = "E-LICENSE-EXPIRED";
    /// Signed payload's Ed25519 signature does not verify.
    pub const LICENSE_INVALID_SIGNATURE: &str = "E-LICENSE-INVALID-SIGNATURE";
    /// Runtime rate / usage limit exceeded.
    pub const LICENSE_RATE_LIMITED: &str = "E-LICENSE-RATE-LIMITED";
    /// Capability requires a feature flag not compiled into this build.
    pub const LICENSE_CAPABILITY_NOT_COMPILED: &str = "E-LICENSE-CAPABILITY-NOT-COMPILED";
    /// Operation is unsupported on the WebAssembly target.
    pub const ENV_UNSUPPORTED_ON_WASM: &str = "E-ENV-UNSUPPORTED-ON-WASM";
    /// Internal safety-net.
    pub const INTERNAL: &str = "E-INTERNAL";
    /// Invalid argument passed to a public WASM method.
    pub const WASM_INVALID_ARGUMENT: &str = "E-WASM-INVALID-ARGUMENT";
    /// Page index outside `0..page_count`.
    pub const WASM_PAGE_OUT_OF_RANGE: &str = "E-WASM-PAGE-OUT-OF-RANGE";
    /// Render call failed.
    pub const WASM_RENDER_FAILED: &str = "E-WASM-RENDER-FAILED";
    /// Vector renderer surfaced an unsupported feature.
    pub const WASM_RENDER_FALLBACK: &str = "E-WASM-RENDER-FALLBACK";
    /// Text extraction failed.
    pub const WASM_TEXT_EXTRACT_FAILED: &str = "E-WASM-TEXT-EXTRACT-FAILED";
    /// XFA flatten/extract failed.
    pub const WASM_XFA_FAILED: &str = "E-WASM-XFA-FAILED";
    /// Merge step failed.
    pub const WASM_MERGE_FAILED: &str = "E-WASM-MERGE-FAILED";
    /// Save / serialise step failed.
    pub const WASM_SAVE_FAILED: &str = "E-WASM-SAVE-FAILED";
    /// JSON parse / serialise failure on the WASM boundary.
    pub const WASM_INVALID_JSON: &str = "E-WASM-INVALID-JSON";
    /// FormCalc scripting failed.
    pub const WASM_FORMCALC_FAILED: &str = "E-WASM-FORMCALC-FAILED";
}

/// Legacy SCREAMING_SNAKE_CASE codes preserved on the `legacyCode` property.
pub mod legacy_code {
    pub const INVALID_PDF: &str = "INVALID_PDF";
    pub const INVALID_JSON: &str = "INVALID_JSON";
    pub const INVALID_ARGUMENT: &str = "INVALID_ARGUMENT";
    pub const PAGE_OUT_OF_RANGE: &str = "PAGE_OUT_OF_RANGE";
    pub const FORMCALC_ERROR: &str = "FORMCALC_ERROR";
    pub const SERIALIZE_ERROR: &str = "SERIALIZE_ERROR";
    pub const RENDER_ERROR: &str = "RENDER_ERROR";
    pub const RENDER_FALLBACK: &str = "RENDER_FALLBACK";
    pub const TEXT_EXTRACT_FAILED: &str = "TEXT_EXTRACT_FAILED";
    pub const XFA_FLATTEN_FAILED: &str = "XFA_FLATTEN_FAILED";
    pub const MERGE_FAILED: &str = "MERGE_FAILED";
    pub const SAVE_FAILED: &str = "SAVE_FAILED";
    pub const OPERATION_FAILED: &str = "OPERATION_FAILED";
    pub const PDFA_CLEANUP_FAILED: &str = "PDFA_CLEANUP_FAILED";
    pub const COLORSPACE_ERROR: &str = "COLORSPACE_ERROR";
    pub const XMP_REPAIR_FAILED: &str = "XMP_REPAIR_FAILED";
    pub const LICENSE_ERROR: &str = "LICENSE_ERROR";
    pub const LICENSE_ALREADY_SET: &str = "LICENSE_ALREADY_SET";
}

fn build(code: &str, message: &str, operation: &str, help: &str, legacy_code: &str) -> JsValue {
    let docs_url = format!("https://pdfluent.com/errors/{code}");
    let ctor_val = pdfluent_error_ctor();

    if !ctor_val.is_function() {
        let err = js_sys::Error::new(&format!("[{code}] {message}"));
        err.set_name("PdfluentError");
        let val: &JsValue = err.as_ref();
        let _ = Reflect::set(val, &JsValue::from_str("code"), &JsValue::from_str(code));
        let _ = Reflect::set(
            val,
            &JsValue::from_str("operation"),
            &JsValue::from_str(operation),
        );
        let _ = Reflect::set(val, &JsValue::from_str("help"), &JsValue::from_str(help));
        let _ = Reflect::set(
            val,
            &JsValue::from_str("docsUrl"),
            &JsValue::from_str(&docs_url),
        );
        let _ = Reflect::set(
            val,
            &JsValue::from_str("legacyCode"),
            &JsValue::from_str(legacy_code),
        );
        return err.into();
    }

    let ctor: js_sys::Function = ctor_val.unchecked_into();
    let args = js_sys::Array::new();
    args.push(&JsValue::from_str(message));
    args.push(&JsValue::from_str(code));
    args.push(&JsValue::from_str(operation));
    args.push(&JsValue::from_str(help));
    args.push(&JsValue::from_str(&docs_url));
    args.push(&JsValue::from_str(legacy_code));

    match Reflect::construct(&ctor, &args) {
        Ok(obj) => obj,
        Err(_) => JsValue::from_str(message),
    }
}

/// Public constructor: caller supplies C8 code, operation label, legacy code.
pub fn pdfluent_error(
    operation: &str,
    code: &str,
    legacy_code: &str,
    message: &str,
    help: &str,
) -> JsValue {
    build(code, message, operation, help, legacy_code)
}

/// Lift a `pdf_engine::api_error::PdfError` into a `PdfluentError`.
pub fn pdf_engine_error<E: PdfError>(operation: &str, e: E) -> JsValue {
    let engine_code = e.code();
    let (c8, legacy) = match engine_code {
        "FILE_NOT_FOUND" => (code::IO_GENERIC, "FILE_NOT_FOUND"),
        "CORRUPT_PDF" => (code::PARSE_INVALID_PDF, legacy_code::INVALID_PDF),
        "INVALID_PAGE_NUMBER" => (code::WASM_PAGE_OUT_OF_RANGE, legacy_code::PAGE_OUT_OF_RANGE),
        "UNSUPPORTED_PDF_VERSION" => (code::PARSE_UNSUPPORTED_VERSION, "UNSUPPORTED_PDF_VERSION"),
        "LICENSE_EXPIRED" | "LICENSE_INVALID" => {
            (code::LICENSE_INVALID, legacy_code::LICENSE_ERROR)
        }
        _ => (code::INTERNAL, legacy_code::OPERATION_FAILED),
    };
    let help = e.help().unwrap_or_default();
    build(c8, &e.to_string(), operation, &help, legacy)
}
