use thiserror::Error;

#[derive(Debug, Error)]
pub enum FormCalcError {
    #[error("Lexer error at line {line}, col {col}: {message}")]
    LexerError {
        line: usize,
        col: usize,
        message: String,
    },

    #[error("Parse error at line {line}, col {col}: {message}")]
    ParseError {
        line: usize,
        col: usize,
        message: String,
    },

    #[error("Runtime error: {0}")]
    RuntimeError(String),

    #[error("Type error: {0}")]
    TypeError(String),

    #[error("Unknown function: {0}")]
    UnknownFunction(String),

    #[error("Wrong number of arguments for {name}: expected {expected}, got {got}")]
    ArityError {
        name: String,
        expected: String,
        got: usize,
    },

    #[error("Division by zero")]
    DivisionByZero,
}

pub type Result<T> = std::result::Result<T, FormCalcError>;
