//! State-module ownership and initializer regression coverage.

mod support;

use osprey_ast::{Expr, ImportSelection, ModuleKind, Stmt, SymbolPath, Visibility};
use osprey_project::assemble;
use support::{
    ast, config, contains, error_messages, function, handler, import, item, path, state_module,
};

#[test]
fn exact_owned_handler_gets_fresh_installer_cells() {
    // Implements [MODULES-STATE-MODULE], [MODULES-INIT].
    let state = state_module(
        "Counter",
        Expr::Integer(0),
        Vec::new(),
        handler("CounterFx", path(&["Counter", "count"])),
    );
    let result = assemble(&config("state.osp"), &[ast("state.osp", vec![state])]);
    assert!(result.is_ok(), "assembly errors: {result:?}");
    if let Ok(project) = result {
        assert!(project.program.statements.iter().any(|statement| {
            matches!(statement, Stmt::Function { body: Expr::Block { statements, .. }, .. }
                if statements.iter().any(|statement| matches!(statement, Stmt::Let { name, mutable: true, .. } if name == "count")))
        }));
    }
}

#[test]
fn ordinary_state_read_and_write_are_rejected() {
    // Implements [MODULES-STATE-SOURCE-OF-TRUTH].
    let read = item(
        Visibility::Private,
        function("readOutside", Expr::Identifier("count".to_string())),
    );
    let write = item(
        Visibility::Private,
        function(
            "writeOutside",
            Expr::Block {
                statements: vec![Stmt::Assignment {
                    name: "count".to_string(),
                    value: Expr::Integer(1),
                    position: None,
                }],
                value: None,
            },
        ),
    );
    let state = state_module(
        "Counter",
        Expr::Integer(0),
        vec![read, write],
        handler("CounterFx", Expr::Identifier("count".to_string())),
    );
    let messages = error_messages(&config("state.osp"), &[ast("state.osp", vec![state])]);
    assert!(
        contains(&messages, "only accessible inside"),
        "{messages:?}"
    );
}

#[test]
fn lambda_and_spawn_cannot_create_handler_factories() {
    // Implements [MODULES-STATE-MODULE].
    let lambda = state_module(
        "LambdaCounter",
        Expr::Integer(0),
        Vec::new(),
        Expr::Lambda {
            parameters: Vec::new(),
            return_type: None,
            body: Box::new(handler("CounterFx", Expr::Identifier("count".to_string()))),
            position: None,
        },
    );
    let lambda_messages = error_messages(&config("lambda.osp"), &[ast("lambda.osp", vec![lambda])]);
    assert!(
        contains(&lambda_messages, "exported handler installer"),
        "{lambda_messages:?}"
    );
    assert!(
        contains(&lambda_messages, "only accessible inside"),
        "{lambda_messages:?}"
    );

    let spawned = state_module(
        "SpawnCounter",
        Expr::Integer(0),
        Vec::new(),
        Expr::Spawn(Box::new(handler(
            "CounterFx",
            Expr::Identifier("count".to_string()),
        ))),
    );
    let spawn_messages = error_messages(&config("spawn.osp"), &[ast("spawn.osp", vec![spawned])]);
    assert!(
        contains(&spawn_messages, "exported handler installer"),
        "{spawn_messages:?}"
    );
    assert!(
        contains(&spawn_messages, "only accessible inside"),
        "{spawn_messages:?}"
    );
}

#[test]
fn differently_qualified_same_tail_effect_does_not_authorize_state() {
    // Implements [MODULES-STATE-MODULE].
    let state = state_module(
        "Counter",
        Expr::Integer(0),
        Vec::new(),
        handler("Other::CounterFx", Expr::Identifier("count".to_string())),
    );
    let messages = error_messages(&config("state.osp"), &[ast("state.osp", vec![state])]);
    assert!(
        contains(&messages, "only accessible inside"),
        "{messages:?}"
    );
    assert!(
        contains(&messages, "exported handler installer"),
        "{messages:?}"
    );
}

#[test]
fn alias_qualified_cell_cannot_bypass_ownership_check() {
    // Implements [MODULES-STATE-SOURCE-OF-TRUTH].
    let leaked = item(Visibility::Private, function("leak", path(&["C", "count"])));
    let state = state_module(
        "Counter",
        Expr::Integer(0),
        vec![leaked],
        handler("C::CounterFx", path(&["C", "count"])),
    );
    let source = ast(
        "state.osp",
        vec![
            import("app", &["Counter"], Some("C"), ImportSelection::Whole),
            state,
        ],
    );
    let messages = error_messages(&config("state.osp"), &[source]);
    assert!(
        contains(&messages, "only accessible inside"),
        "{messages:?}"
    );
}

#[test]
fn state_initializer_must_be_pure_and_dependency_free() {
    // Implements [MODULES-INIT].
    let effectful = Expr::Call {
        function: Box::new(Expr::Identifier("load".to_string())),
        arguments: Vec::new(),
        named_arguments: Vec::new(),
    };
    let state = state_module(
        "Counter",
        effectful,
        Vec::new(),
        handler("CounterFx", Expr::Identifier("count".to_string())),
    );
    let messages = error_messages(&config("state.osp"), &[ast("state.osp", vec![state])]);
    assert!(contains(&messages, "must be pure"), "{messages:?}");
}

