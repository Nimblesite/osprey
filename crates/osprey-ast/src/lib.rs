//! The Osprey abstract syntax tree.
//!
//! Two enums (`Stmt`, `Expr`) with struct-like variants model every statement and
//! expression form in the language. Keeping the tree to two exhaustively-matched
//! enums means the type checker and codegen get compiler-enforced totality for
//! free: adding a variant breaks every consumer until it handles the new form.

mod doc;
mod generics;
mod resume;
mod visit;
pub use doc::{DocComment, DocExample, DocScope};
pub use generics::{EffectRef, TypeParam, Variance};
pub use resume::contains_resume;
pub use visit::walk_each;

/// A source position: 1-based line, 0-based column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    /// 1-based source line.
    pub line: u32,
    /// 0-based column within the line.
    pub column: u32,
}

/// A canonical namespace/module/member path (`Tax::Rates::standard`).
///
/// Paths are structured at the flavor boundary so later phases never have to
/// split source spelling or confuse qualification with value member access.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SymbolPath {
    /// Path segments in source order. A well-formed source path is non-empty;
    /// the empty value is useful for namespace-only imports.
    pub segments: Vec<String>,
}

impl SymbolPath {
    /// Build a path from its ordered segments.
    #[must_use]
    pub fn new(segments: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            segments: segments.into_iter().map(Into::into).collect(),
        }
    }

    /// Build a one-segment path.
    #[must_use]
    pub fn single(segment: impl Into<String>) -> Self {
        Self {
            segments: vec![segment.into()],
        }
    }

    /// Whether this path contains no segments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// The final segment, when present.
    #[must_use]
    pub fn last(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }
}

impl std::fmt::Display for SymbolPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.segments.join("::"))
    }
}

/// A logical namespace label. Quotedness is preserved because quoted labels
/// (notably slash labels) require an import alias before qualified use.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NamespaceName {
    /// An ordinary identifier label (`billing`).
    Identifier(String),
    /// A quoted opaque label (`"billing/api"`). The stored value is unquoted.
    Quoted(String),
}

impl NamespaceName {
    /// The semantic label without source quotes.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Identifier(label) | Self::Quoted(label) => label,
        }
    }

    /// Whether the source used the quoted namespace form.
    #[must_use]
    pub const fn is_quoted(&self) -> bool {
        matches!(self, Self::Quoted(_))
    }
}

/// The namespace plus optional nested path named by an import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportTarget {
    /// Logical namespace label (never a physical file path).
    pub namespace: NamespaceName,
    /// Module path inside the namespace; empty for a namespace-only import.
    pub path: SymbolPath,
}

/// One explicitly imported member and its optional local alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportMember {
    /// Exported member name.
    pub name: String,
    /// Local name introduced by `as`, when present.
    pub alias: Option<String>,
}

/// The surface selected from an import target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportSelection {
    /// Import the target namespace/module as a whole.
    Whole,
    /// Import only the listed exported members.
    Members(Vec<ImportMember>),
    /// Import every exported member (policy-checked in project mode).
    Wildcard,
}

/// A canonical import edge as contributed by one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDecl {
    /// Namespace/module being imported.
    pub target: ImportTarget,
    /// Alias for the whole target (`import billing::Tax as T`).
    pub alias: Option<String>,
    /// Whole target, explicit member list, or wildcard.
    pub selection: ImportSelection,
    /// Source position of the `import` keyword.
    pub position: Option<Position>,
}

/// Whether a module owns ordinary declarations or durable private state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleKind {
    /// A closed, stateless-by-default module boundary.
    Plain,
    /// A declared owner of durable private mutable state.
    State,
}

/// Visibility of a declaration inside a closed module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Visible only inside the owning module.
    Private,
    /// Part of the module's public surface.
    Exported,
}

/// A module/signature ascription (`: StoreSig` / `: StoreSig + extra`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureAscription {
    /// Signature path being ascribed.
    pub path: SymbolPath,
    /// Whether exports beyond the signature are explicitly allowed.
    pub allow_extra: bool,
}

/// A parsed program: the sequence of top-level statements.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    /// Top-level statements in source order.
    pub statements: Vec<Stmt>,
}

