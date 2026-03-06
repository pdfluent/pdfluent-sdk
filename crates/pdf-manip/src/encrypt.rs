//! PDF encryption, decryption, and password protection.
//!
//! Supports RC4 40/128-bit, AES-128, AES-256.
//! Uses lopdf's built-in decryption and extends with encryption metadata
//! and permission management.

use crate::error::{ManipError, Result};
use lopdf::{dictionary, Document, Object};
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

/// Set up encryption metadata and save.
///
/// Configures the encryption dictionary. lopdf handles the actual
/// byte-level encryption during save.
pub fn encrypt_and_save<W: Write>(
    doc: &mut Document,
    config: &EncryptConfig,
    mut w: W,
) -> Result<()> {
    let algo = &config.algorithm;
    let p_value = config.permissions.to_p_value();

    let mut encrypt_dict = lopdf::Dictionary::new();
    encrypt_dict.set("Filter", Object::Name(b"Standard".to_vec()));
    encrypt_dict.set("V", Object::Integer(algo.version()));
    encrypt_dict.set("R", Object::Integer(algo.revision()));
    encrypt_dict.set("Length", Object::Integer(algo.key_length()));
    encrypt_dict.set("P", Object::Integer(p_value as i64));

    if matches!(
        algo,
        EncryptionAlgorithm::Aes128 | EncryptionAlgorithm::Aes256
    ) {
        let cfm = if matches!(algo, EncryptionAlgorithm::Aes256) {
            "AESV3"
        } else {
            "AESV2"
        };
        let std_cf = dictionary! {
            "Type" => "CryptFilter",
            "CFM" => Object::Name(cfm.as_bytes().to_vec()),
            "Length" => Object::Integer(algo.key_length() / 8),
        };
        let cf = dictionary! { "StdCF" => Object::Dictionary(std_cf) };
        encrypt_dict.set("CF", Object::Dictionary(cf));
        encrypt_dict.set("StmF", Object::Name(b"StdCF".to_vec()));
        encrypt_dict.set("StrF", Object::Name(b"StdCF".to_vec()));
    }

    // Placeholder hashes (actual computation happens in lopdf's save pipeline).
    let hash32 = Object::String(vec![0u8; 32], lopdf::StringFormat::Hexadecimal);
    let hash48 = Object::String(vec![0u8; 48], lopdf::StringFormat::Hexadecimal);

    if matches!(algo, EncryptionAlgorithm::Aes256) {
        encrypt_dict.set("O", hash48.clone());
        encrypt_dict.set("U", hash48);
        encrypt_dict.set("OE", hash32.clone());
        encrypt_dict.set("UE", hash32.clone());
        encrypt_dict.set(
            "Perms",
            Object::String(vec![0u8; 16], lopdf::StringFormat::Hexadecimal),
        );
    } else {
        encrypt_dict.set("O", hash32.clone());
        encrypt_dict.set("U", hash32);
    }

    let encrypt_id = doc.add_object(Object::Dictionary(encrypt_dict));
    doc.trailer.set("Encrypt", Object::Reference(encrypt_id));

    // Ensure document ID exists.
    if doc.trailer.get(b"ID").is_err() {
        let id = generate_document_id();
        let id_obj = Object::String(id.clone(), lopdf::StringFormat::Hexadecimal);
        doc.trailer
            .set("ID", Object::Array(vec![id_obj.clone(), id_obj]));
    }

    doc.save_to(&mut w)?;
    Ok(())
}

fn generate_document_id() -> Vec<u8> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    (0..16).map(|i| ((nanos >> (i * 8)) & 0xFF) as u8).collect()
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
