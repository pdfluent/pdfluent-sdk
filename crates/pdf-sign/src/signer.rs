//! PDF signing trait and PKCS#12 signer implementation.
//!
//! Provides the [`PdfSigner`] trait for pluggable signature creation and
//! [`Pkcs12Signer`] which loads a `.p12` / `.pfx` identity and signs
//! using RSA PKCS#1 v1.5 or ECDSA (P-256 / P-384).

use crate::byte_range::DigestAlgorithm;
use crate::cms::parse_tlv;
use thiserror::Error;

/// Errors that can occur during signing.
#[derive(Debug, Error)]
pub enum SignError {
    /// Failed to load or parse the PKCS#12 file.
    #[error("PKCS#12 load error: {0}")]
    Pkcs12Load(String),
    /// Unsupported key type in the identity.
    #[error("unsupported key type: {0}")]
    UnsupportedKeyType(String),
    /// Cryptographic signing operation failed.
    #[error("signing failed: {0}")]
    SigningFailed(String),
    /// CMS structure construction failed.
    #[error("CMS build error: {0}")]
    CmsBuild(String),
    /// No private key found in the identity.
    #[error("no private key in PKCS#12")]
    NoPrivateKey,
    /// No certificate found in the identity.
    #[error("no certificate in PKCS#12")]
    NoCertificate,
}

/// Trait for PDF signature creation.
///
/// Implementations produce a DER-encoded CMS SignedData for the given
/// data bytes (typically the byte-range content of the PDF).
pub trait PdfSigner: Send + Sync {
    /// Sign the given data and return a DER-encoded CMS SignedData.
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, SignError>;

    /// Return the DER-encoded certificate chain (leaf first).
    fn certificate_chain_der(&self) -> &[Vec<u8>];

    /// Return the digest algorithm used by this signer.
    fn digest_algorithm(&self) -> DigestAlgorithm;

    /// Return the signature algorithm OID bytes.
    fn signature_algorithm_oid(&self) -> &[u8];

    /// Return an estimated upper bound for the CMS signature size in bytes.
    ///
    /// Used to pre-allocate the /Contents placeholder in the PDF.
    fn estimated_signature_size(&self) -> usize {
        8192
    }
}

/// The type of private key loaded from PKCS#12.
enum PrivateKey {
    Rsa(rsa::RsaPrivateKey),
    EcP256(p256::ecdsa::SigningKey),
    EcP384(p384::ecdsa::SigningKey),
}

/// A signer backed by a PKCS#12 (.p12 / .pfx) identity.
pub struct Pkcs12Signer {
    key: PrivateKey,
    cert_chain: Vec<Vec<u8>>,
    digest_algo: DigestAlgorithm,
    sig_algo_oid: Vec<u8>,
}

// OIDs for key type detection from PKCS#8 PrivateKeyInfo.
const OID_RSA_ENCRYPTION: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];
const OID_EC_PUBLIC_KEY: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01];
const OID_P256: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07];
const OID_P384: &[u8] = &[0x2B, 0x81, 0x04, 0x00, 0x22];

// Signature algorithm OIDs.
const OID_SHA256_WITH_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B];
const OID_ECDSA_SHA256: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02];
const OID_ECDSA_SHA384: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x03];

impl Pkcs12Signer {
    /// Load a PKCS#12 identity from DER bytes with the given password.
    pub fn from_pkcs12(data: &[u8], password: &str) -> Result<Self, SignError> {
        let pfx = p12::PFX::parse(data).map_err(|e| SignError::Pkcs12Load(format!("{e:?}")))?;

        // Verify MAC if present.
        if !pfx.verify_mac(password) {
            return Err(SignError::Pkcs12Load(
                "MAC verification failed (wrong password?)".into(),
            ));
        }

        // Extract private key bags.
        let key_bags = pfx
            .key_bags(password)
            .map_err(|e| SignError::Pkcs12Load(format!("key extraction: {e:?}")))?;
        let key_der = key_bags.first().ok_or(SignError::NoPrivateKey)?;

        // Extract certificate bags.
        let cert_bags = pfx
            .cert_x509_bags(password)
            .map_err(|e| SignError::Pkcs12Load(format!("cert extraction: {e:?}")))?;
        if cert_bags.is_empty() {
            return Err(SignError::NoCertificate);
        }

        // Detect key type from PrivateKeyInfo AlgorithmIdentifier.
        let (key, digest_algo, sig_algo_oid) = detect_and_load_key(key_der)?;

        Ok(Self {
            key,
            cert_chain: cert_bags,
            digest_algo,
            sig_algo_oid,
        })
    }