/// A type expression — `Result<Int, Error>`, `[String]`, `fn(Int) -> Bool`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeExpr {
    /// The head type name (`Result`, `Int`, the array/function marker aside).
    pub name: String,
    /// Generic arguments, e.g. `Int`/`Error` in `Result<Int, Error>`.
    pub generic_params: Vec<TypeExpr>,
    /// Whether this is an array type `[T]`.
    pub is_array: bool,
    /// The element type when [`is_array`](Self::is_array) is set.
    pub array_element: Option<Box<TypeExpr>>,
    /// Whether this is a function type `fn(...) -> R`.
    pub is_function: bool,
    /// Parameter types when [`is_function`](Self::is_function) is set.
    pub parameter_types: Vec<TypeExpr>,
    /// Return type when [`is_function`](Self::is_function) is set.
    pub return_type: Option<Box<TypeExpr>>,
    /// Source position, when the parser recorded one.
    pub position: Option<Position>,
}

impl TypeExpr {
    /// A bare named type like `Int` or `Ptr`.
    pub fn named(name: impl Into<String>) -> Self {
        TypeExpr {
            name: name.into(),
            generic_params: Vec::new(),
            is_array: false,
            array_element: None,
            is_function: false,
            parameter_types: Vec::new(),
            return_type: None,
            position: None,
        }
    }
}

/// A function parameter with an optional type annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    /// Parameter name.
    pub name: String,
    /// Declared type, if annotated (otherwise inferred).
    pub ty: Option<TypeExpr>,
}

/// An `extern fn` parameter — type annotation required.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternParameter {
    /// Parameter name.
    pub name: String,
    /// Declared type (mandatory for externs).
    pub ty: TypeExpr,
}

/// A variant of a union type.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeVariant {
    /// Variant constructor name.
    pub name: String,
    /// Declared fields, in layout order.
    pub fields: Vec<TypeField>,
}

/// The declared field name of positional payload slot `index`
/// ([TYPE-UNION-POSITIONAL]). A decimal string is not a valid identifier in
/// either flavor, so a positionally-declared payload can never be reached by
/// name — `t.0` does not parse — and a generated slot name can never collide
/// with a user-written field. This is the single definition of the encoding;
/// [`is_positional_field`] is its inverse.
#[must_use]
pub fn positional_field_name(index: usize) -> String {
    index.to_string()
}

/// Whether a declared field name was generated by [`positional_field_name`],
/// i.e. the owning variant declared a positional payload and its binders are
/// resolved by slot rather than by name.
#[must_use]
pub fn is_positional_field(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit())
}

/// A compiler-generated binding name that no source can spell: `$` is absent
/// from the Osprey identifier alphabet in both flavors, and is legal in an
/// unquoted LLVM identifier, so the name survives codegen unescaped. `role`
/// separates generators; `index` separates slots within one head.
#[must_use]
pub fn generated_name(role: &str, index: usize) -> String {
    format!("${role}{index}")
}

/// The generated name of an ignored `_` parameter in slot `index`
/// ([PARAM-WILDCARD]) — unspellable, so repeated `_`s in one head cannot
/// collide and none of them is referenceable from the body.
#[must_use]
pub fn wildcard_param_name(index: usize) -> String {
    generated_name("wild", index)
}

/// The generated scrutinee name of an equational clause set's column `index`,
/// used when no clause in the set spells that column with a plain identifier
/// ([FLAVOR-ML-CLAUSES]).
#[must_use]
pub fn clause_param_name(index: usize) -> String {
    generated_name("arg", index)
}

/// A field within a record/variant, with an optional `where` constraint.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeField {
    /// Field name.
    pub name: String,
    /// Field type as written (`int`, `string`, `Point`, …).
    pub ty: String,
    /// An optional `where` validation expression over the field.
    pub constraint: Option<Box<Expr>>,
}

/// An operation declared inside an `effect` block.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectOperation {
    /// Operation name.
    pub name: String,
    /// The operation's written function type (`fn(T) -> R`).
    pub ty: String,
    /// Parsed parameters of the operation.
    pub parameters: Vec<Parameter>,
    /// The operation's return type as written.
    pub return_type: String,
}

/// Whether a signature type is abstract or exposes a manifest representation.
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureType {
    /// No representation is visible to clients.
    Abstract,
    /// The public representation is the given type expression.
    Manifest(TypeExpr),
}

