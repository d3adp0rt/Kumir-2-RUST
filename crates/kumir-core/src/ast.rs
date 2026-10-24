//! Abstract syntax tree of a Kumir program.
//!
//! The shapes intentionally follow the original `AST` namespace from
//! `src/kumir2-libs/dataformats/ast_*.h`, but are represented with ordinary
//! Rust enums and owned data.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaseType {
    Int,
    Real,
    Str,
    Char,
    Bool,
    File,
    /// Record / user type. Kept for forward compatibility; the interpreter
    /// treats unknown user types as records with named fields.
    User(String),
    Void,
}

impl BaseType {
    pub fn russian_name(&self) -> &str {
        match self {
            BaseType::Int => "цел",
            BaseType::Real => "вещ",
            BaseType::Str => "лит",
            BaseType::Char => "сим",
            BaseType::Bool => "лог",
            BaseType::File => "файл",
            BaseType::User(name) => name,
            BaseType::Void => "—",
        }
    }

    pub fn default_value(&self) -> crate::value::Value {
        use crate::value::Value;
        match self {
            BaseType::Int => Value::Int(0),
            BaseType::Real => Value::Real(0.0),
            BaseType::Str => Value::Str(String::new()),
            BaseType::Char => Value::Char(' '),
            BaseType::Bool => Value::Bool(false),
            BaseType::File => Value::Undefined,
            BaseType::User(_) => Value::Undefined,
            BaseType::Void => Value::Undefined,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeRef {
    pub base: BaseType,
    pub is_array: bool,
}

impl TypeRef {
    pub fn simple(base: BaseType) -> Self {
        Self {
            base,
            is_array: false,
        }
    }

    pub fn array(base: BaseType) -> Self {
        Self {
            base,
            is_array: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Regular,
    In,
    Out,
    InOut,
}

#[derive(Clone, Debug)]
pub struct VarDecl {
    pub name: String,
    pub typ: TypeRef,
    /// One (start, end) pair per array dimension. Empty for scalars.
    pub bounds: Vec<(Expr, Option<Expr>)>,
    pub init: Option<Expr>,
    pub access: Access,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub struct Algorithm {
    pub name: String,
    pub params: Vec<VarDecl>,
    pub ret: Option<TypeRef>,
    pub pre: Vec<Stmt>,
    pub post: Vec<Stmt>,
    pub body: Vec<Stmt>,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub struct Program {
    pub modules: Vec<String>,
    pub globals: Vec<VarDecl>,
    pub initializer: Vec<Stmt>,
    pub algorithms: Vec<Algorithm>,
    pub main: Option<usize>,
    pub source_name: String,
}

impl Program {
    pub fn algorithm_index(&self, name: &str) -> Option<usize> {
        let wanted = normalize_name(name);
        self.algorithms
            .iter()
            .position(|a| normalize_name(&a.name) == wanted)
    }

    pub fn main_algorithm(&self) -> Option<&Algorithm> {
        self.main.map(|i| &self.algorithms[i])
    }
}

/// Collapse runs of whitespace; Kumir names may consist of several words.
pub fn normalize_name(name: &str) -> String {
    let mut out = String::new();
    let mut space = false;
    for ch in name.trim().chars() {
        if ch.is_whitespace() {
            if !out.is_empty() {
                space = true;
            }
        } else {
            if space {
                out.push(' ');
                space = false;
            }
            out.push(ch);
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct Subscript {
    pub start: Expr,
    pub end: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct LValue {
    pub name: String,
    pub subs: Vec<Subscript>,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Int(i64),
    Real(f64),
    Str(String),
    Char(char),
    Bool(bool),
    Newline,
    Var(String),
    Index(Box<Expr>, Vec<Subscript>),
    Call(String, Vec<Expr>),
    /// `{1, 2, 3}` — array initializer.
    ArrayLit(Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Plus,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Decl(Vec<VarDecl>),
    Assign(LValue, Expr),
    Output {
        file: Option<Expr>,
        args: Vec<OutputArg>,
    },
    Input {
        file: Option<Expr>,
        targets: Vec<LValue>,
    },
    If(Vec<Branch>),
    Switch(Vec<Branch>),
    Loop {
        kind: LoopKind,
        end_cond: Option<Expr>,
        body: Vec<Stmt>,
    },
    Assert(Expr),
    Return(Expr),
    Break,
    Pause,
    Halt,
    /// An expression used as a statement: most often a procedure call.
    Expr(Expr),
}

#[derive(Clone, Debug)]
pub struct OutputArg {
    pub expr: Expr,
    pub width: Option<Expr>,
    pub decimals: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct Branch {
    /// `None` means `иначе` / `else`.
    pub condition: Option<Expr>,
    pub body: Vec<Stmt>,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub enum LoopKind {
    For {
        var: String,
        from: Expr,
        to: Expr,
        step: Option<Expr>,
    },
    While(Expr),
    Times(Expr),
    Forever,
}
