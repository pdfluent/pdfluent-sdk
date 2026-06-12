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

    /// RenderingPolicyUnsupported. **D11.** The requested
    /// [`crate::flatten::XfaRenderingPolicy`] is not available in the calling
    /// context (e.g. a future/unknown policy, or a command such as `flatten`
    /// that applies `SavedStateFaithful` only). Both `SavedStateFaithful` (the
    /// default, production policy) and `FreshMergeExperimental` (experimental,
    /// opt-in) are implemented as of D12; this variant exists so an unavailable
    /// policy fails loudly rather than silently producing default output under
    /// the wrong label.
    #[error("XFA rendering policy not supported: {0}")]
    RenderingPolicyUnsupported(String),

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

    /// XFA-JS-HOST-STUBS — A script reached a host capability that requires
    /// genuine user / viewer interaction (e.g. `xfa.host.messageBox`,
    /// `xfa.host.openList`, `xfa.signature.sign`). The flatten pipeline runs
    /// non-interactively, so the call cannot be satisfied honestly. Inside the
    /// sandbox the call is short-circuited to a safe default and counted in
    /// [`crate::DynamicScriptOutcome::js_unsupported_host_calls`]; this error
    /// variant exists for synchronous Rust API surfaces that want to surface
    /// the gap to embedding callers rather than silently absorb it.
    ///
    /// The `capability` string is a stable, well-known identifier such as
    /// `"xfa.host.messageBox"` and is suitable for inclusion in diagnostics.
    #[error("unsupported host capability: {capability}")]
    UnsupportedHostCapability {
        /// Stable identifier of the host capability that was requested.
        capability: String,
    },

    // ---- XfaSession field API (Phase 1 SDK foundation) ----
    /// No XFA field with the given name exists in the current layout.
    #[error("XFA field not found: {0}")]
    FieldNotFound(String),

    /// The field (or one of its ancestor containers) is `access="readOnly"`,
    /// `"protected"`, or `"nonInteractive"` — value writes are rejected.
    #[error("XFA field is read-only: {0}")]
    FieldReadOnly(String),

    /// The value is not assignable to the field (wrong kind, unknown choice
    /// item, unknown radio on-value, …).
    #[error("invalid value for XFA field {name}: {reason}")]
    InvalidFieldValue {
        /// Fully-qualified field name.
        name: String,
        /// Human-readable reason.
        reason: String,
    },

    /// The datasets writeback could not locate or rewrite the XFA packet
    /// stream in the PDF.
    #[error("XFA datasets writeback failed: {0}")]
    WritebackFailed(String),
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
    fn error_message_unsupported_host_capability() {
        let e = XfaError::UnsupportedHostCapability {
            capability: "xfa.host.messageBox".to_string(),
        };
        assert_eq!(
            format!("{e}"),
            "unsupported host capability: xfa.host.messageBox"
        );
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
