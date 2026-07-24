//! The ML **concrete syntax tree**: a faithful record of the ML surface, with
//! no canonicalisation applied. Currying is still a flat parameter/argument
//! list, pipes are still binary operators, parentheses are still present, and a
//! record literal is still its own node. The CST→AST lowering ([`super::lower`])
//! is the *only* place these are normalised into the canonical
//! [`osprey_ast`] — keeping parse and lower cleanly separated
//! ([FLAVOR-FRONTEND], docs/specs/0023-LanguageFlavors.md).
//!
//! This separation is deliberate: the parser ([`super::parser`]) decides only
//! *what was written*; the lowerer decides *what it means*. Nothing in this
//! module references `osprey_ast`.

use osprey_ast::Position;

/// A source-level namespace/module/member path. Segments are kept separate so
/// qualification can never be confused with value-level `.` access
/// ([MODULES-MODEL], [MODULES-ABI]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MlSymbolPath {
    /// Path segments in source order; a valid path is never empty.
    pub segments: Vec<String>,
}

/// The first, logical namespace component of a namespace/import declaration.
/// Quoted slash labels are opaque strings, not path hierarchies
/// ([MODULES-NAMESPACE], [MODULES-PATH-INDEPENDENCE]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MlNamespaceName {
    /// An ordinary identifier label such as `billing`.
    Ident(String),
    /// An opaque quoted label such as `"billing/api"`.
    Quoted(String),
}

/// The selection made by one import ([MODULES-IMPORT]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MlImportSelection {
    /// Import the target namespace/module itself.
    Whole,
    /// Import an explicit layout list of exported members.
    Members(Vec<MlImportMember>),
    /// Import every exported member (policy-controlled escape hatch).
    Wildcard,
}

/// One member inside an explicit layout import list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MlImportMember {
    /// Exported member name at the target.
    pub name: String,
    /// Optional local alias after `as`.
    pub alias: Option<String>,
}

/// A parsed logical import target and its local projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MlImport {
    /// Logical namespace label, independent of a physical file path.
    pub namespace: MlNamespaceName,
    /// Module path below the namespace (possibly empty for a namespace import).
    pub path: MlSymbolPath,
    /// Optional alias for a whole import.
    pub alias: Option<String>,
    /// Whole target, selected members, or wildcard.
    pub selection: MlImportSelection,
}

/// Whether a module is a plain abstraction boundary or a state owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MlModuleKind {
    /// A stateless plain module.
    Plain,
    /// A `state Name` module owning private mutable cells.
    State,
}

/// One public requirement in a named module signature
/// ([MODULES-SIGNATURE]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MlSignatureItem {
    /// A value/function type and optional effect row.
    Value {
        /// Required exported name.
        name: String,
        /// Declared generic binders.
        type_params: Vec<MlTypeParam>,
        /// Required value/function type.
        ty: MlType,
        /// Required effect row.
        effects: Vec<MlEffectRef>,
        /// Source position of the name.
        pos: Position,
    },
    /// An abstract (`type T`) or manifest (`type T = R`) type requirement.
    Type {
        /// Required type name.
        name: String,
        /// `None` for an abstract type; `Some` for a manifest representation.
        manifest: Option<MlType>,
        /// Source position of the `type` keyword.
        pos: Position,
    },
    /// An effect and its operation interface.
    Effect {
        /// Required effect name.
        name: String,
        /// Declared generic binders.
        type_params: Vec<MlTypeParam>,
        /// Required operations.
        operations: Vec<MlEffectOp>,
        /// Source position of the `effect` keyword.
        pos: Position,
    },
    /// A nested module requirement ascribed to another named signature.
    Module {
        /// Required nested module name.
        name: String,
        /// Signature the nested module must satisfy.
        signature: MlSymbolPath,
        /// Source position of the `module` keyword.
        pos: Position,
    },
}

