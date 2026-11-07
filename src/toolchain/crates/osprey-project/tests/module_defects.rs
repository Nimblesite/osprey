//! Regression tests for module defects found on 2026-08-22. Every assertion stays:
//! namespace scope, entrypoint selection and exported constructors remain mandatory
//! contracts. See `src/osprey/tests/modules/README.md` for current coverage.

mod support;

use osprey_ast::Stmt;
use osprey_project::{assemble, AssembledProject, SourceFile};
use osprey_syntax::Flavor;
use std::path::PathBuf;
use support::{config, contains, error_messages};

/// The diagnostic that a cell read outside its `state module` is rejected.
const INSIDE: &str = "only accessible inside";

/// The diagnostic that a record needs named fields instead of positional ones.
const NAMED: &str = "record construction requires named fields";

/// Every message `osprey_types` reports for `program`.
fn type_messages(program: &osprey_ast::Program) -> Vec<String> {
    osprey_types::check_program(program)
        .into_iter()
        .map(|error| error.message)
        .collect()
}

/// Assemble `sources` with `entry` selected, proving the program typechecks clean.
fn checked_sources(
    entry: &str,
    sources: &[SourceFile],
    context: &str,
) -> Result<AssembledProject, Vec<osprey_project::ProjectError>> {
    let assembled = assemble(&config(entry), sources)?;
    let messages = type_messages(&assembled.program);
    assert!(messages.is_empty(), "{context}: {messages:?}");
    Ok(assembled)
}

/// Parse and assemble one written source as a single-file project.
fn project(flavor: Flavor, text: &str) -> Result<AssembledProject, Vec<String>> {
    let name = support::main_name(flavor);
    let source = support::parsed(name, flavor, text);
    assemble(&config(name), &[source]).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
    })
}

/// Assemble one written source and prove the program typechecks with no errors.
fn checked_project(flavor: Flavor, text: &str) -> Result<AssembledProject, String> {
    let assembled = project(flavor, text).map_err(|errors| errors.join("\n"))?;
    let messages = type_messages(&assembled.program);
    assert!(messages.is_empty(), "{flavor:?}: {messages:?}");
    Ok(assembled)
}

/// Source-level names the assembler kept for the flattened program.
fn source_names(assembled: &AssembledProject) -> Vec<String> {
    let mut names: Vec<String> = assembled.source_name_by_mangled.values().cloned().collect();
    names.sort();
    names
}

/// Assert `names` lists `name` exactly when `present`, reporting `message` otherwise.
fn assert_name(names: &[String], name: &str, present: bool, message: &str) {
    let listed = names.iter().any(|kept| kept == name);
    assert!(listed == present, "{message}: {names:?}");
}

/// Parse `body` as a Default-flavour `main.osp` hung off the `namespace app;` header.
fn namespaced_main(body: &str) -> SourceFile {
    let text = format!("namespace app;\n{body}\n");
    support::parsed("main.osp", Flavor::Default, &text)
}

/// Assemble `sources` and prove the diagnostics mention `expected`.
fn assert_reports(sources: &[SourceFile], expected: &str, context: &str) {
    let messages = error_messages(&config("main.osp"), sources);
    assert!(contains(&messages, expected), "{context}: {messages:?}");
}

#[test]
fn python_imports_resolve_by_declaration_identity_and_lexical_scope(
) -> Result<(), Vec<osprey_project::ProjectError>> {
    for (library, main) in [
        ("namespace ledger;\nmodule Values { export type Box<T> = { amount: T }\nexport type Token = Ready\nexport fn Upper(work) = work()\nexport fn Empty() = 42 }\n", "# osprey: namespace=app\nfrom ledger import Values as V\nfrom ledger.Values import Box, Upper\ndef main():\n    token = V.Ready()\n    one = V.Box[int](amount=41)\n    two = Box(amount=\"two\")\n    print(f\"{one.amount}:{two.amount}:{Upper(lambda: 42)}:{V.Empty()}\")\n"),
        ("namespace ledger;\nmodule Values { export type Box = { amount: int }\nexport fn make(n) = Box { amount: n }\nexport let defaults = Box { amount: 3 } }\n", "# osprey: namespace=app\nfrom ledger import Values as V\nfrom ledger.Values import defaults\nimport ledger as Ledger\ndef amount(V):\n    return V.amount\ndef main():\n    print(f\"{amount(V.make(41))}:{Ledger.Values.make(42).amount}:{defaults.amount}\")\n"),
    ] {
        let main = support::parsed("main.py", Flavor::Python, main);
        let library = support::parsed("values.osp", Flavor::Default, library);
        let _ = checked_sources("main.py", &[main, library], "python")?;
    }
    Ok(())
}

