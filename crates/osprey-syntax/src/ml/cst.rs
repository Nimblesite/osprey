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

use osprey_ast::{Multiplicity, OperationMode, Position, Stage};

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
        /// Optional open remainder (`![Log | e]`).
        effect_tail: Option<String>,
        /// Whether `!` was written, including `![]`.
        effect_row_present: bool,
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
    /// on for curried closures and exact `Result` preservation).
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
        /// Optional open remainder (`![Log | e]`).
        effect_tail: Option<String>,
        /// Whether `!` was written, including `![]`.
        effect_row_present: bool,
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
        /// Whether the effect is answered by a compile-time rewrite
        /// (`static effect`) or through the runtime handler stack. Implements
        /// [STAGE-DECL].
        stage: Stage,
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
    /// A `//!` documentation comment's raw text. Unlike [`MlItem::Doc`] it
    /// pairs with nothing that follows: the lowerer hands it to whichever
    /// scope contains it — the file, a namespace or a module
    /// ([DOC-SIGIL-INNER]). The position blames a `//!` written where no scope
    /// can take it.
    InnerDoc {
        /// Raw doc text, sigil stripped.
        text: String,
        /// Source position of the `//!`.
        pos: Position,
    },
}

mod types;
pub(crate) use types::*;
mod expressions;
pub(crate) use expressions::*;
