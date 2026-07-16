#![expect(
    dead_code,
    reason = "this support module is compiled independently by focused integration-test crates"
)]

use osprey_ast::{
    EffectOperation, Expr, HandlerArm, ModuleItem, ModuleKind, Program, Stmt, SymbolPath,
    Visibility,
};
use osprey_project::{assemble, ProjectConfig, SourceFile};
use osprey_syntax::{parse_program_with_flavor, Flavor};
use std::path::PathBuf;

pub(crate) fn config(entry: &str) -> ProjectConfig {
    ProjectConfig {
        name: "app".to_string(),
        source_roots: vec![PathBuf::from("src")],
        default_namespace: Some("app".to_string()),
        entry: Some(PathBuf::from(entry)),
        flavor: None,
        allow_wildcard_imports: false,
    }
}

pub(crate) fn parsed(path: &str, flavor: Flavor, text: &str) -> SourceFile {
    let parsed = parse_program_with_flavor(text, flavor);
    assert!(
        parsed.errors.is_empty(),
        "parse errors: {:?}",
        parsed.errors
    );
    SourceFile {
        path: PathBuf::from(path),
        flavor,
        source: text.to_string(),
        program: parsed.program,
    }
}

pub(crate) fn ast(path: &str, statements: Vec<Stmt>) -> SourceFile {
    SourceFile {
        path: PathBuf::from(path),
        flavor: Flavor::Default,
        source: "\n".repeat(statements.len().max(1)),
        program: Program { statements },
    }
}

pub(crate) fn error_messages(config: &ProjectConfig, sources: &[SourceFile]) -> Vec<String> {
    match assemble(config, sources) {
        Ok(_) => Vec::new(),
        Err(errors) => errors.into_iter().map(|error| error.message).collect(),
    }
}

pub(crate) fn contains(messages: &[String], expected: &str) -> bool {
    messages.iter().any(|message| message.contains(expected))
}

pub(crate) fn state_module(
    name: &str,
    initializer: Expr,
    extra_items: Vec<ModuleItem>,
    installer_body: Expr,
) -> Stmt {
    let mut body = vec![
        item(
            Visibility::Private,
            Stmt::Let {
                name: "count".to_string(),
                mutable: true,
                ty: None,
                value: initializer,
                doc: None,
                position: None,
            },
        ),
        item(
            Visibility::Exported,
            Stmt::Effect {
                name: "CounterFx".to_string(),
                type_params: Vec::new(),
                operations: vec![EffectOperation {
                    name: "next".to_string(),
                    ty: "fn() -> int".to_string(),
                    parameters: Vec::new(),
                    return_type: "int".to_string(),
                }],
                doc: None,
                position: None,
            },
        ),
    ];
    body.extend(extra_items);
    body.push(item(Visibility::Exported, function("run", installer_body)));
    Stmt::Module {
        path: SymbolPath::single(name),
        kind: ModuleKind::State,
        signature: None,
        body,
        doc: None,
        position: None,
    }
}

pub(crate) fn handler(effect: &str, arm_body: Expr) -> Expr {
    Expr::Handler {
        effect: effect.to_string(),
        arms: vec![HandlerArm {
            operation: "next".to_string(),
            params: Vec::new(),
            body: arm_body,
        }],
        body: Box::new(Expr::Bool(true)),
        position: None,
    }
}

pub(crate) fn function(name: &str, body: Expr) -> Stmt {
    Stmt::Function {
        name: name.to_string(),
        type_params: Vec::new(),
        parameters: Vec::new(),
        return_type: None,
        effects: Vec::new(),
        body,
        doc: None,
        position: None,
    }
}

pub(crate) fn item(visibility: Visibility, statement: Stmt) -> ModuleItem {
    ModuleItem {
        visibility,
        opaque: false,
        declaration: Box::new(statement),
    }
}

pub(crate) fn import(
    namespace: &str,
    path: &[&str],
    alias: Option<&str>,
    selection: osprey_ast::ImportSelection,
) -> Stmt {
    Stmt::Import(osprey_ast::ImportDecl {
        target: osprey_ast::ImportTarget {
            namespace: osprey_ast::NamespaceName::Identifier(namespace.to_string()),
            path: SymbolPath::new(path.iter().copied()),
        },
        alias: alias.map(str::to_string),
        selection,
        position: None,
    })
}

pub(crate) fn path(parts: &[&str]) -> Expr {
    Expr::Path(SymbolPath::new(parts.iter().copied()))
}
