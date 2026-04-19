//! Error types for the XFA engine.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XfaError {
    // ---- Existing variants (preserved for backwards compatibility) ----
    #[error("failed to load PDF: {0}")]
    LoadFailed(String),
    #[error("XFA packet not found: {0}")]
    PacketNotFound(String),
    #[error("encrypted PDF: {0}")]
    Encrypted(String),
    #[error("XML parse error: {0}")]
    XmlParse(String),
    #[error("font error: {0}")]
    FontError(String),
    #[error("layout error: {0}")]
    LayoutError(String),
    #[error("layout failed: {0}")]
    LayoutFailed(String),
    #[error("XML parse failed: {0}")]
    ParseFailed(String),
    #[error("FormCalc error: {0}")]
    FormCalcError(String),
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

    /// Layout failed at a specific pipeline stage.
    ///
    /// Use this variant when you can identify which stage (e.g. "paginate",
    /// "split", "occur") caused the failure so callers can give better
    /// diagnostics.
    #[error("Layout failed at {stage}: {reason}")]
    LayoutFailedAt { stage: String, reason: String },

    /// PDF content stream generation failed.
    #[error("Render failed: {0}")]
    RenderFailed(String),

    /// Final PDF serialisation / flatten step failed.
    #[error("Flatten failed: {0}")]
    FlattenFailed(String),
}

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
