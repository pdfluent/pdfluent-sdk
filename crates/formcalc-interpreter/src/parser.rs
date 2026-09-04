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
use crate::budget::{StackBudget, STACK_BUDGET_BYTES};
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

/// How deep the recursive descent itself may go.
///
/// Distinct from `MAX_DEPTH`: parentheses collapse, so `((((1))))` recurses
/// deeply while building a tree one node tall. Measured cost is ~3.7 KB of
/// native stack per level, so 64 survives a 256 KB stack.
const MAX_RECURSION: usize = 64;

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
    // If this fires, construction under-counted: a node was built without being
    // measured, and the backstop is covering for it. In release that is exactly
    // what the backstop is for. In a test build it must be loud, because the
    // construction bound is the only thing protecting the *error* path, where a
    // syntax error unwinds and ordinary `Drop` walks whatever was built --
    // there is no backstop there.
    //
    // Two leaks were found this way after a first attempt at testing them
    // passed vacuously: the test asked whether a too-deep tree was refused, the
    // backstop refused it, and the assertion was skipped.
    debug_assert!(
        depth <= MAX_DEPTH,
        "construction built a tree {depth} deep while believing it was within \
         {MAX_DEPTH}; some node is not being counted"
    );
    if depth > MAX_DEPTH {
        #[cfg(test)]
        BACKSTOP_FIRED.with(|n| n.set(n.get() + 1));
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
/// How often the backstop had to refuse a tree that construction accepted.
///
/// Test-only, and it exists because there is nothing else to observe. The
/// backstop below refuses an under-counted tree in *both* profiles, so
/// "accepted implies `ast_depth <= MAX_DEPTH`" is true no matter how badly
/// construction miscounts -- an assertion on the returned tree cannot fail.
/// Only the `debug_assert` told the two apart, and that is compiled out of the
/// `cargo test --release` that nightly.yml runs. (T2 review, #1642)
#[cfg(test)]
thread_local! {
    pub(crate) static BACKSTOP_FIRED: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

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
    /// Depth of the subtree most recently produced, in absolute terms.
    ///
    /// Every function that produces an `Expr` leaves this as
    /// `outer + depth(subtree)`. See [`Parser::built`].
    depth: usize,

    /// The stack the descent may spend, measured in bytes (#299).
    ///
    /// `MAX_RECURSION` is the semantic bound and this is the physical one, the
    /// same pairing `budget.rs` already gives the evaluator. A count protects
    /// the stack only while each level's cost is known: the 3.7 KB above was
    /// measured on a native release build, and the evaluator's equivalent
    /// number moved by more than 20x between release and debug. That is what
    /// made a counted bound insufficient there, and the parser had only the
    /// count.
    stack: StackBudget,

    /// How deep the recursive descent currently is.
    ///
    /// Separate from `depth`, and restored on the way out: it bounds the
    /// parser's own native stack, which is a different question from how deep
    /// the tree it builds is.
    recursion: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            depth: 0,
            stack: StackBudget::start(STACK_BUDGET_BYTES),
            recursion: 0,
        }
    }

    /// Record that a node has just been built over children whose subtree
    /// depths are already in `self.depth` form, and refuse if it is too deep.
    ///
    /// Every function that produces an `Expr` keeps one invariant: **on return,
    /// `self.depth == outer + depth(returned subtree)`**, where `outer` is
    /// `self.depth` on entry. That is what makes the bound compose. The earlier
    /// version counted the path the parser was *holding* instead, and released
    /// it on return -- so six precedence layers, each releasing its own links,
    /// stacked a tree thousands of nodes deep while the counter read 62.
    ///
    /// `parts` are the children's absolute depths; the node sits one above the
    /// deepest of them.
    fn built(&mut self, outer: usize, parts: &[usize]) -> Result<()> {
        let deepest = parts.iter().copied().max().unwrap_or(outer).max(outer);
        if deepest + 1 > MAX_DEPTH {
            return Err(FormCalcError::ExpressionTooDeep {
                max_depth: MAX_DEPTH,
            });
        }
        self.depth = deepest + 1;
        Ok(())
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
        self.parse_sequence(|p| p.peek() != &TokenKind::Eof)
    }

    fn parse_body(&mut self, terminators: &[TokenKind]) -> Result<Vec<Expr>> {
        self.parse_sequence(|p| !terminators.contains(p.peek()) && p.peek() != &TokenKind::Eof)
    }

    /// Parse one child of a statement from the statement's own base, folding
    /// its depth into `deepest`.
    ///
    /// Children of a statement are siblings: an `if` sits one level above the
    /// deepest of its condition and its branches, not above their sum. Without
    /// the reset the condition's depth leaks into the body, and fifty nested
    /// `if`s -- fifty levels, well inside the bound -- were refused.
    fn child<T>(
        &mut self,
        outer: usize,
        deepest: &mut usize,
        f: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        self.depth = outer;
        let value = f(self)?;
        *deepest = (*deepest).max(self.depth);
        Ok(value)
    }

    /// Parse a sequence of statements, leaving `depth` at the deepest of them.
    ///
    /// Statements side by side are *breadth*, not depth: a script of a hundred
    /// one-line assignments nests one level, not a hundred. `depth` carries the
    /// last subtree's depth and is deliberately not released inside an
    /// expression, so without resetting between siblings each statement would
    /// start where the previous one ended, and a perfectly flat script was
    /// refused at its fiftieth line.
    fn parse_sequence(&mut self, mut more: impl FnMut(&Self) -> bool) -> Result<Vec<Expr>> {
        let outer = self.depth;
        let mut deepest = outer;
        let mut out = Vec::new();
        self.skip_newlines();
        while more(self) {
            self.depth = outer;
            out.push(self.parse_expr()?);
            deepest = deepest.max(self.depth);
            self.skip_newlines();
        }
        self.depth = deepest;
        Ok(out)
    }

    /// Depth-guarded entry to expression parsing.
    ///
    /// `?` inside the parser abandons the whole parse, so `leave` is only
    /// needed on the success path: an error discards the `Parser` with its
    /// counter. Siblings, which do share a counter, are restored here.
    /// Depth-guarded entry to expression parsing.
    ///
    /// `recursion` bounds the parser's own native stack and is restored on the
    /// way out; `depth` carries the built subtree's depth and is deliberately
    /// *not* restored, because that is what makes the bound compose.
    fn parse_expr(&mut self) -> Result<Expr> {
        // Physical bound first: whichever runs out sooner should stop the
        // descent, and on a build where a level costs more than it did when
        // MAX_RECURSION was sized, this one does.
        if self.stack.exhausted() {
            return Err(FormCalcError::StackBudgetExceeded {
                max_bytes: self.stack.limit(),
            });
        }
        if self.recursion >= MAX_RECURSION {
            return Err(FormCalcError::ExpressionTooDeep {
                max_depth: MAX_DEPTH,
            });
        }
        self.recursion += 1;
        let expr = self.parse_expr_inner();
        self.recursion -= 1;
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
                // Backtracking has to rewind the depth counter as well as the
                // token position. It did not, so the abandoned `If(...)` attempt
                // left its arguments' depth behind and every `if` statement cost
                // two levels instead of one -- nesting was refused at 31.
                let saved_pos = self.pos;
                let outer = self.depth;
                let mut deepest = outer;
                self.advance(); // consume `if`
                self.advance(); // consume `(`
                let mut args = Vec::new();
                let mut ok = true;
                if self.peek() != &TokenKind::RParen {
                    loop {
                        self.skip_newlines();
                        self.depth = outer;
                        match self.parse_or() {
                            Ok(expr) => {
                                deepest = deepest.max(self.depth);
                                args.push(expr);
                            }
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
                    self.built(outer, &[deepest])?;
                    Ok(Expr::FuncCall {
                        name: "If".to_string(),
                        args,
                    })
                } else {
                    self.pos = saved_pos;
                    self.depth = outer;
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
                // Leaves, and they have to record it: a function that produces
                // an `Expr` without setting `depth` leaves the previous
                // subtree's value in place, so an enclosing node measured one
                // level short whenever a `break` was its deepest child.
                self.built(self.depth, &[])?;
                Ok(Expr::Break)
            }
            TokenKind::Continue => {
                self.advance();
                self.built(self.depth, &[])?;
                Ok(Expr::Continue)
            }
            _ => self.parse_assignment(),
        }
    }

    fn parse_if(&mut self) -> Result<Expr> {
        let outer = self.depth;
        let mut deepest = outer;
        self.expect(&TokenKind::If)?;
        self.skip_newlines();
        let condition = if self.peek() == &TokenKind::LParen {
            self.advance();
            let condition = self.child(outer, &mut deepest, |p| p.parse_or())?;
            self.skip_newlines();
            self.expect(&TokenKind::RParen)?;
            condition
        } else {
            self.child(outer, &mut deepest, |p| p.parse_or())?
        };
        self.skip_newlines();
        self.expect(&TokenKind::Then)?;
        let then_body = self.child(outer, &mut deepest, |p| {
            p.parse_body(&[TokenKind::ElseIf, TokenKind::Else, TokenKind::EndIf])
        })?;

        let mut elseif_clauses = Vec::new();
        while self.peek() == &TokenKind::ElseIf {
            self.advance();
            self.skip_newlines();
            let cond = if self.peek() == &TokenKind::LParen {
                self.advance();
                let cond = self.child(outer, &mut deepest, |p| p.parse_or())?;
                self.skip_newlines();
                self.expect(&TokenKind::RParen)?;
                cond
            } else {
                self.child(outer, &mut deepest, |p| p.parse_or())?
            };
            self.skip_newlines();
            self.expect(&TokenKind::Then)?;
            let body = self.child(outer, &mut deepest, |p| {
                p.parse_body(&[TokenKind::ElseIf, TokenKind::Else, TokenKind::EndIf])
            })?;
            elseif_clauses.push((cond, body));
        }

        let else_body = if self.peek() == &TokenKind::Else {
            self.advance();
            // Handle `else if` as `elseif` (two-token variant)
            if self.peek() == &TokenKind::If {
                let inner_if = self.child(outer, &mut deepest, |p| p.parse_if())?;
                Some(vec![inner_if])
            } else {
                Some(self.child(outer, &mut deepest, |p| p.parse_body(&[TokenKind::EndIf]))?)
            }
        } else {
            None
        };

        self.skip_newlines();
        self.expect(&TokenKind::EndIf)?;
        self.built(outer, &[deepest])?;
        Ok(Expr::If {
            condition: Box::new(condition),
            then_body,
            elseif_clauses,
            else_body,
        })
    }

    fn parse_while(&mut self) -> Result<Expr> {
        let outer = self.depth;
        let mut deepest = outer;
        self.expect(&TokenKind::While)?;
        self.skip_newlines();
        let condition = if self.peek() == &TokenKind::LParen {
            self.advance();
            let condition = self.child(outer, &mut deepest, |p| p.parse_or())?;
            self.skip_newlines();
            self.expect(&TokenKind::RParen)?;
            condition
        } else {
            self.child(outer, &mut deepest, |p| p.parse_or())?
        };
        self.skip_newlines();
        self.expect(&TokenKind::Do)?;
        let body = self.child(outer, &mut deepest, |p| {
            p.parse_body(&[TokenKind::EndWhile])
        })?;
        self.expect(&TokenKind::EndWhile)?;
        self.built(outer, &[deepest])?;
        Ok(Expr::While {
            condition: Box::new(condition),
            body,
        })
    }

    fn parse_for(&mut self) -> Result<Expr> {
        let outer = self.depth;
        let mut deepest = outer;
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
        let start = self.child(outer, &mut deepest, |p| p.parse_or())?;

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

        let end = self.child(outer, &mut deepest, |p| p.parse_or())?;

        let step = if self.peek() == &TokenKind::Step {
            self.advance();
            Some(Box::new(self.child(outer, &mut deepest, |p| p.parse_or())?))
        } else {
            None
        };

        self.skip_newlines();
        self.expect(&TokenKind::Do)?;
        let body = self.child(outer, &mut deepest, |p| p.parse_body(&[TokenKind::EndFor]))?;
        self.expect(&TokenKind::EndFor)?;

        self.built(outer, &[deepest])?;
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
        let outer = self.depth;
        let mut deepest = outer;
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
                    args.push(self.child(outer, &mut deepest, |p| p.parse_or())?);
                    if self.peek() != &TokenKind::Comma {
                        break;
                    }
                    self.advance();
                }
            }
            self.expect(&TokenKind::RParen)?;
            // A node like any other. It is synthetic -- the parenthesised list
            // of a `foreach` is wrapped in a call that no one wrote -- but it
            // sits in the tree, so leaving it uncounted made the guard report a
            // depth one short of what it had built.
            self.built(outer, &[deepest])?;
            deepest = self.depth;
            Expr::FuncCall {
                name: "__foreach_list".to_string(),
                args,
            }
        } else {
            self.child(outer, &mut deepest, |p| p.parse_or())?
        };
        self.skip_newlines();
        self.expect(&TokenKind::Do)?;
        let body = self.child(outer, &mut deepest, |p| p.parse_body(&[TokenKind::EndFor]))?;
        self.expect(&TokenKind::EndFor)?;

        self.built(outer, &[deepest])?;
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
        let outer = self.depth;
        let mut deepest = outer;
        let body = self.child(outer, &mut deepest, |p| p.parse_body(&[TokenKind::EndFunc]))?;
        self.expect(&TokenKind::EndFunc)?;

        self.built(outer, &[deepest])?;
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
        let outer = self.depth;
        let mut deepest = outer;
        let init = if self.peek() == &TokenKind::Assign {
            self.advance();
            Some(Box::new(self.child(outer, &mut deepest, |p| p.parse_or())?))
        } else {
            None
        };
        self.built(outer, &[deepest])?;
        Ok(Expr::VarDecl { name, init })
    }

    fn parse_return(&mut self) -> Result<Expr> {
        let outer = self.depth;
        let mut deepest = outer;
        self.expect(&TokenKind::Return)?;
        if self.at_statement_end() {
            self.built(outer, &[])?;
            Ok(Expr::Return(None))
        } else {
            let value = self.child(outer, &mut deepest, |p| p.parse_or())?;
            self.built(outer, &[deepest])?;
            Ok(Expr::Return(Some(Box::new(value))))
        }
    }

    fn parse_assignment(&mut self) -> Result<Expr> {
        let outer = self.depth;
        let mut deepest = outer;
        let expr = self.child(outer, &mut deepest, |p| p.parse_or())?;
        if self.peek() == &TokenKind::Assign {
            self.advance();
            let value = self.child(outer, &mut deepest, |p| p.parse_or())?;
            self.built(outer, &[deepest])?;
            Ok(Expr::Assign {
                target: Box::new(expr),
                value: Box::new(value),
            })
        } else {
            // Not an assignment after all: the depth is the operand's own.
            self.depth = deepest;
            Ok(expr)
        }
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let outer = self.depth;
        let mut left = self.parse_and()?;
        let mut left_depth = self.depth;
        while self.peek() == &TokenKind::Or {
            self.advance();
            self.skip_newlines();
            self.depth = outer;
            let right = self.parse_and()?;
            self.built(outer, &[left_depth, self.depth])?;
            left_depth = self.depth;
            left = Expr::BinaryOp {
                op: BinOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let outer = self.depth;
        let mut left = self.parse_equality()?;
        let mut left_depth = self.depth;
        while matches!(self.peek(), TokenKind::And | TokenKind::Amp) {
            self.advance();
            self.skip_newlines();
            self.depth = outer;
            let right = self.parse_equality()?;
            self.built(outer, &[left_depth, self.depth])?;
            left_depth = self.depth;
            left = Expr::BinaryOp {
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr> {
        let outer = self.depth;
        let mut left = self.parse_relational()?;
        let mut left_depth = self.depth;
        loop {
            let op = match self.peek() {
                TokenKind::Eq => BinOp::Eq,
                TokenKind::Ne => BinOp::Ne,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            self.depth = outer;
            let right = self.parse_relational()?;
            self.built(outer, &[left_depth, self.depth])?;
            left_depth = self.depth;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expr> {
        let outer = self.depth;
        let mut left = self.parse_additive()?;
        let mut left_depth = self.depth;
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
            self.depth = outer;
            let right = self.parse_additive()?;
            self.built(outer, &[left_depth, self.depth])?;
            left_depth = self.depth;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr> {
        let outer = self.depth;
        let mut left = self.parse_multiplicative()?;
        let mut left_depth = self.depth;
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            self.depth = outer;
            let right = self.parse_multiplicative()?;
            self.built(outer, &[left_depth, self.depth])?;
            left_depth = self.depth;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr> {
        let outer = self.depth;
        let mut left = self.parse_unary()?;
        let mut left_depth = self.depth;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            self.depth = outer;
            let right = self.parse_unary()?;
            self.built(outer, &[left_depth, self.depth])?;
            left_depth = self.depth;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        if self.recursion >= MAX_RECURSION {
            return Err(FormCalcError::ExpressionTooDeep {
                max_depth: MAX_DEPTH,
            });
        }
        self.recursion += 1;
        let expr = self.parse_unary_inner();
        self.recursion -= 1;
        expr
    }

    fn parse_unary_inner(&mut self) -> Result<Expr> {
        match self.peek() {
            TokenKind::Plus => {
                self.advance();
                let outer = self.depth;
                let expr = self.parse_unary()?;
                self.built(outer, &[self.depth])?;
                Ok(Expr::Positive(Box::new(expr)))
            }
            TokenKind::Minus => {
                self.advance();
                let outer = self.depth;
                let expr = self.parse_unary()?;
                self.built(outer, &[self.depth])?;
                Ok(Expr::Negate(Box::new(expr)))
            }
            TokenKind::Not => {
                self.advance();
                let outer = self.depth;
                let expr = self.parse_unary()?;
                self.built(outer, &[self.depth])?;
                Ok(Expr::Not(Box::new(expr)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let outer = self.depth;
        match self.peek().clone() {
            TokenKind::NumberLit(n) => {
                self.advance();
                self.built(outer, &[])?;
                Ok(Expr::Number(n))
            }
            TokenKind::StringLit(s) => {
                self.advance();
                self.built(outer, &[])?;
                Ok(Expr::StringLit(s))
            }
            TokenKind::Null
                if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::LParen) =>
            {
                self.advance(); // consume `null`
                self.advance(); // consume `(`
                self.expect(&TokenKind::RParen)?;
                self.built(outer, &[])?;
                Ok(Expr::Null)
            }
            TokenKind::Null => {
                self.advance();
                self.built(outer, &[])?;
                Ok(Expr::Null)
            }
            TokenKind::Ident(name) => {
                self.advance();
                // Check for function call
                if self.peek() == &TokenKind::LParen {
                    self.advance();
                    let mut args = Vec::new();
                    // Each argument is measured from the same base, so the call
                    // sits one above the deepest of them rather than above their
                    // sum -- a wide call is not a deep one.
                    let mut deepest = outer;
                    if self.peek() != &TokenKind::RParen {
                        loop {
                            self.skip_newlines();
                            self.depth = outer;
                            args.push(self.parse_or()?);
                            deepest = deepest.max(self.depth);
                            if self.peek() != &TokenKind::Comma {
                                break;
                            }
                            self.advance();
                        }
                    }
                    self.expect(&TokenKind::RParen)?;
                    self.built(outer, &[deepest])?;
                    Ok(Expr::FuncCall { name, args })
                } else {
                    self.built(outer, &[])?;
                    let mut expr = Expr::Ident(name);
                    expr = self.parse_accessor_tail(expr, outer)?;
                    Ok(expr)
                }
            }
            TokenKind::LParen => {
                self.advance();
                self.skip_newlines();
                // Parentheses build no node, so the depth is the inner one.
                let expr = self.parse_or()?;
                self.skip_newlines();
                self.expect(&TokenKind::RParen)?;
                Ok(expr)
            }
            _ => Err(self.error(&format!("unexpected token: {:?}", self.peek()))),
        }
    }

    /// Parse accessor tail: `.member`, `[index]`, `..member`, `.#name` chains.
    ///
    /// XFA Spec 3.3 §25.1 (p1055) — SOM accessor grammar:
    /// - `.name`  — child access
    /// - `[n]`   — 0-based index
    /// - `[*]`   — all occurrences
    /// - `..name` — recursive descent
    /// - `.#name` — class-based access
    ///
    /// `base` is the depth the whole expression started from, before the
    /// receiver was built. Accessor links stack on top of the receiver, but a
    /// method call does not: `expr_to_som_path` flattens the entire chain into
    /// the call's *name*, so `a.b.c…m()` is one `FuncCall` node over its
    /// arguments however long the path was. Measuring that from the current
    /// depth charged it for a receiver that is not in the tree -- a 63-member
    /// path was accepted, and the same path with `.m()` refused, though the
    /// second builds the shallower tree.
    fn parse_accessor_tail(&mut self, mut expr: Expr, base: usize) -> Result<Expr> {
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
                                    // The receiver is flattened into the name,
                                    // so this node sits on `base`, not on the
                                    // depth the accessor chain reached.
                                    let outer = base;
                                    let mut deepest = outer;
                                    if self.peek() != &TokenKind::RParen {
                                        loop {
                                            self.skip_newlines();
                                            args.push(
                                                self.child(outer, &mut deepest, |p| p.parse_or())?,
                                            );
                                            if self.peek() != &TokenKind::Comma {
                                                break;
                                            }
                                            self.advance();
                                        }
                                    }
                                    self.expect(&TokenKind::RParen)?;
                                    self.built(outer, &[deepest])?;
                                    let path = expr_to_som_path(&expr);
                                    return Ok(Expr::FuncCall {
                                        name: format!("{}.{}", path, member),
                                        args,
                                    });
                                }
                                self.built(self.depth, &[])?;
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
                                self.built(self.depth, &[])?;
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
                        self.built(self.depth, &[])?;
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
                            let outer = self.depth;
                            let mut deepest = outer;
                            let idx_expr = self.child(outer, &mut deepest, |p| p.parse_or())?;
                            self.depth = outer;
                            match idx_expr {
                                Expr::Number(n) => AccessIndex::Numeric(n as i64),
                                _ => AccessIndex::Numeric(0),
                            }
                        }
                    };
                    self.expect(&TokenKind::RBracket)?;
                    self.built(self.depth, &[])?;
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

    /// Depth composed across *precedence layers*, which the per-link counters
    /// cannot see.
    ///
    /// Each of the six left-associative parsers releases its own `links` when
    /// it returns, so one expression can accumulate multiplicative, additive,
    /// relational, equality, `and` and `or` chains without any two of them
    /// being held at once. 30 nested `Abs(...)` whose argument carries 30 links
    /// of every precedence stays around 62 held levels while building a tree
    /// over 5400 nodes deep — and a trailing operator then returns before
    /// `ast_depth` runs, leaving that tree to unwinding.
    ///
    /// Found by review after the per-link counters were already in place. The
    /// earlier estimate of the worst half-tree, about 1025, came from a search
    /// that only used `+` chains: the method was right and the search space was
    /// too narrow, for the third time in this fix.
    #[test]
    fn depth_composed_across_precedence_layers_is_still_dropped_safely() {
        let k = 30;
        let mut src = String::from("1");
        for _ in 0..30 {
            let mut arg = src;
            for lit in [" * 1", " + 1", " < 1", " == 1", " and 1", " or 1"] {
                for _ in 0..k {
                    arg.push_str(lit);
                }
            }
            src = format!("Abs({arg})");
        }
        src.push_str(" +");
        // The refusal may be the depth bound or the syntax error, depending on
        // which comes first; what this pins is that neither path overflows.
        assert!(tokenize(&src).and_then(parse).is_err());
    }

    /// Statements side by side are breadth, not depth.
    ///
    /// The counter is deliberately not released inside an expression -- that is
    /// what makes it compose -- so without a reset between siblings each
    /// statement starts where the previous one ended. A perfectly flat script
    /// was refused at its fiftieth line, which no hostile-input test would ever
    /// have found: they all measure whether deep input is *rejected*, and this
    /// is legitimate input being rejected.
    #[test]
    fn a_flat_script_of_many_statements_is_not_refused() {
        let flat = (0..500)
            .map(|i| format!("var v{i} = {i} + 1"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(accepted(&flat).len(), 500);

        let sequential_ifs = (0..500)
            .map(|i| format!("if (1 > 0) then\n  var x{i} = 1\nendif"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(accepted(&sequential_ifs).len(), 500);

        // A wide call is not a deep one either.
        let wide = format!("Sum({})", vec!["1"; 500].join(", "));
        accepted(&wide);
    }

    /// Every statement form costs the same one level per nesting.
    ///
    /// `if (` is ambiguous -- it could be the `If(a,b,c)` function -- so the
    /// parser tries the call, fails, and rewinds. The rewind restored the token
    /// position but not the depth counter, leaving the abandoned attempt's
    /// arguments on it, so an `if` cost two levels where a `while` cost one and
    /// nesting was refused at 31 instead of 62. Backtracking has to rewind
    /// everything it advanced.
    /// The parser has a physical stack bound, not only a counted one (#299).
    ///
    /// `MAX_RECURSION` is sized from a measurement: ~3.7 KB of native stack per
    /// level on a release build, so 64 levels fit a 256 KB stack. A count is
    /// only as good as that measurement, and the evaluator is where this
    /// repository already learned that the number moves: `budget.rs` exists
    /// because the same per-level cost was ~780 bytes in release and ~17 KB in
    /// debug, so one `MAX_EVAL_DEPTH` protected one profile and not the other.
    /// The parser had the count and no budget.
    ///
    /// Measured here, with the budget in place, binary-searching the deepest
    /// accepted nesting:
    ///
    ///     debug     if 55 (StackBudgetExceeded)   while 62 (ExpressionTooDeep)
    ///     release   if 62 (ExpressionTooDeep)     while 62 (ExpressionTooDeep)
    ///
    /// So in a debug build 64 levels of `if` do NOT fit the 512 KB budget, and
    /// before this the parser would have kept descending on the count alone.
    /// That is the whole claim, and it is why this test asserts the *reason* a
    /// deep script is refused rather than only that it is refused.
    #[test]
    fn a_deep_script_that_outruns_the_stack_is_refused_by_the_budget() {
        // Past BOTH bounds in every profile. A depth chosen to sit between
        // them (60 works in debug) passes here and fails in release, because
        // which bound is reached first is exactly what varies -- the first
        // version of this test did that and was caught by running it in both
        // profiles rather than one.
        let mut src = String::from("var x = 1");
        for _ in 0..200 {
            src = format!("if (1) then\n{src}\nendif");
        }
        let refusal = tokenize(&src).and_then(parse);

        assert!(refusal.is_err(), "a 200-deep script was accepted");

        // Which bound stopped it depends on the profile, and both are correct
        // answers. What must never happen is neither.
        let err = refusal.unwrap_err();
        assert!(
            matches!(
                err,
                FormCalcError::StackBudgetExceeded { .. } | FormCalcError::ExpressionTooDeep { .. }
            ),
            "refused for an unrelated reason: {err:?}"
        );
    }

    #[test]
    fn every_statement_form_costs_one_level_per_nesting() {
        // `hi` stops below the stack budget on purpose (#299). Since the
        // parser gained a physical bound beside the counted one, the deepest
        // ACCEPTED nesting is whichever bound is reached first, and in a debug
        // build that is the stack: measured, `if` exhausts 512 KB at 55 levels
        // while `while` still reaches 62. Binary-searching to 200 therefore
        // compares two different questions and reports the profile, not the
        // accounting.
        //
        // 50 is under both bounds in both profiles, so what is compared here is
        // what this test is about: that the two forms cost the same DEPTH.
        fn deepest_accepted(wrap: impl Fn(&str) -> String) -> usize {
            let (mut lo, mut hi) = (0usize, 50usize);
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                let mut src = String::from("var x = 1");
                for _ in 0..mid {
                    src = wrap(&src);
                }
                if tokenize(&src).and_then(parse).is_ok() {
                    lo = mid
                } else {
                    hi = mid
                }
            }
            lo
        }
        let ifs = deepest_accepted(|s| format!("if (1) then\n{s}\nendif"));
        let whiles = deepest_accepted(|s| format!("while (1) do\n{s}\nendwhile"));
        assert_eq!(
            ifs, whiles,
            "`if` and `while` nest identically; a difference means one of them \
             is being charged for something it did not build"
        );
        assert!(
            ifs >= 49,
            "nesting {ifs} is far below the bound of {MAX_DEPTH}"
        );
    }

    /// `ast_depth` is a backstop, and this is what it backs up.
    ///
    /// With the construction-time bound exact, no tree deeper than `MAX_DEPTH`
    /// can be built, so disabling the post-parse check changes nothing that any
    /// test can see -- its mutation stays green, and that is reported rather
    /// than treated as proof it is dead code. What it exists for is the one
    /// failure the invariant is exposed to: a construction site added later
    /// that forgets to call `built`. This pins that the measurement itself is
    /// right, so the backstop would fire if that ever happened.
    #[test]
    fn the_backstop_measures_depth_correctly() {
        // Built by hand rather than parsed, which is precisely the case a
        // missed construction site would produce.
        let mut e = Expr::Number(1.0);
        for _ in 0..(MAX_DEPTH + 10) {
            e = Expr::Negate(Box::new(e));
        }
        let script = vec![e];
        assert_eq!(ast_depth(&script), MAX_DEPTH + 11);
        assert!(ast_depth(&script) > MAX_DEPTH);

        // and it does not over-report breadth as depth
        let wide = vec![Expr::FuncCall {
            name: "Sum".into(),
            args: (0..500).map(|_| Expr::Number(1.0)).collect(),
        }];
        assert_eq!(ast_depth(&wide), 2);
    }

    /// A method call after a long SOM path is not deep, and must not be refused
    /// as if it were.
    ///
    /// `expr_to_som_path` flattens the whole receiver into the call's *name*,
    /// so `a.b.c….m()` is a single `FuncCall` over its arguments no matter how
    /// long the path is. Measuring it from the accessor-inflated depth charged
    /// it for a receiver that is not in the tree: a 63-member path was accepted,
    /// and the same path with `.m()` refused — the shallower of the two.
    ///
    /// The second half of this test is the point. Measuring from the base is
    /// only correct because the receiver disappears; the arguments do not, and
    /// they must still be bounded.
    #[test]
    fn a_method_call_is_measured_from_the_expression_base() {
        let path = format!("a{}", ".b".repeat(63));
        assert!(
            tokenize(&path).and_then(parse).is_ok(),
            "a 63-member path parses on its own"
        );
        let called = accepted(&format!("{path}.m()"));
        assert_eq!(
            ast_depth(&called),
            1,
            "the receiver is flattened into the name, so this is a single \
             argument-less FuncCall node"
        );

        // and the hostile counterpart: deep *arguments* are still refused
        let deep_arg = format!("{}1{}", "Abs(".repeat(200), ")".repeat(200));
        assert!(refused(&format!("a.b.m({deep_arg})")));
        let long_chain = vec!["1"; 200].join(" + ");
        assert!(refused(&format!("a.b.m({long_chain})")));
        // including on the error path, where the tree is dropped by unwinding
        assert!(refused(&format!("a.b.m({long_chain} +)")));
    }

    /// Every node the parser builds must be counted, including the ones nobody
    /// wrote and the ones with no children.
    ///
    /// Two leaks, both found by review, both with the same consequence: a tree
    /// one level deeper than the guard believed. `ast_depth` catches that on a
    /// successful parse, so nothing wrong is ever *accepted* — but the
    /// construction bound is what protects the error path, where a syntax error
    /// unwinds and ordinary `Drop` walks whatever was built. A bound that
    /// under-reports has already lost that guarantee.
    ///
    /// The synthetic `__foreach_list` call wrapping a parenthesised list, and
    /// `Break`/`Continue`, which returned an `Expr` without recording their own
    /// depth and so left the previous subtree's value in place.
    #[test]
    fn synthetic_nodes_and_childless_leaves_are_counted() {
        // This test asserts for itself rather than leaning on the
        // `debug_assert` in `parse`. It used to do the latter and nothing else:
        // no assertion in the body, so under `cargo test --release` -- which
        // nightly.yml:70 runs -- the debug assertion is compiled out and the
        // whole test passed in 0.00 s having checked nothing. A test that only
        // holds in debug does not cover the build we ship. (T2 review, #1642)
        //
        // What it checks instead is the invariant itself: any tree the parser
        // ACCEPTS measures within the bound. If construction under-counts, an
        // accepted tree is deeper than `MAX_DEPTH`, which is exactly the state
        // the error path cannot survive. Refusal is a fine answer too --
        // building more than was counted is not.
        BACKSTOP_FIRED.with(|n| n.set(0));
        let mut gemeten = 0usize;
        let mut controleer = |bron: &str| {
            if let Ok(ast) = tokenize(bron).and_then(parse) {
                assert!(
                    ast_depth(&ast) <= MAX_DEPTH,
                    "an accepted tree measured deeper than the bound"
                );
                gemeten += 1;
            }
        };

        for n in 50..=64 {
            let inner = format!("{}1{}", "Abs(".repeat(n), ")".repeat(n));
            controleer(&format!("foreach v in ({inner}) do\n  1\nendfor"));
            controleer(&format!("while (1) do\n  {inner}\n  break\nendwhile"));
            controleer(&format!("while (1) do\n  break\n  {inner}\nendwhile"));
        }

        // `break` and `continue` are swept too, though no input here fails
        // without their fix and that is worth saying rather than implying.
        // Statement nesting costs one recursion level and one depth level
        // together, so `MAX_RECURSION` refuses at exactly the point the
        // miscount would start to matter: measured, the deepest accepted
        // nested-`while` tree is 64 either way. The fix is kept because the
        // invariant is what later changes will lean on -- a leaf that does not
        // record its depth is a trap for the next node type added beside it --
        // not because a test can currently tell the difference.
        for n in 55..=64 {
            let mut src = String::from("break");
            for _ in 0..n {
                src = format!("while (1) do\n{src}\nendwhile");
            }
            controleer(&src);

            let mut cont = String::from("continue");
            for _ in 0..n {
                cont = format!("while (1) do\n{cont}\nendwhile");
            }
            controleer(&cont);
        }

        // A sweep that accepted nothing would assert nothing, and would say so
        // by passing. The bound is 64 and the shallow end of each sweep is well
        // inside it, so acceptances are expected.
        assert!(
            gemeten > 0,
            "the sweep accepted no tree at all, so nothing above was measured"
        );

        // The assertion that actually distinguishes the two failures. Every
        // refusal above must come from construction counting correctly and
        // stopping; if the backstop had to catch anything, construction built
        // more than it measured -- and on the error path, where a syntax error
        // unwinds through whatever was built, there is no backstop to catch it.
        let backstop = BACKSTOP_FIRED.with(|n| n.get());
        assert_eq!(
            backstop, 0,
            "the backstop refused {backstop} tree(s) that construction had \
             accepted; some node is built without being counted"
        );

        // and the plain statement forms still round-trip
        accepted("while (1) do\n  break\nendwhile");
        accepted("while (1) do\n  continue\nendwhile");
    }

    /// The question that found every leak so far: *what is the deepest tree
    /// this parser accepts?* Asked over compositions, not over single shapes.
    ///
    /// Nesting and chaining are counted by different code -- recursion for one,
    /// a loop for the other -- and the held-path counter this replaced got each
    /// right on its own while under-counting their product sixteen-fold. So the
    /// grid multiplies them: `n` levels of `Abs(...)` around a chain of `k`
    /// links, with the chain drawn from every kind the parser has (one
    /// precedence, two precedences stacked, accessors), and the whole thing
    /// wrapped in `m` statements. The tree is `n + k + m + 1` deep, so the grid
    /// straddles the bound from both sides.
    ///
    /// Three things are pinned, and each catches a different failure:
    ///
    /// * nothing accepted measures deeper than `MAX_DEPTH` -- the bound holds;
    /// * something accepted measures *exactly* `MAX_DEPTH` -- the bound is
    ///   tight, so a counter that over-charges (the method-call regression on
    ///   #1642) shows up as a maximum below 64;
    /// * the backstop never fired -- every refusal came from construction, which
    ///   is the only thing that protects the error path. (#303)
    #[test]
    fn the_deepest_accepted_tree_is_exactly_the_bound_across_compositions() {
        BACKSTOP_FIRED.with(|n| n.set(0));

        let chains: [&dyn Fn(usize) -> String; 4] = [
            // one precedence layer
            &|k| format!("1{}", " + 1".repeat(k)),
            // two layers: the multiplicative chain is the left leaf of the
            // additive one, so the depths add rather than max
            &|k| format!("1{}{}", " * 1".repeat(k / 2), " + 1".repeat(k - k / 2)),
            // accessors, built by the other loop
            &|k| format!("a{}", ".b".repeat(k)),
            // the six-layer shape from the precedence-layer finding
            &|k| {
                let mut s = String::from("1");
                for (i, lit) in [" * 1", " + 1", " < 1", " == 1", " and 1", " or 1"]
                    .iter()
                    .enumerate()
                {
                    let share = k / 6 + usize::from(i < k % 6);
                    for _ in 0..share {
                        s.push_str(lit);
                    }
                }
                s
            },
        ];
        let nestings = [0usize, 1, 2, 8, 31, 32, 33, 62, 63, 64];
        let links = [0usize, 1, 2, 8, 31, 32, 33, 62, 63, 64];
        let statements = [0usize, 1, 3];

        let mut deepest = 0usize;
        let mut accepted_count = 0usize;
        for chain in chains {
            for &n in &nestings {
                for &k in &links {
                    for &m in &statements {
                        let mut src = chain(k);
                        for _ in 0..n {
                            src = format!("Abs({src})");
                        }
                        for _ in 0..m {
                            src = format!("while (1) do\n{src}\nendwhile");
                        }
                        let expected = n + k + m + 1;
                        match tokenize(&src).and_then(parse) {
                            Ok(ast) => {
                                let d = ast_depth(&ast);
                                assert_eq!(
                                    d, expected,
                                    "shape n={n} k={k} m={m}: the test's own depth \
                                     arithmetic is wrong, so its bound claims are too"
                                );
                                assert!(
                                    d <= MAX_DEPTH,
                                    "accepted a tree {d} deep (n={n} k={k} m={m})"
                                );
                                deepest = deepest.max(d);
                                accepted_count += 1;
                            }
                            // A refusal is legitimate for one of two reasons: the
                            // tree would be too deep, or the *descent* would be.
                            // `MAX_RECURSION` is checked on entry to `parse_expr`
                            // and to `parse_unary`. Each `Abs(` level passes
                            // through both on the way to its argument, each
                            // `while` through `parse_expr` once more for its
                            // body, and the leaf itself through both -- so the
                            // leaf's check sees `2n + m + 1`. Chains pass through
                            // neither, being loops, which is exactly why `k` is
                            // absent here and why the tree can be 64 deep while
                            // the descent is 32.
                            Err(_) => assert!(
                                expected > MAX_DEPTH || 2 * n + m + 1 >= MAX_RECURSION,
                                "refused a tree only {expected} deep (n={n} k={k} m={m})"
                            ),
                        }
                    }
                }
            }
        }

        assert!(
            accepted_count > 0,
            "the grid accepted nothing, so nothing was measured"
        );
        assert_eq!(
            deepest, MAX_DEPTH,
            "the deepest accepted tree is {deepest}, not {MAX_DEPTH}: the bound is \
             not where it says it is"
        );
        let backstop = BACKSTOP_FIRED.with(|n| n.get());
        assert_eq!(
            backstop, 0,
            "the backstop refused {backstop} tree(s) that construction accepted; on \
             the error path there is no backstop, so that tree would have been \
             dropped by recursion at its full depth"
        );
    }

    /// The error path, on the smallest stack the bound is meant to hold on.
    ///
    /// A deep tree followed by a syntax error is dropped by unwinding, at the
    /// depth it reached, with nothing after it to measure -- so the only thing
    /// standing between that input and a crash is construction refusing before
    /// the tree gets deep. Recursive `Drop` costs ~61 bytes per level: 64 KB
    /// survives about a thousand levels, and the shapes here would build twenty
    /// thousand. The shapes are chosen so the *parser's* recursion stays shallow
    /// -- chains are loops, and the one nested shape nests twice, because a
    /// debug build's descent costs about 15 KB per level and four levels
    /// already overflow 64 KB on their own. That is what makes the thread's
    /// stack a measurement of the drop and not of the descent; the deep
    /// descent on a small stack is `MAX_RECURSION`'s job, tested elsewhere.
    ///
    /// Removing the construction bound does not fail this test -- it aborts the
    /// test binary with a stack overflow, which is red in a louder register.
    /// That is stated so nobody reads the SIGABRT as flake. (#303)
    #[test]
    fn a_deep_tree_that_fails_to_parse_is_refused_on_a_64_kb_stack() {
        let deep = 20_000;
        let shapes = vec![
            format!("{}1 +", "1 + ".repeat(deep)),
            format!("{}2 *", "2 * ".repeat(deep)),
            format!("{}1 or", "1 or ".repeat(deep)),
            format!("a{}.", ".b".repeat(deep)),
            {
                let tail = " + 1".repeat(2_000);
                let mut src = String::from("1");
                for _ in 0..2 {
                    src = format!("Abs({src}{tail})");
                }
                src.push_str(" +");
                src
            },
        ];
        let worker = std::thread::Builder::new()
            .name("formcalc-64kb".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                for src in shapes {
                    assert!(
                        refused(&src),
                        "a tree was built to full depth before the parse failed; \
                         unwinding then drops it recursively"
                    );
                }
            })
            .expect("spawn a 64 KB thread");
        worker
            .join()
            .expect("the 64 KB thread must finish without overflowing");
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