/// A top-level item or a statement inside a layout block.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MlItem {
    /// A logical namespace/module import ([MODULES-IMPORT]).
    Import {
        /// Parsed target, alias, and member projection.
        import: MlImport,
        /// Source position of the `import` keyword.
        pos: Position,
    },
    /// A logical namespace contribution. `body: None` is file-scoped; `Some`
    /// is an indented block contribution ([MODULES-FILE-SCOPED-NAMESPACE]).
    Namespace {
        /// Logical namespace label.
        name: MlNamespaceName,
        /// Optional indented contribution body.
        body: Option<Vec<MlItem>>,
        /// Source position of the `namespace` keyword.
        pos: Position,
    },
    /// A plain or state module with an optional named signature ascription.
    Module {
        /// Qualified module path.
        path: MlSymbolPath,
        /// Plain versus state-owning module.
        kind: MlModuleKind,
        /// Optional named interface controlling the complete export surface.
        signature: Option<MlSymbolPath>,
        /// Private-by-default implementation items.
        body: Vec<MlItem>,
        /// Source position of the module/state keyword.
        pos: Position,
    },
    /// A named module signature declaration.
    ModuleSignature {
        /// Signature name.
        name: String,
        /// Public requirements in source order.
        items: Vec<MlSignatureItem>,
        /// Source position of the `signature` keyword.
        pos: Position,
    },
    /// Exactly one explicitly exported declaration group. An exported value
    /// signature propagates to its immediately-following bare definition.
    Export {
        /// Declaration carrying explicit public visibility.
        item: Box<MlItem>,
        /// Source position of the `export` keyword.
        pos: Position,
    },
    /// An explicitly opaque type declaration. This wrapper is valid only under
    /// one [`MlItem::Export`] in an un-ascribed module; named signatures express
    /// abstraction with a bare `type T` requirement instead.
    Opaque {
        /// Wrapped type declaration.
        item: Box<MlItem>,
        /// Source position of the `opaque` keyword.
        pos: Position,
    },
    /// `mut? name param* = body`. Zero params ⇒ a value binding; one or more
    /// (including the unit marker) ⇒ a function definition. Currying is not yet
    /// applied — `params` is the flat surface list; `uncurried` records *which*
    /// surface form wrote it ([FLAVOR-ML-CURRY]).
    Binding {
        /// Whether `mut` introduced the binding.
        mutable: bool,
        /// The bound name.
        name: String,
        /// The surface parameter list (empty for a value binding).
        params: Vec<MlParam>,
        /// `true` when the head was the parenthesised comma-list `f (x, y)`
        /// (uncurried → flat multi-parameter `Function`); `false` for the
        /// juxtaposed `f x y` (curried → one-param `Function` returning a
        /// `Lambda` chain). Irrelevant for zero/one parameter, where both forms
        /// lower identically.
        uncurried: bool,
        /// The right-hand side.
        body: MlExpr,
        /// Source position of the name.
        pos: Position,
    },
    /// `name := value` — mutation of an existing binding.
    Assign {
        /// The mutated name.
        name: String,
        /// The new value.
        value: MlExpr,
        /// Source position of the name.
        pos: Position,
    },
    /// `name : type` — a standalone type signature, paired with the binding of
    /// the same name that follows it. Kept in the CST so the lowerer can apply
    /// concrete parameter/return types (which the type checker and codegen rely
    /// on for curried closures and `Result` auto-unwrap).
    ValueSignature {
        /// The signed name.
        name: String,
        /// Declared type parameters from a `name<T, U> :` binder, in order
        /// ([FLAVOR-ML-GENERICS], [TYPE-GENERICS-FN]).
        type_params: Vec<MlTypeParam>,
        /// The declared type.
        ty: MlType,
        /// The effect row from a trailing `! Ref(, Ref)*` (or `! [Ref, …]`),
        /// empty when the signature declares no effects ([FLAVOR-ML-EFFECT]).
        effects: Vec<MlEffectRef>,
        /// Source position of the signed name.
        pos: Position,
    },
    /// `type Name param* =` + an indented layout block of variants
    /// ([FLAVOR-ML-TYPE]). A union/enum lists uppercase constructor variants
    /// (each with an optional indented `field : type` block); a record is the
    /// single-variant form whose lines are lowercase `field : type` — the lowerer
    /// gives that variant the type's own name, matching the Default record shape.
    Type {
        /// The type's name.
        name: String,
        /// Type parameters between the name and `=` (e.g. `T`, `out T`,
        /// `in T`), in order ([TYPE-VARIANCE-DECL]).
        type_params: Vec<MlTypeParam>,
        /// The declared variants (one per constructor; a record has exactly one).
        variants: Vec<MlVariant>,
        /// A direct manifest alias (`type UserId = int`) instead of a
        /// variant/record body.
        alias: Option<MlType>,
        /// Source position of the `type` keyword.
        pos: Position,
    },
    /// `extern name (pname : ptype)* -> rettype` — an external (FFI) function
    /// declaration ([FLAVOR-ML-EXTERN]). Each parameter is a parenthesised
    /// `name : type`; the trailing `-> type` is the return type.
    Extern {
        /// The external symbol name.
        name: String,
        /// The typed parameters, in declaration order.
        params: Vec<MlExternParam>,
        /// The declared return type, if any.
        return_type: Option<MlType>,
        /// Source position of the `extern` keyword.
        pos: Position,
    },
    /// `effect Name` + an indented block of `op : P => R` operation lines — an
    /// algebraic effect declaration ([FLAVOR-ML-EFFECT]).
    Effect {
        /// The effect name.
        name: String,
        /// Type parameters between the name and the operation block (e.g. `T`
        /// in `effect State T`), in order ([EFFECTS-GENERIC-DECL]).
        type_params: Vec<MlTypeParam>,
        /// The declared operations, in order.
        operations: Vec<MlEffectOp>,
        /// Source position of the `effect` keyword.
        pos: Position,
    },
    /// A bare expression evaluated for its effect or trailing value.
    Expr {
        /// The expression.
        value: MlExpr,
        /// Source position.
        pos: Position,
    },
    /// A `(** … *)` documentation comment's raw text, paired by the lowerer
    /// with the declaration that follows it ([DOC-SIGIL-ML]) — the same
    /// pairing pattern as [`MlItem::ValueSignature`].
    Doc(String),
}

