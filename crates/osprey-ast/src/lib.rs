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

/// A statement: every top-level declaration and binding form, plus bare
/// expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `import a.b.c` — a dotted module path.
    Import {
        /// The module path segments.
        module: Vec<String>,
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
    /// A `module` block grouping nested statements.
    Module {
        /// Module name.
        name: String,
        /// Statements inside the module.
        body: Vec<Stmt>,
        /// Structured documentation comment, when written ([DOC-MODEL]).
        doc: Option<DocComment>,
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
