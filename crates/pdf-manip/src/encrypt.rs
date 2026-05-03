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
    /// RC4 with a 40-bit key (PDF 1.1, legacy — avoid for new documents).
    Rc4_40,
    /// RC4 with a 128-bit key (PDF 1.4, legacy).
    Rc4_128,
    /// AES with a 128-bit key (PDF 1.6).
    Aes128,
    /// AES with a 256-bit key (PDF 1.7 ext / PDF 2.0 — recommended).
    Aes256,
}

impl EncryptionAlgorithm {
    /// PDF encryption version number (Table 20, ISO 32000-2).
    #[allow(dead_code)]
    fn version(&self) -> i64 {
        match self {
            Self::Rc4_40 => 1,
            Self::Rc4_128 => 2,
            Self::Aes128 => 4,
            Self::Aes256 => 5,
        }
    }

    /// Standard security handler revision (Table 21, ISO 32000-2).
    #[allow(dead_code)]
    fn revision(&self) -> i64 {
        match self {
            Self::Rc4_40 => 2,
            Self::Rc4_128 => 3,
            Self::Aes128 => 4,
            Self::Aes256 => 6,
        }
    }

    /// Key length in bits.
    #[allow(dead_code)]
    fn key_length(&self) -> i64 {
        match self {
            Self::Rc4_40 => 40,
            Self::Rc4_128 | Self::Aes128 => 128,
            Self::Aes256 => 256,
        }
    }
}

/// PDF permission flags (ISO 32000-2 Table 22).
///
/// Controls what operations an encrypted PDF permits when opened with the
/// user password. The owner password always grants full access.
#[derive(Debug, Clone, Copy)]
pub struct Permissions {
    /// Allow printing (bit 3).
    pub print: bool,
    /// Allow modifying content other than annotations and form fields (bit 4).
    pub modify_contents: bool,
    /// Allow copying or extracting text and graphics (bit 5).
    pub extract_content: bool,
    /// Allow adding or modifying annotations and form fields (bit 6).
    pub modify_annotations: bool,
    /// Allow filling in form fields (bit 9).
    pub fill_forms: bool,
    /// Allow text and graphics extraction for accessibility (bit 10).
    pub extract_for_accessibility: bool,
    /// Allow inserting, rotating, or deleting pages (bit 11).
    pub assemble_document: bool,
    /// Allow high-quality printing (bit 12).
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
    /// All permissions granted — useful as a starting point that the
    /// caller selectively narrows. Equivalent to [`Permissions::default`].
    pub fn allow_all() -> Self {
        Self::default()
    }

    /// All permissions denied — most restrictive starting point.
    /// Note that PDF readers commonly ignore restrictions when the
    /// document is opened with the owner password, so this is best
    /// thought of as a hint rather than a hard guarantee.
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

/// Configuration for encrypting a PDF — passwords, algorithm, and the
/// permission flags written into the encrypted document.
#[derive(Debug, Clone)]
pub struct EncryptConfig {
    /// Password required for "open" access. May be empty (no password
    /// needed to view, only to modify) — but at least one of user or
    /// owner password should be set in practice.
    pub user_password: Vec<u8>,
    /// Password required for full / unrestricted access (owner mode).
    /// Owner-mode opens override the [`Permissions`] flags. Should be
    /// distinct from `user_password`.
    pub owner_password: Vec<u8>,
    /// Encryption algorithm to use (AES-128 or AES-256). AES-256
    /// requires PDF 2.0 or PDF 1.7 ExtensionLevel 3.
    pub algorithm: EncryptionAlgorithm,
    /// Permission flags applied to user-password access. Ignored when
    /// the document is opened with the owner password.
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

    // Honour the permissions from the caller's config rather than forcing
    // `LopdfPerms::all()`. The conversion takes our Permissions → /P integer
    // → LopdfPerms bitflags so the callee sees the exact bits the user
    // requested.
    let perms_bits = config.permissions.to_p_value() as u64;
    let lopdf_perms = LopdfPerms::from_bits_retain(perms_bits);

