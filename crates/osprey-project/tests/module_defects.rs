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
use support::{config, contains, error_messages};

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
        discipline: None,
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

const STATE_READ_EXPRESSIONS: [&str; 26] = [
    "[count]",
    "{ value: count }",
    "{ \"key\": count }",
    "\"${count}\"",
    "count[0]",
    "count.value",
    "-count",
    "count + 1",
    "length(count)",
    "count |> abs",
    "match true { true => count false => 0 }",
    "await(count)",
    "recv(count)",
    "send(count, 0)",
    "send(0, count)",
    "yield count",
    "resume(count)",
    "select { _ => count }",
    "mask CounterFx { count }",
    "perform CounterFx.next(count)",
    "length(text: count)",
    "count.method(value: 0)",
    "{ mut local = 0\n local = count\n local }",
    "handler CounterFx { next => 0 return value => count }",
    "handler CounterFx { next => 0 finally => count }",
    "count { value: 0 }",
];

#[test]
fn nested_expression_shapes_cannot_hide_state_reads() {
    for expression in STATE_READ_EXPRESSIONS {
        let source = state_source(expression);
        let messages = error_messages(&config("state.osp"), &[source]);
        assert!(
            contains(&messages, "only accessible inside"),
            "{expression}: {messages:?}"
        );
    }
}

#[test]
fn nested_expression_shapes_allow_a_shadowing_local_binding() {
    for expression in STATE_READ_EXPRESSIONS {
        let source = state_source(&format!("{{ let count = 0\n {expression} }}"));
        let result = assemble(&config("state.osp"), &[source]);
        assert!(result.is_ok(), "{expression}: {result:?}");
    }
}

#[test]
fn immutable_module_values_cannot_capture_owned_cells() {
    let original = state_source("0");
    let source = support::parsed(
        "state.osp",
        Flavor::Default,
        &original
            .source
            .replace("fn inspect() = 0", "let snapshot = count"),
    );
    let messages = error_messages(&config("state.osp"), &[source]);
    assert!(
        contains(&messages, "only accessible inside"),
        "{messages:?}"
    );
}

const SHADOWED_STATE_EXPRESSIONS: [&str; 6] = [
    "{ let count = 7\n count }",
    "(fn(count) => count)(7)",
    "match 7 { count => count }",
    "match [7] { [count, ...rest] => count _ => 0 }",
    "match { count: 7 } { { count } => count }",
    "match Pick(7) { Pick(count) => count Other => 0 }",
];

#[test]
fn local_bindings_shadow_cells_without_authorizing_later_reads() {
    for expression in SHADOWED_STATE_EXPRESSIONS {
        let source = state_source(expression);
        let result = assemble(&config("state.osp"), &[source]);
        assert!(result.is_ok(), "{expression}: {result:?}");
        let escaped = state_source(&format!("{{ let local = {expression}\n count }}"));
        let messages = error_messages(&config("state.osp"), &[escaped]);
        assert!(
            contains(&messages, "only accessible inside"),
            "{expression}: {messages:?}"
        );
    }
}

fn state_source(expression: &str) -> SourceFile {
    support::parsed("state.osp", Flavor::Default, &format!(
        "namespace app;\ntype Choice = Pick(int) | Other\nstate module Counter {{\n mut count = 0\n export effect CounterFx {{ next: fn() -> int }}\n fn inspect() = {expression}\n export fn run(body) = {{\n handle CounterFx {{ next => count }}\n body()\n }}\n}}\n"
    ))
}

const SIGNATURE_REJECTIONS: &[(&str, &str)] = &[
    ("signature S { let value: int }\nmodule M : S { let value: string = \"wrong\" }", "value `value` type does not match"),
    ("signature S { fn f(x: int, y: int) -> int }\nmodule M : S { fn f(x) = x }", "parameter count does not match"),
    ("signature S { fn f<T>(x: T) -> T }\nmodule M : S { fn f<T, U>(x: T) -> T = x }", "function `f` generic arity"),
    ("signature S { fn f() -> int }\nmodule M : S { fn f() -> string = \"wrong\" }", "return type does not match"),
    ("signature S { fn f(x: int) -> int }\nmodule M : S { extern fn f(x: string) -> int }", "parameter types do not match"),
    ("signature S { fn f(x: int) -> int }\nmodule M : S { extern fn f(x: int) -> string }", "return type does not match"),
    ("signature S { type Box<T> }\nmodule M : S { type Box<T, U> = { value: T } }", "type `Box` generic arity"),
    ("signature S { type Value = int }\nmodule M : S { type Value = string }", "manifest type `Value` does not match"),
    ("signature S { type Value = int }\nmodule M : S { type Value = Ready }", "manifest type `Value` does not match"),
    ("module M : Missing { fn f() = 0 }", "unknown signature `Missing`"),
    ("signature S { fn f() -> int }\nmodule M : S { let f = 0 }", "wrong declaration kind"),
    ("signature Inner { fn f() -> int }\nsignature Outer { module Child : Inner }\nmodule M : Outer { module Child { fn f() = 0 } }", "nested module `Child` does not match"),
    ("effect E { ping: fn() -> int }\nsignature S { fn f() -> int ![] }\nmodule M : S { fn f() -> int ![E] = perform E.ping() }", "effect row does not match"),
    ("signature S { fn f() -> int ![|e] }\nmodule M : S { fn f() -> int ![] = 0 }", "effect row does not match"),
];

