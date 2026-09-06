// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::fmt;
use std::path::PathBuf;

/// Common contract for engine error types.
///
/// `pdf-engine` exposes errors that flow up through the `pdfluent` facade
/// to customers. Implementing this trait gives every error a stable,
/// machine-readable code, an actionable help string, and a deep link to the
/// public error documentation site.
pub trait PdfError: std::error::Error {
    /// Stable machine-readable identifier for the error.
    ///
    /// Format: `SCREAMING_SNAKE_CASE` (for example `"PASSWORD_REQUIRED"`).
    /// These codes are part of the public API contract — bindings, log
    /// pipelines, and customer error handlers depend on them, so they must
    /// not change without a deprecation cycle.
    fn code(&self) -> &str;

    /// Human-readable, actionable help text.
    ///
    /// Suggests what the developer should do next: a code example, a
    /// configuration switch, a check to run. Returned as `Option` because
    /// some variants (e.g. raw I/O wrappers) have no useful generic advice.
    fn help(&self) -> Option<String>;

    /// Deep link to the canonical documentation page for this error.
    ///
    /// Derived from [`code`](Self::code) by lowercasing and substituting
    /// `_` with `-`. Default implementation points at
    /// `https://docs.pdfluent.dev/errors/<slug>`; override only if you
    /// host your own error documentation.
    fn docs_url(&self) -> String {
        format!(
            "https://docs.pdfluent.dev/errors/{}",
            self.code().to_lowercase().replace('_', "-")
        )
    }
}

