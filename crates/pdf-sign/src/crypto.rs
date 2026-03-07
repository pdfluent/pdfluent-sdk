//! Cryptographic signature verification using RustCrypto.
//!
//! Supports RSA PKCS#1 v1.5 (SHA-1/256/384/512), RSA-PSS (SHA-256),
//! ECDSA P-256 (SHA-256), and ECDSA P-384 (SHA-384).

use crate::cms::parse_tlv;

/// Signature algorithm identified from the SignerInfo or SPKI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureAlgorithm {
    /// RSA PKCS#1 v1.5 with SHA-1.
    RsaPkcs1Sha1,
    /// RSA PKCS#1 v1.5 with SHA-256.
    RsaPkcs1Sha256,
    /// RSA PKCS#1 v1.5 with SHA-384.
    RsaPkcs1Sha384,
    /// RSA PKCS#1 v1.5 with SHA-512.
    RsaPkcs1Sha512,
    /// RSA-PSS with SHA-256.
    RsaPssSha256,
    /// ECDSA P-256 with SHA-256.
    EcdsaP256Sha256,
    /// ECDSA P-384 with SHA-384.
    EcdsaP384Sha384,
    /// Unrecognized algorithm.
    Unknown,
}

// OIDs for signature algorithms.
const OID_RSA_ENCRYPTION: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];
const OID_SHA1_WITH_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x05];
const OID_SHA256_WITH_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B];
const OID_SHA384_WITH_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0C];
const OID_SHA512_WITH_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0D];
const OID_RSA_PSS: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0A];
const OID_ECDSA_SHA256: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02];
const OID_ECDSA_SHA384: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x03];
const OID_EC_PUBLIC_KEY: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01];
const OID_P256: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07];
const OID_P384: &[u8] = &[0x2B, 0x81, 0x04, 0x00, 0x22];

impl SignatureAlgorithm {
    /// Identify from a signature algorithm OID.
    pub fn from_sig_oid(oid: &[u8]) -> Self {
        if oid == OID_SHA1_WITH_RSA {
            Self::RsaPkcs1Sha1
        } else if oid == OID_SHA256_WITH_RSA {
            Self::RsaPkcs1Sha256
        } else if oid == OID_SHA384_WITH_RSA {
            Self::RsaPkcs1Sha384
        } else if oid == OID_SHA512_WITH_RSA {
            Self::RsaPkcs1Sha512
        } else if oid == OID_RSA_PSS {
            Self::RsaPssSha256
        } else if oid == OID_ECDSA_SHA256 {
            Self::EcdsaP256Sha256
        } else if oid == OID_ECDSA_SHA384 {
            Self::EcdsaP384Sha384
        } else {
            Self::Unknown
        }
    }
}

/// Verify a CMS signature cryptographically.
///
/// `signed_data` is the DER-encoded signed attributes (with SET OF tag 0x31).
/// `signature` is the raw signature bytes from SignerInfo.
/// `spki_der` is the full DER-encoded SubjectPublicKeyInfo from the signer cert.
/// `sig_algo_oid` is the signature algorithm OID from the SignerInfo.
pub fn verify_cms_signature(
    signed_data: &[u8],
    signature: &[u8],
    spki_der: &[u8],
    sig_algo_oid: &[u8],
) -> Result<bool, String> {
    let algo = SignatureAlgorithm::from_sig_oid(sig_algo_oid);
    match algo {
        SignatureAlgorithm::RsaPkcs1Sha1 => {
            verify_rsa_pkcs1::<sha1::Sha1>(signed_data, signature, spki_der)
        }
        SignatureAlgorithm::RsaPkcs1Sha256 => {
            verify_rsa_pkcs1::<sha2::Sha256>(signed_data, signature, spki_der)
        }
        SignatureAlgorithm::RsaPkcs1Sha384 => {
            verify_rsa_pkcs1::<sha2::Sha384>(signed_data, signature, spki_der)
        }
        SignatureAlgorithm::RsaPkcs1Sha512 => {
            verify_rsa_pkcs1::<sha2::Sha512>(signed_data, signature, spki_der)
        }
        SignatureAlgorithm::RsaPssSha256 => verify_rsa_pss_sha256(signed_data, signature, spki_der),
        SignatureAlgorithm::EcdsaP256Sha256 => verify_ecdsa_p256(signed_data, signature, spki_der),
        SignatureAlgorithm::EcdsaP384Sha384 => verify_ecdsa_p384(signed_data, signature, spki_der),
        SignatureAlgorithm::Unknown => verify_by_spki_type(signed_data, signature, spki_der),
    }
}

