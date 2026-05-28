//! C ABI license activation surface.
//!
//! Wraps [`pdfluent::set_license_key`] and [`pdfluent::license_info`] for
//! consumers that drive the engine through the C boundary (Java/JNI, .NET
//! P/Invoke, native applications).
//!
//! Key strings are passed straight through to the Rust core and never logged
//! or stored locally.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::sync::atomic::{AtomicU8, Ordering};

use pdfluent::Tier;

use crate::error::{set_last_error_str, PdfStatusForLicense};
use crate::types::PdfStatus;

/// Numeric source of the currently-effective tier.
///
/// Mirrored at the binding boundary so callers can render a meaningful
/// "license loaded from …" message.
#[repr(u8)]
#[derive(Clone, Copy)]
enum SourceTag {
    /// No key has been provided this process. The active tier is Trial.
    Default = 0,
    /// A key was resolved from the `PDFLUENT_LICENSE_KEY` environment variable.
    EnvVar = 1,
    /// A key was set explicitly via `pdfluent_license_activate_*`.
    Explicit = 2,
}

/// Tracks the most recently observed activation source. Updated only on
/// successful activation calls; never holds the key itself.
static ACTIVATION_SOURCE: AtomicU8 = AtomicU8::new(SourceTag::Default as u8);

/// C-compatible license status payload.
#[repr(C)]
pub struct PdfluentLicenseStatus {
    /// Effective tier: 0=Trial, 1=Developer, 2=Team, 3=Business, 4=Enterprise.
    pub tier: c_int,
    /// Source: 0=Default, 1=EnvVar, 2=Explicit.
    pub source: c_int,
    /// Whether the current tier marks output via /Producer metadata (1) or
    /// not (0). Only Trial marks output.
    pub output_is_marked: c_int,
}

fn tier_to_int(t: Tier) -> c_int {
    match t {
        Tier::Trial => 0,
        Tier::Developer => 1,
        Tier::Team => 2,
        Tier::Business => 3,
        Tier::Enterprise => 4,
        // `Tier` is non_exhaustive; a future variant maps to -1 until the
        // binding layer adds an explicit code for it.
        _ => -1,
    }
}

fn record_source(tag: SourceTag) {
    ACTIVATION_SOURCE.store(tag as u8, Ordering::Relaxed);
}

fn current_source() -> SourceTag {
    match ACTIVATION_SOURCE.load(Ordering::Relaxed) {
        2 => SourceTag::Explicit,
        1 => SourceTag::EnvVar,
        _ => {
            // No explicit activation yet — if the env var is currently set
            // and parses, the active tier is env-derived.
            if let Ok(key) = std::env::var("PDFLUENT_LICENSE_KEY") {
                if pdfluent::license_info().tier != Tier::Trial && !key.is_empty() {
                    return SourceTag::EnvVar;
                }
            }
            SourceTag::Default
        }
    }
}

