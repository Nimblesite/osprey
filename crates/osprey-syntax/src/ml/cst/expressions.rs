//! ML concrete expressions.
use super::{MlItem, MlSymbolPath, MlType, Position, Stage};

/// An ML expression, recorded exactly as written.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MlExpr {
    /// Headless `{ field = value }` record. [FLAVOR-ML-RECORD-ANON]
    Object(Vec<MlField>),
    /// Integer literal.
    Int(i64),
    /// Float literal.
    Float(f64),
    /// Boolean literal.
    Bool(bool),
    /// Raw string literal text (quotes/escapes/`${…}` unresolved).
    Str {
        /// The literal's source text, delimiters already dropped by the lexer.
        raw: String,
        /// Source position of the literal — the anchor a `${…}` fragment's own
        /// positions are rebased onto ([`crate::strings`]).
        pos: Position,
    },
    /// Identifier or constructor reference.
    Ident(String),
    /// A namespace/module/member-qualified reference such as `Tax::addTax`.
    /// This is distinct from [`MlExpr::Field`] so `.` remains value access.
    Path(MlSymbolPath),
    /// Prefix unary (`-x`, `!x`).
    Unary {
        /// Operator spelling.
        op: String,
        /// The operand.
        operand: Box<MlExpr>,
    },
    /// Binary operator, including the pipe `|>` (the lowerer desugars pipes).
    Binary {
        /// Operator spelling.
        op: String,
        /// Left operand.
        left: Box<MlExpr>,
        /// Right operand.
        right: Box<MlExpr>,
        /// Source location of the operator token.
        pos: Position,
    },
    /// Single-argument application `func arg` (the surface curried form). A
    /// whitespace spine `f a b` nests these (`App(App(f, a), b)`) and lowers to
    /// curried nested single-argument calls ([FLAVOR-ML-CALL]).
    App {
        /// The applied expression.
        func: Box<MlExpr>,
        /// The single argument.
        arg: Box<MlExpr>,
    },
    /// Parenthesised comma-list application `func (a, b, …)` — the **uncurried**
    /// saturated call, lowering to a single multi-argument `Call(func, [a, b,
    /// …])` ([FLAVOR-ML-CALL]). A one-element list `f (a)` is plain grouping and
    /// parses as [`MlExpr::App`], not this node.
    AppMulti {
        /// The applied expression.
        func: Box<MlExpr>,
        /// The argument list (two or more), in order.
        args: Vec<MlExpr>,
    },
    /// Explicit call-site type arguments [FLAVOR-ML-GENERICS].
    TypeApply {
        /// Named callee.
        func: Box<MlExpr>,
        /// Written arguments in binder order.
        args: Vec<MlType>,
        /// Callee position.
        pos: Position,
    },
    /// Zero-argument application `func ()`.
    UnitApp {
        /// The applied expression.
        func: Box<MlExpr>,
    },
    /// `target.field` access.
    Field {
        /// The receiver.
        target: Box<MlExpr>,
        /// The field name.
        name: String,
    },
    /// `[ a, b, c ]` list literal (possibly empty), with the position of its
    /// opening `[` — inference keys the resolved `List<T>` on it so an EMPTY
    /// literal still reaches the backend with an element type.
    List(Vec<MlExpr>, Position),
    /// `[ k => v, … ]` map literal — the bracket form disambiguated from a list
    /// by the `=>` entry separator ([FLAVOR-ML-MAP]).
    Map(Vec<(MlExpr, MlExpr)>),
    /// `target[index]` — a glued postfix index (list/map lookup, returns
    /// `Result`). Only formed when the `[` abuts the target with no space.
    Index {
        /// The indexed expression.
        target: Box<MlExpr>,
        /// The index/key expression.
        index: Box<MlExpr>,
    },
    /// `( inner )` — grouping kept in the CST; the lowerer unwraps it.
    Paren(Box<MlExpr>),
    /// `\param* => body` lambda. The juxtaposed head `\x y => body` is **curried**
    /// (a one-parameter `Lambda` returning a `Lambda` chain); the parenthesised
    /// comma-list head `\(x, y) => body` is **uncurried** (one flat multi-parameter
    /// `Lambda`), twinning Default's `(x, y) => body` ([FLAVOR-ML-CURRY]).
    Lambda {
        /// The surface parameter list.
        params: Vec<MlParam>,
        /// `true` for the parenthesised comma-list head `\(x, y) =>` (flat);
        /// `false` for the juxtaposed `\x y =>` (curried chain).
        uncurried: bool,
        /// The lambda body.
        body: Box<MlExpr>,
        /// Source position.
        pos: Position,
    },
    /// `match scrutinee` + indented arms.
    Match {
        /// The scrutinee.
        scrutinee: Box<MlExpr>,
        /// The arms.
        arms: Vec<MlArm>,
    },
    /// Constructor record literal `Name` + indented `field = value` lines, or
    /// the inline `Name(field = value)` — optionally with explicit
    /// construction-site type arguments `Name<t, …>(field = value)`
    /// ([TYPE-GENERICS-DECL], [FLAVOR-ML-GENERICS]).
    Record {
        /// Constructor/type name.
        name: String,
        /// Explicit construction-site type arguments (empty when inferred).
        type_args: Vec<MlType>,
        /// Field initialisers.
        fields: Vec<MlField>,
    },
    /// A layout block: leading items and an optional trailing value expression.
    Block {
        /// Statements before the trailing value.
        items: Vec<MlItem>,
        /// The trailing value expression, if any.
        value: Option<Box<MlExpr>>,
        /// Position of the trailing value expression.
        pos: Option<Position>,
    },
    /// `spawn body` — start a fiber whose body (an indented block or inline
    /// expression) runs concurrently ([FLAVOR-ML-SPAWN]).
    Spawn(Box<MlExpr>),
    /// `perform Effect.op arg…` — perform an effect operation with
    /// whitespace-applied arguments ([FLAVOR-ML-EFFECT]).
    Perform {
        /// The effect name.
        effect: String,
        /// The operation name.
        operation: String,
        /// The performed arguments, in order.
        args: Vec<MlExpr>,
        /// Source position of the `perform` keyword
        /// ([EFFECTS-GENERIC-INSTANTIATION]).
        pos: Position,
    },
    /// `handler Effect` + indented arms — the handler ITSELF, with no region
    /// attached: a value that can be bound, passed and called
    /// ([EFFECTS-HANDLER-VALUE]).
    HandlerValue {
        /// Selected interpretation stage.
        stage: Stage,
        /// The handled effect name.
        effect: String,
        /// Per-operation handler arms.
        arms: Vec<MlHandleArm>,
        /// Normal-completion transformation as a unary lambda.
        return_clause: Option<Box<MlExpr>>,
        /// Source position of the `handler` keyword.
        pos: Position,
    },
    /// `handle Effect` + indented arms + `in body` — install an effect handler
    /// over the `body` expression ([FLAVOR-ML-EFFECT]).
    Handle {
        /// Whether the handler is discharged at compile time (`handle static`)
        /// or installed on the runtime handler stack. Implements [STAGE-DECL].
        stage: Stage,
        /// The handled effect name.
        effect: String,
        /// The per-operation handler arms.
        arms: Vec<MlHandleArm>,
        /// Normal-completion transformation as a unary lambda.
        return_clause: Option<Box<MlExpr>>,
        /// The handled body expression (after `in`).
        body: Box<MlExpr>,
        /// Source position of the `handle` keyword
        /// ([EFFECTS-GENERIC-INSTANTIATION]).
        pos: Position,
    },
    /// `resume` or `resume value` — resume a suspended continuation
    /// ([FLAVOR-ML-EFFECT]).
    Resume(Option<Box<MlExpr>>),
    /// `await fiber` — block on a spawned fiber's result ([FLAVOR-ML-CONCURRENCY]).
    Await(Box<MlExpr>),
    /// `yield` or `yield value` — yield from the current fiber ([FLAVOR-ML-CONCURRENCY]).
    Yield(Option<Box<MlExpr>>),
    /// `send channel value` — send a value on a channel ([FLAVOR-ML-CONCURRENCY]).
    Send {
        /// The channel expression.
        channel: Box<MlExpr>,
        /// The value to send.
        value: Box<MlExpr>,
    },
    /// `recv channel` — receive a value from a channel ([FLAVOR-ML-CONCURRENCY]).
    Recv(Box<MlExpr>),
    /// `select` + indented `pattern => body` arms — choose among ready channel
    /// arms ([FLAVOR-ML-CONCURRENCY]).
    Select(Vec<MlArm>),
}

