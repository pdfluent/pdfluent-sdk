//! PDF digital signature validation and signing.
//!
//! Provides PAdES baseline signature validation, CMS/PKCS#7 parsing,
//! certificate chain verification, DocMDP/FieldMDP permission handling,
//! and LTV (Long Term Validation) support per ISO 32000-2 §12.8.

mod appearance;
mod byte_range;
mod chain;
pub mod cms;
pub mod crypto;
mod docmdp;
mod ltv;
mod sig_dict;
mod string_util;
mod types;
pub mod x509;

pub use appearance::*;
pub use byte_range::*;
pub use chain::*;
pub use cms::CmsSignedData;
pub use crypto::{verify_cms_signature, SignatureAlgorithm};
pub use docmdp::*;
pub use ltv::*;
pub use sig_dict::*;
pub use types::*;
pub use x509::X509Certificate;

use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Array, Dict, Name};
use pdf_syntax::Pdf;

/// Extract all signature fields from a PDF document.
///
/// Walks the AcroForm /Fields array looking for signature fields
/// (field type /Sig) and returns wrapped `SigDict` values for each.
pub fn signature_fields<'a>(pdf: &'a Pdf) -> Vec<SignatureInfo<'a>> {
    let xref = pdf.xref();
    let root: Dict<'_> = match xref.get(xref.root_id()) {
        Some(r) => r,
        None => return Vec::new(),
    };

    let acroform: Dict<'_> = match root.get(ACRO_FORM) {
        Some(af) => af,
        None => return Vec::new(),
    };

    let fields: Array<'_> = match acroform.get(FIELDS) {
        Some(f) => f,
        None => return Vec::new(),
    };

    let mut sigs = Vec::new();
    collect_sig_fields(&fields, &mut sigs, None);
    sigs
}

fn collect_sig_fields<'a>(
    fields: &Array<'a>,
    out: &mut Vec<SignatureInfo<'a>>,
    parent_name: Option<&str>,
) {
    for dict in fields.iter::<Dict<'_>>() {
        let partial = dict
            .get::<pdf_syntax::object::String>(T)
            .map(|s| string_util::pdf_string_to_string(&s));

        let fq_name = match (&parent_name, &partial) {
            (Some(p), Some(c)) => format!("{p}.{c}"),
            (None, Some(c)) => c.clone(),
            (Some(p), None) => p.to_string(),
            (None, None) => String::new(),
        };

        // Check if this dict itself is a /Sig field with /V, even if it has /Kids.
        let is_sig = dict.get::<Name>(FT).is_some_and(|n| n.as_ref() == b"Sig");
        if is_sig {
            if let Some(v) = dict.get::<Dict<'_>>(V) {
                let sig = SigDict::from_dict(v);
                out.push(SignatureInfo {
                    field_name: fq_name.clone(),
                    sig,
                    field_dict: dict.clone(),
                });
            }
        }

        // Recurse into child fields (/Kids).
        if let Some(kids) = dict.get::<Array<'_>>(KIDS) {
            collect_sig_fields(&kids, out, Some(&fq_name));
        }
    }
}

/// A discovered signature field with its parsed signature dictionary.
pub struct SignatureInfo<'a> {
    /// Fully qualified field name.
    pub field_name: String,
    /// The parsed signature dictionary (/V value).
    pub sig: SigDict<'a>,
    /// The raw field dictionary (for Lock, SV, etc.).
    pub field_dict: Dict<'a>,
}

/// Validate all signatures in a PDF document.
///
/// Returns a `ValidationResult` for each signature field found.
pub fn validate_signatures(pdf: &Pdf) -> Vec<ValidationResult> {
    let data = pdf.data().as_ref();
    let sigs = signature_fields(pdf);
    let mut results = Vec::new();

    for info in &sigs {
        let result = validate_one(&info.sig, data, &info.field_name);
        results.push(result);
    }

    results
}

fn validate_one(sig: &SigDict<'_>, pdf_data: &[u8], field_name: &str) -> ValidationResult {
    // Step 1: Verify byte range digest.
    let byte_range = match sig.byte_range() {
        Some(br) => br,
        None => {
            return ValidationResult {
                status: ValidationStatus::Invalid("missing /ByteRange".into()),
                field_name: field_name.to_string(),
                signer: None,
                timestamp: None,
                sub_filter: sig.sub_filter(),
            };
        }
    };

    let digest_ok = match sig.sub_filter() {
        Some(SubFilter::AdbePkcs7Detached | SubFilter::EtsiCadesDetached) => {
            verify_byte_range_digest(pdf_data, &byte_range, sig)
        }
        Some(SubFilter::AdbePkcs7Sha1) => verify_byte_range_digest(pdf_data, &byte_range, sig),
        _ => DigestVerification::Unsupported,
    };

    let signer = sig.cms_signed_data().and_then(|sd| sd.signer_common_name());

    let timestamp = sig.signing_time();

    let status = match digest_ok {
        DigestVerification::Ok => {
            // Step 2: Check CMS structural integrity and verify crypto signature.
            match sig.cms_signed_data() {
                Some(sd) => {
                    if !sd.verify_structural_integrity() {
                        ValidationStatus::Invalid("CMS signature structurally invalid".into())
                    } else {
                        let certs = sd.certificates();
                        if certs.is_empty() {
                            ValidationStatus::Invalid("no certificates in CMS".into())
                        } else {
                            let leaf = &certs[0];
                            match sd.signed_attributes_raw() {
                                Some(signed_attrs) => {
                                    match crypto::verify_cms_signature(
                                        signed_attrs,
                                        sd.signature_value(),
                                        &leaf.spki_raw,
                                        sd.signature_algorithm_oid(),
                                    ) {
                                        Ok(true) => ValidationStatus::Valid,
                                        Ok(false) => ValidationStatus::Invalid(
                                            "cryptographic signature verification failed".into(),
                                        ),
                                        Err(e) => ValidationStatus::Unknown(format!(
                                            "crypto verification error: {e}"
                                        )),
                                    }
                                }
                                None => ValidationStatus::Unknown(
                                    "no signed attributes for verification".into(),
                                ),
                            }
                        }
                    }
                }
                None => ValidationStatus::Invalid("cannot parse CMS SignedData".into()),
            }
        }
        DigestVerification::Mismatch => {
            ValidationStatus::Invalid("byte range digest mismatch — document modified".into())
        }
        DigestVerification::Unsupported => {
            ValidationStatus::Unknown("unsupported SubFilter".into())
        }
        DigestVerification::Error(e) => ValidationStatus::Invalid(e),
    };

    ValidationResult {
        status,
        field_name: field_name.to_string(),
        signer,
        timestamp,
        sub_filter: sig.sub_filter(),
    }
}
