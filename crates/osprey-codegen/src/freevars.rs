//! Scope-aware free-variable analysis over the AST. The one collector behind
//! every capture mechanism in the backend — closure cells ([`crate::closure`])
//! and `spawn` thunks ([`crate::fiber`]) — so "what does this expression close
//! over" has exactly one answer. Names bound *inside* the expression (lambda
//! parameters, `let`s in blocks, `match`/`select`/handler pattern bindings)
//! are subtracted; everything else referenced is free.

use osprey_ast::{Expr, InterpolatedPart, MatchArm, Pattern, Stmt};
use std::collections::BTreeSet;

/// Collect the free identifiers of `e` into `out` (sorted, deduplicated).
pub(crate) fn free_idents(e: &Expr, out: &mut BTreeSet<String>) {
    let mut bound: Vec<String> = Vec::new();
    walk(e, &mut bound, out);
}

fn note(name: &str, bound: &[String], out: &mut BTreeSet<String>) {
    if !bound.iter().any(|b| b == name) {
        let _ = out.insert(name.to_string());
    }
}

/// Run `f` with `names` pushed onto the bound stack, restoring it after.
fn scoped(
    bound: &mut Vec<String>,
    names: Vec<String>,
    out: &mut BTreeSet<String>,
    f: impl FnOnce(&mut Vec<String>, &mut BTreeSet<String>),
) {
    let depth = bound.len();
    bound.extend(names);
    f(bound, out);
    bound.truncate(depth);
}

fn walk(e: &Expr, bound: &mut Vec<String>, out: &mut BTreeSet<String>) {
    match e {
        Expr::Integer(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) => {}
        Expr::Identifier(n) => note(n, bound, out),
        Expr::Path(path) => note(&path.to_string(), bound, out),
        Expr::InterpolatedStr(parts) => {
            for p in parts {
                if let InterpolatedPart::Expr(inner) = p {
                    walk(inner, bound, out);
                }
            }
        }
        Expr::List(xs) => walk_slice(xs, bound, out, |x| x),
        Expr::Map(entries) => {
            for en in entries {
                walk(&en.key, bound, out);
                walk(&en.value, bound, out);
            }
        }
        Expr::Object(fields) => walk_slice(fields, bound, out, |f| &f.value),
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            walk(left, bound, out);
            walk(right, bound, out);
        }
        Expr::Unary { operand, .. } => walk(operand, bound, out),
        e2 => walk_rest(e2, bound, out),
    }
}

/// Continuation of [`walk`] (kept in thirds so each stays small).
fn walk_rest(e: &Expr, bound: &mut Vec<String>, out: &mut BTreeSet<String>) {
    match e {
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => {
            walk(function, bound, out);
            walk_slice(arguments, bound, out, |x| x);
            walk_slice(named_arguments, bound, out, |n| &n.value);
        }
        Expr::MethodCall {
            target,
            arguments,
            named_arguments,
            ..
        } => {
            walk(target, bound, out);
            walk_slice(arguments, bound, out, |x| x);
            walk_slice(named_arguments, bound, out, |n| &n.value);
        }
        Expr::FieldAccess { target, .. } => walk(target, bound, out),
        Expr::Index { target, index } => {
            walk(target, bound, out);
            walk(index, bound, out);
        }
        Expr::Lambda {
            parameters, body, ..
        } => {
            let names = parameters.iter().map(|p| p.name.clone()).collect();
            scoped(bound, names, out, |b, o| walk(body, b, o));
        }
        Expr::Match { value, arms } => {
            walk(value, bound, out);
            walk_arms(arms, bound, out);
        }
        Expr::Block { statements, value } => walk_block(statements, value.as_deref(), bound, out),
        Expr::TypeConstructor { fields, .. } => walk_slice(fields, bound, out, |f| &f.value),
        Expr::Update { record, fields } => {
            note(record, bound, out);
            walk_slice(fields, bound, out, |f| &f.value);
        }
        e2 => walk_fiber(e2, bound, out),
    }
}

