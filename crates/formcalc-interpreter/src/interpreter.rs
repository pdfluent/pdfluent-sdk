//! FormCalc interpreter — tree-walking evaluator for the FormCalc AST.

use std::collections::HashMap;

use crate::ast::{AccessIndex, BinOp, Expr};
use crate::builtins;
use crate::error::{FormCalcError, Result};
use crate::som_bridge::{self, DomContext, SomResolver};
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

/// Maximum instructions to execute before aborting (prevents infinite loops).
const MAX_INSTRUCTIONS: u64 = 100_000;

/// Maximum user-defined function call depth before aborting.
const MAX_CALL_DEPTH: usize = 64;

/// How many recursive steps the evaluator may take, counting an expression
/// node and a user-defined call frame alike.
///
/// `MAX_DEPTH` (expressions, in the parser) and `MAX_CALL_DEPTH` (frames, here)
/// were set independently and multiplied at run time: 63 frames each carrying a
/// 60-term expression descends 3904 levels with both respected. One budget for
/// both makes that impossible, and it is *counted* rather than measured against
/// the stack -- which matters because the pipeline does not always get a stack
/// of its own. `flatten` hands native work to a 2 MB thread; wasm32 has no
/// `std::thread` and runs the same pipeline inline on the host's stack.
///
/// Four measurements set the number:
///
/// * the 51 FormCalc scripts in `fixtures/formcalc` reach depth **8**;
/// * deliberately generous but realistic scripts -- ten chained helpers, twenty
///   nested `if`s, a sixty-term sum -- reach **60**;
/// * evaluation overflows a **1 MiB** stack past depth **1342**, and 1 MiB is
///   the wasm32 default (nothing sets `-zstack-size` anywhere in the tree);
/// * on the 2 MB thread `flatten` spawns, past **2684**.
///
/// 256 therefore sits four times above the deepest realistic script and five
/// times below the cliff on the smallest stack we ship on.
///
/// Those cliffs are for **release** builds, which is what ships. A debug build
/// costs about 17 KB of stack per level against release's ~780 B — 22 times
/// more — so evaluation there overflows a 2 MB thread around depth 122, below
/// this bound. A debug build of the SDK therefore does not get the
/// "refused, not crashed" guarantee on the deepest inputs; sizing the bound for
/// debug instead would put it under the 60 that real scripts already reach.
const MAX_EVAL_DEPTH: usize = 256;

/// Maximum loop iterations before aborting.
const MAX_LOOP_ITERATIONS: u64 = 10_000;