/// [MODULES-FILE-SCOPED-NAMESPACE]: each header applies to what follows it, so the second
/// namespace keeps `two::B::v` and both statements run. Original defect: ML nested the
/// second header, dropping the declaration and every later statement while exiting ZERO.
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
    assert_name(&names, "one::A::v", true, "first declaration");
    assert_name(&names, "two::B::v", true, "second header");
    let statements = assembled.program.statements.iter();
    let executed = statements
        .filter(|s| matches!(s, Stmt::Expr { .. }))
        .count();
    assert_eq!(
        executed, 2,
        "statements after the second namespace header were silently dropped"
    );
}

/// The Default spelling of the same program. [MODULES-FILE-SCOPED-NAMESPACE]
///
/// Original defect: `namespace two;` was ignored, so `module B` answered to `one::B::v`.
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
    assert_name(&names, "two::B::v", true, "second header");
    assert_name(&names, "one::B::v", false, "`B` misfiled");
}

/// [MODULES-ENTRYPOINT]: "a `main` and a top-level executable statement cannot share it:
/// candidates 2 and 3 would both select that source and only one of them could run".
/// `src/osprey/examples/failscompilation/main_beside_top_level_statement.ospo` pins the
/// rejection for a plain program; a namespaced source routes through project assembly,
/// where the check was missing, so both entries ran.
#[test]
fn namespaced_main_beside_a_top_level_statement_is_rejected() {
    let source = concat!(
        "namespace app\n\n",
        "main () = print \"main ran\"\n\n",
        "print \"statement ran\"\n",
    );
    let errors = project(Flavor::Ml, source).err().unwrap_or_default();
    let unrecorded = contains(&errors, "cannot sit beside `main`");
    assert!(unrecorded, "entry conflict went unreported: {errors:?}");
}

/// [MODULES-EXPORTS] makes `export`ed items the public surface and
/// [MODULES-OPAQUE-TYPES] reserves privacy for OPAQUE constructors, so a plain exported
/// union's constructors must be reachable. Original defect: they all were unreachable.
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
    let messages = type_messages(&assembled.program);
    assert!(messages.is_empty(), "union ctors unreachable: {messages:?}");
}

