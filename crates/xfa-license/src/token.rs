//! License token — Ed25519 signed license files.
//!
//! License format: JSON file with a `signature` field containing a base64-encoded
//! Ed25519 signature over the canonical payload (all fields except `signature`).
//!
//! The public key is embedded in the binary for verification.
//! The private key is only used by the license generation tool.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::claims::LicenseFile;
#[cfg(feature = "signing")]
use crate::claims::LicensePayload;
use crate::error::{LicenseError, Result};

/// Verify a license file JSON string against a public key.
///
/// Returns the parsed `LicenseFile` if the signature is valid.
pub fn verify_license(public_key: &[u8], license_json: &str) -> Result<LicenseFile> {
    // Validate the public key first.
    let key_bytes: [u8; 32] = public_key
        .try_into()
        .map_err(|_| LicenseError::InvalidPublicKey)?;
    let verifying_key =
        VerifyingKey::from_bytes(&key_bytes).map_err(|_| LicenseError::InvalidPublicKey)?;

    let license_file: LicenseFile = serde_json::from_str(license_json)?;

    // Reconstruct the canonical payload JSON (without the signature field).
    let payload_json = serde_json::to_string(&license_file.payload)?;

    // Decode and verify the signature.
    let sig_bytes = STANDARD
        .decode(&license_file.signature)
        .map_err(|e| LicenseError::MalformedToken(format!("bad signature base64: {e}")))?;

    let signature = Signature::from_slice(&sig_bytes)
        .map_err(|_| LicenseError::MalformedToken("invalid signature length".into()))?;

    verifying_key
        .verify(payload_json.as_bytes(), &signature)
        .map_err(|_| LicenseError::InvalidSignature)?;

    Ok(license_file)
}

/// Verify a license and check that it has not expired.
pub fn verify_and_check_expiry(
    public_key: &[u8],
    license_json: &str,
    now: u64,
) -> Result<LicenseFile> {
    let license = verify_license(public_key, license_json)?;
    if license.payload.is_expired(now) {
        return Err(LicenseError::Expired(license.payload.expires_at));
    }
    Ok(license)
}

/// Sign a license payload with a private key (only available with `signing` feature).
///
/// Returns the complete license file JSON string including the signature.
#[cfg(feature = "signing")]
pub fn sign_license(private_key: &[u8], payload: &LicensePayload) -> Result<String> {
    use ed25519_dalek::Signer;
    use ed25519_dalek::SigningKey;

    let key_bytes: [u8; 32] = private_key
        .try_into()
        .map_err(|_| LicenseError::MalformedToken("private key must be 32 bytes".into()))?;
    let signing_key = SigningKey::from_bytes(&key_bytes);

    let payload_json = serde_json::to_string(payload)?;
    let signature = signing_key.sign(payload_json.as_bytes());
    let sig_b64 = STANDARD.encode(signature.to_bytes());

    let license_file = LicenseFile {
        payload: payload.clone(),
        signature: sig_b64,
    };

    Ok(serde_json::to_string_pretty(&license_file)?)
}

/// Generate a new Ed25519 keypair (only available with `signing` feature).
///
/// Returns `(private_key_bytes, public_key_bytes)`.
#[cfg(feature = "signing")]
pub fn generate_keypair() -> ([u8; 32], [u8; 32]) {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    (signing_key.to_bytes(), verifying_key.to_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "signing")]
    use crate::claims::Tier;

    #[cfg(feature = "signing")]
    fn test_keypair() -> ([u8; 32], [u8; 32]) {
        generate_keypair()
    }

    #[cfg(feature = "signing")]
    fn sample_payload() -> LicensePayload {
        LicensePayload {
            licensee: "Test User".into(),
            email: "test@example.com".into(),
            company: "Test Corp".into(),
            tier: Tier::Professional,
            seats: 5,
            issued_at: 1700000000,
            expires_at: 1730000000,
            features: None,
        }
    }

    #[cfg(feature = "signing")]
    #[test]
    fn sign_and_verify_roundtrip() {
        let (private_key, public_key) = test_keypair();
        let payload = sample_payload();
        let license_json = sign_license(&private_key, &payload).unwrap();
        let verified = verify_license(&public_key, &license_json).unwrap();
        assert_eq!(verified.payload, payload);
    }

    #[cfg(feature = "signing")]
    #[test]
    fn wrong_key_fails() {
        let (private_key, _) = test_keypair();
        let (_, other_public) = test_keypair();
        let payload = sample_payload();
        let license_json = sign_license(&private_key, &payload).unwrap();
        let result = verify_license(&other_public, &license_json);
        assert!(matches!(result, Err(LicenseError::InvalidSignature)));
    }

    #[cfg(feature = "signing")]
    #[test]
    fn tampered_payload_fails() {
        let (private_key, public_key) = test_keypair();
        let payload = sample_payload();
        let license_json = sign_license(&private_key, &payload).unwrap();
        let tampered = license_json.replace("Test User", "Evil User");
        let result = verify_license(&public_key, &tampered);
        assert!(matches!(result, Err(LicenseError::InvalidSignature)));
    }

    #[cfg(feature = "signing")]
    #[test]
    fn expiry_check() {
        let (private_key, public_key) = test_keypair();
        let payload = sample_payload(); // expires at 1730000000
        let license_json = sign_license(&private_key, &payload).unwrap();

        let result = verify_and_check_expiry(&public_key, &license_json, 1710000000);
        assert!(result.is_ok());

        let result = verify_and_check_expiry(&public_key, &license_json, 1740000000);
        assert!(matches!(result, Err(LicenseError::Expired(1730000000))));
    }

    #[test]
    fn invalid_public_key_length() {
        // Valid license JSON structure but wrong key length
        let json = r#"{"licensee":"x","email":"x","company":"x","tier":"trial","seats":1,"issued_at":0,"expires_at":0,"signature":"AAAA"}"#;
        let result = verify_license(&[0u8; 16], json);
        assert!(matches!(result, Err(LicenseError::InvalidPublicKey)));
    }

    #[test]
    fn malformed_json() {
        let result = verify_license(&[0u8; 32], "not json");
        assert!(matches!(result, Err(LicenseError::Json(_))));
    }
}
