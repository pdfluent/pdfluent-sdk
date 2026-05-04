//! Error types for the XFA engine.
use thiserror::Error;
/// XfaError.

#[derive(Debug, Error)]
pub enum XfaError {
    /// LoadFailed.
    // ---- Existing variants (preserved for backwards compatibility) ----
    #[error("failed to load PDF: {0}")]
    LoadFailed(String),
    /// PacketNotFound.
    #[error("XFA packet not found: {0}")]
    PacketNotFound(String),
    /// Encrypted.
    #[error("encrypted PDF: {0}")]
    Encrypted(String),
    /// XmlParse.
    #[error("XML parse error: {0}")]
    XmlParse(String),
    /// FontError.
    #[error("font error: {0}")]
    FontError(String),
    /// LayoutError.
    #[error("layout error: {0}")]
    LayoutError(String),
    /// LayoutFailed.
    #[error("layout failed: {0}")]
    LayoutFailed(String),
    /// ParseFailed.
    #[error("XML parse failed: {0}")]
    ParseFailed(String),
    /// FormCalcError.
    #[error("FormCalc error: {0}")]
    FormCalcError(String),
    /// Io.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    // ---- New structured variants (XFA-F9-01 #1120) ----
    /// XFA packet extraction failed (e.g. missing /AcroForm, corrupt stream).
    #[error("XFA extraction failed: {0}")]
    ExtractionFailed(String),

    /// Template XML could not be parsed.
    #[error("Template parse error: {0}")]
    TemplateParse(String),

    /// Data binding from datasets to template failed.
    #[error("Data binding failed: {0}")]
    BindingFailed(String),
    /// LayoutFailedAt.
    /// LayoutFailedAt.

    /// Layout failed at a specific pipeline stage.
    ///
    /// Use this variant when you can identify which stage (e.g. "paginate",
    /// "split", "occur") caused the failure so callers can give better
    /// diagnostics.
    #[error("Layout failed at {stage}: {reason}")]
    LayoutFailedAt {
        /// Pipeline stage where layout failed.
        stage: String,
        /// Reason for the failure.
        reason: String,
    },

    /// PDF content stream generation failed.
    #[error("Render failed: {0}")]
    RenderFailed(String),

    /// Final PDF serialisation / flatten step failed.
    #[error("Flatten failed: {0}")]
    FlattenFailed(String),

    /// Feature is intentionally unsupported by the non-interactive XFA engine.
    #[error("unsupported feature: {0}")]
    UnsupportedFeature(String),
}
/// Result.
pub type Result<T> = std::result::Result<T, XfaError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_message_extraction_failed() {
        let e = XfaError::ExtractionFailed("no /AcroForm key".to_string());
        assert_eq!(format!("{e}"), "XFA extraction failed: no /AcroForm key");
    }

    #[test]
    fn error_message_layout_failed_at() {
        let e = XfaError::LayoutFailedAt {
            stage: "paginate".to_string(),
            reason: "zero-height page area".to_string(),
        };
        assert_eq!(
            format!("{e}"),
            "Layout failed at paginate: zero-height page area"
        );
    }
}
