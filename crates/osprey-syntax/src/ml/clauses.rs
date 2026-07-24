//! `[FLAVOR-ML-CLAUSES]` — collapse equational clause sets into the plain
//! parameter-list-over-`match` form the rest of the frontend already handles.
//!
//! ```text
//! check Leaf         = 0            check c = match c
//! check (Node l r)   = 1 + …   ==>              Leaf       => 0
//!                                               Node l r   => 1 + …
//! ```
//!
//! This is a CST→CST pre-pass, so [`super::lower`] never sees a
//! [`MlParam::Pattern`] and the emitted node is *exactly* the one the Default
//! twin `fn check(c) = match c { … }` produces — the requirement of
//! [FLAVOR-IR-EQUIV].

use osprey_ast::Position;

use crate::SyntaxError;

use super::cst::{MlArm, MlExpr, MlItem, MlParam, MlPattern};

/// The surface spelling of an ignored head column ([PARAM-WILDCARD]).
const WILDCARD: &str = "_";

/// Rewrite every run of consecutive same-name clauses into one binding. Runs
/// with no refutable column are left exactly as they were, so nothing that
/// compiles today changes shape.
pub(super) fn merge(items: Vec<MlItem>, errors: &mut Vec<SyntaxError>) -> Vec<MlItem> {
    let mut out: Vec<MlItem> = Vec::new();
    let mut run: Vec<MlItem> = Vec::new();
    for item in items {
        if !run.is_empty() && !same_clause_set(&run, &item) {
            flush(&mut run, &mut out, errors);
        }
        match item {
            item @ MlItem::Binding { .. } => run.push(item),
            item => {
                flush(&mut run, &mut out, errors);
                out.push(merge_nested(item, errors));
            }
        }
    }
    flush(&mut run, &mut out, errors);
    out
}

/// Recurse into the declaration containers, whose bodies are item lists in
/// their own right ([MODULES-MODULE], [MODULES-NAMESPACE]).
fn merge_nested(item: MlItem, errors: &mut Vec<SyntaxError>) -> MlItem {
    match item {
        MlItem::Namespace {
            name,
            body: Some(body),
            pos,
        } => MlItem::Namespace {
            name,
            body: Some(merge(body, errors)),
            pos,
        },
        MlItem::Module {
            path,
            kind,
            body,
            signature,
            pos,
        } => MlItem::Module {
            path,
            kind,
            body: merge(body, errors),
            signature,
            pos,
        },
        MlItem::Export { item, pos } => MlItem::Export {
            item: Box::new(merge_nested(*item, errors)),
            pos,
        },
        item => item,
    }
}

/// Whether `next` continues the clause set already accumulated in `run`: same
/// name, same arity, immutable, and adjacent. A gap of any other item ends the
/// set, so a scattered definition stays the duplicate it is.
fn same_clause_set(run: &[MlItem], next: &MlItem) -> bool {
    match (run.first(), next) {
        (
            Some(MlItem::Binding {
                name,
                params,
                mutable: false,
                ..
            }),
            MlItem::Binding {
                name: next_name,
                params: next_params,
                mutable: false,
                ..
            },
        ) => name == next_name && params.len() == next_params.len(),
        _ => false,
    }
}

/// Emit the accumulated run: a single binding when it is one clause with no
/// refutable column (the overwhelmingly common case), otherwise the merged
/// `match` form.
fn flush(run: &mut Vec<MlItem>, out: &mut Vec<MlItem>, errors: &mut Vec<SyntaxError>) {
    let clauses = std::mem::take(run);
    match refutable_column(&clauses, errors) {
        Some(column) => out.push(merge_run(clauses, column, errors)),
        None => out.extend(clauses),
    }
}

/// The one column a clause set selects on, or `None` when it selects on
/// nothing (every column is a plain binder — not a clause set at all).
/// Selecting on two columns at once needs nested matches with no source
/// spelling for the fall-through, so it is rejected rather than half-supported.
fn refutable_column(clauses: &[MlItem], errors: &mut Vec<SyntaxError>) -> Option<usize> {
    let arity = clauses
        .iter()
        .filter_map(head)
        .map(|(p, _)| p.len())
        .max()?;
    let mut found = None;
    for column in 0..arity {
        if !clauses
            .iter()
            .filter_map(head)
            .any(|(params, _)| is_refutable(params.get(column)))
        {
            continue;
        }
        if let Some(first) = found {
            let pos = clauses.iter().filter_map(head).map(|(_, p)| p).next_back();
            errors.push(SyntaxError {
                message: format!(
                    "a clause set may select on one column; this one selects on \
                     columns {first} and {column} — bind the second and match it in the body"
                ),
                position: pos.unwrap_or_default(),
            });
            return None;
        }
        found = Some(column);
    }
    found
}