#[test]
fn wildcard_import_from_state_module_is_always_rejected() {
    // Implements [MODULES-IMPORT], [MODULES-STATE-MODULE].
    let state = state_module(
        "Counter",
        Expr::Integer(0),
        Vec::new(),
        handler("CounterFx", Expr::Identifier("count".to_string())),
    );
    let entry = ast(
        "main.osp",
        vec![
            import("app", &["Counter"], None, ImportSelection::Wildcard),
            function("main", Expr::Integer(0)),
        ],
    );
    let mut project = config("main.osp");
    project.allow_wildcard_imports = true;
    let messages = error_messages(&project, &[ast("state.osp", vec![state]), entry]);
    assert!(
        contains(&messages, "wildcard imports from state modules"),
        "{messages:?}"
    );

    let namespace_state = state_module(
        "Counter",
        Expr::Integer(0),
        Vec::new(),
        handler("CounterFx", Expr::Identifier("count".to_string())),
    );
    let namespace_entry = ast(
        "main.osp",
        vec![
            import("app", &[], None, ImportSelection::Wildcard),
            function("main", Expr::Integer(0)),
        ],
    );
    let namespace_messages = error_messages(
        &project,
        &[ast("state.osp", vec![namespace_state]), namespace_entry],
    );
    assert!(
        contains(&namespace_messages, "wildcard imports from state modules"),
        "{namespace_messages:?}"
    );
}

#[test]
fn nested_module_handler_cannot_qualify_as_outer_installer() {
    // Implements [MODULES-STATE-MODULE].
    let nested = Stmt::Module {
        path: SymbolPath::single("Nested"),
        kind: ModuleKind::Plain,
        signature: None,
        body: vec![item(
            Visibility::Private,
            function(
                "run",
                handler("CounterFx", Expr::Identifier("count".to_string())),
            ),
        )],
        doc: None,
        position: None,
    };
    let state = state_module(
        "Counter",
        Expr::Integer(0),
        vec![item(Visibility::Private, nested)],
        Expr::Bool(true),
    );
    let messages = error_messages(&config("state.osp"), &[ast("state.osp", vec![state])]);
    assert!(
        contains(&messages, "exported handler installer"),
        "{messages:?}"
    );
    assert!(
        contains(&messages, "only accessible inside"),
        "{messages:?}"
    );
}

#[test]
fn namespace_cannot_have_two_state_owners() {
    // Implements [MODULES-STATE-MODULE].
    let first = empty_state("First");
    let second = empty_state("Second");
    let messages = error_messages(
        &config("state.osp"),
        &[ast("state.osp", vec![first, second])],
    );
    assert!(
        contains(&messages, "at most one state module"),
        "{messages:?}"
    );
}

#[test]
fn private_effect_is_not_a_valid_state_surface() {
    // Implements [MODULES-STATE-MODULE].
    let mut state = state_module(
        "Counter",
        Expr::Integer(0),
        Vec::new(),
        handler("CounterFx", Expr::Identifier("count".to_string())),
    );
    if let Stmt::Module { body, .. } = &mut state {
        if let Some(effect) = body
            .iter_mut()
            .find(|item| matches!(item.declaration.as_ref(), Stmt::Effect { .. }))
        {
            effect.visibility = Visibility::Private;
        }
    }
    let messages = error_messages(&config("state.osp"), &[ast("state.osp", vec![state])]);
    assert!(contains(&messages, "exported owned effect"), "{messages:?}");
}

#[test]
fn module_mutation_boundaries_preserve_ordinary_local_mut() {
    // Implements [MODULES-STATE-TOPLEVEL], [MODULES-STATE-MODULE].
    let mutable = |name: &str| Stmt::Let {
        name: name.to_string(),
        mutable: true,
        ty: None,
        value: Expr::Integer(0),
        doc: None,
        position: None,
    };
    let namespace_messages = error_messages(
        &config("namespace.osp"),
        &[ast("namespace.osp", vec![mutable("global")])],
    );
    assert!(
        contains(&namespace_messages, "namespace-level `mut`"),
        "{namespace_messages:?}"
    );

    let plain = Stmt::Module {
        path: SymbolPath::single("Plain"),
        kind: ModuleKind::Plain,
        signature: None,
        body: vec![item(Visibility::Private, mutable("cell"))],
        doc: None,
        position: None,
    };
    let plain_messages = error_messages(&config("plain.osp"), &[ast("plain.osp", vec![plain])]);
    assert!(
        contains(&plain_messages, "plain modules cannot declare"),
        "{plain_messages:?}"
    );

    let mut exported = state_module(
        "Counter",
        Expr::Integer(0),
        Vec::new(),
        handler("CounterFx", Expr::Identifier("count".to_string())),
    );
    if let Stmt::Module { body, .. } = &mut exported {
        if let Some(cell) = body.first_mut() {
            cell.visibility = Visibility::Exported;
        }
    }
    let export_messages =
        error_messages(&config("export.osp"), &[ast("export.osp", vec![exported])]);
    assert!(
        contains(&export_messages, "`export mut` is forbidden"),
        "{export_messages:?}"
    );

    let local_main = function(
        "main",
        Expr::Block {
            statements: vec![
                mutable("local"),
                Stmt::Assignment {
                    name: "local".to_string(),
                    value: Expr::Integer(1),
                    position: None,
                },
            ],
            value: Some(Box::new(Expr::Identifier("local".to_string()))),
        },
    );
    let result = assemble(&config("local.osp"), &[ast("local.osp", vec![local_main])]);
    assert!(result.is_ok(), "local mut assembly errors: {result:?}");
}

fn empty_state(name: &str) -> Stmt {
    Stmt::Module {
        path: SymbolPath::single(name),
        kind: ModuleKind::State,
        signature: None,
        body: Vec::new(),
        doc: None,
        position: None,
    }
}