// [TYPE-ANNOTATION-REDUNDANT], [MODULES-SIGNATURE]: aliases retain the
// distinction between a contract and an annotation actually written on a body.
#[test]
fn alias_expansion_preserves_contract_annotations() -> Result<(), String> {
    for (flavor, source) in [
        (Flavor::Default, "signature Api { type Number = int\n fn next(value: Number) -> Number }\nmodule M : Api { type Number = int\n fn next(value) = wrapAdd(value, 1) }\n"),
        (Flavor::Ml, "signature Api\n    type Number = int\n    next : Number -> Number\nmodule M : Api\n    type Number = int\n    next value = wrapAdd value 1\n"),
    ] {
        let assembled = checked_project(flavor, source)?;
        let warnings = osprey_types::redundant_annotations(&assembled.program);
        assert!(warnings.is_empty(), "{flavor:?}: {warnings:?}");
    }
    for (flavor, source, expected_lines) in [
        (Flavor::Default, "type Items<T> = List<T>\ntype Numbers = Items<int>\nfn first() -> Numbers = [1]\nfn second() -> Numbers = [2]\n", [3, 4]),
        (Flavor::Ml, "type Items T = (List T)\ntype Numbers = (Items int)\nfirst : Unit -> Numbers\nfirst () = [1]\nsecond : Unit -> Numbers\nsecond () = [2]\n", [3, 5]),
    ] {
        let assembled = checked_project(flavor, source)?;
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
    for (path, text) in [
        (
            "src/main.py",
            "# osprey: flavor=python\ndef main():\n    return print(\"parent entry\")\n",
        ),
        ("lib/main.py", "# osprey: flavor=python\n# osprey: namespace=app\n# osprey: module=Values\n__all__ = [\"answer\"]\ndef answer():\n    return 7\n"),
        (".osprey/build/osprey.toml", "[project]\nsource_roots = [\"../../src\", \"../../lib\"]\npython_sources = [\"../../src/main.py\", \"../../lib/main.py\"]\ndefault_namespace = \"app\"\nentry = \"../../src/main.py\"\n"),
    ] {
        std::fs::write(root.join(path), text)?;
    }
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

const SHADOWED_STATE_EXPRESSIONS: [&str; 6] = [
    "{ let count = 7\n count }",
    "(fn(count) => count)(7)",
    "match 7 { count => count }",
    "match [7] { [count, ...rest] => count _ => 0 }",
    "match { count: 7 } { { count } => count }",
    "match Pick(7) { Pick(count) => count Other => 0 }",
];

#[test]
fn nested_expression_shapes_cannot_hide_state_reads() {
    for expression in STATE_READ_EXPRESSIONS {
        assert_state_read_rejected(expression);
    }
    for expression in SHADOWED_STATE_EXPRESSIONS {
        assert_state_assembles(expression);
        assert_state_read_rejected(&format!("{{ let local = {expression}\n count }}"));
    }
}

#[test]
fn nested_expression_shapes_allow_a_shadowing_local_binding() {
    for expression in STATE_READ_EXPRESSIONS {
        assert_state_assembles(&format!("{{ let count = 0\n {expression} }}"));
    }
}

#[test]
fn immutable_module_values_cannot_capture_owned_cells() {
    let original = state_source("0");
    let (from, to) = ("fn inspect() = 0", "let snapshot = count");
    let source = original.source.replace(from, to);
    let parsed = support::parsed("state.osp", Flavor::Default, &source);
    let messages = error_messages(&config("state.osp"), &[parsed]);
    assert!(contains(&messages, INSIDE), "{messages:?}");
}

fn state_source(expression: &str) -> SourceFile {
    support::parsed("state.osp", Flavor::Default, &format!(
        "namespace app;\ntype Choice = Pick(int) | Other\nstate module Counter {{\n mut count = 0\n export effect CounterFx {{ next: fn() -> int }}\n fn inspect() = {expression}\n export fn run(body) = {{\n handle CounterFx {{ next => count }}\n body()\n }}\n}}\n"
    ))
}

/// Assemble the `state.osp` fixture and prove it builds with `expression` as its body.
fn assert_state_assembles(expression: &str) {
    let result = assemble(&config("state.osp"), &[state_source(expression)]);
    assert!(result.is_ok(), "{expression}: {result:?}");
}

/// Assemble the `state.osp` fixture and prove `expression`'s cell read is rejected.
fn assert_state_read_rejected(expression: &str) {
    let messages = error_messages(&config("state.osp"), &[state_source(expression)]);
    assert!(contains(&messages, INSIDE), "{expression}: {messages:?}");
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

const MODULE_BOUNDARY_REJECTIONS: &[(&str, &str)] = &[
    ("fn main(value) = value", "project entry function `main` cannot have parameters"),
    ("module A { export extern fn shared(x: int) -> int }\nmodule B { export extern fn shared(x: int) -> int }\nfn main() = 0", "external symbol `shared` is declared by multiple modules"),
    ("module M { export fn value() = 0 }\nlet held = M\nfn main() = 0", "is not a value"),
    ("let session = input()\nfn read() = session\nfn main() = 0", "cannot be captured by a project declaration"),
    ("let NotType = 0\nfn bad(value: NotType) = value\nfn main() = 0", "does not name a type"),
    ("let NotEffect = 0\nfn bad() ![NotEffect] = 0\nfn main() = 0", "does not name an effect"),
];

#[test]
fn incompatible_signatures_and_module_boundaries_are_rejected() {
    let cases = [SIGNATURE_REJECTIONS, MODULE_BOUNDARY_REJECTIONS];
    for tuple in cases.into_iter().flatten() {
        let (body, expected) = tuple;
        assert_reports(&[namespaced_main(body)], expected, body);
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
fn signature_compatible_controls_assemble_and_typecheck(
) -> Result<(), Vec<osprey_project::ProjectError>> {
    for body in SIGNATURE_ACCEPTED {
        let sources = [namespaced_main(&format!("{body}\nfn main() = 0"))];
        let _ = checked_sources("main.osp", &sources, body)?;
    }
    Ok(())
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
fn imported_record_constructors_require_named_fields(
) -> Result<(), Vec<osprey_project::ProjectError>> {
    for call in ["Point(7)", "Point<int>(7)"] {
        assert_reports(&imported_record_sources(call), NAMED, call);
    }
    for call in ["Point(value: 7)", "Point<int>(value: 7)"] {
        let sources = imported_record_sources(call);
        let _ = checked_sources("main.osp", &sources, call)?;
    }
    Ok(())
}

/// A union variant declared `Node T T` in one file and applied `Node a b` in another
/// folds to the construction its own file would have parsed ([TYPE-UNION-POSITIONAL]).
#[test]
fn imported_positional_variants_fold_to_constructions(
) -> Result<(), Vec<osprey_project::ProjectError>> {
    let main =
        "namespace xf\n\nimport xf::Ts\n\nmain () = print (Ts::label (Node (Leaf 1) (Leaf 2)))\n";
    let library = "namespace xf\n\ntype T = Leaf int | Node T T\n\nmodule Ts\n    export label t = match t\n        Leaf n => toString n\n        Node l r => \"(${label l} ${label r})\"\n";
    let sources = [
        support::parsed("main.ospml", Flavor::Ml, main),
        support::parsed("a.ospml", Flavor::Ml, library),
    ];
    let project = checked_sources("main.ospml", &sources, "positional")?;
    let folded = format!("{:?}", project.program.statements);
    assert!(folded.contains("TypeConstructor"), "{folded}");
    Ok(())
}