/// Final third of the walker: fiber/effect forms (and the leaf-handled rest).
fn walk_fiber(e: &Expr, bound: &mut Vec<String>, out: &mut BTreeSet<String>) {
    match e {
        Expr::Spawn(inner) | Expr::Await(inner) | Expr::Recv(inner) | Expr::Yield(Some(inner)) => {
            walk(inner, bound, out);
        }
        Expr::Send { channel, value } => {
            walk(channel, bound, out);
            walk(value, bound, out);
        }
        Expr::Select { arms } => walk_arms(arms, bound, out),
        Expr::Perform {
            arguments,
            named_arguments,
            ..
        } => {
            walk_slice(arguments, bound, out, |x| x);
            walk_slice(named_arguments, bound, out, |n| &n.value);
        }
        Expr::Resume(Some(value)) => walk(value, bound, out),
        Expr::Handler { arms, body, .. } => {
            for arm in arms {
                scoped(bound, arm.params.clone(), out, |b, o| walk(&arm.body, b, o));
            }
            walk(body, bound, out);
        }
        // Every other variant is fully handled by the first two thirds.
        _ => {}
    }
}

/// `let`s bind for the *rest* of the block, so statements thread the bound
/// stack left to right and the whole block's bindings pop together.
fn walk_block(
    statements: &[Stmt],
    value: Option<&Expr>,
    bound: &mut Vec<String>,
    out: &mut BTreeSet<String>,
) {
    let depth = bound.len();
    for s in statements {
        match s {
            Stmt::Let { name, value, .. } => {
                walk(value, bound, out);
                bound.push(name.clone());
            }
            Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => walk(value, bound, out),
            _ => {}
        }
    }
    if let Some(v) = value {
        walk(v, bound, out);
    }
    bound.truncate(depth);
}

fn walk_arms(arms: &[MatchArm], bound: &mut Vec<String>, out: &mut BTreeSet<String>) {
    for arm in arms {
        let names = pattern_bindings(&arm.pattern);
        scoped(bound, names, out, |b, o| walk(&arm.body, b, o));
    }
}

/// Every name a pattern binds in its arm's scope.
fn pattern_bindings(p: &Pattern) -> Vec<String> {
    match p {
        Pattern::Wildcard | Pattern::Literal(_) => Vec::new(),
        Pattern::Constructor {
            fields,
            sub_patterns,
            ..
        } => {
            let mut names = fields.clone();
            for sp in sub_patterns {
                names.extend(pattern_bindings(sp));
            }
            names
        }
        Pattern::TypeAnnotated { name, .. } => vec![name.clone()],
        Pattern::Structural { fields } => fields.clone(),
        Pattern::List { elements, rest } => {
            let mut names: Vec<String> = elements.iter().flat_map(pattern_bindings).collect();
            names.extend(rest.clone());
            names
        }
        Pattern::Binding(n) => vec![n.clone()],
    }
}

/// Recurse into each element of `items`, projecting it to its sub-expression
/// with `pick`. The one place the free-variable walker fans out over a
/// collection node, threading `bound`/`out` through [`osprey_ast::walk_each`].
fn walk_slice<T>(
    items: &[T],
    bound: &mut Vec<String>,
    out: &mut BTreeSet<String>,
    pick: impl Fn(&T) -> &Expr,
) {
    osprey_ast::walk_each(items, &mut (bound, out), pick, |e, (b, o)| walk(e, b, o));
}

#[cfg(test)]
mod tests {
    use super::*;
    use osprey_ast::{Expr, Stmt};

    /// The value expression of the first `let` in `src`.
    fn first_let_value(src: &str) -> Expr {
        osprey_syntax::parse_program(src)
            .program
            .statements
            .into_iter()
            .find_map(|s| match s {
                Stmt::Let { value, .. } => Some(value),
                _ => None,
            })
            .expect("a let binding")
    }

    fn frees(src: &str) -> BTreeSet<String> {
        let e = first_let_value(src);
        let mut out = BTreeSet::new();
        free_idents(&e, &mut out);
        out
    }

    #[test]
    fn lambda_subtracts_parameters_keeps_outer_references() {
        let f = frees("let f = fn(x, y) => add(x, y, outer)");
        assert!(f.contains("add") && f.contains("outer"));
        assert!(!f.contains("x") && !f.contains("y"));
    }

