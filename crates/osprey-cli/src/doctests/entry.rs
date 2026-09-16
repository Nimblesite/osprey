//! Suppress automatic entry execution while keeping explicit calls to `main` valid.
//! Implements [DOC-DOCTEST-HARNESS].

use osprey_ast::{Expr, Stmt};

pub(super) fn qualify(
    sources: &mut [osprey_project::SourceFile],
    config: Option<&osprey_project::ProjectConfig>,
) {
    let mut paths = std::collections::BTreeSet::new();
    for source in &*sources {
        let root = source
            .path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let default = osprey_project::ProjectConfig::for_root(root);
        let config = config.unwrap_or(&default);
        let namespace = config.default_namespace.as_ref().unwrap_or(&config.name);
        main_paths(&source.program.statements, namespace, &mut paths);
    }
    for source in sources {
        for statement in &mut source.program.statements {
            osprey_ast::mutate::statement_children_mut(statement, &mut |expression| {
                rewrite_path(expression, &paths);
            });
        }
    }
}

fn main_paths(
    statements: &[Stmt],
    namespace: &str,
    paths: &mut std::collections::BTreeSet<String>,
) {
    for statement in statements {
        match statement {
            Stmt::Function { name, .. } if name == "main" => {
                let _ = paths.insert(format!("{namespace}::main"));
            }
            Stmt::Namespace { name, body, .. } => main_paths(body, name.label(), paths),
            _ => (),
        }
    }
}

fn rewrite_path(expression: &mut Expr, paths: &std::collections::BTreeSet<String>) {
    if let Expr::Path(path) = expression {
        if paths.contains(&path.segments.join("::")) {
            if let Some(last) = path.segments.last_mut() {
                *last = osprey_ast::generated_name("documentation_application_main", 0);
            }
        }
    }
    osprey_ast::mutate::children_mut(expression, &mut |child| rewrite_path(child, paths));
}

pub(super) fn discard(statements: &mut Vec<Stmt>) {
    statements
        .retain(|statement| !matches!(statement, Stmt::Expr { .. } | Stmt::Assignment { .. }));
    let replacement = osprey_ast::generated_name("documentation_application_main", 0);
    let mut has_main = false;
    for statement in statements.iter_mut() {
        if let Stmt::Function { name, .. } = statement {
            if name == "main" {
                name.clone_from(&replacement);
                has_main = true;
            }
        }
    }
    for statement in statements {
        if has_main {
            bind(statement, &replacement);
        }
        if let Stmt::Namespace { body, .. } = statement {
            discard(body);
        }
    }
}

fn bind(statement: &mut Stmt, replacement: &str) {
    match statement {
        Stmt::Function {
            parameters, body, ..
        } if !parameters.iter().any(|parameter| parameter.name == "main") => {
            bind_expression(body, replacement);
        }
        Stmt::Let { value, .. } => bind_expression(value, replacement),
        Stmt::Module { body, .. } if !body.iter().any(|item| shadows_main(&item.declaration)) => {
            for item in body {
                bind(&mut item.declaration, replacement);
            }
        }
        _ => (),
    }
}

fn shadows_main(statement: &Stmt) -> bool {
    matches!(statement, Stmt::Function { name, .. } | Stmt::Let { name, .. } if name == "main")
}

fn bind_expression(expression: &mut Expr, replacement: &str) {
    let mut free = std::collections::BTreeSet::new();
    osprey_ast::freevars::free_idents(expression, &mut free);
    if !free.contains("main") {
        return;
    }
    let body = std::mem::replace(expression, Expr::Identifier(String::new()));
    *expression = Expr::Block {
        statements: vec![Stmt::Let {
            name: "main".into(),
            mutable: false,
            ty: None,
            value: Expr::Identifier(replacement.into()),
            doc: None,
            position: None,
        }],
        value: Some(Box::new(body)),
    };
}
