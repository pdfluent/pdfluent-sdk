//! FormCalc lexer — tokenizes FormCalc source into tokens.
//!
//! Implements XFA 3.3 §25.3 (Lexical Grammar).

use crate::error::{FormCalcError, Result};

/// Token position in source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

/// A FormCalc token with position information.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// Token types for FormCalc.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    NumberLit(f64),
    StringLit(String),

    // Identifier (variable names, SOM references)
    Ident(String),

    // Keywords
    Break,
    Continue,
    Do,
    Downto,
    Else,
    ElseIf,
    End,
    EndFor,
    EndFunc,
    EndIf,
    EndWhile,
    For,
    Foreach,
    Func,
    If,
    In,
    Null,
    Return,
    Step,
    Then,
    Throw,
    Upto,
    Var,
    While,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Eq,     // == or eq
    Ne,     // <> or ne
    Lt,     // < or lt
    Le,     // <= or le
    Gt,     // > or gt
    Ge,     // >= or ge
    And,    // & or and
    Or,     // | or or
    Not,    // not
    Amp,    // & (string concatenation, context-dependent)
    Assign, // =
    Dot,    // .
    DotDot, // ..
    Hash,   // #

    // Delimiters
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Semicolon,

    // End
    Eof,
    Newline,
}

/// Tokenize FormCalc source code.
pub fn tokenize(source: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    let mut line = 1usize;
    let mut col = 1usize;

    while let Some(&ch) = chars.peek() {
        let span = Span { line, col };

        match ch {
            // Whitespace (not newlines)
            ' ' | '\t' | '\r' => {
                chars.next();
                col += 1;
            }

            // Newlines — significant as statement terminators
            '\n' => {
                chars.next();
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    span,
                });
                line += 1;
                col = 1;
            }

            // Comments
            ';' => {
                chars.next();
                col += 1;
                // Line comment: skip to end of line
                while let Some(&c) = chars.peek() {
                    if c == '\n' {
                        break;
                    }
                    chars.next();
                    col += 1;
                }
            }

            '/' if chars.clone().nth(1) == Some('/') => {
                // Line comment //
                while let Some(&c) = chars.peek() {
                    if c == '\n' {
                        break;
                    }
                    chars.next();
                    col += 1;
                }
            }

            // String literals
            '"' => {
                chars.next();
                col += 1;
                let mut s = String::new();
                loop {
                    match chars.next() {
                        Some('"') => {
                            col += 1;
                            // Check for escaped quote ""
                            if chars.peek() == Some(&'"') {
                                s.push('"');
                                chars.next();
                                col += 1;
                            } else {
                                break;
                            }
                        }
                        Some('\n') => {
                            s.push('\n');
                            line += 1;
                            col = 1;
                        }
                        Some(c) => {
                            s.push(c);
                            col += 1;
                        }
                        None => {
                            return Err(FormCalcError::LexerError {
                                line: span.line,
                                col: span.col,
                                message: "unterminated string literal".to_string(),
                            });
                        }
                    }
                }
                tokens.push(Token {
                    kind: TokenKind::StringLit(s),
                    span,
                });
            }

            // Numbers
            '0'..='9' => {
                let mut num_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() || c == '.' {
                        num_str.push(c);
                        chars.next();
                        col += 1;
                    } else {
                        break;
                    }
                }
                // Handle exponent
                if let Some(&c) = chars.peek() {
                    if c == 'e' || c == 'E' {
                        num_str.push(c);
                        chars.next();
                        col += 1;
                        if let Some(&sign) = chars.peek() {
                            if sign == '+' || sign == '-' {
                                num_str.push(sign);
                                chars.next();
                                col += 1;
                            }
                        }
                        while let Some(&c) = chars.peek() {
                            if c.is_ascii_digit() {
                                num_str.push(c);
                                chars.next();
                                col += 1;
                            } else {
                                break;
                            }
                        }
                    }
                }
                let n: f64 = num_str.parse().map_err(|_| FormCalcError::LexerError {
                    line: span.line,
                    col: span.col,
                    message: format!("invalid number: '{num_str}'"),
                })?;
                tokens.push(Token {
                    kind: TokenKind::NumberLit(n),
                    span,
                });
            }

            // Identifiers and keywords
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut ident = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        ident.push(c);
                        chars.next();
                        col += 1;
                    } else {
                        break;
                    }
                }
                let kind = match ident.to_lowercase().as_str() {
                    "break" => TokenKind::Break,
                    "continue" => TokenKind::Continue,
                    "do" => TokenKind::Do,
                    "downto" => TokenKind::Downto,
                    "else" => TokenKind::Else,
                    "elseif" => TokenKind::ElseIf,
                    "end" => TokenKind::End,
                    "endfor" => TokenKind::EndFor,
                    "endfunc" => TokenKind::EndFunc,
                    "endif" => TokenKind::EndIf,
                    "endwhile" => TokenKind::EndWhile,
                    "for" => TokenKind::For,
                    "foreach" => TokenKind::Foreach,
                    "func" => TokenKind::Func,
                    "if" => TokenKind::If,
                    "in" => TokenKind::In,
                    "null" => TokenKind::Null,
                    "return" => TokenKind::Return,
                    "step" => TokenKind::Step,
                    "then" => TokenKind::Then,
                    "throw" => TokenKind::Throw,
                    "upto" => TokenKind::Upto,
                    "var" => TokenKind::Var,
                    "while" => TokenKind::While,
                    "and" => TokenKind::And,
                    "or" => TokenKind::Or,
                    "not" => TokenKind::Not,
                    "eq" => TokenKind::Eq,
                    "ne" => TokenKind::Ne,
                    "lt" => TokenKind::Lt,
                    "le" => TokenKind::Le,
                    "gt" => TokenKind::Gt,
                    "ge" => TokenKind::Ge,
                    _ => TokenKind::Ident(ident),
                };
                tokens.push(Token { kind, span });
            }

            // Operators and delimiters
            '+' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::Plus,
                    span,
                });
            }
            '-' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::Minus,
                    span,
                });
            }
            '*' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::Star,
                    span,
                });
            }
            '/' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::Slash,
                    span,
                });
            }
            '=' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                    tokens.push(Token {
                        kind: TokenKind::Eq,
                        span,
                    });
                } else {
                    tokens.push(Token {
                        kind: TokenKind::Assign,
                        span,
                    });
                }
            }
            '<' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                    tokens.push(Token {
                        kind: TokenKind::Le,
                        span,
                    });
                } else if chars.peek() == Some(&'>') {
                    chars.next();
                    col += 1;
                    tokens.push(Token {
                        kind: TokenKind::Ne,
                        span,
                    });
                } else {
                    tokens.push(Token {
                        kind: TokenKind::Lt,
                        span,
                    });
                }
            }
            '>' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                    tokens.push(Token {
                        kind: TokenKind::Ge,
                        span,
                    });
                } else {
                    tokens.push(Token {
                        kind: TokenKind::Gt,
                        span,
                    });
                }
            }
            '&' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::Amp,
                    span,
                });
            }
            '|' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::Or,
                    span,
                });
            }
            '.' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'.') {
                    chars.next();
                    col += 1;
                    tokens.push(Token {
                        kind: TokenKind::DotDot,
                        span,
                    });
                } else if chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                    // Decimal number starting with .
                    let mut num_str = String::from("0.");
                    while let Some(&c) = chars.peek() {
                        if c.is_ascii_digit() {
                            num_str.push(c);
                            chars.next();
                            col += 1;
                        } else {
                            break;
                        }
                    }
                    let n: f64 = num_str.parse().map_err(|_| FormCalcError::LexerError {
                        line: span.line,
                        col: span.col,
                        message: format!("invalid number: '{num_str}'"),
                    })?;
                    tokens.push(Token {
                        kind: TokenKind::NumberLit(n),
                        span,
                    });
                } else {
                    tokens.push(Token {
                        kind: TokenKind::Dot,
                        span,
                    });
                }
            }
            '#' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::Hash,
                    span,
                });
            }
            '(' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::LParen,
                    span,
                });
            }
            ')' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::RParen,
                    span,
                });
            }
            '[' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::LBracket,
                    span,
                });
            }
            ']' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::RBracket,
                    span,
                });
            }
            ',' => {
                chars.next();
                col += 1;
                tokens.push(Token {
                    kind: TokenKind::Comma,
                    span,
                });
            }
            '!' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                    tokens.push(Token {
                        kind: TokenKind::Ne,
                        span,
                    });
                } else {
                    tokens.push(Token {
                        kind: TokenKind::Not,
                        span,
                    });
                }
            }
            '$' => {
                chars.next();
                col += 1;
                let mut ident = String::from("$");
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        ident.push(c);
                        chars.next();
                        col += 1;
                    } else {
                        break;
                    }
                }
                tokens.push(Token {
                    kind: TokenKind::Ident(ident),
                    span,
                });
            }
            _ => {
                return Err(FormCalcError::LexerError {
                    line,
                    col,
                    message: format!("unexpected character: '{ch}'"),
                });
            }
        }
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span { line, col },
    });

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| !matches!(k, TokenKind::Newline | TokenKind::Eof))
            .collect()
    }

    #[test]
    fn simple_arithmetic() {
        assert_eq!(
            kinds("1 + 2 * 3"),
            vec![
                TokenKind::NumberLit(1.0),
                TokenKind::Plus,
                TokenKind::NumberLit(2.0),
                TokenKind::Star,
                TokenKind::NumberLit(3.0),
            ]
        );
    }

    #[test]
    fn string_literal() {
        assert_eq!(
            kinds(r#""hello world""#),
            vec![TokenKind::StringLit("hello world".to_string())]
        );
    }

    #[test]
    fn escaped_quote() {
        assert_eq!(
            kinds(r#""say ""hello""""#),
            vec![TokenKind::StringLit("say \"hello\"".to_string())]
        );
    }

    #[test]
    fn keywords() {
        assert_eq!(
            kinds("if x then y else z endif"),
            vec![
                TokenKind::If,
                TokenKind::Ident("x".to_string()),
                TokenKind::Then,
                TokenKind::Ident("y".to_string()),
                TokenKind::Else,
                TokenKind::Ident("z".to_string()),
                TokenKind::EndIf,
            ]
        );
    }

    #[test]
    fn keywords_case_insensitive() {
        assert_eq!(
            kinds("IF Then ELSE EndIf"),
            vec![
                TokenKind::If,
                TokenKind::Then,
                TokenKind::Else,
                TokenKind::EndIf
            ]
        );
    }

    #[test]
    fn comparison_operators() {
        assert_eq!(
            kinds("a == b <> c <= d >= e"),
            vec![
                TokenKind::Ident("a".to_string()),
                TokenKind::Eq,
                TokenKind::Ident("b".to_string()),
                TokenKind::Ne,
                TokenKind::Ident("c".to_string()),
                TokenKind::Le,
                TokenKind::Ident("d".to_string()),
                TokenKind::Ge,
                TokenKind::Ident("e".to_string()),
            ]
        );
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn decimal_number() {
        assert_eq!(kinds("3.14"), vec![TokenKind::NumberLit(3.14)]);
    }

    #[test]
    fn exponent_number() {
        assert_eq!(kinds("1.5e10"), vec![TokenKind::NumberLit(1.5e10)]);
        assert_eq!(kinds("2E-3"), vec![TokenKind::NumberLit(2e-3)]);
    }

    #[test]
    fn line_comment() {
        assert_eq!(
            kinds("1 + 2 ; this is a comment\n3"),
            vec![
                TokenKind::NumberLit(1.0),
                TokenKind::Plus,
                TokenKind::NumberLit(2.0),
                TokenKind::NumberLit(3.0),
            ]
        );
    }

    #[test]
    fn function_call() {
        assert_eq!(
            kinds("Sum(1, 2, 3)"),
            vec![
                TokenKind::Ident("Sum".to_string()),
                TokenKind::LParen,
                TokenKind::NumberLit(1.0),
                TokenKind::Comma,
                TokenKind::NumberLit(2.0),
                TokenKind::Comma,
                TokenKind::NumberLit(3.0),
                TokenKind::RParen,
            ]
        );
    }

    #[test]
    fn assignment() {
        assert_eq!(
            kinds("x = 42"),
            vec![
                TokenKind::Ident("x".to_string()),
                TokenKind::Assign,
                TokenKind::NumberLit(42.0),
            ]
        );
    }

    #[test]
    fn dot_number() {
        assert_eq!(kinds(".5"), vec![TokenKind::NumberLit(0.5)]);
    }
}
