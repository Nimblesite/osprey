use super::*;

#[derive(Default)]
struct Callees(Vec<String>);

impl AstVisitor for Callees {
    fn expression(&mut self, expression: &Expr) {
        if let Expr::Call { function, .. } = expression {
            if let Expr::Identifier(name) = function.as_ref() {
                self.0.push(name.clone());
            }
        }
    }
}

fn callees(expression: &Expr) -> Vec<String> {
    let program = Program {
        statements: vec![Stmt::Expr {
            value: expression.clone(),
            doc: None,
            position: None,
        }],
        doc: None,
    };
    let mut found = Callees::default();
    walk_program(&program, &mut found);
    found.0
}

fn refresh_of(operations: &[&str]) -> Expr {
    let call = call(
        "signalRefresh",
        vec![ident("old"), ident("changed"), ident("work")],
    );
    expand(
        &call,
        operations.iter().map(|op| (*op).to_owned()).collect(),
    )
}

fn bound_names(expression: &Expr) -> Vec<&str> {
    match expression {
        Expr::Block { statements, .. } => statements
            .iter()
            .filter_map(|statement| match statement {
                Stmt::Let { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A refresh runs on every update of every view, and most refreshes keep their
/// snapshot. Deciding that must not allocate: no joined strings, no fold
/// closure, no rebuilt dependency list. The may-read set is a compile-time
/// constant, so the comparison is unrolled against it [STAGE-SIGNALS-CACHE].
#[test]
fn a_refresh_decides_staleness_without_building_anything() {
    let refresh = refresh_of(&["A.read", "B.read"]);
    let called = callees(&refresh);
    for costly in ["join", "fold", "range", "listGet"] {
        assert!(
            !called.iter().any(|name| name == costly),
            "{costly} in {called:?}"
        );
    }
    assert_eq!(
        bound_names(&refresh),
        ["$signalCached", "$signalChanged", "$signalWork"],
        "the dependency list is built only by a rebuild"
    );
    assert_eq!(
        called.iter().filter(|name| *name == "$signalWork").count(),
        1
    );
}

/// A callback whose may-read set is unknown always rebuilds, decided at
/// compile time rather than by searching the list on every refresh.
#[test]
fn an_unknown_dependency_is_stale_without_a_search() {
    let refresh = refresh_of(&["<unknown>"]);
    let Expr::Block {
        value: Some(value), ..
    } = &refresh
    else {
        return assert!(matches!(refresh, Expr::Block { .. }));
    };
    let same = match value.as_ref() {
        Expr::Block { statements, .. } => statements.first(),
        _ => None,
    };
    assert!(
        matches!(same, Some(Stmt::Let { name, value: Expr::Bool(false), .. }) if name == "$signalSame"),
        "{same:?}"
    );
}

/// `signalDependencies` never invokes its callback, so naming one must keep no
/// reference to it: static lowering prunes every function that references a
/// static performer, and a library export has no caller to keep it (#281).
#[test]
fn a_dependency_query_binds_only_work_with_effects() {
    let named = expand(
        &call("signalDependencies", vec![ident("work")]),
        vec!["Signal<T>.read".to_owned()],
    );
    assert_eq!(bound_names(&named), ["$signalDependencies"]);
    let made = expand(
        &call("signalDependencies", vec![call("make", vec![])]),
        vec!["Signal<T>.read".to_owned()],
    );
    assert_eq!(bound_names(&made), ["$signalWork", "$signalDependencies"]);
}
