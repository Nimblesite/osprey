//! Expressions, call arguments, match and handler arms.

use crate::Parameter;
use crate::Pattern;
use crate::Position;
use crate::Stage;
use crate::Stmt;
use crate::SymbolPath;
use crate::TypeExpr;
/// A named argument `name: value` in a call.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedArgument {
    /// Argument name.
    pub name: String,
    /// Argument value expression.
    pub value: Expr,
}

/// A part of an interpolated string — literal text or an embedded expression.
#[derive(Debug, Clone, PartialEq)]
pub enum InterpolatedPart {
    /// Literal text between interpolations.
    Text(String),
    /// An embedded `${expr}`.
    Expr(Expr),
}

/// A match arm `pattern => body`.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    /// The arm's pattern.
    pub pattern: Pattern,
    /// The arm body evaluated when the pattern matches.
    pub body: Expr,
}

/// What one `select` arm waits for.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectOperation {
    /// Receive from this channel.
    Recv(Box<Expr>),
    /// Send on a channel.
    Send {
        /// The channel.
        channel: Box<Expr>,
        /// The value to send.
        value: Box<Expr>,
    },
    /// Give up after this many milliseconds.
    Timeout(Box<Expr>),
    /// Run at once when no other arm is ready.
    Default,
}

/// Every expression a `select` arm's operation evaluates on entry, in written
/// order. Shared by the reading and the rewriting accessor, which differ only
/// in the borrow they hold, so the two can never drift apart.
macro_rules! select_operands {
    ($operation:expr) => {
        match $operation {
            Self::Recv(operand) | Self::Timeout(operand) => vec![operand],
            Self::Send { channel, value } => vec![channel, value],
            Self::Default => Vec::new(),
        }
    };
}

impl SelectOperation {
    /// The expressions evaluated on entry to the `select`.
    #[must_use]
    pub fn operands(&self) -> Vec<&Expr> {
        select_operands!(self)
    }

    /// The expressions evaluated on entry to the `select`, for rewriting.
    pub fn operands_mut(&mut self) -> Vec<&mut Expr> {
        select_operands!(self)
    }
}

/// A field assignment `name: value` in an object/type constructor.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldAssignment {
    /// Field name.
    pub name: String,
    /// Assigned value expression.
    pub value: Expr,
}

/// A map entry `key: value` in a map literal.
#[derive(Debug, Clone, PartialEq)]
pub struct MapEntry {
    /// Entry key expression.
    pub key: Expr,
    /// Entry value expression.
    pub value: Expr,
}

