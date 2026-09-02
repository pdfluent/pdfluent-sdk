#![warn(missing_docs)]
//! FormCalc Interpreter — lexer, parser, and AST evaluator.
//!
//! Implements the FormCalc scripting language from XFA 3.3 §25,
//! including all built-in functions and SOM integration.
//!
//! This crate's public API is panic-free. Errors are returned as `Result<T, FormCalcError>`.

/// Abstract syntax tree for FormCalc expressions.
pub mod ast;
/// The stack budget shared by everything that recurses over a script.
pub mod budget;
/// Built-in FormCalc functions.
pub mod builtins;
/// Error types for the FormCalc interpreter.
pub mod error;
/// Tree-walking interpreter for FormCalc AST.
pub mod interpreter;
/// Lexer for FormCalc source code.
pub mod lexer;
/// Parser for FormCalc tokens.
pub mod parser;
/// SOM bridge for DOM resolution.
pub mod som_bridge;
/// Runtime value types.
pub mod value;