/// One `op param* => body` arm of a `handle` expression ([FLAVOR-ML-EFFECT]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlHandleArm {
    /// The handled operation name.
    pub operation: String,
    /// The operation parameter names bound in the body.
    pub params: Vec<MlBinder>,
    /// The arm body.
    pub body: MlExpr,
    /// Source position of the operation name.
    pub pos: Position,
}

/// One `pattern => body` arm of a `match`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlArm {
    /// The arm pattern.
    pub pattern: MlPattern,
    /// The arm body.
    pub body: MlExpr,
}

/// An ML match pattern.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MlPattern {
    /// `_`.
    Wildcard,
    /// An integer literal pattern.
    Int(i64),
    /// A string literal pattern (raw).
    Str(String),
    /// A boolean literal pattern.
    Bool(bool),
    /// `Ctor field*` — a constructor binding zero or more payload fields.
    Ctor {
        /// Constructor name.
        name: String,
        /// Bound field names.
        fields: Vec<MlBinder>,
    },
    /// A bare lowercase binding.
    Bind(MlBinder),
    /// `{ a, b }` / `{ a, .. }` — a structural row pattern binding each named
    /// field; `..` opens the row ([PATTERN-STRUCTURAL]).
    Structural {
        /// Bound field names in written order.
        fields: Vec<MlBinder>,
        /// Whether a trailing `..` opens the row.
        open: bool,
    },
    /// `(a, b)` — a tuple pattern: the positional spelling of a structural
    /// row, one binder or `_` per decimal slot ([PATTERN-TUPLE]).
    Tuple(Vec<MlPattern>),
    /// `[ p, … ]` or `[ p, …, ...rest ]` — a list pattern with fixed-prefix
    /// element patterns and an optional trailing `...name` rest-binder
    /// ([FLAVOR-ML-MATCH], [TYPE-LIST-PATTERNS]).
    List {
        /// Patterns for the fixed-prefix element positions.
        elements: Vec<MlPattern>,
        /// The trailing `...name` rest-binder, or `None` for a fixed length.
        rest: Option<MlBinder>,
    },
}

/// A `field = value` initialiser inside a record literal.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlField {
    /// The field name.
    pub name: String,
    /// The field value.
    pub value: MlExpr,
}

/// A binder's written name and its exact token position. Generated clause
/// parameters carry no source position; they must never borrow a user's token.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlBinder {
    pub name: String,
    pub pos: Option<Position>,
}

/// A surface parameter pattern in a binding or lambda head.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MlParam {
    /// A named parameter, type left to inference / the signature.
    Named(MlBinder),
    /// A parenthesised type-annotated parameter `(name : type)` — the inline
    /// form a lambda uses for a load-bearing parameter type ([FLAVOR-ML-FN]).
    Typed(MlBinder, MlType, Position),
    /// The unit marker `()` — a zero-argument function boundary, not a value.
    Unit,
    /// A refutable head pattern: the column of an equational clause that
    /// selects on a literal or a constructor ([FLAVOR-ML-CLAUSES]). Never
    /// reaches the lowerer — [`super::clauses::merge`] rewrites every clause
    /// set into a plain parameter list over a `match`.
    Pattern(MlPattern),
}
