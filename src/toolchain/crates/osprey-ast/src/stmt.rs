//! Statements: every declaration and binding form.

use crate::DocComment;
use crate::EffectOperation;
use crate::EffectRef;
use crate::Expr;
use crate::ExternParameter;
use crate::ImportDecl;
use crate::ModuleKind;
use crate::NamespaceName;
use crate::Parameter;
use crate::Position;
use crate::SignatureAscription;
use crate::SignatureItem;
use crate::Stage;
use crate::SymbolPath;
use crate::TypeExpr;
use crate::TypeParam;
use crate::TypeVariant;
use crate::Visibility;
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
        /// The `///` documenting the namespace from outside ([DOC-MODEL]).
        doc: Option<DocComment>,
        /// The `//!` documenting the namespace from inside its own body. A
        /// namespace can carry both at once — they describe the same scope
        /// from opposite sides ([DOC-SIGIL-INNER]).
        inner_doc: Option<DocComment>,
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
        /// Optional open remainder of the effect row (`![Log | e]`).
        effect_tail: Option<String>,
        /// Whether a row was written, including the closed empty row `![]`.
        effect_row_present: bool,
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
        /// Whether the representation is hidden outside the declaring module
        /// ([MODULES-OPAQUE-TYPES]). Project assembly sets it on the flattened
        /// declaration from the item's `export opaque` marker or an ascribed
        /// signature's abstract type; a parser leaves it false because the
        /// marker lives on the [`ModuleItem`].
        opaque: bool,
        /// Structured documentation comment, when written ([DOC-MODEL]).
        doc: Option<DocComment>,
        /// Source position, if recorded.
        position: Option<Position>,
    },
    /// An `effect` declaration listing its operations.
    Effect {
        /// When the effect's operations are answered: [`Stage::Static`] rewrites
        /// them away before type checking, [`Stage::Dynamic`] (the default, and
        /// every effect written without `static`) dispatches them at runtime.
        /// Implements [STAGE-DECL].
        stage: Stage,
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
        /// The `//!` documenting the module from inside its own body, which
        /// coexists with the outer `doc` above ([DOC-SIGIL-INNER]).
        inner_doc: Option<DocComment>,
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
        /// Structured documentation comment, when written ([DOC-MODEL]). A
        /// `test("name", …)` case is an expression statement, so this is the
        /// field that carries a test's documentation ([TESTING-DOC]).
        doc: Option<DocComment>,
        /// Source position, if recorded.
        position: Option<Position>,
    },
}
