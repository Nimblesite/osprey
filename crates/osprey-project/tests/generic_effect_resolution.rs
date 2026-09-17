//! Explicit generic effect mentions resolve heads and argument types separately.

use osprey_ast::{walk_program, AstVisitor, Expr, Stmt};
use osprey_project::{assemble, ProjectConfig, SourceFile};
use osprey_syntax::{parse_program_with_flavor, Flavor};
use std::path::PathBuf;

struct Mentions(Vec<String>);
impl AstVisitor for Mentions {
    fn expression(&mut self, expression: &Expr) {
        if let Expr::Handler { effect, .. } | Expr::Perform { effect, .. } = expression {
            self.0.push(effect.clone());
        }
    }
}

fn source(path: &str, text: &str) -> SourceFile {
    let parsed = parse_program_with_flavor(text, Flavor::Default);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    SourceFile {
        path: path.into(),
        flavor: Flavor::Default,
        source: text.into(),
        program: parsed.program,
    }
}

fn project(entry: &str, library: &str) -> osprey_ast::Program {
    let config = ProjectConfig {
        name: "app".into(),
        source_roots: vec![PathBuf::from("src")],
        default_namespace: Some("app".into()),
        entry: Some("main.osp".into()),
        flavor: None,
        allow_wildcard_imports: false,
    };
    let result = assemble(
        &config,
        &[source("main.osp", entry), source("library.osp", library)],
    );
    assert!(result.is_ok(), "{result:?}");
    result.map_or(
        osprey_ast::Program {
            statements: Vec::new(),
            doc: None,
        },
        |project| project.program,
    )
}

#[test]
fn imported_generic_effect_primitive_instance_reaches_static_validation() {
    let program = project(
        "namespace app;\nimport source::Library::{Echo}\nfn main() = handle static Echo<int> echo value => value in perform Echo<int>.echo(42)",
        "namespace source;\nmodule Library { export effect Echo<T> { echo: fn(T) -> T } }",
    );
    let errors = osprey_types::check_program(&program);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn qualified_generic_effect_arguments_resolve_imported_types() {
    let program = project(
        "namespace app;\nimport source::Library as L\nfn echo(value: L::Marker) = handle static L::Echo<L::Marker> echo item => item in perform L::Echo<L::Marker>.echo(value)\nfn main() = 0",
        "namespace source;\nmodule Library { export type Marker = { value: int }\n export effect Echo<T> { echo: fn(T) -> T } }",
    );
    let effect = program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Stmt::Effect { name, .. } => Some(name.clone()),
            _ => None,
        });
    assert!(effect.is_some(), "{program:?}");
    let mut mentions = Mentions(Vec::new());
    walk_program(&program, &mut mentions);
    assert_eq!(mentions.0.len(), 2);
    for mention in &mentions.0 {
        assert_eq!(
            osprey_ast::effect_name::base(mention),
            effect.as_deref().unwrap_or_default()
        );
        assert!(!mention.contains("L::"), "unresolved argument: {mention}");
    }
    let errors = osprey_types::check_program(&program);
    assert!(errors.is_empty(), "{errors:?}");
}
