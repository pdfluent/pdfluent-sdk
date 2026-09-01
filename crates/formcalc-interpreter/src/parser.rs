//! FormCalc parser — recursive descent parser producing an AST.
//!
//! Implements the currently supported subset of XFA 3.3 §25.1 (Syntactic Grammar).
//! Operator precedence (lowest to highest):
//! 1. Assignment (=)
//! 2. Logical OR (or, |)
//! 3. Logical AND (and, &)
//! 4. Equality (==, <>, eq, ne)
//! 5. Relational (<, <=, >, >=, lt, le, gt, ge)
//! 6. Additive (+, -)
//! 7. Multiplicative (*, /)
//! 8. Unary (+, -, not)
//! 9. Primary (literals, idents, function calls, parenthesized exprs)

use crate::ast::{AccessIndex, BinOp, Expr};
use crate::error::{FormCalcError, Result};
use crate::lexer::{Token, TokenKind};

/// How deep an expression may nest before the parser refuses it.
///
/// FormCalc arrives inside XFA forms, which arrive inside PDFs, which arrive
/// from whoever sends one -- so this is a bound against hostile input, not a
/// fidelity claim. Two measurements set the number:
///
/// * The 51 FormCalc scripts in `fixtures/formcalc` reach a maximum AST depth
///   of **10**, so 64 leaves a factor of six for real forms.
/// * Measured cost is ~3.7 KB of native stack per nesting level while parsing
///   and ~750 bytes per level while evaluating. 64 levels therefore survives
///   even a 256 KB stack, which the smallest embedder thread might have. The
///   unguarded parser overflowed an 8 MB stack at 2242 levels.
///
/// It matches `interpreter::MAX_CALL_DEPTH`, which bounds the other recursion
/// in this crate, deliberately: one number for "how deep is too deep".
const MAX_DEPTH: usize = 64;

/// Parse a token stream into a list of expressions (a script).
pub fn parse(tokens: Vec<Token>) -> Result<Vec<Expr>> {
    let mut parser = Parser::new(tokens);
    let script = parser.parse_script()?;

    // The parser's own counter bounds how deep it may *recurse*, which is what
    // keeps parsing itself off the end of the stack. It does not bound how deep
    // the finished tree is: a completed subtree's depth is released when its
    // parse function returns, so `Abs(x + 1 + 1 + ...)` nested 32 deep with a
    // 31-link chain at each level is accepted at a held path of 63 while being
    // 1025 nodes deep. Measured: that tree still overflowed a 512 KB stack in
    // the evaluator. So the tree is measured once, exactly, before it is handed
    // to anything that walks it recursively.
    let depth = ast_depth(&script);
    if depth > MAX_DEPTH {
        // Dropping it normally would recurse to the depth we just refused, so
        // the refusal would crash exactly where acceptance used to.
        dismantle(script);
        return Err(FormCalcError::ExpressionTooDeep {
            max_depth: MAX_DEPTH,
        });
    }
    Ok(script)
}

