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
/// Construct via presets ([`Permissions::full_access`], [`print_only`],
/// [`read_only`], [`annotate`]) or the individual `with_*` methods for fine
/// control.
///
/// # Accessibility
///
/// All presets enable `extract_accessibility`. Per **ISO 32000-2 §7.6.4.2**,
/// a PDF consumer is required to honour accessibility-extraction regardless
/// of the author's permission bits. Setting `extract_accessibility = false`
/// via [`Permissions::with_extract_accessibility`] is therefore advisory; a
/// spec-compliant reader will still allow screen-reader access.
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
    /// All permissions allowed. Use this when you only want encryption for
    /// confidentiality, not to restrict what authorised readers can do.
    pub const fn full_access() -> Self {
        Self {
            print: true,
            modify: true,
            copy: true,
            annotate: true,
            fill_forms: true,
            extract_accessibility: true,
            assemble: true,
            print_high_quality: true,
        }
    }

    /// Printing allowed (including high-quality); all other operations
    /// denied. Accessibility-extraction remains enabled per ISO 32000-2.
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

    /// All operations denied. Accessibility-extraction remains enabled per
    /// ISO 32000-2 — assistive technologies retain access to the content.
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

    /// Allow commenting, form filling, copying, and printing. Denies
    /// document-structure modification and page assembly.
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

    /// Allow / deny printing (low resolution).
    pub const fn with_print(mut self, v: bool) -> Self {
        self.print = v;
        self
    }

    /// Allow / deny modification.
    pub const fn with_modify(mut self, v: bool) -> Self {
        self.modify = v;
        self
    }

    /// Allow / deny copying text and graphics.
    pub const fn with_copy(mut self, v: bool) -> Self {
        self.copy = v;
        self
    }

    /// Allow / deny adding/editing annotations.
    pub const fn with_annotate(mut self, v: bool) -> Self {
        self.annotate = v;
        self
    }

    /// Allow / deny filling form fields.
    pub const fn with_fill_forms(mut self, v: bool) -> Self {
        self.fill_forms = v;
        self
    }

    /// Allow / deny accessibility extraction (screen readers).
    pub const fn with_extract_accessibility(mut self, v: bool) -> Self {
        self.extract_accessibility = v;
        self
    }

    /// Allow / deny assembling (insert/rotate/delete pages).
    pub const fn with_assemble(mut self, v: bool) -> Self {
        self.assemble = v;
        self
    }

    /// Allow / deny high-resolution printing.
    pub const fn with_print_high_quality(mut self, v: bool) -> Self {
        self.print_high_quality = v;
        self
    }
}

/// Options for encrypting a document.
///
/// Used both to encrypt an un-encrypted document and to re-encrypt an
/// already-encrypted one (requires [`crate::PdfDocument::decrypt`] first).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct EncryptOptions {
    // Captured at construction by `aes128()` / `aes256()`; read by the
    // `PdfDocument::encrypt` body when wired in Epic 2 #1244. Marked with
    // an explicit allow so the scaffold phase does not fail `-D warnings`.
    #[allow(dead_code)]
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
    /// AES-256 with [`Permissions::full_access`]. Intended for the common
    /// case: "encrypt to protect the bytes in transit, let authorised
    /// readers do anything."
    ///
    /// Override with [`with_permissions`](Self::with_permissions) if you
    /// want to restrict operations (e.g. [`Permissions::print_only`]).
    pub fn aes256() -> Self {
        Self {
            algorithm: EncryptionAlgorithm::Aes256,
            user_password: None,
            owner_password: None,
            permissions: Permissions::full_access(),
        }
    }

    /// AES-128 with [`Permissions::full_access`]. Prefer [`aes256`] unless
    /// targeting readers older than PDF 1.7 ext.
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
