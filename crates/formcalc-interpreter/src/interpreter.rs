//! FormCalc interpreter — tree-walking evaluator for the FormCalc AST.

use std::collections::HashMap;

use crate::ast::{BinOp, Expr};
use crate::builtins;
use crate::error::{FormCalcError, Result};
use crate::value::Value;

/// Control flow signal from expression evaluation.
enum Signal {
    Value(Value),
    Return(Value),
    Break,
    Continue,
}

/// Environment for variable scoping.
#[derive(Debug)]
struct Env {
    scopes: Vec<HashMap<String, Value>>,
    functions: HashMap<String, (Vec<String>, Vec<Expr>)>,
}

impl Env {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn get(&self, name: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Some(val);
            }
        }
        None
    }

    fn set(&mut self, name: &str, value: Value) {
        // Update existing variable in nearest scope, or create in current scope
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return;
            }
        }
        // New variable in current scope
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), value);
        }
    }

    fn declare(&mut self, name: &str, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), value);
        }
    }
}

/// The FormCalc interpreter.
pub struct Interpreter {
    env: Env,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        Self { env: Env::new() }
    }

    /// Execute a list of expressions (a script) and return the last value.
    pub fn exec(&mut self, exprs: &[Expr]) -> Result<Value> {
        let mut result = Value::Null;
        for expr in exprs {
            match self.eval_signal(expr)? {
                Signal::Value(v) => result = v,
                Signal::Return(v) => return Ok(v),
                Signal::Break => {
                    return Err(FormCalcError::RuntimeError(
                        "break outside of loop".to_string(),
                    ))
                }
                Signal::Continue => {
                    return Err(FormCalcError::RuntimeError(
                        "continue outside of loop".to_string(),
                    ))
                }
            }
        }
        Ok(result)
    }

    /// Evaluate an expression and return its value.
    pub fn eval(&mut self, expr: &Expr) -> Result<Value> {
        match self.eval_signal(expr)? {
            Signal::Value(v) | Signal::Return(v) => Ok(v),
            Signal::Break => Err(FormCalcError::RuntimeError(
                "break outside of loop".to_string(),
            )),
            Signal::Continue => Err(FormCalcError::RuntimeError(
                "continue outside of loop".to_string(),
            )),
        }
    }

    fn eval_signal(&mut self, expr: &Expr) -> Result<Signal> {
        match expr {
            Expr::Number(n) => Ok(Signal::Value(Value::Number(*n))),
            Expr::StringLit(s) => Ok(Signal::Value(Value::String(s.clone()))),
            Expr::Null => Ok(Signal::Value(Value::Null)),

            Expr::Ident(name) => {
                let val = self.env.get(name).cloned().unwrap_or(Value::Null);
                Ok(Signal::Value(val))
            }

            Expr::Negate(inner) => {
                let val = self.eval(inner)?;
                Ok(Signal::Value(Value::Number(-val.to_number())))
            }

            Expr::Not(inner) => {
                let val = self.eval(inner)?;
                Ok(Signal::Value(Value::Number(if val.to_bool() {
                    0.0
                } else {
                    1.0
                })))
            }

            Expr::BinaryOp { op, left, right } => {
                let lval = self.eval(left)?;
                let rval = self.eval(right)?;
                let result = eval_binop(*op, &lval, &rval)?;
                Ok(Signal::Value(result))
            }

            Expr::Concat(left, right) => {
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                Ok(Signal::Value(Value::String(
                    l.to_string_val() + &r.to_string_val(),
                )))
            }

            Expr::Assign { target, value } => {
                let val = self.eval(value)?;
                match target.as_ref() {
                    Expr::Ident(name) => {
                        self.env.set(name, val.clone());
                        Ok(Signal::Value(val))
                    }
                    _ => Err(FormCalcError::RuntimeError(
                        "invalid assignment target".to_string(),
                    )),
                }
            }

            Expr::FuncCall { name, args } => {
                let mut arg_vals = Vec::with_capacity(args.len());
                for arg in args {
                    arg_vals.push(self.eval(arg)?);
                }

                // Try built-in first
                if let Some(result) = builtins::call_builtin(name, &arg_vals)? {
                    return Ok(Signal::Value(result));
                }

                // Try user-defined function
                if let Some((params, body)) = self.env.functions.get(name).cloned() {
                    if params.len() != arg_vals.len() {
                        return Err(FormCalcError::ArityError {
                            name: name.clone(),
                            expected: params.len().to_string(),
                            got: arg_vals.len(),
                        });
                    }
                    self.env.push_scope();
                    for (param, val) in params.iter().zip(arg_vals) {
                        self.env.declare(param, val);
                    }
                    let result = self.exec(&body);
                    self.env.pop_scope();
                    return Ok(Signal::Value(result?));
                }

                Err(FormCalcError::UnknownFunction(name.clone()))
            }

            Expr::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                if self.eval(condition)?.to_bool() {
                    return self.exec_block(then_body);
                }
                for (cond, body) in elseif_clauses {
                    if self.eval(cond)?.to_bool() {
                        return self.exec_block(body);
                    }
                }
                if let Some(body) = else_body {
                    return self.exec_block(body);
                }
                Ok(Signal::Value(Value::Null))
            }

            Expr::While { condition, body } => {
                let mut result = Value::Null;
                loop {
                    if !self.eval(condition)?.to_bool() {
                        break;
                    }
                    match self.exec_block(body)? {
                        Signal::Value(v) => result = v,
                        Signal::Return(v) => return Ok(Signal::Return(v)),
                        Signal::Break => break,
                        Signal::Continue => continue,
                    }
                }
                Ok(Signal::Value(result))
            }

            Expr::For {
                var,
                start,
                end,
                step,
                ascending,
                body,
            } => {
                let start_val = self.eval(start)?.to_number();
                let end_val = self.eval(end)?.to_number();
                let step_val = step
                    .as_ref()
                    .map(|s| self.eval(s))
                    .transpose()?
                    .map(|v| v.to_number())
                    .unwrap_or(1.0);

                let mut i = start_val;
                let mut result = Value::Null;

                self.env.push_scope();
                loop {
                    if *ascending && i > end_val {
                        break;
                    }
                    if !ascending && i < end_val {
                        break;
                    }
                    self.env.declare(var, Value::Number(i));

                    match self.exec_block(body)? {
                        Signal::Value(v) => result = v,
                        Signal::Return(v) => {
                            self.env.pop_scope();
                            return Ok(Signal::Return(v));
                        }
                        Signal::Break => break,
                        Signal::Continue => {}
                    }

                    if *ascending {
                        i += step_val;
                    } else {
                        i -= step_val;
                    }
                }
                self.env.pop_scope();
                Ok(Signal::Value(result))
            }

            Expr::Foreach { var, list, body } => {
                // For now, foreach only works with comma-separated string values
                let list_val = self.eval(list)?;
                let items: Vec<&str> = list_val.to_string_val().leak().split(',').collect();
                let mut result = Value::Null;

                self.env.push_scope();
                for item in items {
                    self.env
                        .declare(var, Value::String(item.trim().to_string()));
                    match self.exec_block(body)? {
                        Signal::Value(v) => result = v,
                        Signal::Return(v) => {
                            self.env.pop_scope();
                            return Ok(Signal::Return(v));
                        }
                        Signal::Break => break,
                        Signal::Continue => continue,
                    }
                }
                self.env.pop_scope();
                Ok(Signal::Value(result))
            }

            Expr::FuncDecl { name, params, body } => {
                self.env
                    .functions
                    .insert(name.clone(), (params.clone(), body.clone()));
                Ok(Signal::Value(Value::Null))
            }

            Expr::VarDecl { name, init } => {
                let val = match init {
                    Some(expr) => self.eval(expr)?,
                    None => Value::Null,
                };
                self.env.declare(name, val.clone());
                Ok(Signal::Value(val))
            }

            Expr::Return(expr) => {
                let val = match expr {
                    Some(e) => self.eval(e)?,
                    None => Value::Null,
                };
                Ok(Signal::Return(val))
            }

            Expr::Break => Ok(Signal::Break),
            Expr::Continue => Ok(Signal::Continue),
        }
    }

    fn exec_block(&mut self, body: &[Expr]) -> Result<Signal> {
        let mut result = Value::Null;
        for expr in body {
            match self.eval_signal(expr)? {
                Signal::Value(v) => result = v,
                signal @ (Signal::Return(_) | Signal::Break | Signal::Continue) => {
                    return Ok(signal)
                }
            }
        }
        Ok(Signal::Value(result))
    }
}

