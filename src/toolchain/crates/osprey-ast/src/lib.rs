//! The Osprey abstract syntax tree.
//!
//! Two enums (`Stmt`, `Expr`) with struct-like variants model every statement and
//! expression form in the language. Keeping the tree to two exhaustively-matched
//! enums means the type checker and codegen get compiler-enforced totality for
//! free: adding a variant breaks every consumer until it handles the new form.

pub mod canonical;
mod doc;
mod documented;
pub mod effect_name;
mod expr;
pub mod freevars;
mod generics;
mod handler_value;
pub mod json;
mod kernel;
mod lower_static;
pub mod multiplicity;
pub mod mutate;
mod pattern;
pub mod positional;
mod resume;
#[cfg(test)]
mod resume_tests;
mod signature;
pub mod span;
mod stack;
pub mod stage;
mod stage_rows;
mod stmt;
pub mod stmt_meta;
pub mod symbol;
mod visit;
pub use doc::{DocComment, DocExample, DocScope};
pub use documented::Documented;
pub use expr::{
    Expr, FieldAssignment, HandlerArm, InterpolatedPart, MapEntry, MatchArm, NamedArgument,
    SelectOperation,
};
pub use generics::{EffectRef, TypeParam, Variance};
pub use handler_value::{handler_function, handler_value, Clauses};
pub use multiplicity::{
    Multiplicity, OperationMode, OperationTable, CONTROL_KEYWORD, REPLAYABLE_KEYWORD,
};
pub use pattern::Pattern;
pub use resume::resumes_on_one_path;
pub use signature::{EffectOperation, SignatureItem, SignatureType};
pub use stack::with_stack;
pub use stage::{Stage, STATIC_STAGE_KEYWORD};
pub use stmt::{ModuleItem, Stmt};
pub use visit::{walk_each, walk_program, walk_statement, AstNode, AstVisitor};

/// The one wording for an entry conflict [MODULES-ENTRYPOINT]. Two phases can
/// reach it — the type checker for a plain source, the project assembler for a
/// namespaced one — and a reader must not be able to tell which one spoke, so
/// the text lives here, in the crate both already depend on.
pub const ENTRY_CONFLICT: &str =
    "a top-level executable statement cannot sit beside `main`: `main` is the \
     program entry, so move the statement into it (a file-scope `let` or `mut` \
     is a declaration and stays where it is)";

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
    /// The file's own `//!` documentation, when written. Inner scope: it
    /// documents the file that contains it, not the first declaration in it
    /// ([DOC-SIGIL-INNER]).
    pub doc: Option<DocComment>,
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
    /// A module contract supplies a type without adding a written annotation.
    /// Line zero is reserved for compiler-generated source metadata.
    #[must_use]
    pub fn as_contract_annotation(&self) -> Self {
        let mut ty = self.clone();
        ty.position = Some(Position { line: 0, column: 0 });
        ty
    }

    /// Whether this annotation was supplied by module-signature elaboration.
    #[must_use]
    pub fn is_from_contract(&self) -> bool {
        self.position.is_some_and(|position| position.line == 0)
    }

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
    /// Lowering inserted a leading self-alias to check an additional written
    /// inline type against a standalone signature. That alias is part of this
    /// source parameter, not a separately written local binding.
    pub inline_constraint: bool,
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

pub use symbol::{is_constructor_name, is_record_constructor};

/// The expression a brace head denotes: an uppercase or qualified name
/// constructs a type carrying its fields, a lowercase one updates the record it
/// names ([TYPE-RECORD-UPDATE]). One definition for both flavors, so the
/// checker and the backend never see an update dressed as a constructor.
#[must_use]
pub fn record_expression(
    name: String,
    type_args: Vec<TypeExpr>,
    fields: Vec<FieldAssignment>,
) -> Expr {
    if is_record_constructor(&name, !type_args.is_empty()) {
        return Expr::TypeConstructor {
            name,
            type_args,
            fields,
        };
    }
    Expr::Update {
        record: name,
        fields,
    }
}

/// The canonical [`Pattern::Structural`] a tuple pattern `(a, b)` lowers to:
/// slot *i* is the decimal field [`positional_field_name`], its binder the
/// written name (empty for `_`). One definition for both flavors, so ML
/// `(a, b)` and Default `(a, b)` produce byte-identical AST
/// ([PATTERN-TUPLE], [FLAVOR-ML-TUPLE]).
#[must_use]
pub fn tuple_pattern(binders: Vec<String>) -> Pattern {
    Pattern::Structural {
        fields: binders
            .into_iter()
            .enumerate()
            .map(|(i, binder)| (positional_field_name(i), binder))
            .collect(),
        open: false,
    }
}

/// A compiler-generated binding name that no source can spell: `$` is absent
/// from the Osprey identifier alphabet in both flavors, and is legal in an
/// unquoted LLVM identifier, so the name survives codegen unescaped. `role`
/// separates generators; `index` separates slots within one head.
#[must_use]
pub fn generated_name(role: &str, index: usize) -> String {
    format!("${role}{index}")
}

/// The compiler-declared arithmetic effect, in scope in every program and
/// never redeclarable. Implements [ARITH-EFFECT-OPS].
pub const ARITH_EFFECT: &str = "Arith";

/// The generated payload binder of the `Success` arm that `result ?: fallback`
/// desugars to ([PATTERN-RESULT-DEFAULT]). It is named here, rather than inside
/// the desugarer that emits it, because the type checker needs to tell that
/// match apart from a hand-written one: an ordinary `Success` arm may auto-wrap
/// a plain scrutinee, whereas `?:` may not — the spec requires a `Result` on its
/// left and forbids reinterpreting a plain value as `Success`.
pub const RESULT_DEFAULT_PAYLOAD: &str = "$__osprey_result_default";

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
    /// The closed effect row written on a function-typed field
    /// (`edit: fn(string) -> A ![]`), or `None` when the row is left to
    /// inference. A construction must satisfy it and a call through the field
    /// trusts it. Implements [EFFECTS-FIELD-ROWS].
    pub effects: Option<Vec<EffectRef>>,
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
            doc: None,
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

    #[test]
    fn symbol_paths_report_emptiness() {
        assert!(SymbolPath::new(Vec::<String>::new()).is_empty());
        assert!(!SymbolPath::single("Bank").is_empty());
    }

    /// [TYPE-UNION-POSITIONAL]: slot names round-trip through their inverse and
    /// can never be confused with a user-written field name.
    #[test]
    fn positional_slot_names_round_trip() {
        assert_eq!(positional_field_name(0), "0");
        assert_eq!(positional_field_name(12), "12");
        assert!(is_positional_field(&positional_field_name(7)));
        assert!(!is_positional_field(""));
        assert!(!is_positional_field("left"));
        assert!(!is_positional_field("0a"));
    }

    /// [PARAM-WILDCARD], [FLAVOR-ML-CLAUSES]: generated binders are unspellable
    /// in source and separated by role, so two generators never collide.
    #[test]
    fn generated_names_are_unspellable_and_role_separated() {
        assert_eq!(generated_name("tmp", 3), "$tmp3");
        assert_eq!(wildcard_param_name(1), "$wild1");
        assert_eq!(clause_param_name(1), "$arg1");
        assert_ne!(wildcard_param_name(0), clause_param_name(0));
        assert!(!is_positional_field(&clause_param_name(0)));
    }
}
