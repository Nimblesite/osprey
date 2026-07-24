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

/// One immutable binding of a candidate clause set, destructured on the way in
/// so the merge never re-tests the item shape. A run holds clauses of one name
/// and one arity, which is what makes the merged head total.
struct Clause {
    name: String,
    params: Vec<MlParam>,
    uncurried: bool,
    body: MlExpr,
    pos: Position,
}

impl Clause {
    /// The binding this clause was parsed from — re-emitted verbatim when the
    /// run turns out not to be a clause set.
    fn into_item(self) -> MlItem {
        MlItem::Binding {
            mutable: false,
            name: self.name,
            params: self.params,
            uncurried: self.uncurried,
            body: self.body,
            pos: self.pos,
        }
    }
}

/// Rewrite every run of consecutive same-name clauses into one binding. Runs
/// with no refutable column are left exactly as they were, so nothing that
/// compiles today changes shape.
pub(super) fn merge(items: Vec<MlItem>, errors: &mut Vec<SyntaxError>) -> Vec<MlItem> {
    let mut out: Vec<MlItem> = Vec::new();
    let mut run: Vec<Clause> = Vec::new();
    for item in items {
        match clause_of(item) {
            Ok(clause) => {
                if !continues(&run, &clause) {
                    flush(&mut run, &mut out, errors);
                }
                run.push(clause);
            }
            Err(item) => {
                flush(&mut run, &mut out, errors);
                out.push(merge_nested(*item, errors));
            }
        }
    }
    flush(&mut run, &mut out, errors);
    out
}

/// The clause an immutable binding contributes, or the item back unchanged —
/// a `mut` binding is a cell, never a case. The rejected item is boxed because
/// a bare [`MlItem`] is far larger than the clause it fails to be.
fn clause_of(item: MlItem) -> Result<Clause, Box<MlItem>> {
    match item {
        MlItem::Binding {
            mutable: false,
            name,
            params,
            uncurried,
            body,
            pos,
        } => Ok(Clause {
            name,
            params,
            uncurried,
            body,
            pos,
        }),
        other => Err(Box::new(other)),
    }
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
/// name, same arity, and adjacent. A gap of any other item ends the set, so a
/// scattered definition stays the duplicate it is.
fn continues(run: &[Clause], next: &Clause) -> bool {
    run.first()
        .is_none_or(|first| first.name == next.name && first.params.len() == next.params.len())
}

/// Emit the accumulated run: the clauses as they stand when the run has no
/// refutable column (the overwhelmingly common case — one plain binding),
/// otherwise the merged `match` form.
fn flush(run: &mut Vec<Clause>, out: &mut Vec<MlItem>, errors: &mut Vec<SyntaxError>) {
    let clauses = std::mem::take(run);
    match refutable_column(&clauses, errors) {
        Some(column) => out.extend(merge_run(clauses, column)),
        None => out.extend(clauses.into_iter().map(Clause::into_item)),
    }
}

/// The one column a clause set selects on, or `None` when it selects on
/// nothing (every column is a plain binder — not a clause set at all).
/// Selecting on two columns at once needs nested matches with no source
/// spelling for the fall-through, so it is rejected rather than half-supported.
fn refutable_column(clauses: &[Clause], errors: &mut Vec<SyntaxError>) -> Option<usize> {
    let arity = clauses.iter().map(|c| c.params.len()).max()?;
    let mut found = None;
    for column in 0..arity {
        if !clauses.iter().any(|c| is_refutable(c.params.get(column))) {
            continue;
        }
        if let Some(first) = found {
            errors.push(SyntaxError {
                message: format!(
                    "a clause set may select on one column; this one selects on \
                     columns {first} and {column} — bind the second and match it in the body"
                ),
                position: clauses.last().map(|c| c.pos).unwrap_or_default(),
            });
            return None;
        }
        found = Some(column);
    }
    found
}

/// Whether a head column selects rather than binds. A missing column (a short
/// clause) is not refutable; the arity check reports that separately.
fn is_refutable(param: Option<&MlParam>) -> bool {
    matches!(param, Some(MlParam::Pattern(_)))
}

/// Build the merged binding: parameters named after the first plain binder in
/// each column, and a body that matches the selected column. Its name,
/// position and call shape come from the first clause so diagnostics land on
/// the definition a reader looks for.
fn merge_run(clauses: Vec<Clause>, column: usize) -> Option<MlItem> {
    let (name, pos, uncurried, arity) = clauses
        .first()
        .map(|c| (c.name.clone(), c.pos, c.uncurried, c.params.len()))?;
    let names: Vec<String> = (0..arity).map(|c| column_name(&clauses, c)).collect();
    let scrutinee = column_name(&clauses, column);
    let arms = clauses
        .into_iter()
        .map(|clause| clause_arm(clause, column, &names))
        .collect();
    Some(MlItem::Binding {
        mutable: false,
        name,
        params: names.into_iter().map(MlParam::Named).collect(),
        uncurried,
        body: MlExpr::Match {
            scrutinee: Box::new(MlExpr::Ident(scrutinee)),
            arms,
        },
        pos,
    })
}

/// The merged parameter name for `column`: the first plain binder any clause
/// spells there — load-bearing, so `make 0 = …` / `make d = …` produces the
/// same node as the Default `fn make(d) = match d { … }` — else a generated
/// name when every clause selects or ignores that column.
fn column_name(clauses: &[Clause], column: usize) -> String {
    clauses
        .iter()
        .find_map(|c| match c.params.get(column) {
            Some(MlParam::Named(name)) if name != WILDCARD => Some(name.clone()),
            _ => None,
        })
        .unwrap_or_else(|| osprey_ast::clause_param_name(column))
}

/// One clause as a match arm. Columns other than the selected one are plain
/// binders; where a clause spells such a column differently from the merged
/// parameter, the arm body opens with `itsName = mergedName` so every clause
/// keeps its own vocabulary.
fn clause_arm(clause: Clause, column: usize, names: &[String]) -> MlArm {
    let pos = clause.pos;
    let renames: Vec<MlItem> = clause
        .params
        .iter()
        .enumerate()
        .filter_map(|(c, param)| rename(c, param, names, column, pos))
        .collect();
    let pattern = match clause.params.into_iter().nth(column) {
        Some(MlParam::Pattern(pattern)) => pattern,
        Some(MlParam::Named(name)) if name != WILDCARD => MlPattern::Bind(name),
        _ => MlPattern::Wildcard,
    };
    MlArm {
        pattern,
        body: with_renames(renames, clause.body),
    }
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
