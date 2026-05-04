//! FormCalc AST — Abstract Syntax Tree for parsed FormCalc code.

/// A FormCalc expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Numeric literal
    Number(f64),
    /// String literal
    StringLit(String),
    /// Null literal
    Null,
    /// Variable/identifier reference
    Ident(String),
    /// Member access: object.member (SOM path resolution)
    MemberAccess {
        /// Object expression.
        object: Box<Expr>,
        /// Member name.
        member: String,
    },
    /// Indexed access: object[index] — index is 0-based integer or `*` for all.
    IndexAccess {
        /// Object expression.
        object: Box<Expr>,
        /// Index expression.
        index: AccessIndex,
    },
    /// Recursive descent: object..member (SOM `..` separator)
    RecursiveDescent {
        /// Object expression.
        object: Box<Expr>,
        /// Member name.
        member: String,
    },

    /// Unary negation: -expr
    Negate(Box<Expr>),
    /// Unary plus: +expr
    Positive(Box<Expr>),
    /// Logical not: not expr
    Not(Box<Expr>),

    /// Binary operation
    BinaryOp {
        /// Operator.
        op: BinOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },

    /// String concatenation: a & b
    Concat(Box<Expr>, Box<Expr>),

    /// Assignment: target = value
    Assign {
        /// Assignment target.
        target: Box<Expr>,
        /// Value to assign.
        value: Box<Expr>,
    },

    /// Function call: name(args...)
    FuncCall {
        /// Function name.
        name: String,
        /// Arguments.
        args: Vec<Expr>,
    },

    /// If/elseif/else expression
    If {
        /// Condition expression.
        condition: Box<Expr>,
        /// Body when condition is true.
        then_body: Vec<Expr>,
        /// Else-if clauses.
        elseif_clauses: Vec<(Expr, Vec<Expr>)>,
        /// Else body.
        else_body: Option<Vec<Expr>>,
    },

    /// While loop
    While {
        /// Loop condition.
        condition: Box<Expr>,
        /// Loop body.
        body: Vec<Expr>,
    },

    /// For loop (upto/downto)
    For {
        /// Loop variable.
        var: String,
        /// Start value.
        start: Box<Expr>,
        /// End value.
        end: Box<Expr>,
        /// Step expression.
        step: Option<Box<Expr>>,
        /// Whether the loop is ascending.
        ascending: bool,
        /// Loop body.
        body: Vec<Expr>,
    },

    /// Foreach loop
    Foreach {
        /// Loop variable.
        var: String,
        /// List expression.
        list: Box<Expr>,
        /// Loop body.
        body: Vec<Expr>,
    },

    /// Function declaration
    FuncDecl {
        /// Function name.
        name: String,
        /// Parameter names.
        params: Vec<String>,
        /// Function body.
        body: Vec<Expr>,
    },

    /// Var declaration: var name = value
    VarDecl {
        /// Variable name.
        name: String,
        /// Initial value.
        init: Option<Box<Expr>>,
    },

    /// Return statement
    Return(Option<Box<Expr>>),
    /// Break statement
    Break,
    /// Continue statement
    Continue,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
    /// Logical and.
    And,
    /// Logical or.
    Or,
}

/// Index for bracket access in SOM expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum AccessIndex {
    /// Numeric 0-based index: `name[0]`, `name[2]`
    Numeric(i64),
    /// Wildcard: `name[*]` — all occurrences
    All,
}