/// Top-level error type for `pdf-engine` operations.
///
/// Each variant maps 1-to-1 to a stable [`PdfError::code`] string. Variants
/// carry the minimum fields needed to render a useful message and to give
/// the caller enough context to either retry or surface a clear message
/// to a human.
///
/// The [`std::fmt::Display`] implementation produces a multi-line, formatted
/// error message including a `Help:` block and a `Docs:` link. Use it as
/// `format!("{err}")` for human consumption; use [`PdfError::code`] for
/// programmatic dispatch.
#[derive(Debug)]
pub enum Error {
    /// The requested file could not be found at the given path. Returned
    /// before any parse work is attempted.
    FileNotFound {
        /// The path that was requested but does not exist.
        path: PathBuf,
    },
    /// The PDF is encrypted and a password is required to open it. The
    /// caller should retry with `OpenOptions::with_password(...)`.
    PasswordRequired {
        /// The path of the encrypted document.
        path: PathBuf,
    },
    /// The PDF byte stream could not be parsed. The cross-reference table
    /// is malformed, the trailer is unreachable, or a critical object is
    /// missing. Try `OpenOptions::repair(true)` for a best-effort recovery.
    CorruptPdf {
        /// Path to the source file, when known.
        path: Option<PathBuf>,
        /// Human-readable diagnostic of what went wrong (for example
        /// "missing trailer", "bad startxref offset").
        reason: String,
    },
    /// The caller asked for a page index outside the document's range.
    /// Page numbers are 1-based at the public API.
    InvalidPageNumber {
        /// The page index the caller requested.
        requested: usize,
        /// How many pages the document actually has.
        total: usize,
    },
    /// A required font could not be located — neither embedded in the PDF,
    /// nor on the system, nor in the SDK's Standard 14 fallback set.
    FontNotFound {
        /// The PDF base font name that could not be resolved.
        font_name: String,
    },
    /// The requested operation is not allowed by the document's
    /// permission flags. The caller may need an owner password.
    PermissionDenied {
        /// Which operation was denied (for example "modify", "print").
        reason: String,
    },
    /// The document declares a PDF version this build of the SDK does not
    /// understand.
    UnsupportedPdfVersion {
        /// The version string from the document header (e.g. `"2.1"`).
        version: String,
    },
    /// A form-field operation referenced a field that does not exist in
    /// the document's AcroForm dictionary.
    FormFieldNotFound {
        /// The field name the caller asked for. List actual field names
        /// with `doc.form_fields()`.
        field_name: String,
    },
    /// A digital-signature verification step failed (chain of trust,
    /// hash mismatch, expired certificate, etc.).
    SignatureVerificationFailed {
        /// Specific failure reason from the signing pipeline.
        reason: String,
    },
    /// A redaction operation could not complete. The document is
    /// guaranteed to be unchanged when this is returned (redaction is
    /// transactional — failure is total).
    RedactionFailed {
        /// Diagnostic of what blocked the redaction.
        reason: String,
    },
    /// A format conversion (PDF → DOCX, PDF → image, HTML → PDF, etc.)
    /// failed before output was produced.
    ConversionFailed {
        /// Diagnostic of what blocked the conversion.
        reason: String,
    },
    /// A text or string encoding could not be decoded.
    InvalidEncoding {
        /// The encoding name that failed (for example `"WinAnsiEncoding"`).
        encoding: String,
    },
    /// A PDF stream could not be decoded — typically because the stream
    /// uses an unsupported filter or the encoded payload is corrupt.
    StreamDecodeFailed {
        /// The PDF filter name from the stream's `/Filter` entry.
        filter: String,
    },
    /// The cross-reference table is corrupt or unreadable. Try
    /// `OpenOptions::repair(true)` for a best-effort recovery pass.
    XrefCorrupt {
        /// Diagnostic of why the xref could not be parsed.
        reason: String,
    },
    /// Writing the output document failed (disk full, permission denied,
    /// network drive vanished, etc.).
    OutputWriteFailed {
        /// The destination path that was being written.
        path: PathBuf,
        /// Underlying I/O reason from the OS.
        reason: String,
    },
    /// An embedded image could not be decoded. Common causes: corrupt JPEG,
    /// unknown JPX profile, truncated pixel data.
    ImageDecodeFailed {
        /// The image format name (for example `"JPEG"`, `"JPX"`).
        format: String,
    },
    /// An encryption operation failed (wrong key, unsupported cipher,
    /// invalid permission flags).
    EncryptionFailed {
        /// Diagnostic of what blocked encryption.
        reason: String,
    },
    /// The document violates a declared compliance standard
    /// (PDF/A, PDF/UA, PDF/X). Use `pdf-compliance` to repair if possible.
    ComplianceViolation {
        /// The compliance standard the document failed against
        /// (e.g. `"PDF/A-2b"`).
        standard: String,
        /// The specific violation that triggered the failure.
        reason: String,
    },
    /// A wrapped `std::io::Error` from a lower layer. Inspect the inner
    /// error for the concrete cause.
    Io(std::io::Error),
    /// The caller exercised a feature the SDK build does not include
    /// (typically a feature-gated capability that was not enabled at
    /// compile time).
    UnsupportedFeature {
        /// The feature name (matching the Cargo feature flag where
        /// applicable).
        feature: String,
    },
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

impl PdfError for Error {
    fn code(&self) -> &str {
        match self {
            Error::FileNotFound { .. } => "FILE_NOT_FOUND",
            Error::PasswordRequired { .. } => "PASSWORD_REQUIRED",
            Error::CorruptPdf { .. } => "CORRUPT_PDF",
            Error::InvalidPageNumber { .. } => "INVALID_PAGE_NUMBER",
            Error::FontNotFound { .. } => "FONT_NOT_FOUND",
            Error::PermissionDenied { .. } => "PERMISSION_DENIED",
            Error::UnsupportedPdfVersion { .. } => "UNSUPPORTED_PDF_VERSION",
            Error::FormFieldNotFound { .. } => "FORM_FIELD_NOT_FOUND",
            Error::SignatureVerificationFailed { .. } => "SIGNATURE_VERIFICATION_FAILED",
            Error::RedactionFailed { .. } => "REDACTION_FAILED",
            Error::ConversionFailed { .. } => "CONVERSION_FAILED",
            Error::InvalidEncoding { .. } => "INVALID_ENCODING",
            Error::StreamDecodeFailed { .. } => "STREAM_DECODE_FAILED",
            Error::XrefCorrupt { .. } => "XREF_CORRUPT",
            Error::OutputWriteFailed { .. } => "OUTPUT_WRITE_FAILED",
            Error::ImageDecodeFailed { .. } => "IMAGE_DECODE_FAILED",
            Error::EncryptionFailed { .. } => "ENCRYPTION_FAILED",
            Error::ComplianceViolation { .. } => "COMPLIANCE_VIOLATION",
            Error::Io(_) => "IO_ERROR",
            Error::UnsupportedFeature { .. } => "UNSUPPORTED_FEATURE",
        }
    }

