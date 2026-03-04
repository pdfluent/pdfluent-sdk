use thiserror::Error;

#[derive(Debug, Error)]
pub enum XfaDomError {
    #[error("XML parse error: {0}")]
    XmlParse(#[from] roxmltree::Error),

    #[error("XFA packet not found: {0}")]
    PacketNotFound(String),

    #[error("SOM path resolution failed: {path}")]
    SomResolutionFailed { path: String },

    #[error("SOM parse error at position {pos}: {message}")]
    SomParseError { pos: usize, message: String },

    #[error("Invalid node type: expected {expected}, got {got}")]
    InvalidNodeType { expected: &'static str, got: String },

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Index out of bounds: {index} (max {max})")]
    IndexOutOfBounds { index: usize, max: usize },
}

pub type Result<T> = std::result::Result<T, XfaDomError>;
