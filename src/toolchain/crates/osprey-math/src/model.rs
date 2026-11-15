//! What the reader builds and the emitter writes, plus the tables that say
//! which Lean operation is which Osprey operation ([MATH-CODE-MEANING]).

/// A type of the fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ty {
    /// IEEE-754 binary64 on both sides.
    Float,
    /// Two values on both sides.
    Bool,
}

impl Ty {
    /// The Lean spelling, for refusals.
    pub(crate) fn lean(self) -> &'static str {
        match self {
            Self::Float => "Float",
            Self::Bool => "Bool",
        }
    }

    /// The Osprey spelling, for signatures.
    pub(crate) fn osprey(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::Bool => "bool",
        }
    }
}

/// A binary operator, in Osprey's spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Or,
    And,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    /// The operator's text and how tightly it binds when written.
    pub(crate) fn written(self) -> (&'static str, u8) {
        match self {
            Self::Or => ("||", 1),
            Self::And => ("&&", 2),
            Self::Lt => ("<", 3),
            Self::Le => ("<=", 3),
            Self::Gt => (">", 3),
            Self::Ge => (">=", 3),
            Self::Eq => ("==", 3),
            Self::Ne => ("!=", 3),
            Self::Add => ("+", 4),
            Self::Sub => ("-", 4),
            Self::Mul => ("*", 5),
            Self::Div => ("/", 5),
        }
    }
}

/// A term that is one value: it may stand as an operand or an argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expr {
    /// A decimal literal as Osprey reads it, with a leading `-` when negative.
    Num(String),
    /// `true` or `false`.
    Bool(bool),
    /// A parameter or a `let` name.
    Name(String),
    /// An Osprey function and its arguments; no arguments is a constant.
    Call(String, Vec<Expr>),
    /// Boolean negation.
    Not(Box<Expr>),
    /// A binary operation.
    Binary(Op, Box<Expr>, Box<Expr>),
}

/// A term that may also branch or bind: a body, a branch or a `let` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tail {
    /// A single value.
    Expr(Expr),
    /// `if test then yes else no`.
    If(Expr, Box<Tail>, Box<Tail>),
    /// `let name := value` followed by a body.
    Let(String, Box<Tail>, Box<Tail>),
}

/// One generated function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Def {
    pub(crate) name: String,
    pub(crate) doc: Option<String>,
    pub(crate) public: bool,
    pub(crate) params: Vec<(String, Ty)>,
    pub(crate) result: Ty,
    pub(crate) body: Tail,
}

/// One generated module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Module {
    pub(crate) namespace: Option<String>,
    pub(crate) name: String,
    pub(crate) defs: Vec<Def>,
}

/// Lean's `Float.<name>` and the Osprey builtin that computes the same value,
/// with its argument count. Every one takes and returns `Float`.
const FUNCTIONS: &[(&str, &str, usize)] = &[
    ("sqrt", "sqrt", 1),
    ("exp", "exp", 1),
    ("log", "ln", 1),
    ("log2", "log2", 1),
    ("log10", "log10", 1),
    ("sin", "sin", 1),
    ("cos", "cos", 1),
    ("tan", "tan", 1),
    ("asin", "asin", 1),
    ("acos", "acos", 1),
    ("atan", "atan", 1),
    ("atan2", "atan2", 2),
    ("sinh", "sinh", 1),
    ("cosh", "cosh", 1),
    ("tanh", "tanh", 1),
    ("asinh", "asinh", 1),
    ("acosh", "acosh", 1),
    ("atanh", "atanh", 1),
    ("pow", "pow", 2),
    ("floor", "floor", 1),
    ("ceil", "ceil", 1),
    ("abs", "abs", 1),
];

/// The Osprey builtin and argument count for Lean's `Float.<name>`.
pub(crate) fn function(name: &str) -> Option<(&'static str, usize)> {
    FUNCTIONS
        .iter()
        .find(|(lean, _, _)| *lean == name)
        .map(|(_, osprey, arity)| (*osprey, *arity))
}

/// ML-flavor keywords, and `floatDiv`, which the emitter writes for `/`.
const KEYWORDS: &[&str] = &[
    "mut",
    "true",
    "false",
    "match",
    "type",
    "extern",
    "spawn",
    "effect",
    "perform",
    "handle",
    "handler",
    "resume",
    "mask",
    "instance",
    "in",
    "await",
    "yield",
    "send",
    "recv",
    "select",
    "import",
    "namespace",
    "module",
    "signature",
    "export",
    "opaque",
    "state",
    "as",
    "floatDiv",
];

/// Whether `name` is a word of the ML flavor, which no generated name may be.
pub(crate) fn keyword(name: &str) -> bool {
    KEYWORDS.contains(&name)
}

/// Whether `name` is taken by Osprey itself: a keyword, or a builtin a
/// generated body may call.
pub(crate) fn reserved(name: &str) -> bool {
    keyword(name) || FUNCTIONS.iter().any(|(_, osprey, _)| *osprey == name)
}