/// Depth of the deepest path in a script, measured with an explicit stack.
///
/// The match is exhaustive on purpose: a variant added later must not be
/// scored as a leaf by a wildcard arm, which would silently reopen this hole.
fn ast_depth(script: &[Expr]) -> usize {
    let mut max = 0;
    let mut work: Vec<(&Expr, usize)> = script.iter().map(|e| (e, 1)).collect();
    while let Some((expr, d)) = work.pop() {
        max = max.max(d);
        match expr {
            Expr::Number(_)
            | Expr::StringLit(_)
            | Expr::Null
            | Expr::Ident(_)
            | Expr::Break
            | Expr::Continue => {}
            Expr::MemberAccess { object, .. }
            | Expr::RecursiveDescent { object, .. }
            | Expr::IndexAccess { object, .. }
            | Expr::Negate(object)
            | Expr::Positive(object)
            | Expr::Not(object) => work.push((object, d + 1)),
            Expr::BinaryOp { left, right, .. }
            | Expr::Concat(left, right)
            | Expr::Assign {
                target: left,
                value: right,
            } => {
                work.push((left, d + 1));
                work.push((right, d + 1));
            }
            Expr::FuncCall { args, .. } => work.extend(args.iter().map(|a| (a, d + 1))),
            Expr::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                work.push((condition, d + 1));
                work.extend(then_body.iter().map(|e| (e, d + 1)));
                for (cond, body) in elseif_clauses {
                    work.push((cond, d + 1));
                    work.extend(body.iter().map(|e| (e, d + 1)));
                }
                if let Some(body) = else_body {
                    work.extend(body.iter().map(|e| (e, d + 1)));
                }
            }
            Expr::While { condition, body } => {
                work.push((condition, d + 1));
                work.extend(body.iter().map(|e| (e, d + 1)));
            }
            Expr::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                work.push((start, d + 1));
                work.push((end, d + 1));
                if let Some(step) = step {
                    work.push((step, d + 1));
                }
                work.extend(body.iter().map(|e| (e, d + 1)));
            }
            Expr::Foreach { list, body, .. } => {
                work.push((list, d + 1));
                work.extend(body.iter().map(|e| (e, d + 1)));
            }
            Expr::FuncDecl { body, .. } => work.extend(body.iter().map(|e| (e, d + 1))),
            Expr::VarDecl { init, .. } | Expr::Return(init) => {
                if let Some(init) = init {
                    work.push((init, d + 1));
                }
            }
        }
    }
    max
}

