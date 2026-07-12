//! Shared slice-walking helper for the AST expression visitors across the
//! backend and language server. Every visitor that recurses into a collection
//! node (argument lists, named arguments, field assignments, match arms)
//! repeated the same "for each element, recurse into `pick(element)`" step once
//! per collection kind. It lives here once, generic over the visitor's threaded
//! state — the free-variable collector and effect scanner
//! ([`osprey-codegen`]) and the symbol collector ([`osprey-lsp`]) all reuse it.

use crate::Expr;

/// Recurse into every element of `items`, projecting each to its
/// sub-expression with `pick` and visiting it with `recur` under the visitor's
/// threaded state `ctx`. `Ctx` is whatever the caller threads through its
/// traversal — a single `&mut Vec` for the symbol collector, or a pair of
/// `&mut BTreeSet`s wrapped in a tuple for the free-variable / effect scans.
pub fn walk_each<T, Ctx>(
    items: &[T],
    ctx: &mut Ctx,
    pick: impl Fn(&T) -> &Expr,
    recur: impl Fn(&Expr, &mut Ctx),
) {
    for item in items {
        recur(pick(item), ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_each_projects_and_recurses_threading_the_context() {
        // Project each pair to its `Expr` and sum the integer literals into the
        // threaded accumulator — exercising `pick`, `recur`, and `ctx`.
        let items = [("a", Expr::Integer(2)), ("b", Expr::Integer(5))];
        let mut total = 0i64;
        walk_each(
            &items,
            &mut total,
            |(_, e)| e,
            |e, acc| {
                if let Expr::Integer(n) = e {
                    *acc += n;
                }
            },
        );
        assert_eq!(total, 7);
    }
}
