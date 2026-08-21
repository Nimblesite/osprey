//! Free-variable analysis tests. The implementation lives in
//! [`osprey_ast::freevars`], which cannot depend on the parser (the parser
//! depends on the AST), so its parse-driven tests live here.

mod tests {
    use osprey_ast::freevars::free_idents;
    use osprey_ast::{Expr, Stmt};
    use std::collections::BTreeSet;

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