/// Take a refused tree apart one node at a time, so no `Drop` recurses.
fn dismantle(script: Vec<Expr>) {
    let mut work = script;
    while let Some(mut expr) = work.pop() {
        // Children are moved into `work`; the shell left behind drops shallowly.
        match &mut expr {
            Expr::Number(_)
            | Expr::StringLit(_)
            | Expr::Null
            | Expr::Ident(_)
            | Expr::Break
            | Expr::Continue => {}
            Expr::MemberAccess { object, .. }
            | Expr::RecursiveDescent { object, .. }
            | Expr::IndexAccess { object, .. }
            | Expr::Negate(object)
            | Expr::Positive(object)
            | Expr::Not(object) => work.push(std::mem::replace(object.as_mut(), Expr::Break)),
            Expr::BinaryOp { left, right, .. }
            | Expr::Concat(left, right)
            | Expr::Assign {
                target: left,
                value: right,
            } => {
                work.push(std::mem::replace(left.as_mut(), Expr::Break));
                work.push(std::mem::replace(right.as_mut(), Expr::Break));
            }
            Expr::FuncCall { args, .. } => work.append(args),
            Expr::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                work.push(std::mem::replace(condition.as_mut(), Expr::Break));
                work.append(then_body);
                for (cond, body) in elseif_clauses.iter_mut() {
                    work.push(std::mem::replace(cond, Expr::Break));
                    work.append(body);
                }
                if let Some(body) = else_body {
                    work.append(body);
                }
            }
            Expr::While { condition, body } => {
                work.push(std::mem::replace(condition.as_mut(), Expr::Break));
                work.append(body);
            }
            Expr::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                work.push(std::mem::replace(start.as_mut(), Expr::Break));
                work.push(std::mem::replace(end.as_mut(), Expr::Break));
                if let Some(step) = step.take() {
                    work.push(*step);
                }
                work.append(body);
            }
            Expr::Foreach { list, body, .. } => {
                work.push(std::mem::replace(list.as_mut(), Expr::Break));
                work.append(body);
            }
            Expr::FuncDecl { body, .. } => work.append(body),
            Expr::VarDecl { init, .. } | Expr::Return(init) => {
                if let Some(init) = init.take() {
                    work.push(*init);
                }
            }
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// How deep the recursive descent currently is.
    ///
    /// This bounds the parser's own stack only. It says nothing about how deep
    /// the finished tree is -- `a+b+c+...` is built by a loop and costs no
    /// recursion at all -- so `ast_depth` measures the tree separately.
    depth: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            depth: 0,
        }
    }

    /// Descend one level, refusing rather than overflowing the native stack.
    fn enter(&mut self) -> Result<()> {
        if self.depth >= MAX_DEPTH {
            return Err(FormCalcError::ExpressionTooDeep {
                max_depth: MAX_DEPTH,
            });
        }
        self.depth += 1;
        Ok(())
    }

    fn leave(&mut self, levels: usize) {
        self.depth = self.depth.saturating_sub(levels);
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

    /// Depth-guarded entry to expression parsing.
    ///
    /// `?` inside the parser abandons the whole parse, so `leave` is only
    /// needed on the success path: an error discards the `Parser` with its
    /// counter. Siblings, which do share a counter, are restored here.
    fn parse_expr(&mut self) -> Result<Expr> {
        self.enter()?;
        let expr = self.parse_expr_inner();
        self.leave(1);
        expr
    }

    fn parse_expr_inner(&mut self) -> Result<Expr> {
        self.skip_newlines();
        match self.peek().clone() {
            // `if (` could be If(a,b,c) function OR if (cond) then...endif statement.
            // Backtrack: try function call; if < 2 args, parse as statement.
            TokenKind::If
                if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::LParen) =>
            {
                let saved_pos = self.pos;
                self.advance(); // consume `if`
                self.advance(); // consume `(`
                let mut args = Vec::new();
                let mut ok = true;
                if self.peek() != &TokenKind::RParen {
                    loop {
                        self.skip_newlines();
                        match self.parse_or() {
                            Ok(expr) => args.push(expr),
                            Err(_) => {
                                ok = false;
                                break;
                            }
                        }
                        if self.peek() != &TokenKind::Comma {
                            break;
                        }
                        self.advance();
                    }
                }
                if ok && self.peek() == &TokenKind::RParen && args.len() >= 2 {
                    self.advance(); // consume `)`
                    Ok(Expr::FuncCall {
                        name: "If".to_string(),
                        args,
                    })
                } else {
                    self.pos = saved_pos;
                    self.parse_if()
                }
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
        let condition = if self.peek() == &TokenKind::LParen {
            self.advance();
            let condition = self.parse_or()?;
            self.skip_newlines();
            self.expect(&TokenKind::RParen)?;
            condition
        } else {
            self.parse_or()?
        };
        self.skip_newlines();
        self.expect(&TokenKind::Then)?;
        let then_body = self.parse_body(&[TokenKind::ElseIf, TokenKind::Else, TokenKind::EndIf])?;

        let mut elseif_clauses = Vec::new();
        while self.peek() == &TokenKind::ElseIf {
            self.advance();
            self.skip_newlines();
            let cond = if self.peek() == &TokenKind::LParen {
                self.advance();
                let cond = self.parse_or()?;
                self.skip_newlines();
                self.expect(&TokenKind::RParen)?;
                cond
            } else {
                self.parse_or()?
            };
            self.skip_newlines();
            self.expect(&TokenKind::Then)?;
            let body = self.parse_body(&[TokenKind::ElseIf, TokenKind::Else, TokenKind::EndIf])?;
            elseif_clauses.push((cond, body));
        }

        let else_body = if self.peek() == &TokenKind::Else {
            self.advance();
            // Handle `else if` as `elseif` (two-token variant)
            if self.peek() == &TokenKind::If {
                let inner_if = self.parse_if()?;
                Some(vec![inner_if])
            } else {
                Some(self.parse_body(&[TokenKind::EndIf])?)
            }
        } else {
            None
        };

        self.skip_newlines();
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
        let condition = if self.peek() == &TokenKind::LParen {
            self.advance();
            let condition = self.parse_or()?;
            self.skip_newlines();
            self.expect(&TokenKind::RParen)?;
            condition
        } else {
            self.parse_or()?
        };
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
        if self.peek() == &TokenKind::Var {
            self.advance();
            self.skip_newlines();
        }
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
        let list = if self.peek() == &TokenKind::LParen {
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
            Expr::FuncCall {
                name: "__foreach_list".to_string(),
                args,
            }
        } else {
            self.parse_or()?
        };
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
        if self.peek() == &TokenKind::Do {
            self.advance();
        }
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
        let mut links = 0;
        while self.peek() == &TokenKind::Or {
            self.advance();
            self.skip_newlines();
            let right = self.parse_and()?;
            self.enter()?;
            links += 1;
            left = Expr::BinaryOp {
                op: BinOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        self.leave(links);
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_equality()?;
        let mut links = 0;
        while matches!(self.peek(), TokenKind::And | TokenKind::Amp) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_equality()?;
            self.enter()?;
            links += 1;
            left = Expr::BinaryOp {
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        self.leave(links);
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr> {
        let mut left = self.parse_relational()?;
        let mut links = 0;
        loop {
            let op = match self.peek() {
                TokenKind::Eq => BinOp::Eq,
                TokenKind::Ne => BinOp::Ne,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let right = self.parse_relational()?;
            self.enter()?;
            links += 1;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        self.leave(links);
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expr> {
        let mut left = self.parse_additive()?;
        let mut links = 0;
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
            self.enter()?;
            links += 1;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        self.leave(links);
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr> {
        let mut left = self.parse_multiplicative()?;
        let mut links = 0;
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let right = self.parse_multiplicative()?;
            self.enter()?;
            links += 1;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        self.leave(links);
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        let mut links = 0;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let right = self.parse_unary()?;
            self.enter()?;
            links += 1;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        self.leave(links);
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        self.enter()?;
        let expr = self.parse_unary_inner();
        self.leave(1);
        expr
    }

    fn parse_unary_inner(&mut self) -> Result<Expr> {
        match self.peek() {
            TokenKind::Plus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Positive(Box::new(expr)))
            }
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
                    expr = self.parse_accessor_tail(expr)?;
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

    /// Depth-guarded entry to accessor chains.
    ///
    /// `a.b[0]..c` is built by a loop, so it costs no parser stack, but it
    /// nests in the AST exactly as deep as the chain is long. Saving and
    /// restoring `depth` covers this function's several exit paths at once.
    fn parse_accessor_tail(&mut self, expr: Expr) -> Result<Expr> {
        let outer = self.depth;
        let out = self.parse_accessor_tail_inner(expr);
        self.depth = outer;
        out
    }

    /// Parse accessor tail: `.member`, `[index]`, `..member`, `.#name` chains.
    ///
    /// XFA Spec 3.3 §25.1 (p1055) — SOM accessor grammar:
    /// - `.name`  — child access
    /// - `[n]`   — 0-based index
    /// - `[*]`   — all occurrences
    /// - `..name` — recursive descent
    /// - `.#name` — class-based access
    fn parse_accessor_tail_inner(&mut self, mut expr: Expr) -> Result<Expr> {
        loop {
            match self.peek().clone() {
                // `.member` or `.#member`
                TokenKind::Dot => {
                    // Check if next token is an identifier or Hash
                    let next_kind = self.tokens.get(self.pos + 1).map(|t| t.kind.clone());
                    match next_kind {
                        Some(TokenKind::Ident(_)) => {
                            self.advance(); // consume dot
                            if let TokenKind::Ident(member) = self.peek().clone() {
                                self.advance(); // consume member
                                                // Method call: obj.member(args)
                                if self.peek() == &TokenKind::LParen {
                                    self.advance(); // consume (
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
                                    let path = expr_to_som_path(&expr);
                                    return Ok(Expr::FuncCall {
                                        name: format!("{}.{}", path, member),
                                        args,
                                    });
                                }
                                self.enter()?;
                                expr = Expr::MemberAccess {
                                    object: Box::new(expr),
                                    member,
                                };
                            }
                        }
                        Some(TokenKind::Hash) => {
                            // `.#name` — class-based access: flatten to MemberAccess with "#name"
                            self.advance(); // consume dot
                            self.advance(); // consume hash
                            if let TokenKind::Ident(member) = self.peek().clone() {
                                self.advance(); // consume member name
                                self.enter()?;
                                expr = Expr::MemberAccess {
                                    object: Box::new(expr),
                                    member: format!("#{}", member),
                                };
                            } else {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                // `..member` — recursive descent
                TokenKind::DotDot => {
                    let next_kind = self.tokens.get(self.pos + 1).map(|t| t.kind.clone());
                    if let Some(TokenKind::Ident(member)) = next_kind {
                        self.advance(); // consume ..
                        self.advance(); // consume member
                        self.enter()?;
                        expr = Expr::RecursiveDescent {
                            object: Box::new(expr),
                            member,
                        };
                    } else {
                        break;
                    }
                }
                // `[index]` or `[*]`
                TokenKind::LBracket => {
                    self.advance(); // consume [
                    let index = match self.peek().clone() {
                        TokenKind::Star => {
                            self.advance(); // consume *
                            AccessIndex::All
                        }
                        TokenKind::NumberLit(n) => {
                            self.advance(); // consume number
                            AccessIndex::Numeric(n as i64)
                        }
                        _ => {
                            // Expression index: evaluate to number
                            let idx_expr = self.parse_or()?;
                            match idx_expr {
                                Expr::Number(n) => AccessIndex::Numeric(n as i64),
                                _ => AccessIndex::Numeric(0),
                            }
                        }
                    };
                    self.expect(&TokenKind::RBracket)?;
                    self.enter()?;
                    expr = Expr::IndexAccess {
                        object: Box::new(expr),
                        index,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
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

fn expr_to_som_path(expr: &Expr) -> String {
    match expr {
        Expr::Ident(name) => name.clone(),
        Expr::MemberAccess { object, member } => {
            format!("{}.{}", expr_to_som_path(object), member)
        }
        Expr::IndexAccess { object, index } => {
            let idx = match index {
                AccessIndex::All => "[*]".to_string(),
                AccessIndex::Numeric(n) => format!("[{}]", n),
            };
            format!("{}{}", expr_to_som_path(object), idx)
        }
        Expr::RecursiveDescent { object, member } => {
            format!("{}..{}", expr_to_som_path(object), member)
        }
        _ => "<expr>".to_string(),
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
    fn parse_amp_as_logical_and() {
        let expr = parse_one("1 & 0");
        assert!(matches!(expr, Expr::BinaryOp { op: BinOp::And, .. }));
    }

    #[test]
    fn parse_multiline_script() {
        let exprs = parse_str("var x = 1\nvar y = 2\nx + y");
        assert_eq!(exprs.len(), 3);
    }
}

/// Depth bounds against hostile input (#299).
///
/// FormCalc reaches this parser from inside a PDF, so "too deep" has to be a
/// `Result`, never a stack overflow. Every depth here is chosen to be refused
/// by the guard but *shallow enough to parse fine without it* -- so removing
/// the guard fails these as assertions rather than aborting the test binary,
/// which is the difference between a red test and a dead one.
#[cfg(test)]
mod depth_bounds {
    use super::*;
    use crate::lexer::tokenize;

    fn refused(src: &str) -> bool {
        match tokenize(src).and_then(parse) {
            Err(FormCalcError::ExpressionTooDeep { .. }) => true,
            Err(_) | Ok(_) => false,
        }
    }

    fn accepted(src: &str) -> Vec<Expr> {
        tokenize(src).and_then(parse).expect("should parse")
    }

    #[test]
    fn nested_parentheses_are_refused_not_overflowed() {
        assert!(refused(&format!("{}1{}", "(".repeat(200), ")".repeat(200))));
    }

    #[test]
    fn nested_unary_is_refused() {
        assert!(refused(&format!("{}1", "-".repeat(200))));
        assert!(refused(&format!("{}1", "not ".repeat(200))));
    }

    #[test]
    fn nested_calls_are_refused() {
        assert!(refused(&format!(
            "{}1{}",
            "Abs(".repeat(200),
            ")".repeat(200)
        )));
    }

    /// A left-associative chain costs the parser no stack at all -- it is built
    /// by a loop -- but the tree it builds is exactly as deep as the chain is
    /// long, and the evaluator walks that tree recursively. A guard that only
    /// counted recursion would pass every other test here and still let this
    /// one through.
    #[test]
    fn long_operator_chains_are_refused() {
        for op in [" + ", " * ", " or ", " and ", " < ", " == "] {
            let src = (0..200).map(|_| "1").collect::<Vec<_>>().join(op);
            assert!(refused(&src), "chain of `{op}` was accepted");
        }
    }

    /// `a.b.c` and `a[0][0]` are built by the same kind of loop.
    #[test]
    fn long_accessor_chains_are_refused() {
        assert!(refused(&format!("a{}", ".b".repeat(200))));
        assert!(refused(&format!("a{}", "[0]".repeat(200))));
        assert!(refused(&format!("a{}", "..b".repeat(200))));
    }

    /// The case that a recursion-only counter accepts.
    ///
    /// 32 levels of `Abs(...)`, each holding a 31-link chain, keeps the parser's
    /// own recursion at 63 -- under the limit -- while building a tree 1025
    /// nodes deep. Measured before the tree was checked exactly: that tree still
    /// overflowed a 512 KB stack inside the evaluator.
    #[test]
    fn depth_released_on_return_does_not_compose_past_the_bound() {
        let tail = " + 1".repeat(31);
        let mut src = String::from("1");
        for _ in 0..32 {
            src = format!("Abs({src}{tail})");
        }
        assert!(refused(&src));
    }

    /// The bound has to mean what it says, not "64 per stack frame".
    #[test]
    fn nothing_accepted_is_deeper_than_the_bound() {
        let shapes = [
            format!("{}1{}", "Abs(".repeat(60), ")".repeat(60)),
            (0..60).map(|_| "1").collect::<Vec<_>>().join(" + "),
            format!("a{}", ".b".repeat(60)),
        ];
        for src in shapes {
            if let Ok(ast) = tokenize(&src).and_then(parse) {
                assert!(
                    ast_depth(&ast) <= MAX_DEPTH,
                    "accepted a tree deeper than the bound"
                );
            }
        }
    }

    /// Input that builds a deep tree and *then* fails to parse.
    ///
    /// This is the case that the exact post-parse check cannot reach: `?`
    /// returns before it runs, and the half-built tree is dropped by unwinding
    /// instead. Recursive `Drop` costs ~61 bytes per level, so an unbounded
    /// chain here overflowed a 256 KB stack at about 4250 levels -- a crash on
    /// the error path, in a fix whose success path was already clean.
    ///
    /// The bound therefore has to be applied while the tree is being built,
    /// which is what the per-link counters in the chain loops do. Deleting them
    /// leaves every test above green and turns these red.
    #[test]
    fn a_deep_tree_that_then_fails_to_parse_is_still_bounded() {
        let deep = 20_000;
        for src in [
            format!("{}1 +", "1 + ".repeat(deep)),
            format!("{}2 *", "2 * ".repeat(deep)),
            format!("{}1 or", "1 or ".repeat(deep)),
            format!("Abs({}1", "1 + ".repeat(deep)),
            format!("a{}.", ".b".repeat(deep)),
        ] {
            assert!(
                refused(&src),
                "a {deep}-deep tree was built before the parse failed; unwinding drops it"
            );
        }
    }

    /// The same, composed: the held-path counter under-counts nesting, so this
    /// is the deepest half-tree the guard can be made to leave for `Drop`.
    #[test]
    fn a_composed_deep_tree_that_fails_to_parse_is_still_bounded() {
        let tail = " + 1".repeat(200);
        let mut src = String::from("1");
        for _ in 0..200 {
            src = format!("Abs({src}{tail})");
        }
        src.push_str(" +");
        assert!(refused(&src));
    }

    /// The other half: a bound low enough to break real forms is also a bug.
    /// The deepest of the 51 FormCalc scripts in `fixtures/formcalc` measures
    /// 10, so ordinary nesting must keep working.
    #[test]
    fn ordinary_nesting_still_parses() {
        let ast = accepted("var t = Abs(Round(Sum(1 + 2 * 3, 4), 2)) + Len(\"x\" & \"y\")");
        assert!(ast_depth(&ast) > 3, "test lost its own nesting");
        accepted("if (a > 1) then\n  b = c.d.e[0] + 2\nelse\n  b = 0\nendif");
    }
}