    /// Return the signer's leaf certificate common name, if available.
    pub fn signer_name(&self) -> Option<String> {
        self.cert_chain
            .first()
            .and_then(|der| crate::x509::X509Certificate::from_der(der))
            .and_then(|cert| cert.subject_common_name())
    }

    /// Produce the raw signature bytes over the given data.
    ///
    /// This signs the data directly (not wrapped in CMS). Used internally
    /// by the CMS builder.
    pub(crate) fn sign_raw(&self, data: &[u8]) -> Result<Vec<u8>, SignError> {
        match &self.key {
            PrivateKey::Rsa(key) => {
                use rsa::pkcs1v15::SigningKey;
                use signature::{SignatureEncoding, Signer};
                let signing_key = SigningKey::<sha2::Sha256>::new(key.clone());
                let sig = signing_key
                    .try_sign(data)
                    .map_err(|e| SignError::SigningFailed(e.to_string()))?;
                Ok(sig.to_vec())
            }
            PrivateKey::EcP256(key) => {
                use signature::Signer;
                let sig: p256::ecdsa::DerSignature = key
                    .try_sign(data)
                    .map_err(|e| SignError::SigningFailed(e.to_string()))?;
                Ok(sig.as_ref().to_vec())
            }
            PrivateKey::EcP384(key) => {
                use signature::Signer;
                let sig: p384::ecdsa::DerSignature = key
                    .try_sign(data)
                    .map_err(|e| SignError::SigningFailed(e.to_string()))?;
                Ok(sig.as_ref().to_vec())
            }
        }
    }
}

impl PdfSigner for Pkcs12Signer {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, SignError> {
        crate::cms_builder::build_cms_signed_data(self, data)
    }

    fn certificate_chain_der(&self) -> &[Vec<u8>] {
        &self.cert_chain
    }

    fn digest_algorithm(&self) -> DigestAlgorithm {
        self.digest_algo
    }

    fn signature_algorithm_oid(&self) -> &[u8] {
        &self.sig_algo_oid
    }
}

