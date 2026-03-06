//! FormCalc parser — recursive descent parser producing an AST.
//!
//! Implements XFA 3.3 §25.4 (Syntactic Grammar).
//! Operator precedence (lowest to highest):
//! 1. Assignment (=)
//! 2. Logical OR (or, |)
//! 3. Logical AND (and, &)  — note: & is also string concat
//! 4. Equality (==, <>, eq, ne)
//! 5. Relational (<, <=, >, >=, lt, le, gt, ge)
//! 6. Additive (+, -)
//! 7. Multiplicative (*, /)
//! 8. Unary (-, not)
//! 9. Primary (literals, idents, function calls, parenthesized exprs)

use crate::ast::{BinOp, Expr};
use crate::error::{FormCalcError, Result};
use crate::lexer::{Token, TokenKind};

/// Parse a token stream into a list of expressions (a script).
pub fn parse(tokens: Vec<Token>) -> Result<Vec<Expr>> {
    let mut parser = Parser::new(tokens);
    parser.parse_script()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &TokenKind {
        self.tokens
            .get(self.pos)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos];
        self.pos += 1;
        tok
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<()> {
        if self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            let span = self
                .tokens
                .get(self.pos)
                .map(|t| t.span)
                .unwrap_or(crate::lexer::Span { line: 0, col: 0 });
            Err(FormCalcError::ParseError {
                line: span.line,
                col: span.col,
                message: format!("expected {expected:?}, got {:?}", self.peek()),
            })
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek() == &TokenKind::Newline {
            self.advance();
        }
    }

    fn at_statement_end(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Newline
                | TokenKind::Eof
                | TokenKind::Else
                | TokenKind::ElseIf
                | TokenKind::EndIf
                | TokenKind::EndWhile
                | TokenKind::EndFor
                | TokenKind::EndFunc
                | TokenKind::End
        )
    }

    fn parse_script(&mut self) -> Result<Vec<Expr>> {
        let mut exprs = Vec::new();
        self.skip_newlines();
        while self.peek() != &TokenKind::Eof {
            exprs.push(self.parse_expr()?);
            self.skip_newlines();
        }
        Ok(exprs)
    }

    fn parse_body(&mut self, terminators: &[TokenKind]) -> Result<Vec<Expr>> {
        let mut body = Vec::new();
        self.skip_newlines();
        while !terminators.contains(self.peek()) && self.peek() != &TokenKind::Eof {
            body.push(self.parse_expr()?);
            self.skip_newlines();
        }
        Ok(body)
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        self.skip_newlines();
        match self.peek().clone() {
            // `If` followed by `(` is the built-in If() function, not a statement
            TokenKind::If
                if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::LParen) =>
            {
                self.advance(); // consume `if`
                self.advance(); // consume `(`
                let mut args = Vec::new();
                if self.peek() != &TokenKind::RParen {
                    loop {
                        self.skip_newlines();
                        args.push(self.parse_or()?);
                        if self.peek() != &TokenKind::Comma {
                            break;
                        }
                        self.advance();
                    }
                }
                self.expect(&TokenKind::RParen)?;
                Ok(Expr::FuncCall {
                    name: "If".to_string(),
                    args,
                })
            }
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Foreach => self.parse_foreach(),
            TokenKind::Func => self.parse_func_decl(),
            TokenKind::Var => self.parse_var_decl(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Break => {
                self.advance();
                Ok(Expr::Break)
            }
            TokenKind::Continue => {
                self.advance();
                Ok(Expr::Continue)
            }
            _ => self.parse_assignment(),
        }
    }

    fn parse_if(&mut self) -> Result<Expr> {
        self.expect(&TokenKind::If)?;
        self.skip_newlines();
        let condition = self.parse_or()?;
        self.skip_newlines();
        self.expect(&TokenKind::Then)?;
        let then_body = self.parse_body(&[TokenKind::ElseIf, TokenKind::Else, TokenKind::EndIf])?;

        let mut elseif_clauses = Vec::new();
        while self.peek() == &TokenKind::ElseIf {
            self.advance();
            self.skip_newlines();
            let cond = self.parse_or()?;
            self.skip_newlines();
            self.expect(&TokenKind::Then)?;
            let body = self.parse_body(&[TokenKind::ElseIf, TokenKind::Else, TokenKind::EndIf])?;
            elseif_clauses.push((cond, body));
        }

        let else_body = if self.peek() == &TokenKind::Else {
            self.advance();
            Some(self.parse_body(&[TokenKind::EndIf])?)
        } else {
            None
        };

        self.expect(&TokenKind::EndIf)?;
        Ok(Expr::If {
            condition: Box::new(condition),
            then_body,
            elseif_clauses,
            else_body,
        })
    }

    fn parse_while(&mut self) -> Result<Expr> {
        self.expect(&TokenKind::While)?;
        self.skip_newlines();
        let condition = self.parse_or()?;
        self.skip_newlines();
        self.expect(&TokenKind::Do)?;
        let body = self.parse_body(&[TokenKind::EndWhile])?;
        self.expect(&TokenKind::EndWhile)?;
        Ok(Expr::While {
            condition: Box::new(condition),
            body,
        })
    }

    fn parse_for(&mut self) -> Result<Expr> {
        self.expect(&TokenKind::For)?;
        self.skip_newlines();
        let var = match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                name
            }
            _ => {
                return Err(self.error("expected variable name in for loop"));
            }
        };
        self.expect(&TokenKind::Assign)?;
        let start = self.parse_or()?;

        let ascending = match self.peek() {
            TokenKind::Upto => {
                self.advance();
                true
            }
            TokenKind::Downto => {
                self.advance();
                false
            }
            _ => return Err(self.error("expected 'upto' or 'downto' in for loop")),
        };

        let end = self.parse_or()?;

        let step = if self.peek() == &TokenKind::Step {
            self.advance();
            Some(Box::new(self.parse_or()?))
        } else {
            None
        };

        self.skip_newlines();
        self.expect(&TokenKind::Do)?;
        let body = self.parse_body(&[TokenKind::EndFor])?;
        self.expect(&TokenKind::EndFor)?;

        Ok(Expr::For {
            var,
            start: Box::new(start),
            end: Box::new(end),
            step,
            ascending,
            body,
        })
    }

    fn parse_foreach(&mut self) -> Result<Expr> {
        self.expect(&TokenKind::Foreach)?;
        self.skip_newlines();
        let var = match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                name
            }
            _ => return Err(self.error("expected variable name in foreach")),
        };
        self.expect(&TokenKind::In)?;
        let list = self.parse_or()?;
        self.skip_newlines();
        self.expect(&TokenKind::Do)?;
        let body = self.parse_body(&[TokenKind::EndFor])?;
        self.expect(&TokenKind::EndFor)?;

        Ok(Expr::Foreach {
            var,
            list: Box::new(list),
            body,
        })
    }

    fn parse_func_decl(&mut self) -> Result<Expr> {
        self.expect(&TokenKind::Func)?;
        let name = match self.peek().clone() {
            TokenKind::Ident(n) => {
                self.advance();
                n
            }
            _ => return Err(self.error("expected function name")),
        };
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        if self.peek() != &TokenKind::RParen {
            loop {
                match self.peek().clone() {
                    TokenKind::Ident(p) => {
                        self.advance();
                        params.push(p);
                    }
                    _ => return Err(self.error("expected parameter name")),
                }
                if self.peek() != &TokenKind::Comma {
                    break;
                }
                self.advance(); // consume comma
            }
        }
        self.expect(&TokenKind::RParen)?;
        let body = self.parse_body(&[TokenKind::EndFunc])?;
        self.expect(&TokenKind::EndFunc)?;

        Ok(Expr::FuncDecl { name, params, body })
    }

    fn parse_var_decl(&mut self) -> Result<Expr> {
        self.expect(&TokenKind::Var)?;
        let name = match self.peek().clone() {
            TokenKind::Ident(n) => {
                self.advance();
                n
            }
            _ => return Err(self.error("expected variable name")),
        };
        let init = if self.peek() == &TokenKind::Assign {
            self.advance();
            Some(Box::new(self.parse_or()?))
        } else {
            None
        };
        Ok(Expr::VarDecl { name, init })
    }

    fn parse_return(&mut self) -> Result<Expr> {
        self.expect(&TokenKind::Return)?;
        if self.at_statement_end() {
            Ok(Expr::Return(None))
        } else {
            Ok(Expr::Return(Some(Box::new(self.parse_or()?))))
        }
    }

    fn parse_assignment(&mut self) -> Result<Expr> {
        let expr = self.parse_or()?;
        if self.peek() == &TokenKind::Assign {
            self.advance();
            let value = self.parse_or()?;
            Ok(Expr::Assign {
                target: Box::new(expr),
                value: Box::new(value),
            })
        } else {
            Ok(expr)
        }
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;
        while self.peek() == &TokenKind::Or {
            self.advance();
            self.skip_newlines();
            let right = self.parse_and()?;
            left = Expr::BinaryOp {
                op: BinOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_concat()?;
        while self.peek() == &TokenKind::And {
            self.advance();
            self.skip_newlines();
            let right = self.parse_concat()?;
            left = Expr::BinaryOp {
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_concat(&mut self) -> Result<Expr> {
        let mut left = self.parse_equality()?;
        while self.peek() == &TokenKind::Amp {
            self.advance();
            self.skip_newlines();
            let right = self.parse_equality()?;
            left = Expr::Concat(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr> {
        let mut left = self.parse_relational()?;
        loop {
            let op = match self.peek() {
                TokenKind::Eq => BinOp::Eq,
                TokenKind::Ne => BinOp::Ne,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let right = self.parse_relational()?;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek() {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Le => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Ge => BinOp::Ge,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let right = self.parse_additive()?;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let right = self.parse_multiplicative()?;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let right = self.parse_unary()?;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        match self.peek() {
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Negate(Box::new(expr)))
            }
            TokenKind::Not => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Not(Box::new(expr)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.peek().clone() {
            TokenKind::NumberLit(n) => {
                self.advance();
                Ok(Expr::Number(n))
            }
            TokenKind::StringLit(s) => {
                self.advance();
                Ok(Expr::StringLit(s))
            }
            TokenKind::Null
                if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::LParen) =>
            {
                self.advance(); // consume `null`
                self.advance(); // consume `(`
                self.expect(&TokenKind::RParen)?;
                Ok(Expr::Null)
            }
            TokenKind::Null => {
                self.advance();
                Ok(Expr::Null)
            }
            TokenKind::Ident(name) => {
                self.advance();
                // Check for function call
                if self.peek() == &TokenKind::LParen {
                    self.advance();
                    let mut args = Vec::new();
                    if self.peek() != &TokenKind::RParen {
                        loop {
                            self.skip_newlines();
                            args.push(self.parse_or()?);
                            if self.peek() != &TokenKind::Comma {
                                break;
                            }
                            self.advance();
                        }
                    }
                    self.expect(&TokenKind::RParen)?;
                    Ok(Expr::FuncCall { name, args })
                } else {
                    let mut expr = Expr::Ident(name);
                    while self.peek() == &TokenKind::Dot {
                        if let Some(next) = self.tokens.get(self.pos + 1) {
                            if let TokenKind::Ident(_) = &next.kind {
                                self.advance(); // consume dot
                                if let TokenKind::Ident(member) = self.peek().clone() {
                                    self.advance(); // consume member
                                    expr = Expr::MemberAccess {
                                        object: Box::new(expr),
                                        member,
                                    };
                                }
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    Ok(expr)
                }
            }
            TokenKind::LParen => {
                self.advance();
                self.skip_newlines();
                let expr = self.parse_or()?;
                self.skip_newlines();
                self.expect(&TokenKind::RParen)?;
                Ok(expr)
            }
            _ => Err(self.error(&format!("unexpected token: {:?}", self.peek()))),
        }
    }

    fn error(&self, message: &str) -> FormCalcError {
        let span = self
            .tokens
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or(crate::lexer::Span { line: 0, col: 0 });
        FormCalcError::ParseError {
            line: span.line,
            col: span.col,
            message: message.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse_str(src: &str) -> Vec<Expr> {
        let tokens = tokenize(src).unwrap();
        parse(tokens).unwrap()
    }

    fn parse_one(src: &str) -> Expr {
        let mut exprs = parse_str(src);
        assert_eq!(exprs.len(), 1);
        exprs.remove(0)
    }

    #[test]
    fn parse_number() {
        assert_eq!(parse_one("42"), Expr::Number(42.0));
    }

    #[test]
    fn parse_string() {
        assert_eq!(
            parse_one(r#""hello""#),
            Expr::StringLit("hello".to_string())
        );
    }

    #[test]
    fn parse_arithmetic() {
        let expr = parse_one("1 + 2 * 3");
        // Should be 1 + (2 * 3) due to precedence
        assert!(matches!(expr, Expr::BinaryOp { op: BinOp::Add, .. }));
    }

    #[test]
    fn parse_comparison() {
        let expr = parse_one("x == 42");
        assert!(matches!(expr, Expr::BinaryOp { op: BinOp::Eq, .. }));
    }

    #[test]
    fn parse_function_call() {
        let expr = parse_one("Sum(1, 2, 3)");
        match expr {
            Expr::FuncCall { name, args } => {
                assert_eq!(name, "Sum");
                assert_eq!(args.len(), 3);
            }
            _ => panic!("expected FuncCall"),
        }
    }

    #[test]
    fn parse_if_else() {
        let expr = parse_one("if x > 0 then\n  1\nelse\n  2\nendif");
        assert!(matches!(expr, Expr::If { .. }));
    }

    #[test]
    fn parse_while_loop() {
        let expr = parse_one("while x > 0 do\n  x = x - 1\nendwhile");
        assert!(matches!(expr, Expr::While { .. }));
    }

    #[test]
    fn parse_for_loop() {
        let expr = parse_one("for i = 1 upto 10 do\n  x = x + i\nendfor");
        match expr {
            Expr::For { var, ascending, .. } => {
                assert_eq!(var, "i");
                assert!(ascending);
            }
            _ => panic!("expected For"),
        }
    }

    #[test]
    fn parse_var_decl() {
        let expr = parse_one("var x = 42");
        match expr {
            Expr::VarDecl { name, init } => {
                assert_eq!(name, "x");
                assert!(init.is_some());
            }
            _ => panic!("expected VarDecl"),
        }
    }

    #[test]
    fn parse_func_decl() {
        let expr = parse_one("func add(a, b)\n  a + b\nendfunc");
        match expr {
            Expr::FuncDecl { name, params, body } => {
                assert_eq!(name, "add");
                assert_eq!(params, vec!["a", "b"]);
                assert_eq!(body.len(), 1);
            }
            _ => panic!("expected FuncDecl"),
        }
    }

    #[test]
    fn parse_assignment() {
        let expr = parse_one("x = 42");
        assert!(matches!(expr, Expr::Assign { .. }));
    }

    #[test]
    fn parse_negation() {
        let expr = parse_one("-42");
        assert!(matches!(expr, Expr::Negate(_)));
    }

    #[test]
    fn parse_string_concat() {
        let expr = parse_one(r#""hello" & " " & "world""#);
        assert!(matches!(expr, Expr::Concat(_, _)));
    }

    #[test]
    fn parse_multiline_script() {
        let exprs = parse_str("var x = 1\nvar y = 2\nx + y");
        assert_eq!(exprs.len(), 3);
    }
}