/// An expression. Boxing breaks the recursive cycle; positions are attached
/// where the parser records them.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Integer literal.
    Integer(i64),
    /// Float literal.
    Float(f64),
    /// String literal.
    Str(String),
    /// Boolean literal.
    Bool(bool),
    /// Interpolated string literal (`"a ${x} b"`).
    InterpolatedStr(Vec<InterpolatedPart>),
    /// A bare identifier reference.
    Identifier(String),
    /// A namespace/module-qualified reference (`billing::Tax::addTax`).
    Path(SymbolPath),
    /// `[a, b, c]` list literal, with the source position inference keys its
    /// resolved `List<T>` on ([`crate::Position`]). An EMPTY literal has no
    /// element to read a representation from, so the backend can only recover
    /// `T` — and with it the element-typed owner tag every later read depends
    /// on — from what inference published at this position.
    List(Vec<Expr>, Option<Position>),
    /// `{ k: v, ... }` map literal.
    Map(Vec<MapEntry>),
    /// `{ field: value, ... }` anonymous object literal.
    Object(Vec<FieldAssignment>),
    /// A binary operation.
    Binary {
        /// Operator spelling (`+`, `==`, `&&`, …).
        op: String,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
        /// Source location of the operator token.
        position: Option<Position>,
    },
    /// A unary operation.
    Unary {
        /// Operator spelling (`-`, `!`).
        op: String,
        /// The operand.
        operand: Box<Expr>,
    },
    /// `f(args)` — positional or named (UFCS dispatch resolved later).
    Call {
        /// The callee expression.
        function: Box<Expr>,
        /// Positional arguments.
        arguments: Vec<Expr>,
        /// Named arguments.
        named_arguments: Vec<NamedArgument>,
    },
    /// Explicit declaration-binder arguments on a call's callee.
    /// Both flavors use this node for [TYPE-GENERICS-APPLY]. Keeping the
    /// application on the callee preserves ordinary and curried call nodes.
    TypeApply {
        /// The named function being instantiated.
        function: Box<Expr>,
        /// Written type arguments, in declaration order.
        type_args: Vec<TypeExpr>,
        /// Source position of the callee, for application diagnostics.
        position: Option<Position>,
    },
    /// `a |> b` pipe.
    Pipe {
        /// Piped value.
        left: Box<Expr>,
        /// Function applied to it.
        right: Box<Expr>,
    },
    /// `obj.field` field access.
    FieldAccess {
        /// The record/handle expression.
        target: Box<Expr>,
        /// Accessed field name.
        field: String,
    },
    /// `obj.method(args)` method call.
    MethodCall {
        /// The receiver expression.
        target: Box<Expr>,
        /// Method name.
        method: String,
        /// Positional arguments.
        arguments: Vec<Expr>,
        /// Named arguments.
        named_arguments: Vec<NamedArgument>,
    },
    /// `a[i]` index access (returns `Result`).
    Index {
        /// The indexed list/map expression.
        target: Box<Expr>,
        /// The index/key expression.
        index: Box<Expr>,
    },
    /// A lambda `fn(params) => body`.
    Lambda {
        /// Lambda parameters.
        parameters: Vec<Parameter>,
        /// Declared return type, if annotated.
        return_type: Option<TypeExpr>,
        /// Lambda body.
        body: Box<Expr>,
        /// Source position — the key under which inference publishes this
        /// lambda's resolved function type for the code generator.
        position: Option<Position>,
    },
    /// A `match` expression.
    Match {
        /// The scrutinee expression.
        value: Box<Expr>,
        /// The match arms.
        arms: Vec<MatchArm>,
    },
    /// A `{ ... }` block expression.
    Block {
        /// Block statements.
        statements: Vec<Stmt>,
        /// The trailing value-expression, if any.
        value: Option<Box<Expr>>,
        /// Source position of the trailing expression, retained for debugging.
        position: Option<Position>,
    },
    /// `Type<T> { field: value }` type constructor.
    TypeConstructor {
        /// Type name.
        name: String,
        /// Generic type arguments.
        type_args: Vec<TypeExpr>,
        /// Field assignments.
        fields: Vec<FieldAssignment>,
    },
    /// `record { field: newValue }` non-destructive update.
    Update {
        /// The base record variable.
        record: String,
        /// Overridden field assignments.
        fields: Vec<FieldAssignment>,
    },
    /// `spawn expr` — start a fiber.
    Spawn(Box<Expr>),
    /// `yield`/`yield expr` from a fiber.
    Yield(Option<Box<Expr>>),
    /// `await expr` — await a fiber result.
    Await(Box<Expr>),
    /// `channel <- value` send.
    Send {
        /// Target channel expression.
        channel: Box<Expr>,
        /// Value to send.
        value: Box<Expr>,
    },
    /// `<-channel` receive.
    Recv(Box<Expr>),
    /// A `select` over channel operations. Each operation's operands evaluate
    /// once on entry, in source order; `arms[i]` runs when `operations[i]` is
    /// chosen, its pattern binding a received value. [CONCURRENCY-SELECT]
    Select {
        /// What each arm waits for, parallel to `arms`.
        operations: Vec<SelectOperation>,
        /// The select arms.
        arms: Vec<MatchArm>,
    },
    /// `perform Effect.operation(args)`.
    Perform {
        /// Effect name.
        effect: String,
        /// Operation name.
        operation: String,
        /// Positional arguments.
        arguments: Vec<Expr>,
        /// Named arguments.
        named_arguments: Vec<NamedArgument>,
        /// Source position — the key under which inference publishes this
        /// site's instantiated operation signature. Implements
        /// [EFFECTS-GENERIC-INSTANTIATION].
        position: Option<Position>,
    },
    /// `handle Effect op params => body ... in body`.
    Handler {
        /// [`Stage::Static`] for a `handle static` region — the arms are a
        /// rewrite rule, not a runtime handler. Implements
        /// [STAGE-HANDLE-STATIC].
        stage: Stage,
        /// Handled effect name.
        effect: String,
        /// Per-operation handler arms.
        arms: Vec<HandlerArm>,
        /// Optional normal-completion transformation, represented as a unary
        /// lambda. It executes outside this activation. [EFFECTS-HANDLER-ARMS]
        return_clause: Option<Box<Expr>>,
        /// Optional `finally => cleanup` clause: a `Unit` expression that runs
        /// exactly once when the activation ends, after the return clause and
        /// outside this activation. [EFFECTS-FINALIZATION]
        finally: Option<Box<Expr>>,
        /// The handled body expression.
        body: Box<Expr>,
        /// Source position — the key under which inference publishes this
        /// handler's instantiated operation signatures. Implements
        /// [EFFECTS-GENERIC-INSTANTIATION].
        position: Option<Position>,
    },
    /// `mask E { body }` — evaluate `body` with the activation currently
    /// answering `E` (an effect, or a capability bound by `instance`) hidden,
    /// so its requests reach the next one out. Implements [EFFECTS-MASK].
    Mask {
        /// The masked effect or capability name.
        effect: String,
        /// The masked body.
        body: Box<Expr>,
        /// Source position — the key under which inference publishes the
        /// masked effect's instantiation.
        position: Option<Position>,
    },
    /// `instance E` — a fresh capability identity for effect `E`, with no
    /// implementation installed. Implements [EFFECTS-INSTANCE].
    Instance {
        /// The effect the capability performs.
        effect: String,
        /// Source position — the key under which inference publishes the
        /// capability's instantiation.
        position: Option<Position>,
    },
    /// `resume(value)` — resume the performer's delimited continuation with
    /// `value` (or `Unit` when absent). Legal only inside a handler arm body.
    /// Implements [EFFECTS-RESUME].
    Resume(Option<Box<Expr>>),
}

