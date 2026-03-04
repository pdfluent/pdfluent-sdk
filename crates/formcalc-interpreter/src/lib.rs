//! FormCalc Interpreter — lexer, parser, and AST evaluator.
//!
//! Implements the FormCalc scripting language from XFA 3.3 §25,
//! including all built-in functions and SOM integration.

pub mod ast;
pub mod builtins;
pub mod error;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod value;
