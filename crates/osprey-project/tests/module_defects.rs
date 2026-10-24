//! Regression tests for module defects found on 2026-08-22.
//!
//! The original failures are fixed. Preserve every assertion: namespace scope,
//! entrypoint selection and exported constructors remain mandatory contracts.
//! See `tests/modules/README.md` for current coverage.

mod support;

use osprey_ast::Stmt;
use osprey_project::{assemble, SourceFile};
use osprey_syntax::{parse_program_with_flavor, Flavor};
use std::path::PathBuf;
use support::config;

#[test]
fn imported_record_calls_use_declaration_identity_instead_of_capitalization(
) -> Result<(), Vec<osprey_project::ProjectError>> {
    let library = support::parsed("values.osp", Flavor::Default, "namespace ledger;\nmodule Values { export type Box<T> = { amount: T }\nexport type Token = Ready\nexport fn Upper(work) = work()\nexport fn Empty() = 42 }\n");
    let main = support::parsed("main.py", Flavor::Python, "# osprey: namespace=app\nfrom ledger import Values as V\nfrom ledger.Values import Box, Upper\ndef main():\n    token = V.Ready()\n    one = V.Box[int](amount=41)\n    two = Box(amount=\"two\")\n    print(f\"{one.amount}:{two.amount}:{Upper(lambda: 42)}:{V.Empty()}\")\n");
    let project = assemble(&config("main.py"), &[main, library])?;
    let errors = osprey_types::check_program(&project.program);
    assert!(errors.is_empty(), "{errors:?}");
    Ok(())
}

#[test]
fn dotted_module_members_and_record_fields_share_lexical_resolution(
) -> Result<(), Vec<osprey_project::ProjectError>> {
    let library = support::parsed("values.osp", Flavor::Default, "namespace ledger;\nmodule Values { export type Box = { amount: int }\nexport fn make(n) = Box { amount: n }\nexport let defaults = Box { amount: 3 } }\n");
    let main = support::parsed("main.py", Flavor::Python, "# osprey: namespace=app\nfrom ledger import Values as V\nfrom ledger.Values import defaults\nimport ledger as Ledger\ndef amount(V):\n    return V.amount\ndef main():\n    print(f\"{amount(V.make(41))}:{Ledger.Values.make(42).amount}:{defaults.amount}\")\n");
    let project = assemble(&config("main.py"), &[main, library])?;
    let errors = osprey_types::check_program(&project.program);
    assert!(errors.is_empty(), "{errors:?}");
    Ok(())
}

/// Parse and assemble one written source as a single-file project.
fn project(flavor: Flavor, text: &str) -> Result<osprey_project::AssembledProject, Vec<String>> {
    let name = match flavor {
        Flavor::Ml => "main.ospml",
        Flavor::Default => "main.osp",
        Flavor::Python => "main.py",
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
/// Original defect: the ML lowering nests the second header inside the first namespace's
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
/// Original defect: the second `namespace two;` header is ignored outright, so `module B`
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
/// Original defect: a source carrying a namespace routes through project assembly, where
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
/// Original defect: a union declared inside a module has no reachable constructors at
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
        "        Circle radius => satMul radius radius\n",
        "        Square side => satMul side side\n\n",
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

// [TYPE-ANNOTATION-REDUNDANT], [MODULES-SIGNATURE]: aliases retain the
// distinction between a contract and an annotation actually written on a body.
#[test]
fn alias_expansion_preserves_contract_annotations() -> Result<(), String> {
    for (flavor, source) in [
        (Flavor::Default, "signature Api { type Number = int\n fn next(value: Number) -> Number }\nmodule M : Api { type Number = int\n fn next(value) = wrapAdd(value, 1) }\n"),
        (Flavor::Ml, "signature Api\n    type Number = int\n    next : Number -> Number\nmodule M : Api\n    type Number = int\n    next value = wrapAdd value 1\n"),
    ] {
        let assembled = project(flavor, source).map_err(|errors| errors.join("\n"))?;
        let errors = osprey_types::check_program(&assembled.program);
        assert!(errors.is_empty(), "{flavor:?}: {errors:?}");
        let warnings = osprey_types::redundant_annotations(&assembled.program);
        assert!(warnings.is_empty(), "{flavor:?}: {warnings:?}");
    }
    Ok(())
}

#[test]
fn alias_expansion_preserves_separate_written_annotation_sites() -> Result<(), String> {
    for (flavor, source, expected_lines) in [
        (Flavor::Default, "type Items<T> = List<T>\ntype Numbers = Items<int>\nfn first() -> Numbers = [1]\nfn second() -> Numbers = [2]\n", [3, 4]),
        (Flavor::Ml, "type Items T = (List T)\ntype Numbers = (Items int)\nfirst : Unit -> Numbers\nfirst () = [1]\nsecond : Unit -> Numbers\nsecond () = [2]\n", [3, 5]),
    ] {
        let assembled = project(flavor, source).map_err(|errors| errors.join("\n"))?;
        let errors = osprey_types::check_program(&assembled.program);
        assert!(errors.is_empty(), "{flavor:?}: {errors:?}");
        let sites = osprey_types::redundant_annotation_sites(&assembled.program);
        let lines: Vec<_> = sites.iter().filter_map(|site| site.annotation_position.or(site.warning.position).map(|p| p.line)).collect();
        assert_eq!(lines, expected_lines, "{flavor:?}: {sites:?}");
    }
    Ok(())
}

fn parent_python_entry_fixture() -> std::io::Result<(PathBuf, PathBuf)> {
    let root =
        std::env::temp_dir().join(format!("osprey_parent_python_entry_{}", std::process::id()));
    let project = root.join(".osprey/build");
    std::fs::create_dir_all(&project)?;
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::create_dir_all(root.join("lib"))?;
    std::fs::write(
        root.join("src/main.py"),
        "# osprey: flavor=python\ndef main():\n    return print(\"parent entry\")\n",
    )?;
    std::fs::write(root.join("lib/main.py"), "# osprey: flavor=python\n# osprey: namespace=app\n# osprey: module=Values\n__all__ = [\"answer\"]\ndef answer():\n    return 7\n")?;
    std::fs::write(project.join("osprey.toml"), "[project]\nsource_roots = [\"../../src\", \"../../lib\"]\npython_sources = [\"../../src/main.py\", \"../../lib/main.py\"]\ndefault_namespace = \"app\"\nentry = \"../../src/main.py\"\n")?;
    Ok((root, project))
}

#[test]
fn python_entry_is_resolved_relative_to_a_derived_manifest() -> std::io::Result<()> {
    let (root, project) = parent_python_entry_fixture()?;
    let loaded = osprey_project::load_and_assemble(&project);
    let expected = root.join("src/main.py").canonicalize()?;
    std::fs::remove_dir_all(root)?;
    assert!(loaded.is_ok(), "{loaded:#?}");
    assert_eq!(
        loaded.ok().and_then(|p| p.entry().map(|e| e.path.clone())),
        Some(expected)
    );
    Ok(())
}
