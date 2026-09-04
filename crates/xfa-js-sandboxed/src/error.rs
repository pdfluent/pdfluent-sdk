//! Error types for the XFA JavaScript sandbox.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// Errors returned by [`crate::XfaJsRuntime`].
///
/// Every public method returns `Result<T, XfaJsError>` — no panics.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum XfaJsError {
    /// The script threw a JavaScript exception, had a parse error, or failed
    /// at runtime for a reason other than a denied host capability.
    #[error("JavaScript runtime error: {0}")]
    Runtime(String),

    /// The script attempted to access a host capability that is not exposed by
    /// the XFA sandbox (e.g. `require`, `process`, `fetch`, file URLs).
    #[error("host capability not available in XFA sandbox: {0}")]
    UnsupportedHostCapability(String),

    /// Execution was cancelled via the [`crate::ExecCtx::cancel`] token before
    /// or during script evaluation.
    #[error("script execution was cancelled")]
    Cancelled,

    /// The JavaScript engine could not be initialised. This is unrecoverable;
    /// the runtime should be dropped and a new one created.
    #[error("JavaScript engine initialisation failed: {0}")]
    Internal(String),
}
