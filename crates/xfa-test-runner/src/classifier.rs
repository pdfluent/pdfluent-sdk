use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    // Access-level
    Encrypted,

    // Parse-level
    InvalidXref,
    CorruptStream,
    UnsupportedFilter,
    MalformedObject,
    InvalidHeader,

    // Font/text
    MissingFont,
    InvalidCmap,
    EncodingError,

    // Rendering
    UnsupportedColorSpace,
    TransparencyError,
    ImageDecodeError,

    // Structure
    InvalidFormField,
    BrokenAnnotation,
    InvalidSignature,
    ComplianceViolation,

    // System
    OutOfMemory,
    Timeout,
    Panic,
    Unknown,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Encrypted => "encrypted",
            Self::InvalidXref => "invalid_xref",
            Self::CorruptStream => "corrupt_stream",
            Self::UnsupportedFilter => "unsupported_filter",
            Self::MalformedObject => "malformed_object",
            Self::InvalidHeader => "invalid_header",
            Self::MissingFont => "missing_font",
            Self::InvalidCmap => "invalid_cmap",
            Self::EncodingError => "encoding_error",
            Self::UnsupportedColorSpace => "unsupported_colorspace",
            Self::TransparencyError => "transparency_error",
            Self::ImageDecodeError => "image_decode_error",
            Self::InvalidFormField => "invalid_form_field",
            Self::BrokenAnnotation => "broken_annotation",
            Self::InvalidSignature => "invalid_signature",
            Self::ComplianceViolation => "compliance_violation",
            Self::OutOfMemory => "out_of_memory",
            Self::Timeout => "timeout",
            Self::Panic => "panic",
            Self::Unknown => "unknown",
        };
        f.write_str(s)
    }
}

pub fn classify_error(test_name: &str, error: &str) -> ErrorCategory {
    let err_lower = error.to_lowercase();

    // Encryption
    if err_lower.contains("decryption")
        || err_lower.contains("passwordprotected")
        || err_lower.contains("password protected")
        || err_lower.contains("encrypted")
    {
        return ErrorCategory::Encrypted;
    }

    // System-level errors first
    if err_lower.contains("out of memory") || err_lower.contains("alloc") {
        return ErrorCategory::OutOfMemory;
    }
    if err_lower.contains("timeout") || err_lower.contains("timed out") {
        return ErrorCategory::Timeout;
    }
    if err_lower.contains("panic") || err_lower.contains("thread") && err_lower.contains("panicked")
    {
        return ErrorCategory::Panic;
    }

    // Parse-level
    if err_lower.contains("xref") || err_lower.contains("cross-reference") {
        return ErrorCategory::InvalidXref;
    }
    if err_lower.contains("stream")
        && (err_lower.contains("corrupt") || err_lower.contains("invalid"))
    {
        return ErrorCategory::CorruptStream;
    }
    if err_lower.contains("filter")
        || err_lower.contains("flatedecode")
        || err_lower.contains("dctdecode")
        || err_lower.contains("jbig2")
        || err_lower.contains("ccitt")
    {
        return ErrorCategory::UnsupportedFilter;
    }
    if err_lower.contains("header") || err_lower.contains("%pdf") {
        return ErrorCategory::InvalidHeader;
    }
    if err_lower.contains("object")
        || err_lower.contains("expected")
        || err_lower.contains("parse")
        || err_lower.contains("syntax")
    {
        return ErrorCategory::MalformedObject;
    }

    // Font/text
    if err_lower.contains("cmap") {
        return ErrorCategory::InvalidCmap;
    }
    if err_lower.contains("font") || err_lower.contains("glyph") {
        return ErrorCategory::MissingFont;
    }
    if err_lower.contains("encoding") || err_lower.contains("utf") || err_lower.contains("codec") {
        return ErrorCategory::EncodingError;
    }

    // Rendering
    if err_lower.contains("colorspace") || err_lower.contains("color space") {
        return ErrorCategory::UnsupportedColorSpace;
    }
    if err_lower.contains("transparency") || err_lower.contains("blend") {
        return ErrorCategory::TransparencyError;
    }
    if err_lower.contains("image")
        || err_lower.contains("decode")
        || err_lower.contains("jpeg")
        || err_lower.contains("png")
    {
        return ErrorCategory::ImageDecodeError;
    }

    // Structure — use test name for context
    match test_name {
        "form_fields" if err_lower.contains("field") => return ErrorCategory::InvalidFormField,
        "annotations" if err_lower.contains("annot") => return ErrorCategory::BrokenAnnotation,
        "signatures" if err_lower.contains("sign") => return ErrorCategory::InvalidSignature,
        "compliance" => return ErrorCategory::ComplianceViolation,
        _ => {}
    }

    if err_lower.contains("form") || err_lower.contains("field") || err_lower.contains("acroform") {
        return ErrorCategory::InvalidFormField;
    }
    if err_lower.contains("annot") {
        return ErrorCategory::BrokenAnnotation;
    }
    if err_lower.contains("sign") || err_lower.contains("certificate") {
        return ErrorCategory::InvalidSignature;
    }

    ErrorCategory::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_xref_errors() {
        assert_eq!(
            classify_error("parse", "Invalid xref table at offset 1234"),
            ErrorCategory::InvalidXref
        );
        assert_eq!(
            classify_error("parse", "cross-reference stream error"),
            ErrorCategory::InvalidXref
        );
    }

    #[test]
    fn classify_filter_errors() {
        assert_eq!(
            classify_error("parse", "Unsupported filter FlateDecode"),
            ErrorCategory::UnsupportedFilter
        );
    }

    #[test]
    fn classify_font_errors() {
        assert_eq!(
            classify_error("text_extract", "Missing font /F1"),
            ErrorCategory::MissingFont
        );
        assert_eq!(
            classify_error("text_extract", "Invalid CMap encoding"),
            ErrorCategory::InvalidCmap
        );
    }

    #[test]
    fn classify_panic() {
        assert_eq!(
            classify_error("render", "thread 'main' panicked at 'index out of bounds'"),
            ErrorCategory::Panic
        );
    }

    #[test]
    fn classify_encrypted() {
        assert_eq!(
            classify_error("parse", "Decryption(PasswordProtected)"),
            ErrorCategory::Encrypted
        );
        assert_eq!(
            classify_error("render", "invalid PDF: Decryption(UnsupportedAlgorithm)"),
            ErrorCategory::Encrypted
        );
    }

    #[test]
    fn classify_unknown() {
        assert_eq!(
            classify_error("parse", "something went wrong"),
            ErrorCategory::Unknown
        );
    }
}