/// The FormCalc interpreter.
pub struct Interpreter {
    env: Env,
    /// Raw pointer to a SOM resolver, set during `exec_with_*`.
    /// SAFETY: only valid for the duration of the surrounding call.
    som_resolver: Option<*mut (dyn SomResolver + 'static)>,
    /// Instruction counter for timeout detection.
    instruction_count: u64,
    /// User-defined function call depth.
    call_depth: usize,

    /// Current depth of the evaluator's recursion, expression nodes and call
    /// frames together.
    eval_depth: usize,

    /// High-water mark of `eval_depth`, for measuring what real scripts need.
    pub deepest_eval: usize,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    /// Create a new interpreter.
    pub fn new() -> Self {
        Self {
            env: Env::new(),
            som_resolver: None,
            instruction_count: 0,
            call_depth: 0,
            eval_depth: 0,
            deepest_eval: 0,
        }
    }

    /// Reset the instruction counter. Called between script passes.
    pub fn reset_counter(&mut self) {
        self.instruction_count = 0;
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

    /// Execute with a DOM context for SOM path resolution.
    pub fn exec_with_dom(&mut self, exprs: &[Expr], ctx: &mut DomContext<'_>) -> Result<Value> {
        self.exec_with_resolver(exprs, ctx)
    }

    /// Execute with a generic SOM resolver.
    pub fn exec_with_resolver(
        &mut self,
        exprs: &[Expr],
        resolver: &mut dyn SomResolver,
    ) -> Result<Value> {
        // SAFETY: The raw pointer is only used for the duration of this call
        // and is cleared before returning. The transmute extends the lifetime
        // of the trait object to 'static for storage, but we guarantee it
        // never outlives the borrow.
        let ptr: *mut dyn SomResolver = resolver;
        self.som_resolver = Some(unsafe {
            std::mem::transmute::<*mut dyn SomResolver, *mut (dyn SomResolver + 'static)>(ptr)
        });
        let result = self.exec(exprs);
        self.som_resolver = None;
        result
    }

    /// Evaluate an expression and return its value.
    pub fn eval(&mut self, expr: &Expr) -> Result<Value> {
        self.instruction_count += 1;
        if self.instruction_count > MAX_INSTRUCTIONS {
            return Err(FormCalcError::RuntimeError(
                "instruction limit exceeded (possible infinite loop)".to_string(),
            ));
        }
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
        // One budget for every recursive step the evaluator takes: an
        // expression node and a user-defined call frame cost the same one
        // level. Two separate counters (MAX_DEPTH on expressions,
        // MAX_CALL_DEPTH on frames) each stayed inside its own limit while
        // multiplying at run time -- 63 frames each carrying a 60-term
        // expression descends ~3800 levels with both bounds respected. Counted,
        // never measured against the stack, so it holds wherever the pipeline
        // runs: `flatten` gives native a 2 MB thread of its own, and wasm32 has
        // no thread at all and runs on the host's stack.
        if self.eval_depth >= MAX_EVAL_DEPTH {
            return Err(FormCalcError::EvalDepthExceeded {
                max_depth: MAX_EVAL_DEPTH,
            });
        }
        self.eval_depth += 1;
        if self.eval_depth > self.deepest_eval {
            self.deepest_eval = self.eval_depth;
        }
        let out = self.eval_signal_inner(expr);
        self.eval_depth -= 1;
        out
    }

    fn eval_signal_inner(&mut self, expr: &Expr) -> Result<Signal> {
        match expr {
            Expr::Number(n) => Ok(Signal::Value(Value::Number(*n))),
            Expr::StringLit(s) => Ok(Signal::Value(Value::String(s.clone()))),
            Expr::Null => Ok(Signal::Value(Value::Null)),

            Expr::Ident(name) => {
                let val = self
                    .env
                    .get(name)
                    .cloned()
                    .or_else(|| self.resolve_som_value(name))
                    .unwrap_or(Value::Null);
                Ok(Signal::Value(val))
            }

            Expr::MemberAccess { object, member } => {
                let path = flatten_som_path(object, member);
                let val = if self.som_resolver.is_some() {
                    self.resolve_som_value(&path).unwrap_or(Value::Null)
                } else {
                    self.env.get(&path).cloned().unwrap_or(Value::Null)
                };
                Ok(Signal::Value(val))
            }

            Expr::IndexAccess { object, index } => {
                let path = flatten_index_path(object, index);
                let val = if self.som_resolver.is_some() {
                    self.resolve_som_value(&path).unwrap_or(Value::Null)
                } else {
                    self.env.get(&path).cloned().unwrap_or(Value::Null)
                };
                Ok(Signal::Value(val))
            }

            Expr::RecursiveDescent { object, member } => {
                let base = expr_to_accessor_path(object).unwrap_or_else(|| "<expr>".to_string());
                let path = format!("{}..{}", base, member);
                let val = if self.som_resolver.is_some() {
                    self.resolve_som_value(&path).unwrap_or(Value::Null)
                } else {
                    self.env.get(&path).cloned().unwrap_or(Value::Null)
                };
                Ok(Signal::Value(val))
            }

            Expr::Negate(inner) => {
                let val = self.eval(inner)?;
                if val.is_null() {
                    Ok(Signal::Value(Value::Null))
                } else {
                    Ok(Signal::Value(Value::Number(-val.to_number())))
                }
            }

            Expr::Positive(inner) => {
                let val = self.eval(inner)?;
                if val.is_null() {
                    Ok(Signal::Value(Value::Null))
                } else {
                    Ok(Signal::Value(Value::Number(val.to_number())))
                }
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
                        if self.env.get(name).is_some()
                            || !self.assign_som_value(name, val.clone())?
                        {
                            self.env.set(name, val.clone());
                        }
                        Ok(Signal::Value(val))
                    }
                    Expr::MemberAccess { object, member } => {
                        let path = flatten_som_path(object, member);
                        if self.som_resolver.is_some() {
                            let _ = self.assign_som_value(&path, val.clone())?;
                        } else {
                            self.env.set(&path, val.clone());
                        }
                        Ok(Signal::Value(val))
                    }
                    Expr::IndexAccess { object, index } => {
                        let path = flatten_index_path(object, index);
                        if self.som_resolver.is_some() {
                            let _ = self.assign_som_value(&path, val.clone())?;
                        } else {
                            self.env.set(&path, val.clone());
                        }
                        Ok(Signal::Value(val))
                    }
                    _ => Err(FormCalcError::RuntimeError(
                        "invalid assignment target".to_string(),
                    )),
                }
            }

            Expr::FuncCall { name, args } => {
                if name.eq_ignore_ascii_case("Exists")
                    && args.len() == 1
                    && expr_to_accessor_path(&args[0]).is_some()
                {
                    return Ok(Signal::Value(self.eval_exists_arg(&args[0])?));
                }
                if name.eq_ignore_ascii_case("HasValue")
                    && args.len() == 1
                    && expr_to_accessor_path(&args[0]).is_some()
                {
                    return Ok(Signal::Value(self.eval_has_value_arg(&args[0])?));
                }

                let mut arg_vals = Vec::with_capacity(args.len());
                for arg in args {
                    arg_vals.push(self.eval(arg)?);
                }

                // Try SOM built-ins first (if a resolver is bound)
                if let Some(resolver) = self.resolver_mut() {
                    if let Some(result) = som_bridge::call_som_builtin(resolver, name, &arg_vals)? {
                        return Ok(Signal::Value(result));
                    }
                }

                // Try built-in
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
                    if self.call_depth >= MAX_CALL_DEPTH {
                        return Err(FormCalcError::CallDepthExceeded {
                            max_depth: MAX_CALL_DEPTH,
                        });
                    }
                    self.env.push_scope();
                    for (param, val) in params.iter().zip(arg_vals) {
                        self.env.declare(param, val);
                    }
                    self.call_depth += 1;
                    let result = self.exec(&body);
                    self.call_depth -= 1;
                    self.env.pop_scope();
                    return Ok(Signal::Value(result?));
                }

                // DOM method calls (dotted names like xfa.host.resetData) silently
                // return Null — these are XFA host methods we don't implement.
                if name.contains('.') {
                    Ok(Signal::Value(Value::Null))
                } else {
                    Err(FormCalcError::UnknownFunction(name.clone()))
                }
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
                Ok(Signal::Value(Value::Number(0.0)))
            }

            Expr::While { condition, body } => {
                let mut result = Value::Number(0.0);
                let mut iterations: u64 = 0;
                loop {
                    if iterations >= MAX_LOOP_ITERATIONS {
                        return Err(FormCalcError::RuntimeError(
                            "while loop iteration limit exceeded".to_string(),
                        ));
                    }
                    if !self.eval(condition)?.to_bool() {
                        break;
                    }
                    iterations += 1;
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
                let mut result = Value::Number(0.0);
                let mut iterations: u64 = 0;

                self.env.push_scope();
                loop {
                    if iterations >= MAX_LOOP_ITERATIONS {
                        self.env.pop_scope();
                        return Err(FormCalcError::RuntimeError(
                            "for loop iteration limit exceeded".to_string(),
                        ));
                    }
                    if *ascending && i > end_val {
                        break;
                    }
                    if !ascending && i < end_val {
                        break;
                    }
                    self.env.declare(var, Value::Number(i));
                    iterations += 1;

                    match self.exec_block(body) {
                        Ok(Signal::Value(v)) => result = v,
                        Ok(Signal::Return(v)) => {
                            self.env.pop_scope();
                            return Ok(Signal::Return(v));
                        }
                        Ok(Signal::Break) => break,
                        Ok(Signal::Continue) => {}
                        Err(e) => {
                            self.env.pop_scope();
                            return Err(e);
                        }
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
                // XFA Spec 3.3 §25.1 "ForeachExpression" (p1073) iterates over
                // an argument list. Keep the legacy comma-split fallback for
                // existing callers until full SOM accessor sets are implemented.
                let items = match list.as_ref() {
                    Expr::FuncCall { name, args } if name == "__foreach_list" => {
                        let mut items = Vec::with_capacity(args.len());
                        for arg in args {
                            items.push(self.eval(arg)?);
                        }
                        items
                    }
                    _ => {
                        let list_val = self.eval(list)?;
                        list_val
                            .to_string_val()
                            .split(',')
                            .map(|s| Value::String(s.trim().to_string()))
                            .collect()
                    }
                };
                let mut result = Value::Number(0.0);

                self.env.push_scope();
                for item in &items {
                    self.env.declare(var, item.clone());
                    match self.exec_block(body) {
                        Ok(Signal::Value(v)) => result = v,
                        Ok(Signal::Return(v)) => {
                            self.env.pop_scope();
                            return Ok(Signal::Return(v));
                        }
                        Ok(Signal::Break) => break,
                        Ok(Signal::Continue) => continue,
                        Err(e) => {
                            self.env.pop_scope();
                            return Err(e);
                        }
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

    fn resolver_mut(&mut self) -> Option<&mut dyn SomResolver> {
        let resolver = self.som_resolver?;
        // SAFETY: pointer is only set by exec_with_resolver and cleared before return.
        Some(unsafe { &mut *resolver })
    }

    fn resolve_som_value(&mut self, path: &str) -> Option<Value> {
        self.resolver_mut()
            .and_then(|resolver| resolver.resolve_path(path).ok().flatten())
    }

    fn assign_som_value(&mut self, path: &str, value: Value) -> Result<bool> {
        let Some(resolver) = self.resolver_mut() else {
            return Ok(false);
        };
        resolver.assign_path(path, value)
    }

    fn eval_exists_arg(&mut self, expr: &Expr) -> Result<Value> {
        let Some(path) = expr_to_accessor_path(expr) else {
            return Ok(Value::Number(0.0));
        };

        if self.env.get(&path).is_some() {
            return Ok(Value::Number(0.0));
        }

        let exists = if let Some(resolver) = self.resolver_mut() {
            resolver.exists_path(&path)?
        } else {
            false
        };
        Ok(Value::Number(if exists { 1.0 } else { 0.0 }))
    }

    fn eval_has_value_arg(&mut self, expr: &Expr) -> Result<Value> {
        if let Some(path) = expr_to_accessor_path(expr) {
            if self.env.get(&path).is_none() {
                if let Some(resolver) = self.resolver_mut() {
                    let value = resolver.resolve_path(&path)?.unwrap_or(Value::Null);
                    return Ok(Value::Number(if value.is_blankish() { 0.0 } else { 1.0 }));
                }
            }
        }

        let value = self.eval(expr)?;
        Ok(Value::Number(if value.is_blankish() { 0.0 } else { 1.0 }))
    }
}

fn flatten_som_path(object: &Expr, member: &str) -> String {
    let mut parts = Vec::new();
    collect_path_parts(object, &mut parts);
    parts.push(member.to_string());
    parts.join(".")
}

fn collect_path_parts(expr: &Expr, parts: &mut Vec<String>) {
    match expr {
        Expr::Ident(name) => parts.push(name.clone()),
        Expr::MemberAccess { object, member } => {
            collect_path_parts(object, parts);
            parts.push(member.clone());
        }
        Expr::IndexAccess { object, index } => {
            collect_path_parts(object, parts);
            if let Some(last) = parts.last_mut() {
                match index {
                    AccessIndex::All => last.push_str("[*]"),
                    AccessIndex::Numeric(n) => {
                        last.push_str(&format!("[{}]", n));
                    }
                }
            }
        }
        Expr::RecursiveDescent { object, member } => {
            collect_path_parts(object, parts);
            if let Some(last) = parts.last_mut() {
                *last = format!("{}..{}", last, member);
            } else {
                parts.push(format!("..{}", member));
            }
        }
        _ => parts.push("<expr>".to_string()),
    }
}

fn flatten_index_path(object: &Expr, index: &AccessIndex) -> String {
    let base = expr_to_accessor_path(object).unwrap_or_else(|| "<expr>".to_string());
    match index {
        AccessIndex::All => format!("{}[*]", base),
        AccessIndex::Numeric(n) => format!("{}[{}]", base, n),
    }
}

fn expr_to_accessor_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::MemberAccess { object, member } => {
            Some(format!("{}.{}", expr_to_accessor_path(object)?, member))
        }
        Expr::IndexAccess { object, index } => {
            let base = expr_to_accessor_path(object)?;
            Some(match index {
                AccessIndex::All => format!("{}[*]", base),
                AccessIndex::Numeric(n) => format!("{}[{}]", base, n),
            })
        }
        Expr::RecursiveDescent { object, member } => {
            Some(format!("{}..{}", expr_to_accessor_path(object)?, member))
        }
        _ => None,
    }
}

fn eval_binop(op: BinOp, left: &Value, right: &Value) -> Result<Value> {
    match op {
        BinOp::Add => {
            if left.is_null() && right.is_null() {
                Ok(Value::Null)
            } else {
                Ok(Value::Number(left.to_number() + right.to_number()))
            }
        }
        BinOp::Sub => {
            if left.is_null() && right.is_null() {
                Ok(Value::Null)
            } else {
                Ok(Value::Number(left.to_number() - right.to_number()))
            }
        }
        BinOp::Mul => {
            if left.is_null() && right.is_null() {
                Ok(Value::Null)
            } else {
                Ok(Value::Number(left.to_number() * right.to_number()))
            }
        }
        BinOp::Div => {
            if left.is_null() && right.is_null() {
                return Ok(Value::Null);
            }
            let r = right.to_number();
            if r == 0.0 {
                Err(FormCalcError::DivisionByZero)
            } else {
                Ok(Value::Number(left.to_number() / r))
            }
        }
        BinOp::Eq => Ok(Value::Number(if both_string(left, right) {
            if matches!((left, right), (Value::String(a), Value::String(b)) if a == b) {
                1.0
            } else {
                0.0
            }
        } else if left.is_null() || right.is_null() {
            if left.is_null() && right.is_null() {
                1.0
            } else {
                0.0
            }
        } else if left.to_number() == right.to_number() {
            1.0
        } else {
            0.0
        })),
        BinOp::Ne => Ok(Value::Number(if both_string(left, right) {
            if matches!((left, right), (Value::String(a), Value::String(b)) if a != b) {
                1.0
            } else {
                0.0
            }
        } else if left.is_null() || right.is_null() {
            if left.is_null() && right.is_null() {
                0.0
            } else {
                1.0
            }
        } else if left.to_number() != right.to_number() {
            1.0
        } else {
            0.0
        })),
        BinOp::Lt => Ok(Value::Number(if compare_relational(op, left, right) {
            1.0
        } else {
            0.0
        })),
        BinOp::Le => Ok(Value::Number(if compare_relational(op, left, right) {
            1.0
        } else {
            0.0
        })),
        BinOp::Gt => Ok(Value::Number(if compare_relational(op, left, right) {
            1.0
        } else {
            0.0
        })),
        BinOp::Ge => Ok(Value::Number(if compare_relational(op, left, right) {
            1.0
        } else {
            0.0
        })),
        BinOp::And => {
            if left.is_null() && right.is_null() {
                Ok(Value::Null)
            } else {
                Ok(Value::Number(if left.to_bool() && right.to_bool() {
                    1.0
                } else {
                    0.0
                }))
            }
        }
        BinOp::Or => {
            if left.is_null() && right.is_null() {
                Ok(Value::Null)
            } else {
                Ok(Value::Number(if left.to_bool() || right.to_bool() {
                    1.0
                } else {
                    0.0
                }))
            }
        }
    }
}

fn both_string(left: &Value, right: &Value) -> bool {
    matches!((left, right), (Value::String(_), Value::String(_)))
}

fn compare_relational(op: BinOp, left: &Value, right: &Value) -> bool {
    if left.is_null() || right.is_null() {
        return matches!(op, BinOp::Le | BinOp::Ge) && left.is_null() && right.is_null();
    }

    if let (Value::String(lhs), Value::String(rhs)) = (left, right) {
        return match op {
            BinOp::Lt => lhs < rhs,
            BinOp::Le => lhs <= rhs,
            BinOp::Gt => lhs > rhs,
            BinOp::Ge => lhs >= rhs,
            _ => false,
        };
    }

    let lhs = left.to_number();
    let rhs = right.to_number();
    match op {
        BinOp::Lt => lhs < rhs,
        BinOp::Le => lhs <= rhs,
        BinOp::Gt => lhs > rhs,
        BinOp::Ge => lhs >= rhs,
        _ => false,
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

    fn run_result(src: &str) -> crate::error::Result<Value> {
        let tokens = crate::lexer::tokenize(src).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        let mut interp = Interpreter::new();
        interp.exec(&ast)
    }

    #[test]
    fn arithmetic() {
        assert_eq!(run("1 + 2 * 3"), Value::Number(7.0));
        assert_eq!(run("(1 + 2) * 3"), Value::Number(9.0));
        assert_eq!(run("10 - 3 - 2"), Value::Number(5.0));
        assert_eq!(run("10 / 2"), Value::Number(5.0));
    }

    #[test]
    fn concat_builtin() {
        assert_eq!(
            run(r#"Concat("hello", " ", "world")"#),
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

    /// The two bounds used to multiply, and this is the shape that did it.
    ///
    /// 63 nested user functions, each body carrying a 60-term expression: both
    /// `MAX_CALL_DEPTH` (63 < 64) and the parser's expression bound (60 < 64)
    /// are respected, and evaluation still descended 3904 levels. Measured, that
    /// overflows a 1 MiB stack past 1342 — and 1 MiB is the wasm32 default,
    /// where `flatten` runs inline with no thread of its own.
    #[test]
    fn frames_and_expressions_share_one_budget() {
        let chain = vec!["1"; 60].join(" + ");
        let mut src = format!("func f0()\n  {chain}\nendfunc\n");
        for k in 1..=63 {
            src.push_str(&format!("func f{k}()\n  f{}() + {chain}\nendfunc\n", k - 1));
        }
        src.push_str("f63()\n");

        // Run on a thread with room to spare, because reaching the bound means
        // descending to it: a debug build costs ~17 KB of stack per level
        // against release's ~780 B, so a default 2 MB test thread overflows
        // around depth 122 -- before the budget at 256 can refuse anything. The
        // bound is sized for the shipped configuration (release, and wasm32's
        // 1 MiB default, where evaluation overflows past 1342); this stack is
        // the test's own need, not the guard's.
        let refused = std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                matches!(
                    run_result(&src),
                    Err(FormCalcError::EvalDepthExceeded { .. })
                )
            })
            .expect("spawn")
            .join()
            .expect("the budget must refuse this, not overflow the stack");
        assert!(
            refused,
            "63 frames x 60 terms must be refused, not descend 3904 levels"
        );
    }

    /// The other failure direction: a budget low enough to break real forms.
    ///
    /// The 51 scripts in `fixtures/formcalc` reach depth 8; these are
    /// deliberately more generous than any of them and must still run.
    #[test]
    fn ordinary_scripts_fit_the_budget() {
        // ten chained helpers
        let mut helpers = String::from("func h0()\n  1\nendfunc\n");
        for k in 1..10 {
            helpers.push_str(&format!("func h{k}()\n  h{}() + 1\nendfunc\n", k - 1));
        }
        helpers.push_str("h9()\n");
        assert_eq!(run(&helpers), Value::Number(10.0));

        // a sixty-term sum
        assert_eq!(run(&vec!["1"; 60].join(" + ")), Value::Number(60.0));

        // twenty nested ifs
        let mut nested = String::from("var x = 1\nx");
        for _ in 0..20 {
            nested = format!("if (1 > 0) then\n{nested}\nendif");
        }
        run_result(&nested).expect("twenty nested ifs must still evaluate");

        // a hundred flat statements: breadth, not depth
        let flat = (0..100)
            .map(|i| format!("var v{i} = {i} + 1"))
            .collect::<Vec<_>>()
            .join("\n");
        run_result(&flat).expect("a hundred statements must still evaluate");
    }

    /// The shared budget must not quietly replace the frame bound.
    ///
    /// Plain recursion costs only a few levels per frame, so `MAX_CALL_DEPTH`
    /// is still what stops it, and its error is the one that names the cause.
    #[test]
    fn plain_recursion_is_still_caught_by_the_frame_bound() {
        assert!(matches!(
            run_result("func f()\n  f()\nendfunc\nf()"),
            Err(FormCalcError::CallDepthExceeded { .. })
        ));
    }

    #[test]
    fn recursion_direct_hits_limit() {
        // func f() calls itself immediately -- must error, not crash
        let result = run_result("func f()\n  f()\nendfunc\nf()");
        assert!(
            matches!(result, Err(FormCalcError::CallDepthExceeded { .. })),
            "expected CallDepthExceeded, got: {:?}",
            result
        );
    }

    #[test]
    fn recursion_mutual_hits_limit() {
        // f() -> g() -> f() -> ... must error, not crash
        let result = run_result("func f()\n  g()\nendfunc\nfunc g()\n  f()\nendfunc\nf()");
        assert!(
            matches!(result, Err(FormCalcError::CallDepthExceeded { .. })),
            "expected CallDepthExceeded, got: {:?}",
            result
        );
    }

    #[test]
    fn recursion_depth_error_is_recoverable() {
        // After a recursion-limit error, the same Interpreter still evaluates normally.
        let mut interp = Interpreter::new();
        let tokens = crate::lexer::tokenize("func f()\n  f()\nendfunc\nf()").unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        let _ = interp.exec(&ast);
        assert_eq!(interp.call_depth, 0);
        assert_eq!(
            interp.env.scopes.len(),
            1,
            "scope stack must be clean after direct recursion error"
        );

        let tokens2 = crate::lexer::tokenize("1 + 1").unwrap();
        let ast2 = crate::parser::parse(tokens2).unwrap();
        assert_eq!(interp.exec(&ast2).unwrap(), Value::Number(2.0));
    }

    #[test]
    fn recursion_inside_for_loop_is_recoverable() {
        // Recursion inside a for-loop body must not leak loop or param scopes.
        // Without the fix, each recursion level leaks one function-param scope
        // because the for-loop `?` propagation skips `pop_scope`.
        //
        // Spawned with a large stack: for-loop recursion uses ~5 Rust frames per
        // FormCalc level (eval_signal → exec → eval_signal(For) → exec_block →
        // eval_signal(FuncCall)), vs ~3 for direct recursion. At MAX_CALL_DEPTH=64
        // that is ~320 frames; the extra stack keeps the guard firing before any
        // native overflow in debug builds.
        let handle = std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let mut interp = Interpreter::new();
                let script =
                    "func f(x)\n  for i = 0 upto 1 do\n    f(x + 1)\n  endfor\nendfunc\nf(0)";
                let tokens = crate::lexer::tokenize(script).unwrap();
                let ast = crate::parser::parse(tokens).unwrap();
                let result = interp.exec(&ast);
                assert!(
                    matches!(result, Err(FormCalcError::CallDepthExceeded { .. })),
                    "expected CallDepthExceeded, got: {:?}",
                    result
                );
                assert_eq!(interp.call_depth, 0);
                assert_eq!(
                    interp.env.scopes.len(),
                    1,
                    "scope stack must be clean after for-loop recursion error"
                );
                let tokens2 = crate::lexer::tokenize("1 + 1").unwrap();
                let ast2 = crate::parser::parse(tokens2).unwrap();
                assert_eq!(interp.exec(&ast2).unwrap(), Value::Number(2.0));
            })
            .expect("thread spawn failed");
        handle
            .join()
            .expect("for-loop recursion recovery test failed");
    }

    #[test]
    fn nested_calls_within_limit_still_work() {
        // Normal nested calls (well under MAX_CALL_DEPTH) must continue to work.
        assert_eq!(
            run("func add(a, b)\n  a + b\nendfunc\nadd(add(1, 2), add(3, 4))"),
            Value::Number(10.0)
        );
    }

    #[test]
    fn user_function_basic_still_works() {
        // The simplest user function must still evaluate.
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
