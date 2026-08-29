//! LTV (Long Term Validation) and DSS (Document Security Store) support.
//!
//! Per ISO 32000-2 §12.8.7, the DSS dictionary stores validation-related
//! information (certificates, OCSP responses, CRLs) for offline verification.

use lopdf::{Dictionary, Document, Object, Stream};
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

/// Embed a DSS (Document Security Store) as an incremental update to a signed PDF.
///
/// Per ISO 32000-2 §12.8.7 and ETSI EN 319 102-1, the DSS is appended outside
/// the signed byte range so existing signature integrity is preserved. This is
/// the standard mechanism for adding LTV (Long-Term Validation) data after signing.
///
/// `certificates` — DER-encoded certificates to place in /Certs.
/// `ocsp_responses` — DER-encoded OCSP responses for /OCSPs.
/// `crls` — DER-encoded CRLs for /CRLs.
/// `vri_entries` — per-signature VRI entries for /VRI.
pub fn embed_dss_incremental(
    pdf_bytes: &[u8],
    certificates: Vec<Vec<u8>>,
    ocsp_responses: Vec<Vec<u8>>,
    crls: Vec<Vec<u8>>,
    vri_entries: Vec<VriEntry>,
) -> Result<Vec<u8>, String> {
    let prev = Document::load_mem(pdf_bytes).map_err(|e| format!("load PDF for DSS embed: {e}"))?;
    let mut doc = Document::new_from_prev(&prev);

    // Add certificate streams to the document, collecting indirect-reference objects.
    let cert_refs: Vec<Object> = certificates
        .iter()
        .map(|data| {
            let s = Stream::new(Dictionary::new(), data.clone());
            Object::Reference(doc.add_object(Object::Stream(s)))
        })
        .collect();

    let ocsp_refs: Vec<Object> = ocsp_responses
        .iter()
        .map(|data| {
            let s = Stream::new(Dictionary::new(), data.clone());
            Object::Reference(doc.add_object(Object::Stream(s)))
        })
        .collect();

    let crl_refs: Vec<Object> = crls
        .iter()
        .map(|data| {
            let s = Stream::new(Dictionary::new(), data.clone());
            Object::Reference(doc.add_object(Object::Stream(s)))
        })
        .collect();

    // Build /VRI sub-dictionary.
    let mut vri_dict = Dictionary::new();
    for entry in &vri_entries {
        let mut e = Dictionary::new();

        let ecerts: Vec<Object> = entry
            .certificates
            .iter()
            .map(|data| {
                let s = Stream::new(Dictionary::new(), data.clone());
                Object::Reference(doc.add_object(Object::Stream(s)))
            })
            .collect();
        if !ecerts.is_empty() {
            e.set("Cert", Object::Array(ecerts));
        }

        let eocsps: Vec<Object> = entry
            .ocsp_responses
            .iter()
            .map(|data| {
                let s = Stream::new(Dictionary::new(), data.clone());
                Object::Reference(doc.add_object(Object::Stream(s)))
            })
            .collect();
        if !eocsps.is_empty() {
            e.set("OCSP", Object::Array(eocsps));
        }

        let ecrls: Vec<Object> = entry
            .crls
            .iter()
            .map(|data| {
                let s = Stream::new(Dictionary::new(), data.clone());
                Object::Reference(doc.add_object(Object::Stream(s)))
            })
            .collect();
        if !ecrls.is_empty() {
            e.set("CRL", Object::Array(ecrls));
        }

        if let Some(ts) = &entry.timestamp {
            e.set("TU", Object::string_literal(ts.as_bytes()));
        }

        vri_dict.set(entry.key.as_bytes(), Object::Dictionary(e));
    }

    // Build /DSS dictionary.
    let mut dss = Dictionary::new();
    if !cert_refs.is_empty() {
        dss.set("Certs", Object::Array(cert_refs));
    }
    if !ocsp_refs.is_empty() {
        dss.set("OCSPs", Object::Array(ocsp_refs));
    }
    if !crl_refs.is_empty() {
        dss.set("CRLs", Object::Array(crl_refs));
    }
    if !vri_dict.is_empty() {
        dss.set("VRI", Object::Dictionary(vri_dict));
    }

    let dss_id = doc.add_object(Object::Dictionary(dss));

    // For an incremental update, the catalog lives in `prev`'s object table —
    // the new `doc` has an empty objects map. Get the catalog ID from the
    // trailer, clone the dict from prev, add /DSS, then re-write it in `doc`
    // with the same ID so the incremental xref shadows the original entry.
    let catalog_id = prev
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .ok_or_else(|| "no /Root in trailer".to_string())?;
    let mut catalog = prev
        .get_dictionary(catalog_id)
        .map_err(|e| {
            // lopdf takes the decryption route when the trailer carries
            // /Encrypt. If that route fails the object table stays empty, so
            // every lookup returns "object ID ... not found" -- a message that
            // points at the cross-reference table while the cause is
            // encryption. After a successful decrypt the reader removes
            // /Encrypt from the trailer, so its presence here means exactly
            // "loaded, not decrypted".
            //
            // Deliberately the trailer predicate and not is_encrypted(): that
            // one requires /Encrypt to be a reference, and a direct dictionary
            // is legal and does occur, so the check would silently do nothing.
            if prev.trailer.get(b"Encrypt").is_ok() {
                "catalog: document is encrypted and could not be decrypted; \
                 LTV/DSS embedding requires a decrypted document"
                    .to_string()
            } else {
                format!("catalog: {e}")
            }
        })?
        .clone();
    catalog.set("DSS", Object::Reference(dss_id));
    doc.set_object(catalog_id, Object::Dictionary(catalog));

    // Save the incremental portion and prepend the original bytes.
    let mut incremental = Vec::new();
    doc.save_to(&mut incremental)
        .map_err(|e| format!("save DSS increment: {e}"))?;

    let mut result = pdf_bytes.to_vec();
    result.extend_from_slice(&incremental);
    Ok(result)
}

