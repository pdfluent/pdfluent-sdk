use thiserror::Error;

/// Errors that can occur during FormCalc execution.
#[derive(Debug, Error)]
pub enum FormCalcError {
    #[error("Lexer error at line {line}, col {col}: {message}")]
    /// Lexer error.
    LexerError {
        /// Line number.
        line: usize,
        /// Column number.
        col: usize,
        /// Error message.
        message: String,
    },

    #[error("Parse error at line {line}, col {col}: {message}")]
    /// Parse error.
    ParseError {
        /// Line number.
        line: usize,
        /// Column number.
        col: usize,
        /// Error message.
        message: String,
    },

    #[error("Runtime error: {0}")]
    /// Runtime error.
    RuntimeError(String),

    #[error("Type error: {0}")]
    /// Type error.
    TypeError(String),

    #[error("Unknown function: {0}")]
    /// Unknown function.
    UnknownFunction(String),

    #[error("Wrong number of arguments for {name}: expected {expected}, got {got}")]
    /// Wrong number of arguments.
    ArityError {
        /// Function name.
        name: String,
        /// Expected arity.
        expected: String,
        /// Got arity.
        got: usize,
    },

    #[error("Division by zero")]
    /// Division by zero.
    DivisionByZero,
}

/// Result type alias for FormCalc operations.
pub type Result<T> = std::result::Result<T, FormCalcError>;
