use std::fmt;
use std::path::PathBuf;

/// The central Error trait for all PDFluent operations.
pub trait PdfError: std::error::Error {
    /// Machine-readable error code (bijv. "PASSWORD_REQUIRED")
    fn code(&self) -> &str;

    /// Menselijk leesbare help tekst met code example
    fn help(&self) -> Option<String>;

    /// URL naar de error docs
    fn docs_url(&self) -> String {
        format!(
            "https://docs.pdfluent.dev/errors/{}",
            self.code().to_lowercase().replace('_', "-")
        )
    }
}

/// The central Error enum for all PDFluent operations.
/// Designed to provide high context and actionable help for developers.
#[derive(Debug)]
pub enum Error {
    FileNotFound {
        path: PathBuf,
    },
    PasswordRequired {
        path: PathBuf,
    },
    CorruptPdf {
        path: Option<PathBuf>,
        reason: String,
    },
    InvalidPageNumber {
        requested: usize,
        total: usize,
    },
    FontNotFound {
        font_name: String,
    },
    PermissionDenied {
        reason: String,
    },
    UnsupportedPdfVersion {
        version: String,
    },
    FormFieldNotFound {
        field_name: String,
    },
    SignatureVerificationFailed {
        reason: String,
    },
    RedactionFailed {
        reason: String,
    },
    ConversionFailed {
        reason: String,
    },
    InvalidEncoding {
        encoding: String,
    },
    StreamDecodeFailed {
        filter: String,
    },
    XrefCorrupt {
        reason: String,
    },
    LicenseExpired {
        expired_since: String,
    },
    LicenseInvalid {
        reason: String,
    },
    OutputWriteFailed {
        path: PathBuf,
        reason: String,
    },
    ImageDecodeFailed {
        format: String,
    },
    EncryptionFailed {
        reason: String,
    },
    ComplianceViolation {
        standard: String,
        reason: String,
    },
    Io(std::io::Error),
    UnsupportedFeature {
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
            Error::LicenseExpired { .. } => "LICENSE_EXPIRED",
            Error::LicenseInvalid { .. } => "LICENSE_INVALID",
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
            Error::LicenseExpired { .. } => Some("Please renew your license key at https://pdfluent.dev/pricing".to_string()),
            Error::LicenseInvalid { .. } => Some("Check your license key or environment variables.".to_string()),
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
            Error::LicenseExpired { .. } => "LicenseExpired",
            Error::LicenseInvalid { .. } => "LicenseInvalid",
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
            Error::LicenseExpired { expired_since } => {
                write!(
                    f,
                    "  Your PDFluent license expired on {}.\n\n",
                    expired_since
                )?;
            }
            Error::LicenseInvalid { reason } => {
                write!(f, "  Invalid license key: {}\n\n", reason)?;
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
                    write!(f, "{}\n", line)?;
                    first = false;
                } else {
                    if line.is_empty() {
                        write!(f, "\n")?;
                    } else {
                        write!(f, "        {}\n", line)?;
                    }
                }
            }
            write!(f, "\n")?;
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