#[cfg(test)]
mod vri_key_tests {
    use super::*;

    /// De VRI-sleutel is de sleutel waaronder een lezer de validatiegegevens van
    /// één handtekening terugvindt in de DSS. Klopt hij niet, dan staan die
    /// gegevens er wel maar vindt niemand ze, en valt LTV-validatie terug op
    /// "geen informatie" in plaats van op een fout — stil, en precies bij de
    /// handtekeningen die het langst moeten meegaan.
    ///
    /// Getoetst tegen de SHA-1-standaard zelf, niet tegen onze eigen uitvoer:
    /// anders legt de test alleen vast wat de code nu toevallig doet.
    #[test]
    fn vri_key_is_uppercase_hex_sha1() {
        // NIST-vectoren voor SHA-1.
        assert_eq!(
            compute_vri_key(b""),
            "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709",
            "SHA-1 van de lege invoer"
        );
        assert_eq!(
            compute_vri_key(b"abc"),
            "A9993E364706816ABA3E25717850C26C9CD0D89D",
            "SHA-1 van \"abc\""
        );
    }

    #[test]
    fn vri_key_is_forty_uppercase_hex_characters() {
        // ISO 32000-2 §12.8.4.3 wil hoofdletters; een lezer die op de sleutel
        // matcht vindt een kleine-letterversie niet.
        let sleutel = compute_vri_key(&[0x30, 0x82, 0x01, 0x00, 0xFF]);
        assert_eq!(sleutel.len(), 40, "SHA-1 is 20 bytes, dus 40 hextekens");
        assert!(
            sleutel
                .chars()
                .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c)),
            "moet hoofdletter-hex zijn, kreeg {sleutel}"
        );
    }

    #[test]
    fn different_signatures_get_different_keys() {
        assert_ne!(
            compute_vri_key(b"handtekening-een"),
            compute_vri_key(b"handtekening-twee")
        );
    }

    /// De ruwe /Contents van een handtekening is gevuld met nullen tot zijn
    /// gereserveerde lengte. Die nullen horen mee te tellen: een lezer hasht
    /// wat er in het bestand staat, niet wat wij ervan zouden willen afknippen.
    #[test]
    fn trailing_zero_padding_changes_the_key() {
        let kaal = compute_vri_key(b"\x30\x82\x01\x00");
        let met_opvulling = compute_vri_key(b"\x30\x82\x01\x00\x00\x00\x00\x00");
        assert_ne!(kaal, met_opvulling, "opvulling hoort de hash te veranderen");
    }
}
