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
        object: Box<Expr>,
        member: String,
    },

    /// Unary negation: -expr
    Negate(Box<Expr>),
    /// Logical not: not expr
    Not(Box<Expr>),

    /// Binary operation
    BinaryOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// String concatenation: a & b
    Concat(Box<Expr>, Box<Expr>),

    /// Assignment: target = value
    Assign { target: Box<Expr>, value: Box<Expr> },

    /// Function call: name(args...)
    FuncCall { name: String, args: Vec<Expr> },

    /// If/elseif/else expression
    If {
        condition: Box<Expr>,
        then_body: Vec<Expr>,
        elseif_clauses: Vec<(Expr, Vec<Expr>)>,
        else_body: Option<Vec<Expr>>,
    },

    /// While loop
    While {
        condition: Box<Expr>,
        body: Vec<Expr>,
    },

    /// For loop (upto/downto)
    For {
        var: String,
        start: Box<Expr>,
        end: Box<Expr>,
        step: Option<Box<Expr>>,
        ascending: bool,
        body: Vec<Expr>,
    },

    /// Foreach loop
    Foreach {
        var: String,
        list: Box<Expr>,
        body: Vec<Expr>,
    },

    /// Function declaration
    FuncDecl {
        name: String,
        params: Vec<String>,
        body: Vec<Expr>,
    },

    /// Var declaration: var name = value
    VarDecl {
        name: String,
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
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}
