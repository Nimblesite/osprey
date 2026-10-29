//! Explicit generic effect mentions resolve heads and argument types separately.

mod support;

use osprey_ast::{walk_program, AstVisitor, Expr, Stmt};
use osprey_project::{assemble, SourceFile};
use osprey_syntax::{parse_program_with_flavor, Flavor};

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
        discipline: None,
    }
}

fn project(entry: &str, library: &str) -> osprey_ast::Program {
    let result = assemble(
        &support::config("main.osp"),
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
fn module_function_aliases_keep_the_original_generic_scheme() {
    // Implements [MODULES-INIT], [TYPE-FN-CLOSURE]: taking a function does not call it.
    let program = project(
        "namespace app;\nimport source::Facade\nprint(toString(Facade::identity(7)))\nprint(Facade::identity(\"ok\"))",
        "namespace source;\nmodule Library { export fn identity(x) = x }\nmodule Facade { export let identity = Library::identity }",
    );
    let errors = osprey_types::check_program(&program);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn module_function_aliases_preserve_call_time_effect_requirements() {
    let program = project(
        "namespace app;\nimport source::Facade\nprint(toString(Facade::read()))",
        "namespace source;\nmodule Library { export effect Secret { read: fn() -> int }\nexport fn read() = perform Secret.read() }\nmodule Facade { export let read = Library::read }",
    );
    let errors = osprey_types::check_program(&program);
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("unhandled effect operations")),
        "{errors:?}"
    );
}

#[test]
fn imported_generic_effect_primitive_instance_reaches_static_validation() {
    let program = project(
        "namespace app;\nimport source::Library::{Echo}\nfn main() = {\n    handle static Echo<int> {\n        echo value => value\n    }\n    perform Echo<int>.echo(42)\n}",
        "namespace source;\nmodule Library { export effect Echo<T> { echo: fn(T) -> T } }",
    );
    let errors = osprey_types::check_program(&program);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn qualified_generic_effect_arguments_resolve_imported_types() {
    let program = project(
        "namespace app;\nimport source::Library as L\nfn echo(value: L::Marker) = {\n    handle static L::Echo<L::Marker> {\n        echo item => item\n    }\n    perform L::Echo<L::Marker>.echo(value)\n}\nfn main() = 0",
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
