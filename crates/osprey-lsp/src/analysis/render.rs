//! Shared symbol signatures and inferred type rendering.

use super::{
    render_type, DocComment, EffectRef, ExternParameter, Parameter, Position, SymbolInfo,
    SymbolKind, TypeExpr,
};

/// Render a declaration's structured doc comment to the Markdown a hover shows,
/// or `None` when it has none ([DOC-EXPORT], hover half).
pub(super) fn render_doc(doc: Option<&DocComment>) -> Option<String> {
    doc.map(DocComment::render_markdown)
}

/// A type/effect declaration symbol whose signature shows the binder
/// (`type Option<T>`, `effect State<T>`) while the name stays bare for
/// lookups. Implements [TYPE-GENERICS-DECL].
pub(super) fn generic_decl_sym(
    name: &str,
    type_params: &[osprey_ast::TypeParam],
    kind: &str,
    doc: Option<String>,
    position: Option<Position>,
) -> SymbolInfo {
    let mut sym = decl_sym(name, kind, position);
    sym.doc = doc;
    let binder = render_type_params(type_params);
    if !binder.is_empty() {
        sym.signature = Some(format!("{kind} {name}{binder}"));
    }
    sym
}

/// Render a declaration's type-parameter binder (`<T, out U>`), empty when it
/// has none. Implements [TYPE-GENERICS-DECL].
#[must_use]
pub fn render_type_params(params: &[osprey_ast::TypeParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let shown: Vec<String> = params
        .iter()
        .map(|p| match p.variance {
            osprey_ast::Variance::Covariant => format!("out {}", p.name),
            osprey_ast::Variance::Contravariant => format!("in {}", p.name),
            osprey_ast::Variance::Invariant => p.name.clone(),
        })
        .collect();
    format!("<{}>", shown.join(", "))
}

pub(super) fn fn_sym(
    name: &str,
    binder: &str,
    parameters: Vec<(String, String)>,
    return_type: Option<&TypeExpr>,
    declared_effect_row: Option<String>,
    doc: Option<String>,
    position: Option<Position>,
) -> SymbolInfo {
    let written = return_type.map(render_type);
    // `Unit` is only the *display* fallback for a function whose return type
    // the author left to inference. `return_type` keeps the written type alone
    // (`None` when unwritten) so a reader is told "declared Unit" apart from
    // "not declared" and the latter is filled in from the checker by
    // [`fill_inferred`] ([LSP-HOVER-INFERRED-SIGNATURE]).
    let signature = render_signature(
        name,
        binder,
        &parameters,
        written.as_deref(),
        declared_effect_row.as_deref(),
    );
    SymbolInfo {
        name: name.into(),
        source_name: name.into(),
        kind: SymbolKind::Function,
        ty: signature.clone(),
        position,
        signature: Some(signature),
        binder: binder.to_owned(),
        parameters,
        return_type: written,
        declared_effect_row,
        doc,
    }
}

/// The function's written row is an admissible upper bound, not an inferred
/// effect set. Preserve its source-level shape in every editor signature.
pub(super) fn render_effect_row(
    effects: &[EffectRef],
    tail: Option<&str>,
    present: bool,
) -> Option<String> {
    if !present {
        return None;
    }
    let labels: Vec<String> = effects
        .iter()
        .map(|effect| {
            let args = if effect.type_args.is_empty() {
                String::new()
            } else {
                format!(
                    "<{}>",
                    effect
                        .type_args
                        .iter()
                        .map(render_type)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            format!("{}{args}", effect.name)
        })
        .collect();
    Some(match (labels.as_slice(), tail) {
        ([], Some(tail)) => format!("!{tail}"),
        (labels, Some(tail)) => format!("![{} | {tail}]", labels.join(", ")),
        ([], None) => "![]".to_owned(),
        ([only], None) => format!("!{only}"),
        (labels, None) => format!("![{}]", labels.join(", ")),
    })
}

/// The one spelling of a function signature: `fn name<T>(a: int) -> string`.
///
/// A return type nobody supplied and the checker could not prove leaves the
/// arrow OFF. It used to read `-> Unit`, but `Unit` is a positive claim, and
/// for a body like `if f { Success { .. } } else { Error { .. } }` the checker
/// refutes it outright ("cannot unify Unit with Result<t5, t6>"). Saying
/// nothing is the only honest option left once there is nothing to say.
pub(super) fn render_signature(
    name: &str,
    binder: &str,
    parameters: &[(String, String)],
    ret: Option<&str>,
    effect_row: Option<&str>,
) -> String {
    let shown: Vec<String> = parameters.iter().map(render_param).collect();
    let head = format!("fn {name}{binder}({})", shown.join(", "));
    let signature = match ret {
        Some(ret) => format!("{head} -> {ret}"),
        None => head,
    };
    effect_row.map_or_else(|| signature.clone(), |row| format!("{signature} {row}"))
}

/// How a checker-supplied type is shown to a reader, or `None` when it carries
/// no information at all.
///
/// An inferred type that still holds a type VARIABLE is not a finished answer,
/// but it is rarely empty: `Result<int, t6>` proves the payload even where the
/// error side stays open. Rendering `t6` would leak the checker's private name,
/// whose number moves when an unrelated line is edited, so every unsolved slot
/// becomes `_` ([`osprey_types::render_with_holes`]). A type that is nothing
/// BUT a hole says nothing, so it yields `None` and the slot is left as the
/// author wrote it — bare — rather than decorated with `_`.
///
/// "Nothing but a hole" is a question about the type's STRUCTURE, not its
/// spelling: nothing stops an author writing `type _ = { x: int }`, and asking
/// whether the rendering equals `"_"` threw that fully proven type away.
pub(super) fn shown(ty: &osprey_types::Type) -> Option<String> {
    if matches!(ty, osprey_types::Type::Var(_)) {
        return None;
    }
    Some(osprey_types::render_with_holes(ty))
}

/// Fill every slot the author left to inference — unannotated parameters and an
/// unwritten return type — with the type the checker proved.
///
/// Osprey is Hindley-Milner and the house style DELETES every inferable
/// annotation, so blank slots are the common case. Rendering them literally
/// reported `fn describeAny(v: any) -> Unit`: the return type flatly wrong,
/// because `Unit` was only ever a display fallback. That made the annotation
/// rule unenforceable from tooling — obeying it downgraded every outline entry
/// — so both hover and `--symbols` answer from inference through here.
/// Implements [LSP-HOVER-INFERRED-SIGNATURE].
pub(crate) fn fill_inferred(sym: &mut SymbolInfo, types: &osprey_types::ProgramTypes) {
    if sym.kind == SymbolKind::Variable && sym.ty.is_empty() {
        if let Some(inferred) = types.let_type(sym.position).and_then(shown) {
            sym.ty = inferred;
        }
    }
    if sym.kind != SymbolKind::Function
        || (sym.return_type.is_some() && sym.parameters.iter().all(|(_, t)| !t.is_empty()))
    {
        return;
    }
    let inferred = types.param_types(&sym.name).unwrap_or_default();
    for (slot, (_, written)) in sym.parameters.iter_mut().enumerate() {
        if let (true, Some(found)) = (written.is_empty(), inferred.get(slot)) {
            if let Some(rendered) = shown(found) {
                *written = rendered;
            }
        }
    }
    sym.return_type = sym
        .return_type
        .take()
        .or_else(|| types.return_type(&sym.name).and_then(shown));
    let signature = render_signature(
        &sym.name,
        &sym.binder,
        &sym.parameters,
        sym.return_type.as_deref(),
        sym.declared_effect_row.as_deref(),
    );
    sym.ty.clone_from(&signature);
    sym.signature = Some(signature);
}

pub(crate) fn render_param((n, t): &(String, String)) -> String {
    if t.is_empty() {
        n.clone()
    } else {
        format!("{n}: {t}")
    }
}

pub(super) fn let_sym(
    name: &str,
    ty: Option<&TypeExpr>,
    doc: Option<String>,
    position: Option<Position>,
) -> SymbolInfo {
    SymbolInfo {
        name: name.into(),
        source_name: name.into(),
        kind: SymbolKind::Variable,
        ty: ty.map(render_type).unwrap_or_default(),
        position,
        signature: None,
        binder: String::new(),
        parameters: Vec::new(),
        return_type: None,
        declared_effect_row: None,
        doc,
    }
}

pub(super) fn decl_sym(name: &str, ty: &str, position: Option<Position>) -> SymbolInfo {
    SymbolInfo {
        name: name.into(),
        source_name: name.into(),
        kind: SymbolKind::Type,
        ty: ty.into(),
        position,
        signature: None,
        binder: String::new(),
        parameters: Vec::new(),
        return_type: None,
        declared_effect_row: None,
        doc: None,
    }
}

pub(super) fn param_pairs(params: &[Parameter]) -> Vec<(String, String)> {
    params
        .iter()
        .map(|p| {
            (
                p.name.clone(),
                p.ty.as_ref().map(render_type).unwrap_or_default(),
            )
        })
        .collect()
}

pub(super) fn extern_pairs(params: &[ExternParameter]) -> Vec<(String, String)> {
    params
        .iter()
        .map(|p| (p.name.clone(), render_type(&p.ty)))
        .collect()
}
