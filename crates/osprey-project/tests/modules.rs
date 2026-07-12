//! Project resolver coverage for module graph, imports, signatures, and entry rules.

mod support;

use osprey_ast::{
    Expr, ImportMember, ImportSelection, ModuleItem, ModuleKind, NamespaceName, Parameter,
    SignatureAscription, SignatureItem, SignatureType, Stmt, SymbolPath, TypeExpr, Visibility,
};
use osprey_project::assemble;
use osprey_syntax::Flavor;
use support::{ast, config, contains, error_messages, function, import, item, parsed};

#[test]
fn mixed_flavors_resolve_by_namespace_not_file_path() {
    // Implements [MODULES-FLAVOR-PROJECTION], [MODULES-PATH-INDEPENDENCE], [MODULES-IMPORT].
    let library = parsed(
        "src/completely/unrelated/location.ospml",
        Flavor::Ml,
        "namespace billing\nmodule Tax\n    export addTax cents = cents + 1\n",
    );
    let entry = parsed(
        "src/main.osp",
        Flavor::Default,
        "namespace app;\nimport billing::Tax::{addTax}\nfn main() = addTax(1)\n",
    );
    let result = assemble(&config("src/main.osp"), &[entry, library]);
    assert!(result.is_ok(), "assembly errors: {result:?}");
    if let Ok(project) = result {
        assert!(project
            .source_name_by_mangled
            .values()
            .any(|name| name == "billing::Tax::addTax"));
        assert!(project.program.statements.iter().all(|statement| {
            !matches!(
                statement,
                Stmt::Module { .. } | Stmt::Namespace { .. } | Stmt::Import(_)
            )
        }));
    }
}

#[test]
fn private_member_import_is_rejected() {
    // Implements [MODULES-EXPORTS], [MODULES-IMPORT].
    let library = parsed(
        "lib.osp",
        Flavor::Default,
        "namespace billing;\nmodule Tax { fn secret() = 1 }\n",
    );
    let entry = parsed(
        "main.osp",
        Flavor::Default,
        "import billing::Tax::{secret}\nfn main() = 0\n",
    );
    let messages = error_messages(&config("main.osp"), &[library, entry]);
    assert!(
        contains(&messages, "private imported member"),
        "{messages:?}"
    );
}

#[test]
fn duplicate_declarations_across_open_namespace_contributions_are_rejected() {
    // Implements [MODULES-NAMESPACE], [MODULES-EXPORTS].
    let first = parsed(
        "one.osp",
        Flavor::Default,
        "namespace shared;\nfn clash() = 1\n",
    );
    let second = parsed("two.ospml", Flavor::Ml, "namespace shared\nclash () = 2\n");
    let messages = error_messages(&config("one.osp"), &[first, second]);
    assert!(
        contains(&messages, "duplicate declaration `shared::clash`"),
        "{messages:?}"
    );
}

#[test]
fn ambiguous_member_imports_are_rejected_even_when_unused() {
    // Implements [MODULES-IMPORT].
    let modules = parsed(
        "modules.osp",
        Flavor::Default,
        "namespace lib;\nmodule A { export fn run() = 1 }\nmodule B { export fn run() = 2 }\n",
    );
    let entry = parsed(
        "main.osp",
        Flavor::Default,
        "import lib::A::{run}\nimport lib::B::{run}\nfn main() = 0\n",
    );
    let messages = error_messages(&config("main.osp"), &[modules, entry]);
    assert!(
        contains(&messages, "ambiguous import binding `run`"),
        "{messages:?}"
    );
}

#[test]
fn module_declaration_precedes_imported_member() {
    // Implements [MODULES-RESOLUTION].
    let imported = parsed(
        "imported.osp",
        Flavor::Default,
        "namespace lib;\nmodule Other { export fn value() = 1 }\n",
    );
    let entry = parsed(
        "main.osp",
        Flavor::Default,
        "namespace app;\nimport lib::Other::{value}\nfn value() = 2\nfn main() = value()\n",
    );
    let result = assemble(&config("main.osp"), &[entry, imported]);
    assert!(result.is_ok(), "assembly errors: {result:?}");
}

