// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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

    #[error("call depth limit exceeded (max {max_depth} frames)")]
    /// User-defined function call stack too deep.
    CallDepthExceeded {
        /// The depth limit that was reached.
        max_depth: usize,
    },

    #[error("expression nests deeper than {max_depth} levels")]
    /// Expression tree too deep to parse or evaluate without exhausting the
    /// native stack. Reached from untrusted input, so a bound, not a panic.
    ExpressionTooDeep {
        /// The depth limit that was reached.
        max_depth: usize,
    },

    #[error("evaluation nests deeper than {max_depth} steps")]
    /// The evaluator's shared depth budget was exhausted. Counts expression
    /// nodes and user-defined call frames alike, so the two cannot multiply.
    EvalDepthExceeded {
        /// The depth limit that was reached.
        max_depth: usize,
    },

    #[error("evaluation needs more than {max_bytes} bytes of stack")]
    /// The evaluator consumed its stack budget before its step budget: this
    /// build costs more stack per step than the count assumes. Measured, not
    /// counted, so it holds in every build profile and on every stack size.
    StackBudgetExceeded {
        /// The budget, in bytes, that was reached.
        max_bytes: usize,
    },

    #[error("Division by zero")]
    /// Division by zero.
    DivisionByZero,
}

/// Result type alias for FormCalc operations.
pub type Result<T> = std::result::Result<T, FormCalcError>;