    let state = match config.algorithm {
        // AES-256 (PDF 2.0, V=5, R=6) — no /ID required; random key generated internally.
        EncryptionAlgorithm::Aes256 => {
            lopdf::aes256_encryption_state(owner_pw, user_pw, lopdf_perms)
                .map_err(|e| ManipError::Encryption(e.to_string()))?
        }
        // AES-128 (PDF 1.6, V=4, R=4) — uses document /ID in key derivation.
        EncryptionAlgorithm::Aes128 => {
            use lopdf::encryption::crypt_filters::Aes128CryptFilter;
            use std::collections::BTreeMap;
            use std::sync::Arc;
            ensure_document_id(doc);
            let crypt_filter: Arc<dyn lopdf::encryption::crypt_filters::CryptFilter> =
                Arc::new(Aes128CryptFilter);
            EncryptionState::try_from(EncryptionVersion::V4 {
                document: doc,
                encrypt_metadata: true,
                crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password: owner_pw,
                user_password: user_pw,
                permissions: lopdf_perms,
            })
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
                permissions: lopdf_perms,
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
                permissions: lopdf_perms,
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
        assert_eq!(EncryptionAlgorithm::Aes128.version(), 4);
        assert_eq!(EncryptionAlgorithm::Aes128.revision(), 4);
        assert_eq!(EncryptionAlgorithm::Aes128.key_length(), 128);
    }

    fn make_minimal_doc() -> lopdf::Document {
        use lopdf::{dictionary, Object, Stream};
        let mut doc = lopdf::Document::with_version("1.7");
        let content = Stream::new(dictionary! {}, b"BT (Hello) Tj ET".to_vec());
        let content_id = doc.add_object(Object::Stream(content));
        let page = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
        };
        let page_id = doc.add_object(Object::Dictionary(page));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc
    }

    #[test]
    fn aes128_encrypt_decrypt_roundtrip() {
        let mut doc = make_minimal_doc();
        let config = EncryptConfig {
            user_password: b"secret".to_vec(),
            owner_password: b"owner".to_vec(),
            algorithm: EncryptionAlgorithm::Aes128,
            permissions: Permissions::allow_all(),
        };

        let mut output = Vec::new();
        encrypt_and_save(&mut doc, &config, &mut output).expect("AES-128 encrypt should succeed");
        assert!(!output.is_empty(), "encrypted output must not be empty");

        // Load + decrypt — lopdf decrypts content streams in-place.
        let mut dec_doc = lopdf::Document::load_mem(&output).expect("load encrypted doc");
        dec_doc
            .decrypt("secret")
            .expect("AES-128 decrypt should succeed with correct password");
    }

    #[test]
    fn aes128_trailer_v4_r4_aesv2() {
        let mut doc = make_minimal_doc();
        let config = EncryptConfig {
            user_password: b"".to_vec(),
            owner_password: b"owner".to_vec(),
            algorithm: EncryptionAlgorithm::Aes128,
            permissions: Permissions::allow_all(),
        };

        let mut output = Vec::new();
        encrypt_and_save(&mut doc, &config, &mut output).expect("encrypt");

        // Scan raw PDF bytes for encryption parameters — dictionary keys and
        // name values are not encrypted, so they appear as plain text.
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("/V 4"),
            "raw PDF must contain /V 4 for AES-128"
        );
        assert!(
            text.contains("/R 4"),
            "raw PDF must contain /R 4 for AES-128"
        );
        assert!(text.contains("/StmF"), "raw PDF must contain /StmF");
        assert!(text.contains("/StrF"), "raw PDF must contain /StrF");
        assert!(
            text.contains("/StdCF"),
            "raw PDF must reference StdCF filter"
        );
        assert!(
            text.contains("AESV2"),
            "raw PDF must contain AESV2 crypt filter method"
        );
    }
}