#[test]
fn relative_module_path_precedes_import_alias() {
    // Implements [MODULES-RESOLUTION].
    let remote = parsed(
        "remote.osp",
        Flavor::Default,
        "namespace lib;\nmodule Remote { export fn read() = 1 }\n",
    );
    let local = Stmt::Module {
        path: SymbolPath::single("Local"),
        kind: ModuleKind::Plain,
        signature: None,
        body: vec![item(
            Visibility::Exported,
            function("read", Expr::Integer(2)),
        )],
        doc: None,
        position: None,
    };
    let entry = ast(
        "main.osp",
        vec![
            import("lib", &["Remote"], Some("Local"), ImportSelection::Whole),
            local,
            function("main", support::path(&["Local", "read"])),
        ],
    );
    let result = assemble(&config("main.osp"), &[remote, entry]);
    assert!(result.is_ok(), "assembly errors: {result:?}");
}

#[test]
fn private_intermediate_module_cannot_be_traversed() {
    // Implements [MODULES-EXPORTS].
    let hidden = Stmt::Module {
        path: SymbolPath::single("Hidden"),
        kind: ModuleKind::Plain,
        signature: None,
        body: vec![item(
            Visibility::Exported,
            Stmt::Module {
                path: SymbolPath::single("Nested"),
                kind: ModuleKind::Plain,
                signature: None,
                body: vec![item(
                    Visibility::Exported,
                    function("read", Expr::Integer(1)),
                )],
                doc: None,
                position: None,
            },
        )],
        doc: None,
        position: None,
    };
    let outer = Stmt::Module {
        path: SymbolPath::single("Outer"),
        kind: ModuleKind::Plain,
        signature: None,
        body: vec![item(Visibility::Private, hidden)],
        doc: None,
        position: None,
    };
    let entry = ast(
        "main.osp",
        vec![
            outer,
            function(
                "main",
                support::path(&["app", "Outer", "Hidden", "Nested", "read"]),
            ),
        ],
    );
    let messages = error_messages(&config("main.osp"), &[entry]);
    assert!(contains(&messages, "is private"), "{messages:?}");
}

#[test]
fn explicit_function_types_must_conform_to_signature() {
    // Implements [MODULES-SIGNATURE].
    let contract = Stmt::Signature {
        name: "Api".to_string(),
        items: vec![SignatureItem::Function {
            name: "convert".to_string(),
            type_params: Vec::new(),
            parameters: vec![TypeExpr::named("int")],
            return_type: TypeExpr::named("int"),
            effects: Vec::new(),
            position: None,
        }],
        doc: None,
        position: None,
    };
    let implementation = Stmt::Function {
        name: "convert".to_string(),
        type_params: Vec::new(),
        parameters: vec![Parameter {
            name: "value".to_string(),
            ty: Some(TypeExpr::named("string")),
        }],
        return_type: Some(TypeExpr::named("int")),
        effects: Vec::new(),
        body: Expr::Integer(0),
        doc: None,
        position: None,
    };
    let module = ascribed_module("Api", vec![item(Visibility::Private, implementation)]);
    let messages = error_messages(
        &config("api.osp"),
        &[ast("api.osp", vec![contract, module])],
    );
    assert!(
        contains(&messages, "parameter types do not match"),
        "{messages:?}"
    );
}

#[test]
fn abstract_alias_is_rejected_instead_of_leaking_representation() {
    // Implements [MODULES-OPAQUE-TYPES], [MODULES-SIGNATURE].
    let contract = Stmt::Signature {
        name: "Api".to_string(),
        items: vec![SignatureItem::Type {
            name: "Id".to_string(),
            type_params: Vec::new(),
            definition: SignatureType::Abstract,
            opaque: true,
            position: None,
        }],
        doc: None,
        position: None,
    };
    let implementation = Stmt::Type {
        name: "Id".to_string(),
        type_params: Vec::new(),
        variants: Vec::new(),
        alias: Some(TypeExpr::named("int")),
        validation_func: None,
        doc: None,
        position: None,
    };
    let module = ascribed_module("Api", vec![item(Visibility::Private, implementation)]);
    let messages = error_messages(
        &config("api.osp"),
        &[ast("api.osp", vec![contract, module])],
    );
    assert!(contains(&messages, "opaque alias"), "{messages:?}");
    assert!(contains(&messages, "unsupported"), "{messages:?}");
}

