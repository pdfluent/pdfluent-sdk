//! Error types for the XFA engine.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XfaError {
    #[error("failed to load PDF: {0}")]
    LoadFailed(String),
    #[error("XFA packet not found: {0}")]
    PacketNotFound(String),
    #[error("XML parse error: {0}")]
    XmlParse(String),
    #[error("font error: {0}")]
    FontError(String),
    #[error("layout error: {0}")]
    LayoutError(String),
    #[error("FormCalc error: {0}")]
    FormCalcError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, XfaError>;
