//! The ML **lowerer**: CST ([`super::cst`]) → canonical [`osprey_ast::Program`].
//! This is the *only* place ML surface syntax is normalised into the shared
//! core, and it is where the boundary law is enforced — the output is canonical
//! AST that no later phase can distinguish from Default-flavor output
//! ([FLAVOR-BOUNDARY], [FLAVOR-LAYER], [FLAVOR-LOWER-CONTRACT], docs/specs/0023).
//!
//! What this module canonicalises (and the parser deliberately does not):
//! - **Curry-by-default** ([FLAVOR-ML-CURRY], [FLAVOR-CURRY]): ML curries by
//!   default. A
//!   multi-parameter binding `f a b = …` lowers to a one-parameter
//!   [`Stmt::Function`] whose body is a one-parameter [`Expr::Lambda`] chain,
//!   whitespace application `f a b` to nested single-argument calls
//!   `Call(Call(f, [a]), [b])`, and a lambda `\a b => …` to the same curried
//!   [`Expr::Lambda`] chain — each byte-identical to the Default flavor's
//!   *explicit-curry* `fn f(a) = fn(b) => …`, `f(a)(b)`, and `fn(a) => fn(b) =>
//!   …`. Partial application therefore just works: `f a` is the inner saturated
//!   call returning a function value. The IR-equivalence guarantee
//!   ([FLAVOR-IR-EQUIV]) holds against the Default *explicit-curry* twin; a
//!   saturated curried call may be folded back to a multi-argument call by the
//!   backend, but the lowered AST is always the curried form.
//! - **Pipes**: `x |> f` desugars to a call, exactly as the Default lowerer does.
//! - **Records / blocks / interpolation**: constructors map to
//!   [`Expr::TypeConstructor`], lowercase record updates to [`Expr::Update`],
//!   and layout blocks/interpolation to their canonical expression nodes.