#[test]
fn unknown_and_quoted_unaliased_imports_are_rejected() {
    // Implements [MODULES-IMPORT].
    let unknown = import("missing", &["Nope"], None, ImportSelection::Whole);
    let quoted = Stmt::Import(osprey_ast::ImportDecl {
        target: osprey_ast::ImportTarget {
            namespace: NamespaceName::Quoted("vendor/api".to_string()),
            path: SymbolPath::default(),
        },
        alias: None,
        selection: ImportSelection::Whole,
        position: None,
    });
    let vendor = ast(
        "vendor.osp",
        vec![Stmt::Namespace {
            name: NamespaceName::Quoted("vendor/api".to_string()),
            body: vec![function("ok", Expr::Bool(true))],
            file_scoped: false,
            position: None,
        }],
    );
    let entry = ast(
        "main.osp",
        vec![unknown, quoted, function("main", Expr::Integer(0))],
    );
    let messages = error_messages(&config("main.osp"), &[vendor, entry]);
    assert!(contains(&messages, "unknown import target"), "{messages:?}");
    assert!(contains(&messages, "requires `as Alias`"), "{messages:?}");
}

#[test]
fn non_entry_main_and_top_level_execution_are_rejected() {
    // Implements [MODULES-PROJECT].
    let entry = ast("entry.osp", vec![function("main", Expr::Integer(0))]);
    let other = ast(
        "other.osp",
        vec![
            function("main", Expr::Integer(1)),
            Stmt::Expr {
                value: Expr::Integer(2),
                position: None,
            },
        ],
    );
    let messages = error_messages(&config("entry.osp"), &[entry, other]);
    assert!(
        contains(&messages, "only allowed in the selected entry"),
        "{messages:?}"
    );
    assert!(
        contains(&messages, "only allowed in the entry source"),
        "{messages:?}"
    );
}

#[test]
fn member_alias_resolves_to_exported_declaration() {
    let library = parsed(
        "lib.osp",
        Flavor::Default,
        "namespace lib;\nmodule Values { export fn original() = 1 }\n",
    );
    let entry = ast(
        "main.osp",
        vec![
            import(
                "lib",
                &["Values"],
                None,
                ImportSelection::Members(vec![ImportMember {
                    name: "original".to_string(),
                    alias: Some("renamed".to_string()),
                }]),
            ),
            function("main", Expr::Identifier("renamed".to_string())),
        ],
    );
    let result = assemble(&config("main.osp"), &[library, entry]);
    assert!(result.is_ok(), "assembly errors: {result:?}");
}

#[test]
fn constant_initializer_cycles_fail_before_codegen() {
    // Implements [MODULES-CYCLES], [MODULES-INIT].
    let binding = |name: &str, target: &str| Stmt::Let {
        name: name.to_string(),
        mutable: false,
        ty: None,
        value: Expr::Identifier(target.to_string()),
        doc: None,
        position: None,
    };
    let source = ast(
        "main.osp",
        vec![
            binding("first", "second"),
            binding("second", "first"),
            function("main", Expr::Integer(0)),
        ],
    );
    let messages = error_messages(&config("main.osp"), &[source]);
    assert!(
        contains(&messages, "constant initializer cycle"),
        "{messages:?}"
    );
}

fn ascribed_module(signature: &str, body: Vec<ModuleItem>) -> Stmt {
    Stmt::Module {
        path: SymbolPath::single("Implementation"),
        kind: ModuleKind::Plain,
        signature: Some(SignatureAscription {
            path: SymbolPath::single(signature),
            allow_extra: false,
        }),
        body,
        doc: None,
        position: None,
    }
}