fn verify_rsa_pkcs1<D>(
    signed_data: &[u8],
    signature_bytes: &[u8],
    spki_der: &[u8],
) -> Result<bool, String>
where
    D: digest::Digest + digest::const_oid::AssociatedOid,
{
    use rsa::pkcs1v15;
    use rsa::RsaPublicKey;
    use signature::Verifier;
    use spki::DecodePublicKey;
    let key =
        RsaPublicKey::from_public_key_der(spki_der).map_err(|e| format!("RSA key parse: {e}"))?;
    let verifying_key = pkcs1v15::VerifyingKey::<D>::new(key);
    let sig = pkcs1v15::Signature::try_from(signature_bytes)
        .map_err(|e| format!("RSA signature parse: {e}"))?;
    match verifying_key.verify(signed_data, &sig) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

fn verify_rsa_pss_sha256(
    signed_data: &[u8],
    signature_bytes: &[u8],
    spki_der: &[u8],
) -> Result<bool, String> {
    use rsa::pss;
    use rsa::RsaPublicKey;
    use signature::Verifier;
    use spki::DecodePublicKey;
    let key =
        RsaPublicKey::from_public_key_der(spki_der).map_err(|e| format!("RSA key parse: {e}"))?;
    let verifying_key = pss::VerifyingKey::<sha2::Sha256>::new(key);
    let sig = pss::Signature::try_from(signature_bytes)
        .map_err(|e| format!("PSS signature parse: {e}"))?;
    match verifying_key.verify(signed_data, &sig) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

fn verify_ecdsa_p256(
    signed_data: &[u8],
    signature_bytes: &[u8],
    spki_der: &[u8],
) -> Result<bool, String> {
    use p256::ecdsa::{Signature, VerifyingKey};
    use signature::Verifier;
    use spki::DecodePublicKey;
    let key =
        VerifyingKey::from_public_key_der(spki_der).map_err(|e| format!("P-256 key parse: {e}"))?;
    let sig =
        Signature::from_der(signature_bytes).map_err(|e| format!("ECDSA signature parse: {e}"))?;
    match key.verify(signed_data, &sig) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

fn verify_ecdsa_p384(
    signed_data: &[u8],
    signature_bytes: &[u8],
    spki_der: &[u8],
) -> Result<bool, String> {
    use p384::ecdsa::{Signature, VerifyingKey};
    use signature::Verifier;
    use spki::DecodePublicKey;
    let key =
        VerifyingKey::from_public_key_der(spki_der).map_err(|e| format!("P-384 key parse: {e}"))?;
    let sig =
        Signature::from_der(signature_bytes).map_err(|e| format!("ECDSA signature parse: {e}"))?;
    match key.verify(signed_data, &sig) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Fallback: determine algorithm from SPKI type.
fn verify_by_spki_type(
    signed_data: &[u8],
    signature_bytes: &[u8],
    spki_der: &[u8],
) -> Result<bool, String> {
    let (_, spki_seq) = parse_tlv(spki_der).ok_or("cannot parse SPKI")?;
    let (_, algo_seq) = parse_tlv(spki_seq).ok_or("cannot parse SPKI algorithm")?;
    let (rest, algo_oid) = parse_tlv(algo_seq).ok_or("cannot parse SPKI algorithm OID")?;
    if algo_oid == OID_RSA_ENCRYPTION {
        verify_rsa_pkcs1::<sha2::Sha256>(signed_data, signature_bytes, spki_der)
    } else if algo_oid == OID_EC_PUBLIC_KEY {
        let (_, curve_oid) = parse_tlv(rest).ok_or("no EC curve parameter in SPKI")?;
        if curve_oid == OID_P256 {
            verify_ecdsa_p256(signed_data, signature_bytes, spki_der)
        } else if curve_oid == OID_P384 {
            verify_ecdsa_p384(signed_data, signature_bytes, spki_der)
        } else {
            Err("unsupported EC curve".into())
        }
    } else {
        Err(format!(
            "unsupported public key algorithm OID: {algo_oid:02x?}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_algorithm_from_known_oids() {
        assert_eq!(
            SignatureAlgorithm::from_sig_oid(OID_SHA1_WITH_RSA),
            SignatureAlgorithm::RsaPkcs1Sha1
        );
        assert_eq!(
            SignatureAlgorithm::from_sig_oid(OID_SHA256_WITH_RSA),
            SignatureAlgorithm::RsaPkcs1Sha256
        );
        assert_eq!(
            SignatureAlgorithm::from_sig_oid(OID_SHA384_WITH_RSA),
            SignatureAlgorithm::RsaPkcs1Sha384
        );
        assert_eq!(
            SignatureAlgorithm::from_sig_oid(OID_SHA512_WITH_RSA),
            SignatureAlgorithm::RsaPkcs1Sha512
        );
        assert_eq!(
            SignatureAlgorithm::from_sig_oid(OID_RSA_PSS),
            SignatureAlgorithm::RsaPssSha256
        );
        assert_eq!(
            SignatureAlgorithm::from_sig_oid(OID_ECDSA_SHA256),
            SignatureAlgorithm::EcdsaP256Sha256
        );
        assert_eq!(
            SignatureAlgorithm::from_sig_oid(OID_ECDSA_SHA384),
            SignatureAlgorithm::EcdsaP384Sha384
        );
        assert_eq!(
            SignatureAlgorithm::from_sig_oid(&[0xFF]),
            SignatureAlgorithm::Unknown
        );
    }

    #[test]
    fn verify_invalid_spki_returns_error() {
        let result = verify_cms_signature(b"test", b"sig", b"invalid", OID_SHA256_WITH_RSA);
        assert!(result.is_err());
    }

    #[test]
    fn verify_unknown_algo_with_bad_spki_returns_error() {
        let result = verify_cms_signature(b"test", b"sig", b"invalid", &[0xFF]);
        assert!(result.is_err());
    }

    #[test]
    fn verify_rsa_encryption_oid_fallback_with_bad_spki() {
        let mut spki = vec![0x30, 0x0D];
        spki.push(0x30);
        spki.push(0x0B);
        spki.push(0x06);
        spki.push(OID_RSA_ENCRYPTION.len() as u8);
        spki.extend_from_slice(OID_RSA_ENCRYPTION);
        let result = verify_cms_signature(b"test", b"sig", &spki, &[0xFF]);
        assert!(result.is_err());
    }

    #[test]
    fn verify_ec_spki_fallback_unsupported_curve() {
        let unknown_curve: &[u8] = &[0x01, 0x02, 0x03];
        let algo_inner_len = 2 + OID_EC_PUBLIC_KEY.len() + 2 + unknown_curve.len();
        let mut spki = vec![0x30];
        spki.push((2 + algo_inner_len) as u8);
        spki.push(0x30);
        spki.push(algo_inner_len as u8);
        spki.push(0x06);
        spki.push(OID_EC_PUBLIC_KEY.len() as u8);
        spki.extend_from_slice(OID_EC_PUBLIC_KEY);
        spki.push(0x06);
        spki.push(unknown_curve.len() as u8);
        spki.extend_from_slice(unknown_curve);
        let result = verify_cms_signature(b"test", b"sig", &spki, &[0xFF]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unsupported EC curve"));
    }
}
