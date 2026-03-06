//! LTV (Long Term Validation) and DSS (Document Security Store) support.
//!
//! Per ISO 32000-2 §12.8.7, the DSS dictionary stores validation-related
//! information (certificates, OCSP responses, CRLs) for offline verification.

use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Array, Dict};
use pdf_syntax::Pdf;

/// The Document Security Store (ISO 32000-2 §12.8.7.1).
#[derive(Debug, Clone)]
pub struct DocumentSecurityStore {
    /// DER-encoded certificates stored in /Certs.
    pub certificates: Vec<Vec<u8>>,
    /// DER-encoded OCSP responses stored in /OCSPs.
    pub ocsp_responses: Vec<Vec<u8>>,
    /// DER-encoded CRLs stored in /CRLs.
    pub crls: Vec<Vec<u8>>,
    /// Validation Related Information entries, keyed by signature hash.
    pub vri_entries: Vec<VriEntry>,
}

/// A Validation Related Information (VRI) entry.
///
/// Each VRI entry is associated with a specific signature and contains
/// the validation data needed to verify that signature.
#[derive(Debug, Clone)]
pub struct VriEntry {
    /// The key (hex-encoded SHA-1 hash of the signature value).
    pub key: String,
    /// DER-encoded certificates for this signature.
    pub certificates: Vec<Vec<u8>>,
    /// DER-encoded OCSP responses for this signature.
    pub ocsp_responses: Vec<Vec<u8>>,
    /// DER-encoded CRLs for this signature.
    pub crls: Vec<Vec<u8>>,
    /// Timestamp of when this VRI was created.
    pub timestamp: Option<String>,
}

impl DocumentSecurityStore {
    /// Extract the DSS from a PDF document, if present.
    pub fn from_pdf(pdf: &Pdf) -> Option<Self> {
        let xref = pdf.xref();
        let root: Dict<'_> = xref.get(xref.root_id())?;
        let dss: Dict<'_> = root.get(DSS)?;

        let certificates = extract_binary_array(&dss, CERTS);
        let ocsp_responses = extract_binary_array(&dss, OCSPS);
        let crls = extract_binary_array(&dss, CRLS);

        let vri_entries = if let Some(vri_dict) = dss.get::<Dict<'_>>(VRI) {
            parse_vri_entries(&vri_dict)
        } else {
            Vec::new()
        };

        Some(Self {
            certificates,
            ocsp_responses,
            crls,
            vri_entries,
        })
    }

    /// Check if the DSS contains any LTV data.
    pub fn has_ltv_data(&self) -> bool {
        !self.certificates.is_empty() || !self.ocsp_responses.is_empty() || !self.crls.is_empty()
    }

    /// Find a VRI entry for a specific signature hash.
    pub fn vri_for_signature(&self, sig_hash: &str) -> Option<&VriEntry> {
        let upper = sig_hash.to_uppercase();
        self.vri_entries
            .iter()
            .find(|v| v.key.to_uppercase() == upper)
    }
}

/// Extract an array of binary streams from a DSS dictionary.
fn extract_binary_array(dict: &Dict<'_>, key: &[u8]) -> Vec<Vec<u8>> {
    dict.get::<Array<'_>>(key)
        .map(|arr| {
            arr.iter::<pdf_syntax::object::Stream<'_>>()
                .filter_map(|s| s.decoded().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Parse VRI entries from the /VRI dictionary.
///
/// VRI is a dictionary where keys are hex-encoded SHA-1 hashes of
/// the signature /Contents value, and values are dictionaries.
fn parse_vri_entries(vri_dict: &Dict<'_>) -> Vec<VriEntry> {
    let mut entries = Vec::new();
    for (name, _) in vri_dict.entries() {
        let key = std::str::from_utf8(name.as_ref()).unwrap_or("").to_string();
        if let Some(entry_dict) = vri_dict.get::<Dict<'_>>(name.as_ref()) {
            // VRI entries use /Cert, /OCSP, /CRL (not /Certs, /OCSPs, /CRLs).
            let certificates = extract_binary_array(&entry_dict, CERT);
            let ocsp_responses = extract_binary_array(&entry_dict, OCSP);
            let crls = extract_binary_array(&entry_dict, CRL);
            let timestamp = entry_dict
                .get::<pdf_syntax::object::String>(TU)
                .map(|s| String::from_utf8_lossy(s.as_bytes()).to_string());
            entries.push(VriEntry {
                key,
                certificates,
                ocsp_responses,
                crls,
                timestamp,
            });
        }
    }
    entries
}

/// Compute the SHA-1 hash of signature contents for VRI lookup.
///
/// The VRI key is the uppercase hex SHA-1 of the raw /Contents bytes.
pub fn compute_vri_key(sig_contents: &[u8]) -> String {
    use sha1::Digest;
    let hash = sha1::Sha1::new_with_prefix(sig_contents).finalize();
    hash.iter().map(|b| format!("{b:02X}")).collect::<String>()
}