/// One typed item in an explicit module signature.
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureItem {
    /// An immutable exported value (`let empty: Store`).
    Value {
        /// Exported value name.
        name: String,
        /// Declared value type.
        ty: TypeExpr,
        /// Source position of the item.
        position: Option<Position>,
    },
    /// An exported function contract.
    Function {
        /// Exported function name.
        name: String,
        /// Declared generic binders.
        type_params: Vec<TypeParam>,
        /// Parameter types in call order (parameter names are not contractual).
        parameters: Vec<TypeExpr>,
        /// Declared result type.
        return_type: TypeExpr,
        /// Declared effect row.
        effects: Vec<EffectRef>,
        /// Source position of the item.
        position: Option<Position>,
    },
    /// An abstract or manifest exported type contract.
    Type {
        /// Exported type name.
        name: String,
        /// Declared generic binders.
        type_params: Vec<TypeParam>,
        /// Abstract or manifest representation.
        definition: SignatureType,
        /// Whether the representation stays opaque outside the module even
        /// when the implementation supplies a manifest type.
        opaque: bool,
        /// Source position of the item.
        position: Option<Position>,
    },
    /// An exported algebraic-effect contract.
    Effect {
        /// Exported effect name.
        name: String,
        /// Declared generic binders.
        type_params: Vec<TypeParam>,
        /// Operation contracts.
        operations: Vec<EffectOperation>,
        /// Source position of the item.
        position: Option<Position>,
    },
    /// A nested module constrained by another signature.
    Module {
        /// Nested module path.
        path: SymbolPath,
        /// Required signature and optional-extra policy.
        signature: SignatureAscription,
        /// Source position of the item.
        position: Option<Position>,
    },
}

/// A declaration inside a closed module, with its public-surface metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleItem {
    /// Private by default; `export` makes the item public.
    pub visibility: Visibility,
    /// Representation-hiding marker for exported type declarations.
    pub opaque: bool,
    /// The underlying declaration.
    pub declaration: Box<Stmt>,
}

/// A statement: every top-level declaration and binding form, plus bare
/// expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// An import edge to a logical namespace/module surface.
    Import(ImportDecl),
    /// A block-scoped or file-scoped logical namespace contribution.
    Namespace {
        /// Opaque logical namespace label.
        name: NamespaceName,
        /// Statements contributed to the namespace. For a file-scoped
        /// declaration this contains the declarations following the semicolon.
        body: Vec<Stmt>,
        /// `true` for `namespace name;`, `false` for a brace block.
        file_scoped: bool,
        /// Source position of the `namespace` keyword.
        position: Option<Position>,
    },
    /// `let`/`mut` binding.
    Let {
        /// Bound name.
        name: String,
        /// Whether declared `mut`.
        mutable: bool,
        /// Declared type, if annotated.
        ty: Option<TypeExpr>,
        /// The bound value expression.
        value: Expr,
        /// Structured documentation comment, when written ([DOC-MODEL]).
        doc: Option<DocComment>,
        /// Source position, if recorded.
        position: Option<Position>,
    },
    /// Reassignment of a `mut` binding.
    Assignment {
        /// Target name.
        name: String,
        /// The new value expression.
        value: Expr,
        /// Source position, if recorded.
        position: Option<Position>,
    },
    /// A function definition.
    Function {
        /// Function name.
        name: String,
        /// Declared type parameters (`fn map<T, U>`). Implements
        /// [TYPE-GENERICS-FN].
        type_params: Vec<TypeParam>,
        /// Declared parameters.
        parameters: Vec<Parameter>,
        /// Declared return type, if annotated.
        return_type: Option<TypeExpr>,
        /// Declared effect row (`!Effect` / `![State<int>, Log]` annotations).
        effects: Vec<EffectRef>,
        /// Function body expression.
        body: Expr,
        /// Structured documentation comment, when written ([DOC-MODEL]).
        doc: Option<DocComment>,
        /// Source position, if recorded.
        position: Option<Position>,
    },
    /// An `extern fn` declaration (FFI).
    Extern {
        /// External symbol name.
        name: String,
        /// Declared parameters (all typed).
        parameters: Vec<ExternParameter>,
        /// Declared return type, if any.
        return_type: Option<TypeExpr>,
        /// Structured documentation comment, when written ([DOC-MODEL]).
        doc: Option<DocComment>,
        /// Source position, if recorded.
        position: Option<Position>,
    },
    /// A record/union `type` declaration.
    Type {
        /// Type name.
        name: String,
        /// Generic type parameters, with declared variance.
        type_params: Vec<TypeParam>,
        /// Variants (one for a record, many for a union).
        variants: Vec<TypeVariant>,
        /// Manifest alias representation (`type UserId = int`), when this is a
        /// type alias rather than a record/union declaration.
        alias: Option<TypeExpr>,
        /// An optional validation function name (`where`-constrained type).
        validation_func: Option<String>,
        /// Structured documentation comment, when written ([DOC-MODEL]).
        doc: Option<DocComment>,
        /// Source position, if recorded.
        position: Option<Position>,
    },
    /// An `effect` declaration listing its operations.
    Effect {
        /// Effect name.
        name: String,
        /// Declared type parameters (`effect State<T>`). Implements
        /// [EFFECTS-GENERIC-DECL].
        type_params: Vec<TypeParam>,
        /// Declared operations.
        operations: Vec<EffectOperation>,
        /// Structured documentation comment, when written ([DOC-MODEL]).
        doc: Option<DocComment>,
        /// Source position, if recorded.
        position: Option<Position>,
    },
    /// A closed `module` / `state module` implementation boundary.
    Module {
        /// Possibly nested module path.
        path: SymbolPath,
        /// Plain module or durable-state owner.
        kind: ModuleKind,
        /// Explicit signature ascription, when present.
        signature: Option<SignatureAscription>,
        /// Declarations and their module-local visibility metadata.
        body: Vec<ModuleItem>,
        /// Structured documentation comment, when written ([DOC-MODEL]).
        doc: Option<DocComment>,
        /// Source position of the `module` keyword (or leading `state`).
        position: Option<Position>,
    },
    /// An explicit interface contract for modules.
    Signature {
        /// Signature name.
        name: String,
        /// Typed public-surface items.
        items: Vec<SignatureItem>,
        /// Structured documentation comment, when written ([DOC-MODEL]).
        doc: Option<DocComment>,
        /// Source position of the `signature` keyword.
        position: Option<Position>,
    },
    /// A bare expression statement.
    Expr {
        /// The expression being evaluated for side effects.
        value: Expr,
        /// Source position, if recorded.
        position: Option<Position>,
    },
}

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

