//! FormCalc interpreter — tree-walking evaluator for the FormCalc AST.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;

use crate::ast::{AccessIndex, BinOp, Expr};
use crate::budget::{StackBudget, STACK_BUDGET_BYTES};
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
/// of its own. `flatten` hands native work to a 32 MB thread; wasm32 has no
/// `std::thread` and runs the same pipeline inline on the host's stack.
///
/// The measurements behind the number:
///
/// * the 51 FormCalc scripts in `fixtures/formcalc` reach depth **8**;
/// * deliberately generous but realistic scripts -- ten chained helpers, twenty
///   nested `if`s, a sixty-term sum -- reach **60**;
/// * a level costs about **440 bytes** of stack in a release build and about
///   **1.7 KB** in a debug build, now that the evaluator's arms each have a
///   frame of their own (one function holding every arm cost 780 bytes and
///   17 KB respectively), so 256 levels spend 113 KB or 445 KB.
///
/// 256 therefore sits four times above the deepest realistic script, and the
/// stack it spends fits inside `budget::STACK_BUDGET_BYTES` in both profiles.
/// A count only protects the stack while a level costs what it cost when the
/// count was chosen; `Interpreter::enter` also *measures* the stack spent and
/// refuses at the byte budget, so the guarantee does not rest on this number
/// alone.
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

    /// Bytes of stack the evaluator may consume below the frame that entered
    /// it. `budget::STACK_BUDGET_BYTES` unless a host that knows its own stack
    /// set otherwise with [`Interpreter::with_stack_budget`].
    stack_limit: usize,

    /// The budget for the evaluation in progress, started when the outermost
    /// `eval_signal` is entered. `None` between evaluations.
    stack: Option<StackBudget>,

    /// High-water mark of stack consumed, in bytes, for measuring what a level
    /// costs in this build.
    pub deepest_stack: usize,
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
            stack_limit: STACK_BUDGET_BYTES,
            stack: None,
            deepest_stack: 0,
        }
    }

    /// Evaluate with `bytes` of stack instead of `STACK_BUDGET_BYTES`.
    ///
    /// For a host that knows how much stack it is standing on. The default is
    /// sized for the smallest stack the pipeline ships on; a host with less
    /// must say so, and one with more gains nothing by saying so, because
    /// `MAX_EVAL_DEPTH` refuses first on every stack the default fits.
    pub fn with_stack_budget(mut self, bytes: usize) -> Self {
        self.stack_limit = bytes;
        self
    }

    /// Reset the instruction counter. Called between script passes.
    pub fn reset_counter(&mut self) {
        self.instruction_count = 0;
    }

    /// Execute a list of expressions (a script) and return the last value.
    pub fn exec(&mut self, exprs: &[Expr]) -> Result<Value> {
        // Live once per user-function frame; see `eval` for why the work
        // around the recursive call is elsewhere.
        let mut result = Value::Null;
        for expr in exprs {
            match self.eval_signal(expr) {
                Ok(Signal::Value(v)) => result = v,
                Ok(Signal::Return(v)) => return Ok(v),
                Ok(stray) => return Err(stray_signal(stray)),
                Err(e) => return Err(e),
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
    // Not `?`: at opt-level 0 its `ControlFlow` and residual temporaries stay
    // in a frame that is live once per level; see `eval_binary`.
    #[allow(clippy::question_mark)]
    pub fn eval(&mut self, expr: &Expr) -> Result<Value> {
        // This frame is live once per level of every expression chain, so the
        // work around the recursive call is kept in helpers that are not.
        if let Err(e) = self.tick() {
            return Err(e);
        }
        match self.eval_signal(expr) {
            Ok(signal) => value_of(signal),
            Err(e) => Err(e),
        }
    }

    /// Count one instruction against `MAX_INSTRUCTIONS`.
    #[inline(never)]
    fn tick(&mut self) -> Result<()> {
        self.instruction_count += 1;
        if self.instruction_count > MAX_INSTRUCTIONS {
            return Err(FormCalcError::RuntimeError(
                "instruction limit exceeded (possible infinite loop)".to_string(),
            ));
        }
        Ok(())
    }

    // Not `?`, for the frame's sake; see `eval_binary`.
    #[allow(clippy::question_mark)]
    fn eval_signal(&mut self, expr: &Expr) -> Result<Signal> {
        // One budget for every recursive step the evaluator takes: an
        // expression node and a user-defined call frame cost the same one
        // level. Two separate counters (MAX_DEPTH on expressions,
        // MAX_CALL_DEPTH on frames) each stayed inside its own limit while
        // multiplying at run time -- 63 frames each carrying a 60-term
        // expression descends ~3800 levels with both bounds respected. Counted,
        // never measured against the stack, so it holds wherever the pipeline
        // runs: `flatten` gives native a 32 MB thread of its own, and wasm32
        // has no thread at all and runs on the host's stack.
        //
        // The count assumes a level costs what it cost when the number was
        // chosen. `enter` also measures what a level costs *now*, in this
        // build, and refuses when the levels so far have consumed
        // `STACK_BUDGET_BYTES` -- so a profile, a compiler, or a future arm
        // that spends more stack per level becomes a refusal, not an overflow.
        //
        // This frame is live once per level; the checks live in `enter` and
        // `leave`, whose frames are not.
        if let Err(refused) = self.enter() {
            return Err(refused);
        }
        let out = self.eval_signal_inner(expr);
        self.leave();
        out
    }

    /// Charge one level against both budgets, or refuse.
    #[inline(never)]
    fn enter(&mut self) -> Result<()> {
        if self.eval_depth >= MAX_EVAL_DEPTH {
            return Err(FormCalcError::EvalDepthExceeded {
                max_depth: MAX_EVAL_DEPTH,
            });
        }
        let budget = match self.stack {
            Some(budget) if self.eval_depth > 0 => budget,
            _ => {
                let budget = StackBudget::start(self.stack_limit);
                self.stack = Some(budget);
                budget
            }
        };
        let used = budget.used();
        if used > self.deepest_stack {
            self.deepest_stack = used;
        }
        if used > budget.limit() {
            return Err(FormCalcError::StackBudgetExceeded {
                max_bytes: budget.limit(),
            });
        }
        self.eval_depth += 1;
        if self.eval_depth > self.deepest_eval {
            self.deepest_eval = self.eval_depth;
        }
        Ok(())
    }

    /// Give one level back; close the stack budget when the last one is.
    #[inline(never)]
    fn leave(&mut self) {
        self.eval_depth -= 1;
        if self.eval_depth == 0 {
            self.stack = None;
        }
    }

    /// Dispatch only, and only the forms that make up expression chains.
    ///
    /// This frame sits on the stack once per level of recursion, so it has to
    /// be small, and at opt-level 0 a frame is as large as the temporaries of
    /// *all* its arms together. Every arm that does work lives in its own
    /// `#[inline(never)]` method, and the statement forms -- which nest a few
    /// deep at most -- are one more call away in `eval_statement`, so that a
    /// `1 + 1 + 1 ...` chain pays for ten arms a level and not twenty-five.
    ///
    /// As one function holding every arm, a debug build spent about 17 KB of
    /// stack per level (release: ~780 bytes), which put `MAX_EVAL_DEPTH` far
    /// past the 1 MiB the pipeline has on wasm32: the count refused nothing,
    /// because the stack ran out first. `budget::StackBudget` is the guard
    /// against that; keeping frames small is what keeps the guard from firing
    /// on ordinary scripts.
    fn eval_signal_inner(&mut self, expr: &Expr) -> Result<Signal> {
        match expr {
            Expr::Number(n) => Ok(Signal::Value(Value::Number(*n))),
            Expr::StringLit(s) => Ok(Signal::Value(Value::String(s.clone()))),
            Expr::Null => Ok(Signal::Value(Value::Null)),
            Expr::Ident(name) => self.eval_ident(name),
            Expr::Negate(inner) => self.eval_negate(inner),
            Expr::Positive(inner) => self.eval_positive(inner),
            Expr::Not(inner) => self.eval_not(inner),
            Expr::BinaryOp { op, left, right } => self.eval_binary(*op, left, right),
            Expr::Concat(left, right) => self.eval_concat(left, right),
            Expr::FuncCall { name, args } => self.eval_func_call(name, args),
            other => self.eval_statement(other),
        }
    }

    /// The forms `eval_signal_inner` does not dispatch itself: accessors,
    /// assignment, control flow and declarations. Exhaustive, so a new `Expr`
    /// variant lands here by construction and is dispatched from here even if
    /// it is never added to the fast path above.
    #[inline(never)]
    fn eval_statement(&mut self, expr: &Expr) -> Result<Signal> {
        match expr {
            Expr::Number(n) => Ok(Signal::Value(Value::Number(*n))),
            Expr::StringLit(s) => Ok(Signal::Value(Value::String(s.clone()))),
            Expr::Null => Ok(Signal::Value(Value::Null)),
            Expr::Ident(name) => self.eval_ident(name),
            Expr::MemberAccess { object, member } => self.eval_member_access(object, member),
            Expr::IndexAccess { object, index } => self.eval_index_access(object, index),
            Expr::RecursiveDescent { object, member } => {
                self.eval_recursive_descent(object, member)
            }
            Expr::Negate(inner) => self.eval_negate(inner),
            Expr::Positive(inner) => self.eval_positive(inner),
            Expr::Not(inner) => self.eval_not(inner),
            Expr::BinaryOp { op, left, right } => self.eval_binary(*op, left, right),
            Expr::Concat(left, right) => self.eval_concat(left, right),
            Expr::Assign { target, value } => self.eval_assign(target, value),
            Expr::FuncCall { name, args } => self.eval_func_call(name, args),
            Expr::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => self.eval_if(condition, then_body, elseif_clauses, else_body.as_deref()),
            Expr::While { condition, body } => self.eval_while(condition, body),
            Expr::For {
                var,
                start,
                end,
                step,
                ascending,
                body,
            } => self.eval_for(var, start, end, step.as_deref(), *ascending, body),
            Expr::Foreach { var, list, body } => self.eval_foreach(var, list, body),
            Expr::FuncDecl { name, params, body } => self.eval_func_decl(name, params, body),
            Expr::VarDecl { name, init } => self.eval_var_decl(name, init.as_deref()),
            Expr::Return(expr) => self.eval_return(expr.as_deref()),
            Expr::Break => Ok(Signal::Break),
            Expr::Continue => Ok(Signal::Continue),
        }
    }

    #[inline(never)]
    fn eval_ident(&mut self, name: &str) -> Result<Signal> {
        let val = self
            .env
            .get(name)
            .cloned()
            .or_else(|| self.resolve_som_value(name))
            .unwrap_or(Value::Null);
        Ok(Signal::Value(val))
    }

    /// Read `path` through the resolver when one is bound, else from the
    /// environment. Shared by the three accessor forms.
    #[inline(never)]
    fn read_path(&mut self, path: &str) -> Signal {
        let val = if self.som_resolver.is_some() {
            self.resolve_som_value(path).unwrap_or(Value::Null)
        } else {
            self.env.get(path).cloned().unwrap_or(Value::Null)
        };
        Signal::Value(val)
    }

    #[inline(never)]
    fn eval_member_access(&mut self, object: &Expr, member: &str) -> Result<Signal> {
        let path = flatten_som_path(object, member);
        Ok(self.read_path(&path))
    }

    #[inline(never)]
    fn eval_index_access(&mut self, object: &Expr, index: &AccessIndex) -> Result<Signal> {
        let path = flatten_index_path(object, index);
        Ok(self.read_path(&path))
    }

    #[inline(never)]
    fn eval_recursive_descent(&mut self, object: &Expr, member: &str) -> Result<Signal> {
        let base = expr_to_accessor_path(object).unwrap_or_else(|| "<expr>".to_string());
        let path = format!("{}..{}", base, member);
        Ok(self.read_path(&path))
    }

    #[inline(never)]
    fn eval_negate(&mut self, inner: &Expr) -> Result<Signal> {
        let val = self.eval(inner)?;
        if val.is_null() {
            Ok(Signal::Value(Value::Null))
        } else {
            Ok(Signal::Value(Value::Number(-val.to_number())))
        }
    }

    #[inline(never)]
    fn eval_positive(&mut self, inner: &Expr) -> Result<Signal> {
        let val = self.eval(inner)?;
        if val.is_null() {
            Ok(Signal::Value(Value::Null))
        } else {
            Ok(Signal::Value(Value::Number(val.to_number())))
        }
    }

    #[inline(never)]
    fn eval_not(&mut self, inner: &Expr) -> Result<Signal> {
        let val = self.eval(inner)?;
        Ok(Signal::Value(Value::Number(if val.to_bool() {
            0.0
        } else {
            1.0
        })))
    }

    /// Written without `?`: at opt-level 0 each `?` keeps some 250 bytes of
    /// `ControlFlow` and residual temporaries in the frame, and this frame is
    /// live once per level of every operator chain.
    #[inline(never)]
    #[allow(clippy::question_mark)]
    fn eval_binary(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Result<Signal> {
        let lval = match self.eval(left) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let rval = match self.eval(right) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        eval_binop(op, &lval, &rval).map(Signal::Value)
    }

    #[inline(never)]
    fn eval_concat(&mut self, left: &Expr, right: &Expr) -> Result<Signal> {
        let l = self.eval(left)?;
        let r = self.eval(right)?;
        Ok(Signal::Value(Value::String(
            l.to_string_val() + &r.to_string_val(),
        )))
    }

    #[inline(never)]
    fn eval_assign(&mut self, target: &Expr, value: &Expr) -> Result<Signal> {
        let val = self.eval(value)?;
        match target {
            Expr::Ident(name) => {
                if self.env.get(name).is_some() || !self.assign_som_value(name, val.clone())? {
                    self.env.set(name, val.clone());
                }
                Ok(Signal::Value(val))
            }
            Expr::MemberAccess { object, member } => {
                let path = flatten_som_path(object, member);
                self.write_path(&path, val.clone())?;
                Ok(Signal::Value(val))
            }
            Expr::IndexAccess { object, index } => {
                let path = flatten_index_path(object, index);
                self.write_path(&path, val.clone())?;
                Ok(Signal::Value(val))
            }
            _ => Err(FormCalcError::RuntimeError(
                "invalid assignment target".to_string(),
            )),
        }
    }

    /// Write `path` through the resolver when one is bound, else into the
    /// environment. The resolver's "was it assigned" answer is deliberately
    /// dropped here, as it always was for accessor targets.
    #[inline(never)]
    fn write_path(&mut self, path: &str, val: Value) -> Result<()> {
        if self.som_resolver.is_some() {
            let _ = self.assign_som_value(path, val)?;
        } else {
            self.env.set(path, val);
        }
        Ok(())
    }

    /// Evaluate the arguments, then hand off. Split three ways because the
    /// frames below stay live while a user function's body runs, and the
    /// single function this was spent 3.3 KB of stack per call in a debug
    /// build -- more than the 51 fixtures' whole evaluation.
    #[inline(never)]
    #[allow(clippy::question_mark)]
    fn eval_func_call(&mut self, name: &str, args: &[Expr]) -> Result<Signal> {
        if let Some(answer) = self.accessor_predicate(name, args) {
            return answer.map(Signal::Value);
        }
        let mut arg_vals = Vec::with_capacity(args.len());
        for arg in args {
            match self.eval(arg) {
                Ok(v) => arg_vals.push(v),
                Err(e) => return Err(e),
            }
        }
        self.invoke(name, arg_vals)
    }

    /// `Exists(path)` and `HasValue(path)` look at the accessor, not its value.
    #[inline(never)]
    fn accessor_predicate(&mut self, name: &str, args: &[Expr]) -> Option<Result<Value>> {
        if args.len() != 1 || expr_to_accessor_path(&args[0]).is_none() {
            return None;
        }
        if name.eq_ignore_ascii_case("Exists") {
            Some(self.eval_exists_arg(&args[0]))
        } else if name.eq_ignore_ascii_case("HasValue") {
            Some(self.eval_has_value_arg(&args[0]))
        } else {
            None
        }
    }

    /// SOM built-ins first when a resolver is bound, then the language's own,
    /// then a user-defined function, then the host-method fallback.
    #[inline(never)]
    fn invoke(&mut self, name: &str, arg_vals: Vec<Value>) -> Result<Signal> {
        match self.call_builtin(name, &arg_vals) {
            Ok(Some(result)) => return Ok(Signal::Value(result)),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        if let Some((params, body)) = self.env.functions.get(name).cloned() {
            return self.call_user_function(name, &params, &body, arg_vals);
        }
        // DOM method calls (dotted names like xfa.host.resetData) silently
        // return Null — these are XFA host methods we don't implement.
        if name.contains('.') {
            Ok(Signal::Value(Value::Null))
        } else {
            Err(FormCalcError::UnknownFunction(name.to_string()))
        }
    }

    /// A SOM built-in if a resolver is bound and knows the name, else a
    /// language built-in; `None` when neither claims it.
    #[inline(never)]
    fn call_builtin(&mut self, name: &str, arg_vals: &[Value]) -> Result<Option<Value>> {
        if let Some(resolver) = self.resolver_mut() {
            if let Some(result) = som_bridge::call_som_builtin(resolver, name, arg_vals)? {
                return Ok(Some(result));
            }
        }
        builtins::call_builtin(name, arg_vals)
    }

    #[inline(never)]
    fn call_user_function(
        &mut self,
        name: &str,
        params: &[String],
        body: &[Expr],
        arg_vals: Vec<Value>,
    ) -> Result<Signal> {
        if params.len() != arg_vals.len() {
            return Err(FormCalcError::ArityError {
                name: name.to_string(),
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
        let result = self.exec(body);
        self.call_depth -= 1;
        self.env.pop_scope();
        result.map(Signal::Value)
    }

    #[inline(never)]
    fn eval_if(
        &mut self,
        condition: &Expr,
        then_body: &[Expr],
        elseif_clauses: &[(Expr, Vec<Expr>)],
        else_body: Option<&[Expr]>,
    ) -> Result<Signal> {
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

    #[inline(never)]
    fn eval_while(&mut self, condition: &Expr, body: &[Expr]) -> Result<Signal> {
        let mut result = Value::Number(0.0);
        let mut iterations: u64 = 0;
        loop {
            if iterations >= MAX_LOOP_ITERATIONS {
                return Err(loop_limit("while"));
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

    /// The bounds are evaluated in a helper whose frame is gone before the
    /// body runs: this frame stays live under every iteration, and under a
    /// user function that recurses from inside the loop.
    #[inline(never)]
    #[allow(clippy::question_mark)]
    fn eval_for(
        &mut self,
        var: &str,
        start: &Expr,
        end: &Expr,
        step: Option<&Expr>,
        ascending: bool,
        body: &[Expr],
    ) -> Result<Signal> {
        let (start_val, end_val, step_val) = match self.for_bounds(start, end, step) {
            Ok(bounds) => bounds,
            Err(e) => return Err(e),
        };

        let mut i = start_val;
        let mut result = Value::Number(0.0);
        let mut iterations: u64 = 0;

        self.env.push_scope();
        loop {
            if iterations >= MAX_LOOP_ITERATIONS {
                self.env.pop_scope();
                return Err(loop_limit("for"));
            }
            if ascending && i > end_val {
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

            if ascending {
                i += step_val;
            } else {
                i -= step_val;
            }
        }
        self.env.pop_scope();
        Ok(Signal::Value(result))
    }

    /// `start`, `end` and `step` of a `for`, as numbers; `step` defaults to 1.
    #[inline(never)]
    fn for_bounds(
        &mut self,
        start: &Expr,
        end: &Expr,
        step: Option<&Expr>,
    ) -> Result<(f64, f64, f64)> {
        let start_val = self.eval(start)?.to_number();
        let end_val = self.eval(end)?.to_number();
        let step_val = match step {
            Some(s) => self.eval(s)?.to_number(),
            None => 1.0,
        };
        Ok((start_val, end_val, step_val))
    }

    #[inline(never)]
    fn eval_foreach(&mut self, var: &str, list: &Expr, body: &[Expr]) -> Result<Signal> {
        // XFA Spec 3.3 §25.1 "ForeachExpression" (p1073) iterates over
        // an argument list. Keep the legacy comma-split fallback for
        // existing callers until full SOM accessor sets are implemented.
        let items = match list {
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

    #[inline(never)]
    fn eval_func_decl(&mut self, name: &str, params: &[String], body: &[Expr]) -> Result<Signal> {
        self.env
            .functions
            .insert(name.to_string(), (params.to_vec(), body.to_vec()));
        Ok(Signal::Value(Value::Null))
    }

    #[inline(never)]
    fn eval_var_decl(&mut self, name: &str, init: Option<&Expr>) -> Result<Signal> {
        let val = match init {
            Some(expr) => self.eval(expr)?,
            None => Value::Null,
        };
        self.env.declare(name, val.clone());
        Ok(Signal::Value(val))
    }

    #[inline(never)]
    fn eval_return(&mut self, expr: Option<&Expr>) -> Result<Signal> {
        let val = match expr {
            Some(e) => self.eval(e)?,
            None => Value::Null,
        };
        Ok(Signal::Return(val))
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

/// The value an expression produced, or the error for a signal that has no
/// business outside a loop.
#[inline(never)]
fn value_of(signal: Signal) -> Result<Value> {
    match signal {
        Signal::Value(v) | Signal::Return(v) => Ok(v),
        stray => Err(stray_signal(stray)),
    }
}

/// A loop that ran `MAX_LOOP_ITERATIONS` times.
#[cold]
#[inline(never)]
fn loop_limit(kind: &str) -> FormCalcError {
    FormCalcError::RuntimeError(format!("{kind} loop iteration limit exceeded"))
}

/// `break` or `continue` where no loop is.
#[cold]
#[inline(never)]
fn stray_signal(signal: Signal) -> FormCalcError {
    let which = match signal {
        Signal::Break => "break",
        Signal::Continue => "continue",
        Signal::Value(_) | Signal::Return(_) => "signal",
    };
    FormCalcError::RuntimeError(format!("{which} outside of loop"))
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

    /// 63 nested user functions, each body carrying a `terms`-term expression.
    ///
    /// Both values are legal on their own -- 63 is under `MAX_CALL_DEPTH`, 60
    /// is under the parser's expression bound -- and before the shared budget
    /// evaluating them descended 3904 levels.
    fn hostile_call_chain(frames: usize, terms: usize) -> String {
        let chain = vec!["1"; terms].join(" + ");
        let mut src = format!("func f0()\n  {chain}\nendfunc\n");
        for k in 1..=frames {
            src.push_str(&format!("func f{k}()\n  f{}() + {chain}\nendfunc\n", k - 1));
        }
        src.push_str(&format!("f{frames}()\n"));
        src
    }

    /// Run `f` on a thread with exactly `bytes` of stack, and fail the test
    /// if the thread does not come back -- which, for a stack overflow, it
    /// does not: the process aborts.
    fn on_a_stack_of<T: Send + 'static>(bytes: usize, f: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::Builder::new()
            .stack_size(bytes)
            .spawn(f)
            .expect("spawn")
            .join()
            .expect("the thread must return: an overflow aborts the process instead")
    }

    /// The two bounds used to multiply, and this is the shape that did it.
    ///
    /// 63 nested user functions, each body carrying a 60-term expression: both
    /// `MAX_CALL_DEPTH` (63 < 64) and the parser's expression bound (60 < 64)
    /// are respected, and evaluation still descended 3904 levels.
    ///
    /// Runs on a **1 MiB** thread, the wasm32 default and the smallest stack
    /// the pipeline ships on (`flatten` runs inline there, with no thread of
    /// its own). Either budget may be the one that refuses: a level costs
    /// ~440 bytes in release and ~1.7 KB in debug, so `MAX_EVAL_DEPTH` is
    /// reached first in both today, and the stack budget takes over the moment
    /// a level grows past `STACK_BUDGET_BYTES / MAX_EVAL_DEPTH`. Before the
    /// stack budget existed this test needed a 32 MB thread to pass in debug
    /// -- a level cost 17 KB then -- which is to say the guarantee did not
    /// hold in debug. Take both budgets out and this thread overflows.
    #[test]
    fn frames_and_expressions_share_one_budget() {
        let src = hostile_call_chain(63, 60);
        let result = on_a_stack_of(1 << 20, move || run_result(&src));
        assert!(
            matches!(
                result,
                Err(FormCalcError::EvalDepthExceeded { .. })
                    | Err(FormCalcError::StackBudgetExceeded { .. })
            ),
            "63 frames x 60 terms must be refused by a budget, got {result:?}"
        );
    }

    /// The count is a bound of its own, not a restatement of the stack budget.
    ///
    /// With stack to spare -- a 32 MB thread and an 8 MiB budget -- nothing but
    /// the count can stop the hostile form, and it must, at exactly
    /// `MAX_EVAL_DEPTH`. Take the count out and this arrives as a stack-budget
    /// refusal instead, five thousand levels deeper than any script needs.
    #[test]
    fn the_count_refuses_on_its_own_when_stack_is_not_short() {
        let src = hostile_call_chain(63, 60);
        let (result, deepest) = on_a_stack_of(32 << 20, move || {
            let tokens = crate::lexer::tokenize(&src).unwrap();
            let ast = crate::parser::parse(tokens).unwrap();
            let mut interp = Interpreter::new().with_stack_budget(8 << 20);
            let result = interp.exec(&ast);
            (result, interp.deepest_eval)
        });
        assert!(
            matches!(result, Err(FormCalcError::EvalDepthExceeded { max_depth }) if max_depth == MAX_EVAL_DEPTH),
            "expected the count to refuse, got {result:?}"
        );
        assert_eq!(deepest, MAX_EVAL_DEPTH, "the count must stop at its bound");
    }

    /// The stack budget is its own bound, not a restatement of the count.
    ///
    /// A host on a smaller stack says so with `with_stack_budget`, and the
    /// refusal then arrives from the bytes before the count could reach 256 --
    /// which is the mechanism that keeps a build whose levels cost more than
    /// the count assumed from overflowing. Refused within one level of the
    /// budget, and the interpreter is clean and usable afterwards.
    #[test]
    fn the_stack_budget_refuses_on_its_own_before_the_count() {
        const SMALL: usize = 64 * 1024;
        let src = hostile_call_chain(63, 60);
        let tokens = crate::lexer::tokenize(&src).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        let mut interp = Interpreter::new().with_stack_budget(SMALL);
        let result = interp.exec(&ast);
        assert!(
            matches!(
                result,
                Err(FormCalcError::StackBudgetExceeded { max_bytes }) if max_bytes == SMALL
            ),
            "expected the stack budget to refuse, got {result:?}"
        );
        assert!(
            interp.deepest_eval < MAX_EVAL_DEPTH,
            "the count refused first at {}, so the stack budget was never tested",
            interp.deepest_eval
        );
        assert!(
            interp.deepest_stack > SMALL && interp.deepest_stack < SMALL + 32 * 1024,
            "refusal must land within one level of the budget, not at {} bytes",
            interp.deepest_stack
        );
        assert_eq!(interp.eval_depth, 0, "depth must unwind to zero");
        assert!(interp.stack.is_none(), "the budget must be closed");
        assert_eq!(
            interp.env.scopes.len(),
            1,
            "scope stack must be clean after a stack-budget refusal"
        );

        let tokens = crate::lexer::tokenize("1 + 1").unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        assert_eq!(interp.exec(&ast).unwrap(), Value::Number(2.0));
    }

    /// The acceptance side on the smallest stack: scripts more generous than
    /// any of the 51 fixtures evaluate on a 1 MiB thread, and the deepest of
    /// them leaves most of `STACK_BUDGET_BYTES` unspent -- so the budget is
    /// not sitting on top of real forms in this build profile.
    #[test]
    fn ordinary_scripts_fit_on_the_smallest_stack() {
        let deepest_stack = on_a_stack_of(1 << 20, || {
            let mut deepest_stack = 0;
            let mut run_measured = |src: &str| -> Value {
                let tokens = crate::lexer::tokenize(src).unwrap();
                let ast = crate::parser::parse(tokens).unwrap();
                let mut interp = Interpreter::new();
                let value = interp.exec(&ast).expect("an ordinary script must evaluate");
                deepest_stack = deepest_stack.max(interp.deepest_stack);
                value
            };

            // ten chained helpers, each a frame carrying a small expression
            let mut helpers = String::from("func h0()\n  1\nendfunc\n");
            for k in 1..10 {
                helpers.push_str(&format!("func h{k}()\n  h{}() + 1\nendfunc\n", k - 1));
            }
            helpers.push_str("h9()\n");
            assert_eq!(run_measured(&helpers), Value::Number(10.0));

            // a sixty-term sum: depth 61, the deepest realistic shape
            assert_eq!(
                run_measured(&vec!["1"; 60].join(" + ")),
                Value::Number(60.0)
            );

            // two helpers each carrying a sixty-term sum: the hostile shape at
            // a size a real form could have, twice as deep as any fixture's
            assert_eq!(
                run_measured(&hostile_call_chain(1, 60)),
                Value::Number(120.0)
            );

            // twenty nested ifs
            let mut nested = String::from("var x = 1\nx");
            for _ in 0..20 {
                nested = format!("if (1 > 0) then\n{nested}\nendif");
            }
            run_measured(&nested);

            deepest_stack
        });
        assert!(
            deepest_stack <= STACK_BUDGET_BYTES / 4 * 3,
            "the deepest ordinary script spent {deepest_stack} bytes of a \
             {STACK_BUDGET_BYTES}-byte budget; a level costs more than it did \
             when the budget was sized"
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
        // For-loop recursion is the most expensive shape per FormCalc level --
        // some ten Rust frames from one `FuncCall` to the next -- so it is
        // the shape that decides whether `MAX_CALL_DEPTH` or the stack budget
        // speaks first: in release the frame bound, in debug either, and the
        // subject here is the scope stack afterwards, not which budget spoke.
        // A 1 MiB thread, the smallest stack the pipeline ships on; without
        // the budgets this overflowed a 2 MB thread in debug.
        let handle = std::thread::Builder::new()
            .stack_size(1 << 20)
            .spawn(|| {
                let mut interp = Interpreter::new();
                let script =
                    "func f(x)\n  for i = 0 upto 1 do\n    f(x + 1)\n  endfor\nendfunc\nf(0)";
                let tokens = crate::lexer::tokenize(script).unwrap();
                let ast = crate::parser::parse(tokens).unwrap();
                let result = interp.exec(&ast);
                assert!(
                    matches!(
                        result,
                        Err(FormCalcError::CallDepthExceeded { .. })
                            | Err(FormCalcError::StackBudgetExceeded { .. })
                    ),
                    "expected a depth or stack refusal, got: {:?}",
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