#[test]
fn signature_shapes_reject_explicitly_incompatible_declarations() {
    for (body, expected) in SIGNATURE_REJECTIONS {
        let source = support::parsed(
            "main.osp",
            Flavor::Default,
            &format!("namespace app;\n{body}\n"),
        );
        let messages = error_messages(&config("main.osp"), &[source]);
        assert!(contains(&messages, expected), "{body}: {messages:?}");
    }
}

const SIGNATURE_ACCEPTED: &[&str] = &[
    "signature S { let value: int }\nmodule M : S { let value: int = 7 }",
    "signature S { fn f(x: int) -> int }\nmodule M : S { extern fn f(x: int) -> int }",
    "signature S { fn f<T>(x: T) -> T }\nmodule M : S { fn f<U>(x: U) -> U = x }",
    "signature S { type Box<T> = List<T> }\nmodule M : S { type Box<U> = List<U> }",
    "signature Inner { fn f() -> int }\nsignature Outer { module Child : Inner }\nmodule M : Outer { module Child : Inner { fn f() = 0 } }",
    "module Wrapper { signature S { fn f() -> int }\n export module M : S { fn f() = 0 } }",
    "effect E<T> { ping: fn(T) -> int }\nsignature S { fn f() -> int ![E<int>] }\nmodule M : S { fn f() -> int ![E<int>] = 0 }",
];

#[test]
fn signature_compatible_controls_assemble_and_typecheck() {
    for body in SIGNATURE_ACCEPTED {
        let source = support::parsed(
            "main.osp",
            Flavor::Default,
            &format!("namespace app;\n{body}\nfn main() = 0\n"),
        );
        let project = assemble(&config("main.osp"), &[source]).expect(body);
        let errors = osprey_types::check_program(&project.program);
        assert!(errors.is_empty(), "{body}: {errors:?}");
    }
}

const MODULE_BOUNDARY_REJECTIONS: &[(&str, &str)] = &[
    ("fn main(value) = value", "project entry function `main` cannot have parameters"),
    ("module A { export extern fn shared(x: int) -> int }\nmodule B { export extern fn shared(x: int) -> int }\nfn main() = 0", "external symbol `shared` is declared by multiple modules"),
    ("module M { export fn value() = 0 }\nlet held = M\nfn main() = 0", "is not a value"),
    ("let session = input()\nfn read() = session\nfn main() = 0", "cannot be captured by a project declaration"),
    ("let NotType = 0\nfn bad(value: NotType) = value\nfn main() = 0", "does not name a type"),
    ("let NotEffect = 0\nfn bad() ![NotEffect] = 0\nfn main() = 0", "does not name an effect"),
];

#[test]
fn module_boundaries_preserve_entry_and_symbol_kinds() {
    for (body, expected) in MODULE_BOUNDARY_REJECTIONS {
        let source = support::parsed(
            "main.osp",
            Flavor::Default,
            &format!("namespace app;\n{body}\n"),
        );
        let messages = error_messages(&config("main.osp"), &[source]);
        assert!(contains(&messages, expected), "{body}: {messages:?}");
    }
}

fn imported_record_sources(call: &str) -> [SourceFile; 2] {
    [
        support::parsed(
            "main.osp",
            Flavor::Default,
            &format!("namespace app;\nimport shared::Data::{{Point}}\nfn main() = {call}\n"),
        ),
        support::parsed(
            "data.osp",
            Flavor::Default,
            "namespace shared;\nmodule Data { export type Point<T> = { value: T } }\n",
        ),
    ]
}

#[test]
fn imported_record_constructors_reject_positional_fields() {
    for call in ["Point(7)", "Point<int>(7)"] {
        let messages = error_messages(&config("main.osp"), &imported_record_sources(call));
        assert!(
            contains(&messages, "record construction requires named fields"),
            "{call}: {messages:?}"
        );
    }
}

/// A union variant declared `Node T T` in one file and applied `Node a b` in
/// another folds to the same construction its own file would have parsed
/// ([TYPE-UNION-POSITIONAL]): the applying file's parser never saw the
/// declaration, so the resolver folds it after linking.
#[test]
fn imported_positional_variants_fold_to_constructions() {
    let main =
        "namespace xf\n\nimport xf::Ts\n\nmain () = print (Ts::label (Node (Leaf 1) (Leaf 2)))\n";
    let library = "namespace xf\n\ntype T = Leaf int | Node T T\n\nmodule Ts\n    export label t = match t\n        Leaf n => toString n\n        Node l r => \"(${label l} ${label r})\"\n";
    let sources = [
        support::parsed("main.ospml", Flavor::Ml, main),
        support::parsed("a.ospml", Flavor::Ml, library),
    ];
    let project = assemble(&config("main.ospml"), &sources).expect("assembles");
    let errors = osprey_types::check_program(&project.program);
    assert!(errors.is_empty(), "{errors:?}");
    let folded = format!("{:?}", project.program.statements);
    assert!(folded.contains("TypeConstructor"), "{folded}");
}

#[test]
fn imported_record_constructors_accept_named_fields_and_type_arguments() {
    for call in ["Point(value: 7)", "Point<int>(value: 7)"] {
        let project = assemble(&config("main.osp"), &imported_record_sources(call)).expect(call);
        let errors = osprey_types::check_program(&project.program);
        assert!(errors.is_empty(), "{call}: {errors:?}");
    }
}