/// Declaration-site variance of a type parameter, exactly as written
/// ([TYPE-VARIANCE-DECL]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MlVariance {
    /// Unannotated.
    Invariant,
    /// `out T`.
    Covariant,
    /// `in T`.
    Contravariant,
}

/// One declared type parameter with its optional variance marker.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlTypeParam {
    /// The parameter name.
    pub name: String,
    /// The written variance marker (`Invariant` when unannotated).
    pub variance: MlVariance,
}

/// One effect reference inside an effect row, optionally applied to type
/// arguments (`State<int>`) ([EFFECTS-GENERIC-ROWS]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlEffectRef {
    /// The effect name.
    pub name: String,
    /// The applied type arguments (empty for a bare reference).
    pub args: Vec<MlType>,
    /// Source position of the effect name.
    pub pos: Position,
}

/// A parenthesised `name : type` parameter of an `extern` declaration.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlExternParam {
    /// The parameter name.
    pub name: String,
    /// The parameter's declared type.
    pub ty: MlType,
}

/// One `op : P => R` operation line of an `effect` declaration. The payload and
/// result types are rendered into the canonical `fn(P) -> R` string by the
/// lowerer ([FLAVOR-ML-EFFECT]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlEffectOp {
    /// The operation name.
    pub name: String,
    /// The operation's payload (argument) type.
    pub payload: MlType,
    /// The operation's result type.
    pub result: MlType,
}

/// One variant of a `type` declaration: a constructor name and its payload
/// fields (empty for a bare enum case like `Active`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlVariant {
    /// The constructor name.
    pub name: String,
    /// The payload fields, in declaration order.
    pub fields: Vec<MlTypeField>,
}

/// A `field : type` line inside a variant's payload block.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlTypeField {
    /// The field name.
    pub name: String,
    /// The field's declared type.
    pub ty: MlType,
}

/// An ML type expression. Arrows are right-associative; application binds
/// tighter (`Handler Db`, `Result int string`) ([FLAVOR-ML-FN]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MlType {
    /// A bare type name (`int`, `string`, `Unit`, a user type).
    Name(String),
    /// Type application `head arg…` (`Handler Db`, `Result int string`).
    App {
        /// The head type name.
        head: String,
        /// The applied argument types.
        args: Vec<MlType>,
    },
    /// `a -> b` (right-associative).
    Arrow {
        /// The argument type.
        from: Box<MlType>,
        /// The result type.
        to: Box<MlType>,
    },
    /// `(a, b, …)` a tupled single argument.
    Tuple(Vec<MlType>),
}

/// A surface parameter pattern in a binding or lambda head.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MlParam {
    /// A named parameter, type left to inference / the signature.
    Named(String),
    /// A parenthesised type-annotated parameter `(name : type)` — the inline
    /// form a lambda uses for a load-bearing parameter type ([FLAVOR-ML-FN]).
    Typed(String, MlType),
    /// The unit marker `()` — a zero-argument function boundary, not a value.
    Unit,
    /// A refutable head pattern: the column of an equational clause that
    /// selects on a literal or a constructor ([FLAVOR-ML-CLAUSES]). Never
    /// reaches the lowerer — [`super::clauses::merge`] rewrites every clause
    /// set into a plain parameter list over a `match`.
    Pattern(MlPattern),
}

/// An ML expression, recorded exactly as written.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MlExpr {
    /// Integer literal.
    Int(i64),
    /// Float literal.
    Float(f64),
    /// Boolean literal.
    Bool(bool),
    /// Raw string literal text (quotes/escapes/`${…}` unresolved).
    Str(String),
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
    /// `[ a, b, c ]` list literal (possibly empty).
    List(Vec<MlExpr>),
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
    /// `handle Effect` + indented arms + `in body` — install an effect handler
    /// over the `body` expression ([FLAVOR-ML-EFFECT]).
    Handle {
        /// The handled effect name.
        effect: String,
        /// The per-operation handler arms.
        arms: Vec<MlHandleArm>,
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
    pub params: Vec<String>,
    /// The arm body.
    pub body: MlExpr,
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
        fields: Vec<String>,
    },
    /// A bare lowercase binding.
    Bind(String),
    /// `[ p, … ]` or `[ p, …, ...rest ]` — a list pattern with fixed-prefix
    /// element patterns and an optional trailing `...name` rest-binder
    /// ([FLAVOR-ML-MATCH], [TYPE-LIST-PATTERNS]).
    List {
        /// Patterns for the fixed-prefix element positions.
        elements: Vec<MlPattern>,
        /// The trailing `...name` rest-binder, or `None` for a fixed length.
        rest: Option<String>,
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
