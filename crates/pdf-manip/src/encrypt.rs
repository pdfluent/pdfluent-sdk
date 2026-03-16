//! PDF encryption, decryption, and password protection.
//!
//! Supports RC4 40/128-bit, AES-128, AES-256.
//! Uses lopdf's built-in decryption and extends with encryption metadata
//! and permission management.

use crate::error::{ManipError, Result};
use lopdf::{Document, EncryptionState, EncryptionVersion, Object, Permissions as LopdfPerms};
use std::io::Write;
use std::path::Path;

/// Encryption algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    Rc4_40,
    Rc4_128,
    Aes128,
    Aes256,
}

impl EncryptionAlgorithm {
    fn version(&self) -> i64 {
        match self {
            Self::Rc4_40 => 1,
            Self::Rc4_128 | Self::Aes128 => 2,
            Self::Aes256 => 5,
        }
    }

    fn revision(&self) -> i64 {
        match self {
            Self::Rc4_40 => 2,
            Self::Rc4_128 => 3,
            Self::Aes128 => 4,
            Self::Aes256 => 6,
        }
    }

    fn key_length(&self) -> i64 {
        match self {
            Self::Rc4_40 => 40,
            Self::Rc4_128 | Self::Aes128 => 128,
            Self::Aes256 => 256,
        }
    }
}

/// PDF permission flags (ISO 32000-2 Table 22).
#[derive(Debug, Clone, Copy)]
pub struct Permissions {
    pub print: bool,
    pub modify_contents: bool,
    pub extract_content: bool,
    pub modify_annotations: bool,
    pub fill_forms: bool,
    pub extract_for_accessibility: bool,
    pub assemble_document: bool,
    pub print_high_quality: bool,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            print: true,
            modify_contents: true,
            extract_content: true,
            modify_annotations: true,
            fill_forms: true,
            extract_for_accessibility: true,
            assemble_document: true,
            print_high_quality: true,
        }
    }
}

impl Permissions {
    pub fn allow_all() -> Self {
        Self::default()
    }

    pub fn deny_all() -> Self {
        Self {
            print: false,
            modify_contents: false,
            extract_content: false,
            modify_annotations: false,
            fill_forms: false,
            extract_for_accessibility: false,
            assemble_document: false,
            print_high_quality: false,
        }
    }

    /// Convert to the /P integer value.
    pub fn to_p_value(&self) -> i32 {
        let mut p: i32 = !0i32 << 12; // bits 13-32 must be 1
        if self.print {
            p |= 1 << 2;
        }
        if self.modify_contents {
            p |= 1 << 3;
        }
        if self.extract_content {
            p |= 1 << 4;
        }
        if self.modify_annotations {
            p |= 1 << 5;
        }
        if self.fill_forms {
            p |= 1 << 8;
        }
        if self.extract_for_accessibility {
            p |= 1 << 9;
        }
        if self.assemble_document {
            p |= 1 << 10;
        }
        if self.print_high_quality {
            p |= 1 << 11;
        }
        p
    }

    /// Parse from a /P integer value.
    pub fn from_p_value(p: i32) -> Self {
        Self {
            print: p & (1 << 2) != 0,
            modify_contents: p & (1 << 3) != 0,
            extract_content: p & (1 << 4) != 0,
            modify_annotations: p & (1 << 5) != 0,
            fill_forms: p & (1 << 8) != 0,
            extract_for_accessibility: p & (1 << 9) != 0,
            assemble_document: p & (1 << 10) != 0,
            print_high_quality: p & (1 << 11) != 0,
        }
    }
}

/// Read permission flags from an encrypted document.
pub fn read_permissions(doc: &Document) -> Option<Permissions> {
    let encrypt_ref = doc.trailer.get(b"Encrypt").ok()?.as_reference().ok()?;
    let dict = doc.get_dictionary(encrypt_ref).ok()?;
    let p = dict.get(b"P").ok()?.as_i64().ok()? as i32;
    Some(Permissions::from_p_value(p))
}

/// Check if a document is encrypted.
pub fn is_encrypted(doc: &Document) -> bool {
    doc.trailer.get(b"Encrypt").is_ok()
}

/// Decrypt a PDF document with the given password.
pub fn decrypt(doc: &mut Document, password: &str) -> Result<()> {
    if !is_encrypted(doc) {
        return Ok(());
    }
    doc.decrypt(password)
        .map_err(|_| ManipError::DecryptionFailed)
}