/// Detect key type from PKCS#8 PrivateKeyInfo and load the appropriate key.
fn detect_and_load_key(
    key_der: &[u8],
) -> Result<(PrivateKey, DigestAlgorithm, Vec<u8>), SignError> {
    // PrivateKeyInfo ::= SEQUENCE {
    //   version INTEGER,
    //   privateKeyAlgorithm AlgorithmIdentifier,
    //   privateKey OCTET STRING
    // }
    let (_, pki_seq) =
        parse_tlv(key_der).ok_or_else(|| SignError::Pkcs12Load("invalid PKCS#8 DER".into()))?;
    let mut pos = pki_seq;

    // version INTEGER
    let (rest, _version) =
        parse_tlv(pos).ok_or_else(|| SignError::Pkcs12Load("missing PKCS#8 version".into()))?;
    pos = rest;

    // privateKeyAlgorithm AlgorithmIdentifier SEQUENCE { OID, params }
    let (_, algo_seq) = parse_tlv(pos)
        .ok_or_else(|| SignError::Pkcs12Load("missing algorithm identifier".into()))?;
    let (params_rest, algo_oid) =
        parse_tlv(algo_seq).ok_or_else(|| SignError::Pkcs12Load("missing algorithm OID".into()))?;

    if algo_oid == OID_RSA_ENCRYPTION {
        use pkcs8::DecodePrivateKey;
        let rsa_key = rsa::RsaPrivateKey::from_pkcs8_der(key_der)
            .map_err(|e| SignError::Pkcs12Load(format!("RSA key parse: {e}")))?;
        Ok((
            PrivateKey::Rsa(rsa_key),
            DigestAlgorithm::Sha256,
            OID_SHA256_WITH_RSA.to_vec(),
        ))
    } else if algo_oid == OID_EC_PUBLIC_KEY {
        // Parse curve OID from parameters.
        let (_, curve_oid) = parse_tlv(params_rest)
            .ok_or_else(|| SignError::Pkcs12Load("missing EC curve OID".into()))?;
        if curve_oid == OID_P256 {
            use pkcs8::DecodePrivateKey;
            let secret = p256::SecretKey::from_pkcs8_der(key_der)
                .map_err(|e| SignError::Pkcs12Load(format!("P-256 key parse: {e}")))?;
            let signing_key = p256::ecdsa::SigningKey::from(secret);
            Ok((
                PrivateKey::EcP256(signing_key),
                DigestAlgorithm::Sha256,
                OID_ECDSA_SHA256.to_vec(),
            ))
        } else if curve_oid == OID_P384 {
            use pkcs8::DecodePrivateKey;
            let secret = p384::SecretKey::from_pkcs8_der(key_der)
                .map_err(|e| SignError::Pkcs12Load(format!("P-384 key parse: {e}")))?;
            let signing_key = p384::ecdsa::SigningKey::from(secret);
            Ok((
                PrivateKey::EcP384(signing_key),
                DigestAlgorithm::Sha384,
                OID_ECDSA_SHA384.to_vec(),
            ))
        } else {
            Err(SignError::UnsupportedKeyType(format!(
                "EC curve OID: {curve_oid:02x?}"
            )))
        }
    } else {
        Err(SignError::UnsupportedKeyType(format!(
            "algorithm OID: {algo_oid:02x?}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn load_rsa_p12() {
        let data = std::fs::read(fixture_path("test-rsa.p12")).unwrap();
        let signer = Pkcs12Signer::from_pkcs12(&data, "test123").unwrap();
        assert_eq!(signer.digest_algorithm(), DigestAlgorithm::Sha256);
        assert_eq!(signer.signature_algorithm_oid(), OID_SHA256_WITH_RSA);
        assert!(!signer.certificate_chain_der().is_empty());
        assert!(signer.signer_name().is_some());
    }

    #[test]
    fn load_ec_p256_p12() {
        let data = std::fs::read(fixture_path("test-ec-p256.p12")).unwrap();
        let signer = Pkcs12Signer::from_pkcs12(&data, "test123").unwrap();
        assert_eq!(signer.digest_algorithm(), DigestAlgorithm::Sha256);
        assert_eq!(signer.signature_algorithm_oid(), OID_ECDSA_SHA256);
        assert!(!signer.certificate_chain_der().is_empty());
    }

    #[test]
    fn wrong_password_fails() {
        let data = std::fs::read(fixture_path("test-rsa.p12")).unwrap();
        let result = Pkcs12Signer::from_pkcs12(&data, "wrong");
        assert!(result.is_err());
    }

    #[test]
    fn rsa_sign_raw_roundtrip() {
        let data = std::fs::read(fixture_path("test-rsa.p12")).unwrap();
        let signer = Pkcs12Signer::from_pkcs12(&data, "test123").unwrap();
        let message = b"hello world";
        let sig = signer.sign_raw(message).unwrap();
        assert!(!sig.is_empty());

        // Verify with the public key.
        let cert_der = &signer.certificate_chain_der()[0];
        let cert = crate::x509::X509Certificate::from_der(cert_der).unwrap();
        let ok = crate::crypto::verify_cms_signature(
            message,
            &sig,
            &cert.spki_raw,
            signer.signature_algorithm_oid(),
            &[],
        )
        .unwrap();
        assert!(ok);
    }

    #[test]
    fn ec_p256_sign_raw_roundtrip() {
        let data = std::fs::read(fixture_path("test-ec-p256.p12")).unwrap();
        let signer = Pkcs12Signer::from_pkcs12(&data, "test123").unwrap();
        let message = b"hello world";
        let sig = signer.sign_raw(message).unwrap();
        assert!(!sig.is_empty());

        let cert_der = &signer.certificate_chain_der()[0];
        let cert = crate::x509::X509Certificate::from_der(cert_der).unwrap();
        let ok = crate::crypto::verify_cms_signature(
            message,
            &sig,
            &cert.spki_raw,
            signer.signature_algorithm_oid(),
            &[],
        )
        .unwrap();
        assert!(ok);
    }

    #[test]
    fn full_sign_produces_valid_cms() {
        let data = std::fs::read(fixture_path("test-rsa.p12")).unwrap();
        let signer = Pkcs12Signer::from_pkcs12(&data, "test123").unwrap();
        let message = b"PDF byte range content here";
        let cms_der = signer.sign(message).unwrap();

        // Parse with our CMS parser.
        let parsed = crate::cms::CmsSignedData::from_der(&cms_der);
        assert!(parsed.is_some(), "CMS DER must be parseable");
        let parsed = parsed.unwrap();
        assert!(parsed.verify_structural_integrity());
        assert_eq!(parsed.digest_algorithm(), DigestAlgorithm::Sha256);
        assert!(!parsed.signature_value().is_empty());
        assert!(parsed.signed_attributes_raw().is_some());
        assert!(!parsed.certificates().is_empty());
    }

    #[test]
    fn full_sign_ec_produces_valid_cms() {
        let data = std::fs::read(fixture_path("test-ec-p256.p12")).unwrap();
        let signer = Pkcs12Signer::from_pkcs12(&data, "test123").unwrap();
        let message = b"PDF byte range content here";
        let cms_der = signer.sign(message).unwrap();

        let parsed = crate::cms::CmsSignedData::from_der(&cms_der).unwrap();
        assert!(parsed.verify_structural_integrity());
        assert!(!parsed.signature_value().is_empty());
    }
}
