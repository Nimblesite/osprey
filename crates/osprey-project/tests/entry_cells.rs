//! [MODULES-FILE-SCOPE-BINDING]: the selected entry source of a project may hold
//! handler-owned cells at file scope, as a single-file program can, so an
//! application library keeps state between host calls. Every other source keeps
//! [MODULES-STATE-TOPLEVEL].
mod support;

use osprey_ast::Stmt;
use osprey_project::assemble;
use osprey_syntax::Flavor;
use support::{config, parsed};

const ENTRY: &str = "namespace app\n\
effect Keep\n    keep : int => Unit\n\
mut live = 0\n\
bump n =\n    handle Keep\n        keep value => live := value\n    perform Keep.keep n\n    live\n";

/// The entry's cell is declared before the functions that read it and keeps its own name.
#[test]
fn an_entry_source_cell_is_declared_ahead_of_its_readers() {
    let sources = [parsed("main.ospml", Flavor::Ml, ENTRY)];
    let project = assemble(&config("main.ospml"), &sources)
        .unwrap_or_else(|errors| panic!("an entry cell must assemble: {errors:?}"));
    let cell = project.program.statements.iter().position(
        |statement| matches!(statement, Stmt::Let { name, mutable: true, .. } if name == "live"),
    );
    let first_function = project
        .program
        .statements
        .iter()
        .position(|statement| matches!(statement, Stmt::Function { .. }));
    assert!(cell.is_some(), "the cell is in the program");
    assert!(first_function.is_some(), "the reader is in the program");
    assert!(
        cell < first_function,
        "the cell comes before every function, so its readers see it"
    );
}

/// A cell in any other source of the project is still namespace-level state.
#[test]
fn a_cell_outside_the_entry_source_is_rejected() {
    let sources = [
        parsed("main.ospml", Flavor::Ml, "namespace app\nanswer x = x\n"),
        parsed(
            "src/state.ospml",
            Flavor::Ml,
            "namespace app\nmut hidden = 0\n",
        ),
    ];
    let errors =
        assemble(&config("main.ospml"), &sources).expect_err("a non-entry cell is rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("namespace-level `mut` is forbidden")),
        "{errors:?}"
    );
}

/// Only the entry source's own functions may assign its cell.
#[test]
fn another_source_cannot_assign_the_entry_cell() {
    let sources = [
        parsed("main.ospml", Flavor::Ml, ENTRY),
        parsed("src/other.ospml", Flavor::Ml, "namespace app\nset n =\n    handle Keep\n        keep value => live := value\n    perform Keep.keep n\n"),
    ];
    let errors =
        assemble(&config("main.ospml"), &sources).expect_err("a foreign assignment is rejected");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("cannot assign to module declaration `live`")),
        "{errors:?}"
    );
}

/// With a `main`, the entry's other statements still run inside it, but its cells keep module
/// storage, so the entry's other functions read and write the same cell.
#[test]
fn an_entry_cell_beside_main_has_module_storage() {
    let text = format!("{ENTRY}main () =\n    print \"${{bump 2}}\"\n    0\n");
    let sources = [parsed("main.ospml", Flavor::Ml, &text)];
    let project = assemble(&config("main.ospml"), &sources)
        .unwrap_or_else(|errors| panic!("an entry cell beside main must assemble: {errors:?}"));
    assert!(
        matches!(project.program.statements.first(), Some(Stmt::Let { name, mutable: true, .. }) if name == "live"),
        "the cell leads the program: {:?}",
        project.program.statements.first()
    );
}