/// Activate the process-global license from a key string.
///
/// On success returns [`PdfStatus::Ok`] (0). On failure returns one of:
/// - [`PdfStatus::ErrorInvalidArgument`] if `key` is null or not UTF-8.
/// - `PdfStatus::ErrorInvalidLicense` if the key does not match the expected
///   format (see [`pdfluent::set_license_key`]).
/// - `PdfStatus::ErrorLicenseAlreadySet` if the process has already been
///   activated to a different tier this run.
///
/// The error message is available via [`pdf_get_last_error`](crate::error::pdf_get_last_error).
///
/// # Safety
/// `key` must be a valid null-terminated string when non-null.
#[no_mangle]
pub unsafe extern "C" fn pdfluent_license_activate_key(key: *const c_char) -> PdfStatus {
    if key.is_null() {
        set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let s = match unsafe { CStr::from_ptr(key) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error_str("license key is not valid UTF-8");
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    match pdfluent::set_license_key(s) {
        Ok(()) => {
            record_source(SourceTag::Explicit);
            PdfStatus::Ok
        }
        Err(e) => PdfStatus::from_pdfluent_error(&e),
    }
}

/// Activate the process-global license by reading a key from a UTF-8 text
/// file. The file's full contents (trimmed) are passed to
/// [`pdfluent_license_activate_key`].
///
/// Returns [`PdfStatus::ErrorLicenseFile`] (18) if the file cannot be read.
///
/// # Safety
/// `path` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pdfluent_license_activate_file(path: *const c_char) -> PdfStatus {
    if path.is_null() {
        set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error_str("path is not valid UTF-8");
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let contents = match std::fs::read_to_string(path_str) {
        Ok(c) => c,
        Err(e) => {
            set_last_error_str(&format!("could not read license file: {e}"));
            return PdfStatus::ErrorLicenseFile;
        }
    };
    let key = contents.trim();
    match pdfluent::set_license_key(key) {
        Ok(()) => {
            record_source(SourceTag::Explicit);
            PdfStatus::Ok
        }
        Err(e) => PdfStatus::from_pdfluent_error(&e),
    }
}

/// Return the effective tier as a numeric value (0..4). See [`PdfluentLicenseStatus::tier`].
#[no_mangle]
pub extern "C" fn pdfluent_license_effective_tier() -> c_int {
    tier_to_int(pdfluent::license_info().tier)
}

/// Fill `out` with the current license status. Returns
/// [`PdfStatus::ErrorInvalidArgument`] if `out` is null.
///
/// # Safety
/// `out` must point to a valid, writable `PdfluentLicenseStatus`.
#[no_mangle]
pub unsafe extern "C" fn pdfluent_license_status(out: *mut PdfluentLicenseStatus) -> PdfStatus {
    if out.is_null() {
        set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let info = pdfluent::license_info();
    unsafe {
        (*out).tier = tier_to_int(info.tier);
        (*out).source = current_source() as c_int;
        (*out).output_is_marked = if info.output_is_marked { 1 } else { 0 };
    }
    PdfStatus::Ok
}

/// Activate the process-global license from a **signed JSON payload**
/// (Ed25519-verified).
///
/// `payload_json` must be a null-terminated UTF-8 string containing the
/// full signed license JSON (`{"payload": {…}, "signature": "…"}`).
///
/// The public verification key must already have been injected via
/// [`pdfluent_license_set_public_key`]; otherwise this call returns
/// [`PdfStatus::ErrorInvalidLicense`].
///
/// Returns:
/// - [`PdfStatus::Ok`] on successful activation;
/// - [`PdfStatus::ErrorLicenseInvalidSignature`] (=20) if the signature
///   does not verify;
/// - [`PdfStatus::ErrorLicenseExpired`] (=19) if `expires_at` is in the
///   past;
/// - [`PdfStatus::ErrorLicenseAlreadySet`] (=17) if the process is
///   already activated to a different tier;
/// - [`PdfStatus::ErrorInvalidLicense`] (=16) for malformed JSON,
///   unknown tier names, or missing public key.
///
/// On any error the process-global tier is **NOT** modified.
///
/// # Safety
/// `payload_json` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pdfluent_license_activate_payload(
    payload_json: *const c_char,
) -> PdfStatus {
    if payload_json.is_null() {
        set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let s = match unsafe { CStr::from_ptr(payload_json) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error_str("license payload is not valid UTF-8");
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    match pdfluent::set_license_payload(s) {
        Ok(()) => {
            record_source(SourceTag::Explicit);
            PdfStatus::Ok
        }
        Err(e) => PdfStatus::from_pdfluent_error(&e),
    }
}

/// Inject the public Ed25519 verification key used by
/// [`pdfluent_license_activate_payload`].
///
/// Must be called once at process startup before any
/// `pdfluent_license_activate_payload` call. Calling twice with the SAME
/// key is idempotent; calling with a DIFFERENT key returns
/// [`PdfStatus::ErrorInvalidLicense`].
///
/// `public_key` must be a 32-byte buffer (raw Ed25519 verifying key).
///
/// # Safety
/// `public_key` must point to a readable buffer of at least `key_len`
/// bytes, and `key_len` must equal 32.
#[no_mangle]
pub unsafe extern "C" fn pdfluent_license_set_public_key(
    public_key: *const u8,
    key_len: usize,
) -> PdfStatus {
    if public_key.is_null() {
        set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    if key_len != 32 {
        set_last_error_str("public key must be exactly 32 bytes");
        return PdfStatus::ErrorInvalidArgument;
    }
    let slice = unsafe { std::slice::from_raw_parts(public_key, key_len) };
    match pdfluent::set_license_public_key(slice) {
        Ok(()) => PdfStatus::Ok,
        Err(e) => PdfStatus::from_pdfluent_error(&e),
    }
}