    #[test]
    fn list_pattern_binds_prefix_and_rest_not_free() {
        let f = frees(
            "let f = fn(xs) => match xs { \
               [head, ...tail] => use(head, tail, free1) \
               [only]          => use2(only, free2) \
               _               => free3 }",
        );
        assert!(f.contains("use") && f.contains("use2"));
        assert!(f.contains("free1") && f.contains("free2") && f.contains("free3"));
        for bound in ["head", "tail", "only", "xs"] {
            assert!(
                !f.contains(bound),
                "`{bound}` must be bound, not free: {f:?}"
            );
        }
    }

    #[test]
    fn block_lets_scope_following_statements() {
        let f = frees("let f = fn() => { let a = seed  let b = step(a)  result(a, b, glob) }");
        assert!(
            f.contains("seed") && f.contains("step") && f.contains("result") && f.contains("glob")
        );
        assert!(!f.contains("a") && !f.contains("b"));
    }

    #[test]
    fn interpolation_index_field_and_binary_are_all_walked() {
        let f = frees("let f = fn(r) => \"${field(r)} ${arr[idx]} ${r.member} ${lhs + rhs}\"");
        for name in ["field", "arr", "idx", "lhs", "rhs"] {
            assert!(f.contains(name), "missing free `{name}`: {f:?}");
        }
        assert!(!f.contains("r"));
    }

    #[test]
    fn constructor_pattern_binds_positional_fields() {
        let f = frees(
            "let f = fn(v) => match v { Some(inner) => keep(inner, outer)  None => fallback }",
        );
        assert!(f.contains("keep") && f.contains("outer") && f.contains("fallback"));
        assert!(!f.contains("inner"));
    }

    #[test]
    fn structural_pattern_binds_named_fields() {
        let f = frees("let f = fn(v) => match v { { name, age } => label(name, age, tag) }");
        assert!(f.contains("label") && f.contains("tag"));
        assert!(!f.contains("name") && !f.contains("age"));
    }

    #[test]
    fn spawn_and_pipe_capture_through_to_inner_expressions() {
        let f = frees("let f = fn(seed) => spawn worker(seed, shared)");
        assert!(f.contains("worker") && f.contains("shared"));
        assert!(!f.contains("seed"));
        let p = frees("let f = fn(n) => n |> scale |> offset(base)");
        assert!(p.contains("scale") && p.contains("offset") && p.contains("base"));
        assert!(!p.contains("n"));
    }

    #[test]
    fn method_call_walks_target_and_arguments() {
        // walk_rest's MethodCall arm: target + positional + named arguments.
        let f = frees("let f = fn(s) => s.replace(needle, with: repl)");
        assert!(f.contains("needle") && f.contains("repl"));
        assert!(!f.contains("s"));
    }

    #[test]
    fn record_update_walks_record_and_field_values() {
        // walk_rest's Update arm: the updated record name + each field value.
        // `ident { … }` always parses as a TypeConstructor (higher dynamic
        // precedence), so the Update node is built directly here.
        use osprey_ast::FieldAssignment;
        let e = Expr::Update {
            record: String::from("base"),
            fields: vec![FieldAssignment {
                name: String::from("x"),
                value: Expr::Identifier(String::from("dx")),
            }],
        };
        let mut out = BTreeSet::new();
        free_idents(&e, &mut out);
        assert!(out.contains("base") && out.contains("dx"));
    }

    #[test]
    fn fiber_forms_capture_inner_expressions() {
        // walk_fiber's Send/Recv arms.
        let s = frees("let f = fn() => send(chan, payload)");
        assert!(s.contains("chan") && s.contains("payload"));
        let r = frees("let f = fn() => recv(inbox)");
        assert!(r.contains("inbox"));
    }

    #[test]
    fn block_assignment_and_expr_statements_are_walked() {
        // walk_block's Assignment + Expr statement arms.
        let f = frees("let f = fn() => { mut a = seed  a = step(a)  emit(a) }");
        assert!(f.contains("seed") && f.contains("step") && f.contains("emit"));
    }
}
