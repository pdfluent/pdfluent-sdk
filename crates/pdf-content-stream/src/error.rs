use thiserror::Error;

/// All failure modes for content stream parsing.
///
/// No variant silently drops operators or bytes.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum ContentStreamError {
    /// An operator was recognised but its operands could not be parsed or
    /// had the wrong type/count.
    #[error("malformed operands for operator '{op}': {reason}")]
    MalformedOperand { op: String, reason: String },

    /// A literal string `(...)` was opened but never closed before end-of-input.
    #[error("unterminated string literal at offset {at}")]
    UnterminatedString { at: usize },

    /// A hex string `<...>` was opened but never closed before end-of-input.
    #[error("unterminated hex string at offset {at}")]
    UnterminatedHexString { at: usize },

    /// An array `[...]` was opened but never closed before end-of-input.
    #[error("unterminated array at offset {at}")]
    UnterminatedArray { at: usize },

    /// A number token contained invalid bytes.
    #[error("malformed number at offset {at}: '{token}'")]
    MalformedNumber { at: usize, token: String },

    /// The stream ended unexpectedly while reading an operator or operand.
    #[error("unexpected end of stream at offset {at}")]
    UnexpectedEndOfStream { at: usize },

    /// A byte was encountered that is invalid in the current context.
    #[error("unexpected byte {byte:#04x} at offset {at}")]
    UnexpectedByte { byte: u8, at: usize },

    /// An operator that the parser knows but which falls outside the G3
    /// operator coverage list was encountered in strict mode.
    #[error("unsupported operator '{name}' at offset {at}")]
    UnsupportedOperator { name: String, at: usize },
}