fn eval_binop(op: BinOp, left: &Value, right: &Value) -> Result<Value> {
    match op {
        BinOp::Add => Ok(Value::Number(left.to_number() + right.to_number())),
        BinOp::Sub => Ok(Value::Number(left.to_number() - right.to_number())),
        BinOp::Mul => Ok(Value::Number(left.to_number() * right.to_number())),
        BinOp::Div => {
            let r = right.to_number();
            if r == 0.0 {
                Err(FormCalcError::DivisionByZero)
            } else {
                Ok(Value::Number(left.to_number() / r))
            }
        }
        BinOp::Eq => Ok(Value::Number(if left == right { 1.0 } else { 0.0 })),
        BinOp::Ne => Ok(Value::Number(if left != right { 1.0 } else { 0.0 })),
        BinOp::Lt => Ok(Value::Number(if left < right { 1.0 } else { 0.0 })),
        BinOp::Le => Ok(Value::Number(if left <= right { 1.0 } else { 0.0 })),
        BinOp::Gt => Ok(Value::Number(if left > right { 1.0 } else { 0.0 })),
        BinOp::Ge => Ok(Value::Number(if left >= right { 1.0 } else { 0.0 })),
        BinOp::And => Ok(Value::Number(if left.to_bool() && right.to_bool() {
            1.0
        } else {
            0.0
        })),
        BinOp::Or => Ok(Value::Number(if left.to_bool() || right.to_bool() {
            1.0
        } else {
            0.0
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser;

    fn run(src: &str) -> Value {
        let tokens = tokenize(src).unwrap();
        let ast = parser::parse(tokens).unwrap();
        let mut interp = Interpreter::new();
        interp.exec(&ast).unwrap()
    }

    #[test]
    fn arithmetic() {
        assert_eq!(run("1 + 2 * 3"), Value::Number(7.0));
        assert_eq!(run("(1 + 2) * 3"), Value::Number(9.0));
        assert_eq!(run("10 - 3 - 2"), Value::Number(5.0));
        assert_eq!(run("10 / 2"), Value::Number(5.0));
    }

    #[test]
    fn string_concat() {
        assert_eq!(
            run(r#""hello" & " " & "world""#),
            Value::String("hello world".to_string())
        );
    }

    #[test]
    fn variables() {
        assert_eq!(run("var x = 42\nx"), Value::Number(42.0));
        assert_eq!(run("var x = 1\nx = x + 1\nx"), Value::Number(2.0));
    }

    #[test]
    fn if_then_else() {
        assert_eq!(
            run("if 1 > 0 then\n  42\nelse\n  0\nendif"),
            Value::Number(42.0)
        );
        assert_eq!(
            run("if 0 > 1 then\n  42\nelse\n  99\nendif"),
            Value::Number(99.0)
        );
    }

    #[test]
    fn while_loop() {
        assert_eq!(
            run("var x = 0\nwhile x < 5 do\n  x = x + 1\nendwhile\nx"),
            Value::Number(5.0)
        );
    }

    #[test]
    fn for_loop() {
        assert_eq!(
            run("var sum = 0\nfor i = 1 upto 5 do\n  sum = sum + i\nendfor\nsum"),
            Value::Number(15.0)
        );
    }

    #[test]
    fn for_downto() {
        assert_eq!(
            run("var sum = 0\nfor i = 5 downto 1 do\n  sum = sum + i\nendfor\nsum"),
            Value::Number(15.0)
        );
    }

    #[test]
    fn user_function() {
        assert_eq!(
            run("func double(x)\n  x * 2\nendfunc\ndouble(21)"),
            Value::Number(42.0)
        );
    }

    #[test]
    fn nested_calls() {
        assert_eq!(
            run("func add(a, b)\n  a + b\nendfunc\nadd(add(1, 2), add(3, 4))"),
            Value::Number(10.0)
        );
    }

    #[test]
    fn builtin_abs() {
        assert_eq!(run("Abs(-42)"), Value::Number(42.0));
        assert_eq!(run("Abs(42)"), Value::Number(42.0));
    }

    #[test]
    fn builtin_sum() {
        assert_eq!(run("Sum(1, 2, 3, 4, 5)"), Value::Number(15.0));
    }

    #[test]
    fn builtin_max_min() {
        assert_eq!(run("Max(1, 5, 3)"), Value::Number(5.0));
        assert_eq!(run("Min(1, 5, 3)"), Value::Number(1.0));
    }

    #[test]
    fn builtin_ceil_floor() {
        assert_eq!(run("Ceil(3.2)"), Value::Number(4.0));
        assert_eq!(run("Floor(3.8)"), Value::Number(3.0));
    }

    #[test]
    fn builtin_round() {
        assert_eq!(run("Round(3.456, 2)"), Value::Number(3.46));
    }

    #[test]
    fn builtin_len() {
        assert_eq!(run(r#"Len("hello")"#), Value::Number(5.0));
    }

    #[test]
    fn builtin_concat() {
        assert_eq!(
            run(r#"Concat("a", "b", "c")"#),
            Value::String("abc".to_string())
        );
    }

    #[test]
    fn builtin_upper_lower() {
        assert_eq!(run(r#"Upper("hello")"#), Value::String("HELLO".to_string()));
        assert_eq!(run(r#"Lower("HELLO")"#), Value::String("hello".to_string()));
    }

    #[test]
    fn builtin_substr() {
        assert_eq!(
            run(r#"Substr("hello world", 7, 5)"#),
            Value::String("world".to_string())
        );
    }

    #[test]
    fn builtin_if_function() {
        assert_eq!(run("If(1, 42, 99)"), Value::Number(42.0));
        assert_eq!(run("If(0, 42, 99)"), Value::Number(99.0));
    }

    #[test]
    fn comparison() {
        assert_eq!(run("3 == 3"), Value::Number(1.0));
        assert_eq!(run("3 <> 4"), Value::Number(1.0));
        assert_eq!(run("3 < 4"), Value::Number(1.0));
        assert_eq!(run("4 > 3"), Value::Number(1.0));
    }

    #[test]
    fn logical_operators() {
        assert_eq!(run("1 and 1"), Value::Number(1.0));
        assert_eq!(run("1 and 0"), Value::Number(0.0));
        assert_eq!(run("0 or 1"), Value::Number(1.0));
        assert_eq!(run("not 0"), Value::Number(1.0));
    }

    #[test]
    fn division_by_zero() {
        let tokens = tokenize("1 / 0").unwrap();
        let ast = parser::parse(tokens).unwrap();
        let mut interp = Interpreter::new();
        assert!(interp.exec(&ast).is_err());
    }

    #[test]
    fn break_in_loop() {
        assert_eq!(
            run("var x = 0\nwhile 1 do\n  x = x + 1\n  if x == 3 then\n    break\n  endif\nendwhile\nx"),
            Value::Number(3.0)
        );
    }
}