/// A pattern in a match/select arm.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `_` — matches anything, binds nothing.
    Wildcard,
    /// A literal pattern (int/float/string/bool).
    Literal(Box<Expr>),
    /// `Ctor` / `Ctor { a, b }` / `Ctor(p, ...)` constructor & destructuring forms.
    Constructor {
        /// Constructor name.
        name: String,
        /// Bound field names (`{ a, b }` form).
        fields: Vec<String>,
        /// Positional sub-patterns (`Ctor(p, ...)` form).
        sub_patterns: Vec<Pattern>,
    },
    /// `value: Int` type-annotated binding.
    TypeAnnotated {
        /// Bound name.
        name: String,
        /// The annotated type.
        ty: TypeExpr,
    },
    /// `{ name, age }` anonymous structural.
    Structural {
        /// Bound field names.
        fields: Vec<String>,
    },
    /// `[]` / `[a, b]` / `[head, ...tail]` list destructuring. `elements` are the
    /// fixed-prefix position patterns; `rest` is the optional tail binder name
    /// (`...rest`), `None` for a fixed-length match. Implements
    /// [TYPE-LIST-PATTERNS].
    List {
        /// Patterns for the fixed-prefix element positions.
        elements: Vec<Pattern>,
        /// Tail rest-binder name, or `None` for a fixed-length pattern.
        rest: Option<String>,
    },
    /// A bare identifier capture.
    Binding(String),
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
    /// `[a, b, c]` list literal.
    List(Vec<Expr>),
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
    },
    /// A unary operation.
    Unary {
        /// Operator spelling (`-`, `!`, `not`).
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
    /// A `select { ... }` over channel arms.
    Select {
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
        /// Handled effect name.
        effect: String,
        /// Per-operation handler arms.
        arms: Vec<HandlerArm>,
        /// The handled body expression.
        body: Box<Expr>,
        /// Source position — the key under which inference publishes this
        /// handler's instantiated operation signatures. Implements
        /// [EFFECTS-GENERIC-INSTANTIATION].
        position: Option<Position>,
    },
    /// `resume(value)` — resume the performer's delimited continuation with
    /// `value` (or `Unit` when absent). Legal only inside a handler arm body.
    /// Implements [EFFECTS-RESUME].
    Resume(Option<Box<Expr>>),
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
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_small_program() {
        let p = Program {
            statements: vec![Stmt::Let {
                name: "x".into(),
                mutable: false,
                ty: None,
                value: Expr::Integer(1),
                doc: None,
                position: None,
            }],
        };
        assert_eq!(p.statements.len(), 1);
        match &p.statements[0] {
            Stmt::Let { name, value, .. } => {
                assert_eq!(name, "x");
                assert_eq!(*value, Expr::Integer(1));
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn named_type_helper() {
        let t = TypeExpr::named("Ptr");
        assert_eq!(t.name, "Ptr");
        assert!(!t.is_function);
    }
}
