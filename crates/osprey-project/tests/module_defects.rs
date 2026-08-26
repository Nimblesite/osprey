//! RED tests pinning module defects found on 2026-08-22.
//!
//! Every test here fails against the current compiler. That is deliberate: a
//! failing test that pins a compiler bug outranks a speculative fix, because it
//! survives refactors and turns a suspicion into an enforceable contract. Do
//! not weaken an assertion here to make one pass — fix the compiler.
//!
//! Two of these pin SILENT failures, which is the worst outcome the language
//! can produce: the program exits zero having run something other than what was
//! written. See `tests/modules/README.md` for the full write-up.

mod support;

use osprey_ast::Stmt;
use osprey_project::{assemble, SourceFile};
use osprey_syntax::{parse_program_with_flavor, Flavor};
use std::path::PathBuf;
use support::config;

/// Parse and assemble one written source as a single-file project.
fn project(flavor: Flavor, text: &str) -> Result<osprey_project::AssembledProject, Vec<String>> {
    let name = match flavor {
        Flavor::Ml => "main.ospml",
        Flavor::Default => "main.osp",
    };
    let parsed = parse_program_with_flavor(text, flavor);
    assert!(
        parsed.errors.is_empty(),
        "parse errors: {:?}",
        parsed.errors
    );
    let source = SourceFile {
        path: PathBuf::from(name),
        flavor,
        source: text.to_string(),
        program: parsed.program,
    };
    assemble(&config(name), &[source]).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
    })
}

/// Source-level names the assembler kept for the flattened program.
fn source_names(project: &osprey_project::AssembledProject) -> Vec<String> {
    let mut names: Vec<String> = project.source_name_by_mangled.values().cloned().collect();
    names.sort();
    names
}

/// Statements the flattened program will actually execute.
fn executable_count(project: &osprey_project::AssembledProject) -> usize {
    project
        .program
        .statements
        .iter()
        .filter(|statement| matches!(statement, Stmt::Expr { .. }))
        .count()
}

/// Two file-scoped namespace headers in one ML source. [MODULES-FILE-SCOPED-NAMESPACE]
/// says each applies to the declarations that follow it, so `two::B::v` must
/// exist and both `print` statements must run.
///
/// DEFECT: the ML lowering nests the second header inside the first namespace's
/// body instead of closing it, and `osprey_project::contribution` only walks
/// TOP-LEVEL `Stmt::Namespace`. The nested contribution is filed as an ordinary
/// declaration, so `two::B::v` never registers and every statement written
/// after the second header is dropped. The program still exits ZERO printing
/// nothing at all — a silent failure, not a diagnostic.
#[test]
fn ml_second_file_scoped_namespace_keeps_its_declarations_and_statements() {
    let source = concat!(
        "namespace one\n\n",
        "module A\n    export v () = 1\n\n",
        "namespace two\n\n",
        "module B\n    export v () = 2\n\n",
        "print \"${one::A::v ()}\"\n",
        "print \"${two::B::v ()}\"\n",
    );
    let assembled = project(Flavor::Ml, source).expect("two file-scoped namespaces must assemble");
    let names = source_names(&assembled);
    assert!(
        names.iter().any(|name| name == "one::A::v"),
        "first namespace lost its declaration: {names:?}"
    );
    assert!(
        names.iter().any(|name| name == "two::B::v"),
        "second file-scoped namespace did not take effect: {names:?}"
    );
    assert_eq!(
        executable_count(&assembled),
        2,
        "statements after the second namespace header were silently dropped"
    );
}

/// The Default spelling of the same program. [MODULES-FILE-SCOPED-NAMESPACE]
///
/// DEFECT: the second `namespace two;` header is ignored outright, so `module B`
/// is filed under `one`. Nothing is reported; the declaration simply answers to
/// `one::B::v`, a name the source never wrote. A program that asks for
/// `two::B::v` is then told the path is unknown, blaming the call site for the
/// assembler's misfiling.
#[test]
fn default_second_file_scoped_namespace_rescopes_the_declarations_after_it() {
    let source = concat!(
        "namespace one;\n\n",
        "module A { export fn v() = 1 }\n\n",
        "namespace two;\n\n",
        "module B { export fn v() = 2 }\n",
    );
    let assembled =
        project(Flavor::Default, source).expect("two file-scoped namespaces must assemble");
    let names = source_names(&assembled);
    assert!(
        names.iter().any(|name| name == "two::B::v"),
        "second file-scoped namespace did not rescope `B`: {names:?}"
    );
    assert!(
        !names.iter().any(|name| name == "one::B::v"),
        "`B` was misfiled into the first namespace: {names:?}"
    );
}

/// [MODULES-ENTRYPOINT]: "a `main` and a top-level executable statement cannot
/// share it: candidates 2 and 3 would both select that source and only one of
/// them could run". `examples/failscompilation/main_beside_top_level_statement.ospo`
/// pins the rejection for a plain program.
///
/// DEFECT: a source carrying a namespace routes through project assembly, where
/// the check is missing. Both entries are then kept and BOTH run — the statement
/// first, then `main` — which is the exact "only one of them could run"
/// situation the rule exists to prevent.
#[test]
fn namespaced_main_beside_a_top_level_statement_is_rejected() {
    let source = concat!(
        "namespace app\n\n",
        "main () = print \"main ran\"\n\n",
        "print \"statement ran\"\n",
    );
    let errors = project(Flavor::Ml, source).err().unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|message| message.contains("cannot sit beside `main`")),
        "entry conflict went unreported: {errors:?}"
    );
}

/// [MODULES-EXPORTS] makes a module's `export`ed items its public surface, and
/// [MODULES-OPAQUE-TYPES] singles out OPAQUE union constructors as the private
/// ones — which only means anything if a plain exported union's constructors
/// are public.
///
/// DEFECT: a union declared inside a module has no reachable constructors at
/// all. Neither the bare name nor the qualified path resolves, so an exported
/// type can only be built by also exporting a factory function for every
/// variant. The type is exported; the only way to make a value of it is not.
#[test]
fn constructors_of_an_exported_module_union_are_reachable() {
    let source = concat!(
        "namespace shapes\n\n",
        "module Geo\n",
        "    export type Shape = Circle(radius : int) | Square(side : int)\n",
        "    export area shape = match shape\n",
        "        Circle radius => (radius * radius) ?: 0\n",
        "        Square side => (side * side) ?: 0\n\n",
        "print \"${Geo::area (Geo::Circle(radius = 3))}\"\n",
    );
    let assembled = project(Flavor::Ml, source).expect("module union must assemble");
    let errors: Vec<String> = osprey_types::check_program(&assembled.program)
        .into_iter()
        .map(|error| error.message)
        .collect();
    assert!(
        errors.is_empty(),
        "an exported union's constructors are unreachable: {errors:?}"
    );
}