    fn help(&self) -> Option<String> {
        match self {
            Error::FileNotFound { .. } => Some(format!("Check that the file exists and the path is correct.\n        Current directory: {}", std::env::current_dir().unwrap_or_default().display())),
            Error::PasswordRequired { path } => {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                Some(format!("Pass a password when reading the file:\n\n    let doc = pdfluent::read_with(\"{}\", |opts| {{\n        opts.password(\"your-password\")\n    }})?;", file_name))
            }
            Error::CorruptPdf { .. } => Some("Try opts.repair(true) to attempt automatic repair.".to_string()),
            Error::InvalidPageNumber { requested, total } => Some(format!("The document has {} pages. Requested {}, use a 1-based index up to doc.page_count().", total, requested)),
            Error::FontNotFound { font_name } => Some(format!("Provide a custom font mapping for '{}', or ensure the font is installed on the system.", font_name)),
            Error::PermissionDenied { .. } => Some("The PDF's permissions do not allow this operation. You may need an owner password.".to_string()),
            Error::UnsupportedPdfVersion { version } => Some(format!("The SDK currently does not support PDF version {}.", version)),
            Error::FormFieldNotFound { field_name } => Some(format!("Double-check the field name '{}' using doc.form_fields().", field_name)),
            Error::SignatureVerificationFailed { .. } => Some("Check the certificate chain, validity period, and document integrity.".to_string()),
            Error::RedactionFailed { .. } => Some("Ensure coordinates are within page bounds and the document allows redaction.".to_string()),
            Error::ConversionFailed { .. } => Some("The document could not be converted to the requested format.".to_string()),
            Error::InvalidEncoding { encoding } => Some(format!("The encoding '{}' is invalid or unsupported.", encoding)),
            Error::StreamDecodeFailed { filter } => Some(format!("The stream could not be decoded using filter '{}'.", filter)),
            Error::XrefCorrupt { .. } => Some("The cross-reference table is corrupt. Try opts.repair(true).".to_string()),
            Error::OutputWriteFailed { .. } => Some("Ensure the destination path is writable and you have sufficient disk space.".to_string()),
            Error::ImageDecodeFailed { format } => Some(format!("The image format '{}' could not be decoded.", format)),
            Error::EncryptionFailed { .. } => Some("Check the encryption parameters and permissions.".to_string()),
            Error::ComplianceViolation { standard, .. } => Some(format!("The document violates the {} standard. Consider using a compliance repair tool.", standard)),
            Error::Io(_) => Some("Check the underlying I/O error details.".to_string()),
            Error::UnsupportedFeature { .. } => Some("This feature is not yet supported by the PDFluent SDK.".to_string()),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Error::FileNotFound { .. } => "FileNotFound",
            Error::PasswordRequired { .. } => "PasswordRequired",
            Error::CorruptPdf { .. } => "CorruptPdf",
            Error::InvalidPageNumber { .. } => "InvalidPageNumber",
            Error::FontNotFound { .. } => "FontNotFound",
            Error::PermissionDenied { .. } => "PermissionDenied",
            Error::UnsupportedPdfVersion { .. } => "UnsupportedPdfVersion",
            Error::FormFieldNotFound { .. } => "FormFieldNotFound",
            Error::SignatureVerificationFailed { .. } => "SignatureVerificationFailed",
            Error::RedactionFailed { .. } => "RedactionFailed",
            Error::ConversionFailed { .. } => "ConversionFailed",
            Error::InvalidEncoding { .. } => "InvalidEncoding",
            Error::StreamDecodeFailed { .. } => "StreamDecodeFailed",
            Error::XrefCorrupt { .. } => "XrefCorrupt",
            Error::OutputWriteFailed { .. } => "OutputWriteFailed",
            Error::ImageDecodeFailed { .. } => "ImageDecodeFailed",
            Error::EncryptionFailed { .. } => "EncryptionFailed",
            Error::ComplianceViolation { .. } => "ComplianceViolation",
            Error::Io(_) => "IoError",
            Error::UnsupportedFeature { .. } => "UnsupportedFeature",
        };

        write!(f, "Error: {}\n\n", name)?;

        match self {
            Error::FileNotFound { path } => {
                write!(f, "  Could not find the file at: {}\n\n", path.display())?;
            }
            Error::PasswordRequired { path } => {
                write!(
                    f,
                    "  This PDF is encrypted and requires a password to open.\n\n"
                )?;
                write!(f, "  File: {}\n\n", path.display())?;
            }
            Error::CorruptPdf { path, reason } => {
                if let Some(p) = path {
                    write!(
                        f,
                        "  The PDF file '{}' is corrupt: {}\n\n",
                        p.display(),
                        reason
                    )?;
                } else {
                    write!(f, "  The PDF data is corrupt: {}\n\n", reason)?;
                }
            }
            Error::InvalidPageNumber { requested, total } => {
                write!(
                    f,
                    "  Requested page number {}, but the document only has {} pages.\n\n",
                    requested, total
                )?;
            }
            Error::FontNotFound { font_name } => {
                write!(
                    f,
                    "  The required font '{}' could not be found.\n\n",
                    font_name
                )?;
            }
            Error::PermissionDenied { reason } => {
                write!(
                    f,
                    "  Operation denied by document permissions: {}\n\n",
                    reason
                )?;
            }
            Error::UnsupportedPdfVersion { version } => {
                write!(f, "  PDF version {} is not supported.\n\n", version)?;
            }
            Error::FormFieldNotFound { field_name } => {
                write!(f, "  Could not find form field: '{}'\n\n", field_name)?;
            }
            Error::SignatureVerificationFailed { reason } => {
                write!(f, "  Signature verification failed: {}\n\n", reason)?;
            }
            Error::RedactionFailed { reason } => {
                write!(f, "  Redaction operation failed: {}\n\n", reason)?;
            }
            Error::ConversionFailed { reason } => {
                write!(f, "  Conversion failed: {}\n\n", reason)?;
            }
            Error::InvalidEncoding { encoding } => {
                write!(
                    f,
                    "  Invalid or unsupported text encoding: {}\n\n",
                    encoding
                )?;
            }
            Error::StreamDecodeFailed { filter } => {
                write!(f, "  Failed to decode stream using filter: {}\n\n", filter)?;
            }
            Error::XrefCorrupt { reason } => {
                write!(f, "  The cross-reference table is corrupt: {}\n\n", reason)?;
            }
            Error::OutputWriteFailed { path, reason } => {
                write!(
                    f,
                    "  Failed to write output to '{}': {}\n\n",
                    path.display(),
                    reason
                )?;
            }
            Error::ImageDecodeFailed { format } => {
                write!(f, "  Failed to decode {} image.\n\n", format)?;
            }
            Error::EncryptionFailed { reason } => {
                write!(f, "  Encryption operation failed: {}\n\n", reason)?;
            }
            Error::ComplianceViolation { standard, reason } => {
                write!(
                    f,
                    "  Document violates {} compliance: {}\n\n",
                    standard, reason
                )?;
            }
            Error::Io(err) => {
                write!(f, "  I/O error occurred: {}\n\n", err)?;
            }
            Error::UnsupportedFeature { feature } => {
                write!(f, "  Unsupported feature: {}\n\n", feature)?;
            }
        }

        if let Some(help) = self.help() {
            write!(f, "  Help: ")?;
            let mut first = true;
            for line in help.lines() {
                if first {
                    writeln!(f, "{}", line)?;
                    first = false;
                } else if line.is_empty() {
                    writeln!(f)?;
                } else {
                    writeln!(f, "        {}", line)?;
                }
            }
            writeln!(f)?;
        }

        write!(f, "  Docs: {}", self.docs_url())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_not_found_message() {
        let err = Error::FileNotFound {
            path: PathBuf::from("/tmp/nonexistent.pdf"),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("FileNotFound"));
        assert!(msg.contains("/tmp/nonexistent.pdf"));
        assert!(msg.contains("Help:"));
        assert!(msg.contains("docs.pdfluent.dev"));
    }

    #[test]
    fn test_password_required_message() {
        let err = Error::PasswordRequired {
            path: PathBuf::from("invoice-2024.pdf"),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("PasswordRequired"));
        assert!(msg.contains("invoice-2024.pdf"));
        assert!(msg.contains("Help: Pass a password when reading the file:"));
        assert!(msg.contains("docs.pdfluent.dev"));
    }
}