impl Expr {
    /// Build `-operand`, folding a negated numeric **literal** into the literal
    /// itself so it keeps the plain `int`/`float` type.
    ///
    /// A runtime negation is fallible (`-i64::MIN` overflows) and performs
    /// `Arith.overflow` under [ARITH-CHECKED]. A *literal*
    /// negation is a compile-time constant: the lexer already rejects every
    /// magnitude above `i64::MAX`, so the fold below cannot lose a value. Not
    /// folding used to make `-1` a `Result` and therefore unusable anywhere
    /// an `int` was required — `let x: int = -1` failed to typecheck.
    ///
    /// `checked_neg` keeps the one genuinely-overflowing case (`-(i64::MIN)`,
    /// reachable only by double negation) as a checked `Unary`, so this is
    /// total: it never panics and never silently wraps.
    /// Implements [ARITH-NEG-LITERAL].
    #[must_use]
    pub fn negated(operand: Self) -> Self {
        match operand {
            Self::Integer(value) => match value.checked_neg() {
                Some(negated) => Self::Integer(negated),
                None => Self::unary_minus(Self::Integer(value)),
            },
            Self::Float(value) => Self::Float(-value),
            other => Self::unary_minus(other),
        }
    }

    /// The unfolded `-operand` node.
    fn unary_minus(operand: Self) -> Self {
        Self::Unary {
            op: String::from("-"),
            operand: Box::new(operand),
        }
    }
}

/// One arm of a `handle ... in` expression (`ast` handler arm).
#[derive(Debug, Clone, PartialEq)]
pub struct HandlerArm {
    /// The handled operation name.
    pub operation: String,
    /// The operation parameter names bound in the body.
    pub params: Vec<String>,
    /// The arm body.
    pub body: Expr,
    /// Source position of the operation name. Implements
    /// [LSP-IMPLEMENTATIONS-EFFECT-HANDLERS].
    pub position: Option<Position>,
}