/// Load and decrypt a PDF from a file path.
pub fn open_encrypted<P: AsRef<Path>>(path: P, password: &str) -> Result<Document> {
    let mut doc = Document::load(path.as_ref())?;
    decrypt(&mut doc, password)?;
    Ok(doc)
}

/// Remove encryption from a document.
pub fn remove_encryption(doc: &mut Document) {
    doc.trailer.remove(b"Encrypt");
}

/// Encryption configuration.
#[derive(Debug, Clone)]
pub struct EncryptConfig {
    pub user_password: Vec<u8>,
    pub owner_password: Vec<u8>,
    pub algorithm: EncryptionAlgorithm,
    pub permissions: Permissions,
}

impl Default for EncryptConfig {
    fn default() -> Self {
        Self {
            user_password: Vec::new(),
            owner_password: Vec::new(),
            algorithm: EncryptionAlgorithm::Aes256,
            permissions: Permissions::allow_all(),
        }
    }
}

/// Encrypt and save a document to an arbitrary writer.
///
/// Calls lopdf's encryption pipeline, which encrypts every object in-place
/// before writing.  The caller's `doc` is mutated (encrypted) after the call.
pub fn encrypt_and_save<W: Write>(
    doc: &mut Document,
    config: &EncryptConfig,
    mut w: W,
) -> Result<()> {
    let user_pw = std::str::from_utf8(&config.user_password)
        .map_err(|_| ManipError::Encryption("user_password is not valid UTF-8".into()))?;
    let owner_pw = std::str::from_utf8(&config.owner_password)
        .map_err(|_| ManipError::Encryption("owner_password is not valid UTF-8".into()))?;

    let state = match config.algorithm {
        // AES-256 (PDF 2.0, V=5, R=6) — no /ID required; random key generated internally.
        EncryptionAlgorithm::Aes256 | EncryptionAlgorithm::Aes128 => {
            lopdf::aes256_encryption_state(owner_pw, user_pw, LopdfPerms::all())
                .map_err(|e| ManipError::Encryption(e.to_string()))?
        }
        // RC4-128 (V=2, R=3) — requires /ID in the trailer.
        EncryptionAlgorithm::Rc4_128 => {
            ensure_document_id(doc);
            EncryptionState::try_from(EncryptionVersion::V2 {
                document: doc,
                owner_password: owner_pw,
                user_password: user_pw,
                key_length: 128,
                permissions: LopdfPerms::all(),
            })
            .map_err(|e| ManipError::Encryption(e.to_string()))?
        }
        // RC4-40 (V=1, R=2) — requires /ID in the trailer.
        EncryptionAlgorithm::Rc4_40 => {
            ensure_document_id(doc);
            EncryptionState::try_from(EncryptionVersion::V1 {
                document: doc,
                owner_password: owner_pw,
                user_password: user_pw,
                permissions: LopdfPerms::all(),
            })
            .map_err(|e| ManipError::Encryption(e.to_string()))?
        }
    };

    doc.encrypt(&state)
        .map_err(|e| ManipError::Encryption(e.to_string()))?;
    doc.save_to(&mut w)?;
    Ok(())
}

fn ensure_document_id(doc: &mut Document) {
    if doc.trailer.get(b"ID").is_err() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let id: Vec<u8> = (0..16).map(|i| ((nanos >> (i * 8)) & 0xFF) as u8).collect();
        let id_obj = Object::String(id, lopdf::StringFormat::Hexadecimal);
        doc.trailer
            .set("ID", Object::Array(vec![id_obj.clone(), id_obj]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permissions_roundtrip() {
        let perms = Permissions {
            print: true,
            modify_contents: false,
            extract_content: true,
            modify_annotations: false,
            fill_forms: true,
            extract_for_accessibility: true,
            assemble_document: false,
            print_high_quality: true,
        };
        let p = perms.to_p_value();
        let restored = Permissions::from_p_value(p);
        assert_eq!(perms.print, restored.print);
        assert_eq!(perms.modify_contents, restored.modify_contents);
        assert_eq!(perms.extract_content, restored.extract_content);
        assert_eq!(perms.fill_forms, restored.fill_forms);
    }

    #[test]
    fn test_not_encrypted() {
        let doc = lopdf::Document::with_version("1.7");
        assert!(!is_encrypted(&doc));
        assert!(read_permissions(&doc).is_none());
    }

    #[test]
    fn test_algorithm_params() {
        assert_eq!(EncryptionAlgorithm::Aes256.version(), 5);
        assert_eq!(EncryptionAlgorithm::Aes256.revision(), 6);
        assert_eq!(EncryptionAlgorithm::Aes256.key_length(), 256);
    }
}