use super::cst::{
    MlArm, MlEffectOp, MlEffectRef, MlExpr, MlExternParam, MlField, MlHandleArm, MlImport,
    MlImportSelection, MlItem, MlModuleKind, MlNamespaceName, MlParam, MlPattern, MlSignatureItem,
    MlSymbolPath, MlType, MlTypeField, MlTypeParam, MlVariance, MlVariant,
};
use crate::strings::{lower_interpolation, unquote};
use osprey_ast::{
    DocComment, DocScope, EffectOperation, EffectRef, Expr, ExternParameter, FieldAssignment,
    HandlerArm, ImportDecl, ImportMember, ImportSelection, ImportTarget, MapEntry, MatchArm,
    ModuleItem, ModuleKind, NamespaceName, Parameter, Pattern, Position, Program,
    SignatureAscription, SignatureItem, Stmt, SymbolPath, TypeExpr, TypeField, TypeParam,
    TypeVariant, Variance,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

thread_local! {
    /// The LEXICAL scopes enclosing the expression being lowered, outermost
    /// first: the file-scope definitions, then one frame per enclosing
    /// parameter list and block.
    ///
    /// A whitespace-application spine whose head is bound in one of these is a
    /// user definition or a parameter, kept CURRIED — nested one-argument
    /// calls — so partial application works ([FLAVOR-ML-CURRY]). Any other head
    /// (a multi-argument builtin or `extern`, which cannot be partially
    /// applied) has its SATURATED spine folded to ONE flat multi-argument call
    /// — the saturated-call optimisation the spec assigns to the backend, done
    /// here while the surface spine is still visible.
    ///
    /// The frames are what make this scope-SAFE. A single program-wide set of
    /// spellings let an unrelated parameter change calls outside its own scope:
    /// declaring `useUnrelated contains = contains` anywhere in the file made
    /// `contains "alpha" "ph"` lower as `contains("alpha")("ph")`, so the
    /// two-argument builtin was called with one argument and the program was
    /// rejected ([FLAVOR-ML-CALL-SATURATED]).
    static SCOPES: RefCell<Vec<HashSet<String>>> = const { RefCell::new(Vec::new()) };

    /// Diagnostics raised while lowering — signatures the canonical AST cannot
    /// represent, which would otherwise be dropped SILENTLY (a written type
    /// vanishing is exactly the miscompile [FLAVOR-ML-FN] forbids). Ambient
    /// because lowering recurses through [`lower_expr`] blocks with no error
    /// channel; [`lower`] clears the sink on entry and drains it on exit.
    static LOWER_ERRORS: RefCell<Vec<crate::SyntaxError>> = const { RefCell::new(Vec::new()) };
}

/// Lower a parsed ML CST into the canonical program. Collects every bound name
/// first so [`lower_application`] can tell a curried user call from a saturated
/// builtin/`extern` call.
pub(crate) fn lower(items: Vec<MlItem>) -> (Program, Vec<crate::SyntaxError>) {
    SCOPES.with(|s| s.borrow_mut().clear());
    LOWER_ERRORS.with(|sink| sink.borrow_mut().clear());
    let mut ctors = HashMap::new();
    collect_positional_ctors(&items, &mut ctors);
    let _positional = crate::positional::install(ctors.into_iter());
    let file_scope = scope_of(&items);
    let mut items = items;
    let file_doc = take_inner_doc(&mut items);
    let program = Program {
        statements: in_scope(file_scope, move || lower_items(items)),
        doc: file_doc,
    };
    (
        program,
        LOWER_ERRORS.with(|sink| sink.borrow_mut().drain(..).collect()),
    )
}

/// Lower a run of items, pairing each type signature with the binding of the
/// same name that immediately follows it. An orphaned signature (no matching
/// binding next) is dropped. Used at top level and inside layout blocks so
/// local signed functions work too.
pub(super) fn lower_items(items: Vec<MlItem>) -> Vec<Stmt> {
    if let Some(index) = items
        .iter()
        .position(|item| matches!(item, MlItem::Namespace { body: None, .. }))
    {
        return lower_file_namespace(items, index);
    }
    ItemLower::default().lower_all(items)
}

/// A file-scoped namespace owns the declarations after its header UP TO the
/// next file-scoped header, which opens a SIBLING namespace rather than a
/// nested one ([MODULES-FILE-SCOPED-NAMESPACE]). Imports before the first
/// header remain file-level edges.
///
/// Handing the whole tail to `lower_items` instead nested every later header
/// inside the first namespace's body, where `osprey_project::contribution` —
/// which walks only TOP-LEVEL `Stmt::Namespace` — filed it as an ordinary
/// declaration: the second namespace never registered and every statement
/// written after its header was dropped, the program still exiting ZERO.
fn lower_file_namespace(mut items: Vec<MlItem>, index: usize) -> Vec<Stmt> {
    let tail = items.split_off(index.saturating_add(1));
    let Some(MlItem::Namespace { name, pos, .. }) = items.pop() else {
        return lower_items(items);
    };
    let (owned, rest) = split_at_next_file_namespace(tail);
    let mut out = lower_items(items);
    out.push(Stmt::Namespace {
        name: lower_namespace_name(name),
        body: lower_items(owned),
        file_scoped: true,
        doc: None,
        inner_doc: None,
        position: Some(pos),
    });
    out.extend(lower_items(rest));
    out
}

/// Split `items` before the next file-scoped namespace header: what the current
/// namespace owns, and what the next header opens (empty when there is none).
fn split_at_next_file_namespace(mut items: Vec<MlItem>) -> (Vec<MlItem>, Vec<MlItem>) {
    let Some(at) = items
        .iter()
        .position(|item| matches!(item, MlItem::Namespace { body: None, .. }))
    else {
        return (items, Vec::new());
    };
    let rest = items.split_off(at);
    (items, rest)
}

/// Stateful pairing of docs/signatures with the declaration they annotate.
#[derive(Default)]
struct ItemLower {
    out: Vec<Stmt>,
    pending: Option<MlSig>,
    pending_doc: Option<DocComment>,
}

impl ItemLower {
    fn lower_all(mut self, items: Vec<MlItem>) -> Vec<Stmt> {
        for item in items {
            self.lower_item(item);
        }
        self.out
    }

    fn lower_item(&mut self, item: MlItem) {
        match item {
            MlItem::Doc(text) => {
                self.pending_doc = Some(crate::docparse::parse_doc(&text, DocScope::Outer));
            }
            // Every scope that can hold an inner doc removed its own with
            // `take_inner_doc` before lowering, so one arriving here sits where
            // nothing encloses it. Reporting beats dropping it: a silently
            // discarded doc reads exactly like one that was never written.
            MlItem::InnerDoc { pos, .. } => {
                lower_error(crate::docparse::misplaced_inner_doc("(** … *)"), pos);
            }
            item @ MlItem::ValueSignature { .. } => self.lower_signature(item),
            item @ MlItem::Binding { .. } => self.lower_binding_item(item),
            item @ (MlItem::Assign { .. }
            | MlItem::Expr { .. }
            | MlItem::Import { .. }
            | MlItem::Export { .. }
            | MlItem::Opaque { .. }) => self.lower_simple(item),
            item @ (MlItem::Type { .. } | MlItem::Extern { .. } | MlItem::Effect { .. }) => {
                self.lower_declaration(item);
            }
            item @ (MlItem::Namespace { .. }
            | MlItem::Module { .. }
            | MlItem::ModuleSignature { .. }) => self.lower_container(item),
        }
    }

    fn lower_signature(&mut self, item: MlItem) {
        if let MlItem::ValueSignature {
            name,
            type_params,
            ty,
            effects,
            effect_tail,
            effect_row_present,
            pos,
        } = item
        {
            self.pending = Some(MlSig {
                name,
                type_params,
                ty,
                effects,
                effect_tail,
                effect_row_present,
                position: pos,
            });
        }
    }

    fn lower_binding_item(&mut self, item: MlItem) {
        let MlItem::Binding {
            mutable,
            name,
            params,
            uncurried,
            body,
            pos,
        } = item
        else {
            return;
        };
        let sig = self
            .pending
            .take()
            .filter(|signature| signature.name == name);
        let stmt = lower_binding(mutable, name, params, uncurried, body, pos, sig);
        self.out.push(attach_doc(stmt, self.pending_doc.take()));
    }

    fn lower_simple(&mut self, item: MlItem) {
        match item {
            MlItem::Assign { name, value, pos } => {
                self.clear_pending();
                self.out.push(Stmt::Assignment {
                    name,
                    value: lower_expr(value),
                    position: Some(pos),
                });
            }
            // A `(** … *)` block preceding a bare expression documents it —
            // the shape a `test "name" case` case takes ([TESTING-DOC]).
            MlItem::Expr { value, pos } => {
                self.pending = None;
                self.out.push(Stmt::Expr {
                    value: super::binding_ranges::with_expression_owner(pos, || lower_expr(value)),
                    doc: self.pending_doc.take(),
                    position: Some(pos),
                });
            }
            MlItem::Import { import, pos } => {
                self.clear_pending();
                self.out.push(Stmt::Import(lower_import(import, pos)));
            }
            MlItem::Export { item, .. } | MlItem::Opaque { item, .. } => {
                self.pending = None;
                self.out.extend(lower_items(vec![*item]));
            }
            _ => {}
        }
    }

    fn lower_declaration(&mut self, item: MlItem) {
        self.pending = None;
        match item {
            MlItem::Type {
                name,
                type_params,
                variants,
                alias,
                pos,
            } => {
                self.out.push(Stmt::Type {
                    name,
                    type_params: type_params.into_iter().map(lower_type_param).collect(),
                    variants: variants.into_iter().map(lower_variant).collect(),
                    alias: alias.as_ref().map(required_type_expr),
                    validation_func: None,
                    opaque: false,
                    doc: self.pending_doc.take(),
                    position: Some(pos),
                });
            }
            MlItem::Extern {
                name,
                params,
                return_type,
                pos,
            } => {
                self.out.push(Stmt::Extern {
                    name,
                    parameters: params.into_iter().map(lower_extern_param).collect(),
                    return_type: return_type.as_ref().and_then(type_expr),
                    doc: self.pending_doc.take(),
                    position: Some(pos),
                });
            }
            MlItem::Effect {
                stage,
                name,
                type_params,
                operations,
                pos,
            } => {
                self.out.push(Stmt::Effect {
                    stage,
                    name,
                    type_params: type_params.into_iter().map(lower_type_param).collect(),
                    operations: operations.into_iter().map(lower_effect_op).collect(),
                    doc: self.pending_doc.take(),
                    position: Some(pos),
                });
            }
            _ => {}
        }
    }

    fn lower_container(&mut self, item: MlItem) {
        self.pending = None;
        match item {
            MlItem::Namespace {
                name,
                body: Some(mut body),
                pos,
            } => {
                let inner_doc = take_inner_doc(&mut body);
                self.out.push(Stmt::Namespace {
                    name: lower_namespace_name(name),
                    body: lower_items(body),
                    file_scoped: false,
                    doc: self.pending_doc.take(),
                    inner_doc,
                    position: Some(pos),
                });
            }
            MlItem::Namespace {
                body: None,
                name,
                pos,
            } => {
                self.out.push(Stmt::Namespace {
                    name: lower_namespace_name(name),
                    body: Vec::new(),
                    file_scoped: true,
                    doc: self.pending_doc.take(),
                    inner_doc: None,
                    position: Some(pos),
                });
            }
            MlItem::Module {
                path,
                kind,
                signature,
                mut body,
                pos,
            } => {
                let inner_doc = take_inner_doc(&mut body);
                self.out.push(Stmt::Module {
                    path: lower_symbol_path(path),
                    kind: lower_module_kind(kind),
                    signature: signature.map(|path| SignatureAscription {
                        path: lower_symbol_path(path),
                        allow_extra: false,
                    }),
                    body: lower_module_items(body),
                    doc: self.pending_doc.take(),
                    inner_doc,
                    position: Some(pos),
                });
            }
            MlItem::ModuleSignature { name, items, pos } => {
                self.out.push(Stmt::Signature {
                    name,
                    items: items.into_iter().map(lower_signature_item).collect(),
                    doc: self.pending_doc.take(),
                    position: Some(pos),
                });
            }
            _ => {}
        }
    }

    fn clear_pending(&mut self) {
        self.pending = None;
        self.pending_doc = None;
    }
}

const fn lower_module_kind(kind: MlModuleKind) -> ModuleKind {
    match kind {
        MlModuleKind::Plain => ModuleKind::Plain,
        MlModuleKind::State => ModuleKind::State,
    }
}

/// Attach a pending doc comment to the `Function`/`Let` a binding lowered to.
/// A binding lowers to exactly one of those two, so this sets whichever it is —
/// matching by mutable reference to the `doc` field only, no struct rebuild.
pub(super) fn attach_doc(mut stmt: Stmt, doc: Option<DocComment>) -> Stmt {
    if let Stmt::Function { doc: slot, .. } | Stmt::Let { doc: slot, .. } = &mut stmt {
        *slot = doc;
    }
    stmt
}

/// The shared tail of every tuple-type diagnostic: what is unimplemented and
/// the working alternative ([TYPE-TUPLE]).
const TUPLE_UNIMPLEMENTED: &str = "has no value form; declare a positional record type instead";

/// A parsed signature awaiting its binding: `name<T, U> : ty ! effects`.
pub(super) struct MlSig {
    pub(super) name: String,
    type_params: Vec<MlTypeParam>,
    ty: MlType,
    effects: Vec<MlEffectRef>,
    effect_tail: Option<String>,
    effect_row_present: bool,
    position: Position,
}

impl MlSig {
    pub(super) fn new(
        name: String,
        type_params: Vec<MlTypeParam>,
        ty: MlType,
        effects: Vec<MlEffectRef>,
        effect_tail: Option<String>,
        effect_row_present: bool,
        position: Position,
    ) -> Self {
        Self {
            name,
            type_params,
            ty,
            effects,
            effect_tail,
            effect_row_present,
            position,
        }
    }
}

/// The payload name that denotes a zero-argument effect operation: `Unit => R`
/// in ML mirrors the Default flavor's `fn() -> R` (no argument), so it must
/// render to an EMPTY payload — not `fn(Unit) -> R`, which the codegen would
/// count as one argument and emit a differently-typed handler thunk.
const UNIT_PAYLOAD: &str = "Unit";

/// The surface spelling of an ignored parameter ([PARAM-WILDCARD]).
const WILDCARD: &str = "_";

mod scope;
pub(super) use scope::*;
mod bindings;
pub(super) use bindings::*;
mod types;
pub(super) use types::*;
mod expressions;
pub(super) use expressions::*;
#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod effect_tests;
#[cfg(test)]
mod value_tests;

mod modules;
use modules::{
    lower_import, lower_module_items, lower_namespace_name, lower_signature_item,
    lower_symbol_path, required_type_expr,
};