/// The head of a binding: its surface parameters and its source position.
fn head(item: &MlItem) -> Option<(&[MlParam], Position)> {
    match item {
        MlItem::Binding { params, pos, .. } => Some((params.as_slice(), *pos)),
        _ => None,
    }
}

/// Whether a head column selects rather than binds. A missing column (a short
/// clause) is not refutable; the arity check reports that separately.
fn is_refutable(param: Option<&MlParam>) -> bool {
    matches!(param, Some(MlParam::Pattern(_)))
}

/// Build the merged binding: parameters named after the first plain binder in
/// each column, and a body that matches the selected column.
fn merge_run(clauses: Vec<MlItem>, column: usize, errors: &mut Vec<SyntaxError>) -> MlItem {
    let arity = clauses.iter().filter_map(head).map(|(p, _)| p.len()).max();
    let names: Vec<String> = (0..arity.unwrap_or_default())
        .map(|c| column_name(&clauses, c))
        .collect();
    let (name, pos, uncurried) = identity(&clauses);
    let scrutinee = names
        .get(column)
        .cloned()
        .unwrap_or_else(|| osprey_ast::clause_param_name(column));
    let arms = clauses
        .into_iter()
        .filter_map(|clause| clause_arm(clause, column, &names, errors))
        .collect();
    MlItem::Binding {
        mutable: false,
        name,
        params: names.into_iter().map(MlParam::Named).collect(),
        uncurried,
        body: MlExpr::Match {
            scrutinee: Box::new(MlExpr::Ident(scrutinee)),
            arms,
        },
        pos,
    }
}

/// The merged parameter name for `column`: the first plain binder any clause
/// spells there — load-bearing, so `make 0 = …` / `make d = …` produces the
/// same node as the Default `fn make(d) = match d { … }` — else a generated
/// name when every clause selects or ignores that column.
fn column_name(clauses: &[MlItem], column: usize) -> String {
    clauses
        .iter()
        .filter_map(head)
        .find_map(|(params, _)| match params.get(column) {
            Some(MlParam::Named(name)) if name != WILDCARD => Some(name.clone()),
            _ => None,
        })
        .unwrap_or_else(|| osprey_ast::clause_param_name(column))
}

/// The merged binding's name, position and call shape, taken from the first
/// clause so diagnostics land on the definition a reader looks for.
fn identity(clauses: &[MlItem]) -> (String, Position, bool) {
    match clauses.first() {
        Some(MlItem::Binding {
            name,
            pos,
            uncurried,
            ..
        }) => (name.clone(), *pos, *uncurried),
        _ => (String::new(), Position::default(), false),
    }
}

/// One clause as a match arm. Columns other than the selected one are plain
/// binders; where a clause spells such a column differently from the merged
/// parameter, the arm body opens with `itsName = mergedName` so every clause
/// keeps its own vocabulary.
fn clause_arm(
    clause: MlItem,
    column: usize,
    names: &[String],
    errors: &mut Vec<SyntaxError>,
) -> Option<MlArm> {
    let MlItem::Binding {
        params, body, pos, ..
    } = clause
    else {
        return None;
    };
    if params.len() != names.len() {
        errors.push(SyntaxError {
            message: format!(
                "every clause of a definition must take the same number of arguments \
                 (expected {}, found {})",
                names.len(),
                params.len()
            ),
            position: pos,
        });
        return None;
    }
    let renames: Vec<MlItem> = params
        .iter()
        .enumerate()
        .filter_map(|(c, param)| rename(c, param, names, column, pos))
        .collect();
    let pattern = match params.into_iter().nth(column) {
        Some(MlParam::Pattern(pattern)) => pattern,
        Some(MlParam::Named(name)) if name != WILDCARD => MlPattern::Bind(name),
        _ => MlPattern::Wildcard,
    };
    Some(MlArm {
        pattern,
        body: with_renames(renames, body),
    })
}

/// `itsName = mergedName` for a column this clause spells differently, or
/// `None` when the names already agree (the usual case) or nothing is bound.
fn rename(
    c: usize,
    param: &MlParam,
    names: &[String],
    column: usize,
    pos: Position,
) -> Option<MlItem> {
    let MlParam::Named(name) = param else {
        return None;
    };
    let merged = names.get(c)?;
    if c == column || name == WILDCARD || name == merged {
        return None;
    }
    Some(MlItem::Binding {
        mutable: false,
        name: name.clone(),
        params: Vec::new(),
        uncurried: false,
        body: MlExpr::Ident(merged.clone()),
        pos,
    })
}

/// Wrap an arm body in its column renames, if it has any.
fn with_renames(renames: Vec<MlItem>, body: MlExpr) -> MlExpr {
    if renames.is_empty() {
        return body;
    }
    MlExpr::Block {
        items: renames,
        value: Some(Box::new(body)),
    }
}
