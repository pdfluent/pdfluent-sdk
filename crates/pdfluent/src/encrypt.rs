//! Encryption, decryption, and permissions.

/// Encryption algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncryptionAlgorithm {
    /// AES with 128-bit key (PDF 1.6).
    Aes128,
    /// AES with 256-bit key (PDF 1.7 ext / 2.0). **Recommended.**
    Aes256,
}

/// Permissions granted on an encrypted PDF.
///
/// Construct via presets ([`Permissions::print_only`], [`read_only`],
/// [`annotate`]) or the individual `with_*` methods for fine control.
#[derive(Debug, Clone, Copy)]
pub struct Permissions {
    pub(crate) print: bool,
    pub(crate) modify: bool,
    pub(crate) copy: bool,
    pub(crate) annotate: bool,
    pub(crate) fill_forms: bool,
    pub(crate) extract_accessibility: bool,
    pub(crate) assemble: bool,
    pub(crate) print_high_quality: bool,
}

impl Permissions {
    /// All permissions denied except high-quality printing.
    pub const fn print_only() -> Self {
        Self {
            print: true,
            modify: false,
            copy: false,
            annotate: false,
            fill_forms: false,
            extract_accessibility: true,
            assemble: false,
            print_high_quality: true,
        }
    }

    /// All permissions denied.
    pub const fn read_only() -> Self {
        Self {
            print: false,
            modify: false,
            copy: false,
            annotate: false,
            fill_forms: false,
            extract_accessibility: true,
            assemble: false,
            print_high_quality: false,
        }
    }

    /// Allow commenting and form filling, deny modification.
    pub const fn annotate() -> Self {
        Self {
            print: true,
            modify: false,
            copy: true,
            annotate: true,
            fill_forms: true,
            extract_accessibility: true,
            assemble: false,
            print_high_quality: true,
        }
    }
}

/// Options for encrypting a document.
///
/// Used both to encrypt an un-encrypted document and to re-encrypt an
/// already-encrypted one (requires [`crate::PdfDocument::decrypt`] first).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct EncryptOptions {
    pub(crate) algorithm: EncryptionAlgorithm,
    pub(crate) user_password: Option<String>,
    pub(crate) owner_password: Option<String>,
    pub(crate) permissions: Permissions,
}

impl Default for EncryptOptions {
    fn default() -> Self {
        Self::aes256()
    }
}

impl EncryptOptions {
    /// AES-256 with default permissions (annotate preset).
    pub fn aes256() -> Self {
        Self {
            algorithm: EncryptionAlgorithm::Aes256,
            user_password: None,
            owner_password: None,
            permissions: Permissions::annotate(),
        }
    }

    /// AES-128 with default permissions.
    pub fn aes128() -> Self {
        Self {
            algorithm: EncryptionAlgorithm::Aes128,
            ..Self::aes256()
        }
    }

    /// Set a user password (required to open).
    pub fn with_user_password(mut self, pw: impl Into<String>) -> Self {
        self.user_password = Some(pw.into());
        self
    }

    /// Set an owner password (required to change permissions).
    pub fn with_owner_password(mut self, pw: impl Into<String>) -> Self {
        self.owner_password = Some(pw.into());
        self
    }

    /// Restrict permissions.
    pub fn with_permissions(mut self, p: Permissions) -> Self {
        self.permissions = p;
        self
    }
}
