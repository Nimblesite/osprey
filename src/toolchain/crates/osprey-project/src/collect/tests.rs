use super::collect;
use crate::contribution::Contribution;
use osprey_ast::NamespaceName;
use osprey_syntax::parse_program;

/// Collect a single-file project and return its graph.
fn graph_of(source: &str) -> crate::model::ProjectGraph {
    let statements = parse_program(source).program.statements;
    let contributions = vec![Contribution {
        position: None,
        source: 0,
        namespace: NamespaceName::Identifier("app".to_owned()),
        imports: Vec::new(),
        statements,
    }];
    collect(&contributions, &[], None).0
}

#[test]
fn a_modules_own_documentation_survives_project_collection() {
    // Collecting synthesizes the implementation node the graph stores. It
    // used to build that node with `doc: None`, so every module's
    // documentation was erased the moment the file was loaded as part of a
    // PROJECT rather than on its own — the docs parsed, then vanished with
    // nothing to report it. Both scopes' docs must survive.
    // Implements [DOC-ATTACH], [DOC-SIGIL-INNER].
    let graph = graph_of(
        "/// Outer module doc.\n\
         module Storage {\n\
           //! Inner module doc.\n\
           export let value = 1\n\
         }\n",
    );
    let implementation = graph
        .implementations
        .values()
        .find(|stmt| matches!(stmt, osprey_ast::Stmt::Module { .. }))
        .expect("the module implementation is recorded");
    let osprey_ast::Stmt::Module { doc, inner_doc, .. } = implementation else {
        panic!("expected a module implementation, got {implementation:?}");
    };
    assert_eq!(
        doc.as_ref().map(|d| d.summary.as_str()),
        Some("Outer module doc.")
    );
    assert_eq!(
        inner_doc.as_ref().map(|d| d.summary.as_str()),
        Some("Inner module doc.")
    );
}
