//! ML scope lowering.
use super::{
    lower_expr, DocComment, DocScope, Expr, HashMap, HashSet, MlExpr, MlItem, MlParam, Position,
    LOWER_ERRORS, SCOPES,
};

/// Record one lowering diagnostic at `pos`.
pub(in crate::ml) fn lower_error(message: String, pos: Position) {
    LOWER_ERRORS.with(|sink| {
        sink.borrow_mut().push(crate::SyntaxError {
            message,
            position: pos,
        });
    });
}

/// Apply `descend` to the items a declaration CONTAINER holds — a namespace or
/// module body, or the single item an `export`/`opaque` wraps — and report
/// whether `item` was one. Every collector that walks a unit shares this so it
/// only spells out the arms it actually cares about.
pub(in crate::ml) fn descend_containers<T>(
    item: &MlItem,
    out: &mut T,
    descend: fn(&[MlItem], &mut T),
) -> bool {
    match item {
        MlItem::Namespace {
            body: Some(body), ..
        }
        | MlItem::Module { body, .. } => descend(body, out),
        MlItem::Export { item, .. } | MlItem::Opaque { item, .. } => {
            descend(std::slice::from_ref(item.as_ref()), out);
        }
        _ => return false,
    }
    true
}

/// Lower with `names` pushed as a lexical frame, popping it after.
pub(in crate::ml) fn in_scope<T>(names: HashSet<String>, lower: impl FnOnce() -> T) -> T {
    SCOPES.with(|s| s.borrow_mut().push(names));
    let lowered = lower();
    SCOPES.with(|s| {
        let _ = s.borrow_mut().pop();
    });
    lowered
}

/// Whether `name` is BOUND at the point being lowered — a file-scope
/// definition, an enclosing parameter, or a binding of an enclosing block.
pub(in crate::ml) fn is_bound(name: &str) -> bool {
    SCOPES.with(|s| s.borrow().iter().any(|frame| frame.contains(name)))
}

/// The names the items at ONE lexical level bind. Every binding of a level is
/// visible to every other (file-scope definitions are mutually recursive), so a
/// level's frame is collected whole before it is entered.
pub(in crate::ml) fn scope_of(items: &[MlItem]) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_scope_names(items, &mut out);
    out
}

/// The names a parameter list binds.
pub(in crate::ml) fn params_scope(params: &[MlParam]) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_param_names(params, &mut out);
    out
}

/// Record every name a `name … = …` binding introduces at THIS level,
/// recursing only through declaration containers — never into expressions,
/// whose own bindings belong to the inner scopes those expressions open.
pub(in crate::ml) fn collect_scope_names(items: &[MlItem], out: &mut HashSet<String>) {
    for item in items {
        if let MlItem::Binding { name, .. } = item {
            let _ = out.insert(name.clone());
        } else {
            let _ = descend_containers(item, out, collect_scope_names);
        }
    }
}

/// Record every constructor this unit declares with a positional payload, and
/// how many slots it takes ([TYPE-UNION-POSITIONAL]). Recurses into the
/// declaration containers so a module-local type is seen too.
pub(in crate::ml) fn collect_positional_ctors(items: &[MlItem], out: &mut HashMap<String, usize>) {
    for item in items {
        match item {
            MlItem::Type { variants, .. } => {
                for variant in variants {
                    if crate::positional::declares_slots(
                        variant.fields.iter().map(|f| f.name.as_str()),
                    ) {
                        let _ = out.insert(variant.name.clone(), variant.fields.len());
                    }
                }
            }
            other => {
                let _ = descend_containers(other, out, collect_positional_ctors);
            }
        }
    }
}

/// A saturated whitespace application of a positionally-declared constructor,
/// folded to the construction node ([FLAVOR-ML-CTOR-POSITIONAL],
/// [TYPE-UNION-POSITIONAL]).
pub(in crate::ml) fn positional_construction(head: &MlExpr, args: &[MlExpr]) -> Option<Expr> {
    let MlExpr::Ident(name) = head else {
        return None;
    };
    crate::positional::construct(
        name,
        args.iter().cloned().map(lower_expr).collect::<Vec<_>>(),
    )
}

/// Record the names a binding or lambda head BINDS. A parameter is a callable
/// like any other name: `apply f a b = f a b` applies `f` one argument at a
/// time, so its spine must stay curried. Leaving parameters out made every
/// higher-order whitespace spine fold to one flat call, and passing a curried
/// function to it was rejected with `function arity mismatch: 2 vs 1
/// parameters` while the Default twin `fn apply(f, a, b) = f(a)(b)` was
/// accepted ([FLAVOR-ML-CURRY], [FLAVOR-IR-EQUIV]).
pub(in crate::ml) fn collect_param_names(params: &[MlParam], out: &mut HashSet<String>) {
    for param in params {
        match param {
            MlParam::Named(name) | MlParam::Typed(name, _, _) => {
                let _ = out.insert(name.name.clone());
            }
            MlParam::Unit | MlParam::Pattern(_) => {}
        }
    }
}

/// Split a scope's `//!` block off the items it encloses. An inner doc
/// documents the scope that CONTAINS it, so it never reaches the item lowerer:
/// the file, namespace or module constructor takes it instead. Removing it here
/// is also what makes a stray `//!` — one in a scope that cannot hold one —
/// reach [`ItemLower::lower_item`] and be reported. Implements
/// [DOC-SIGIL-INNER].
pub(in crate::ml) fn take_inner_doc(items: &mut Vec<MlItem>) -> Option<DocComment> {
    // Only a `//!` that OPENS the scope documents it. Searching the whole run
    // instead would silently hoist a stray one from the middle or the end of a
    // body into the scope's documentation, which is precisely the kind of
    // quietly-wrong lowering the negative tests below exist to forbid — the
    // Default flavor's grammar pins the same position rule.
    if !matches!(items.first(), Some(MlItem::InnerDoc { .. })) {
        return None;
    }
    let MlItem::InnerDoc { text, .. } = items.remove(0) else {
        return None;
    };
    Some(crate::docparse::parse_doc(&text, DocScope::Inner))
}
