//! Cross-flavor project assembly and binary contracts.
use super::{compile_program, first_diff, parsed_source, Flavor, Path, SourceFile};

const PROJECT_SOURCES: [(&str, &str, &str); 3] = [
    (
        "main.osp",
        r"namespace app;
import ledger::Ledger
import ledger::Policy
fn main() = {
    handle Arith { overflow _ _ _ wrapped => wrapped }
    handle Policy::Fee { value => 2 }
    print(Ledger::amount(Ledger::make(Policy::credit(40))))
}
",
        r"namespace app
import ledger::Ledger
import ledger::Policy
main () =
    handle Arith
        overflow _ _ _ wrapped => wrapped
    handle Policy::Fee
        value => 2
    print (Ledger::amount (Ledger::make (Policy::credit 40)))
",
    ),
    (
        "unrelated/model.osp",
        r"namespace ledger;
signature LedgerApi {
    type Entry
    type Amount = int
    fn make(value: Amount) -> Entry
    fn amount(value: Entry) -> Amount
}
module Ledger : LedgerApi {
    type Entry = { amount: int }
    type Amount = int
    fn make(value) = Entry { amount: value }
    fn amount(value) = value.amount
}
",
        r"namespace ledger
signature LedgerApi
    type Entry
    type Amount = int
    make : Amount -> Entry
    amount : Entry -> Amount
module Ledger : LedgerApi
    type Entry =
        amount : int
    type Amount = int
    make value = Entry(amount = value)
    amount value = value.amount
",
    ),
    (
        "another/place/policy.osp",
        r"namespace ledger;
module Policy {
    export effect Fee { value: fn() -> int }
    export fn credit(value) = value + perform Fee.value()
}
",
        r"namespace ledger
module Policy
    export effect Fee
        value : Unit => int
    export credit value = value + perform Fee.value ()
",
    ),
];

fn project_sources(mask: usize) -> Result<Vec<SourceFile>, String> {
    PROJECT_SOURCES
        .iter()
        .enumerate()
        .map(|(index, (path, default, ml))| {
            let (source, flavor) = if mask & (1 << index) == 0 {
                (*default, Flavor::Default)
            } else {
                (*ml, Flavor::Ml)
            };
            parsed_source(source, flavor, path)
        })
        .collect()
}

fn project_ir(mask: usize) -> Result<String, String> {
    Ok(observe_graph(&project_sources(mask)?)?.ir)
}

/// [MODULES-FLAVOR-PROJECTION] [MODULES-ABI] [FLAVOR-IR-EQUIV]
/// Abstract and manifest types, split namespaces, imports and effect dispatch keep
/// identical IR under all eight flavor assignments to this three-file graph.
#[test]
fn signature_ascription_is_identical_across_eight_native_assignments() -> Result<(), String> {
    let expected = project_ir(0)?;
    for mask in 1..(1 << PROJECT_SOURCES.len()) {
        let actual = project_ir(mask)?;
        assert_eq!(
            expected,
            actual,
            "project flavor mask {mask}: {}",
            first_diff(&expected, &actual)
        );
    }
    Ok(())
}

/// The two-file graph every flavor can spell, as Default, ML and TypeScript.
const SPELLED_GRAPH: [(&str, [(&str, Flavor); 3]); 2] = [
    (
        "main",
        [
            (
                "namespace app;\nimport app::Policy::{credit, Fee}\nfn main() = {\n    handle Arith { overflow op lhs rhs wrapped => wrapped }\n    handle Fee { value => 2 }\n    print(credit(40))\n}\n",
                Flavor::Default,
            ),
            (
                "namespace app\nimport app::Policy\n    credit\n    Fee\nmain () =\n    handle Arith\n        overflow op lhs rhs wrapped => wrapped\n    handle Fee\n        value => 2\n    print (credit 40)\n",
                Flavor::Ml,
            ),
            (
                "// osprey: namespace=app\nimport { credit, Fee } from \"./policy\";\nfunction main() {\n  return handle<Arith>({ overflow: (op, lhs, rhs, wrapped) => wrapped }, () =>\n    handle<Fee>({ value: () => 2 }, () => print(credit(40))),\n  );\n}\n",
                Flavor::TypeScript,
            ),
        ],
    ),
    (
        "policy",
        [
            (
                "namespace app;\nmodule Policy {\n    export effect Fee { value: fn() -> int }\n    export fn credit(value) = value + perform Fee.value()\n}\n",
                Flavor::Default,
            ),
            (
                "namespace app\nmodule Policy\n    export effect Fee\n        value : Unit => int\n    export credit value = value + perform Fee.value ()\n",
                Flavor::Ml,
            ),
            (
                "// osprey: namespace=app\n// osprey: module=Policy\nexport interface Fee extends Effect {\n  value(): int;\n}\nexport function credit(value) {\n  return value + perform<Fee>().value();\n}\n",
                Flavor::TypeScript,
            ),
        ],
    ),
];

/// The IR of the graph with file `i` in the flavor at index `choice[i]`.
fn spelled_graph_ir(choice: [usize; 2]) -> Result<String, String> {
    let sources = SPELLED_GRAPH
        .iter()
        .zip(choice)
        .map(|((stem, spellings), index)| {
            let (source, flavor) = *spellings
                .get(index)
                .ok_or_else(|| format!("{stem}: no spelling {index}"))?;
            let extension = match flavor {
                Flavor::Ml => "ospml",
                Flavor::TypeScript => "ts",
                _ => "osp",
            };
            parsed_source(source, flavor, &format!("{stem}.{extension}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(observe_graph(&sources)?.ir)
}

/// [FLAVOR-TS-INTEROP] [FLAVOR-IR-EQUIV] An effect declared in one module,
/// performed by an exported function and handled by the importing entry emits
/// identical IR under all nine Default/ML/TypeScript assignments to the graph.
#[test]
fn typescript_joins_the_mixed_flavor_project_graph() -> Result<(), String> {
    let expected = spelled_graph_ir([0, 0])?;
    for main in 0..3 {
        for policy in 0..3 {
            let actual = spelled_graph_ir([main, policy])?;
            assert_eq!(
                expected,
                actual,
                "main flavor {main}, policy flavor {policy}: {}",
                first_diff(&expected, &actual)
            );
        }
    }
    Ok(())
}

/// The same three-file ledger, expressed using the public module surface that
/// all three frontends support. The native ascription matrix above retains the
/// abstract-type checks that Python source cannot spell.
const LEDGER_GRAPH: [(&str, [&str; 3]); 3] = [
    ("main", [
        "namespace app;\nimport ledger::Ledger::{make, amount}\nimport ledger::Policy::{Fee, credit}\nfn main() = { let fees = handler Fee { value => 2 }\nfees(|| => print(amount(make(credit(40))))) }\n",
        "namespace app\nimport ledger::Ledger\n    make\n    amount\nimport ledger::Policy\n    Fee\n    credit\nmain () =\n    fees = handler Fee\n        value => 2\n    fees (\\() => print (amount (make (credit 40))))\n",
        "# osprey: namespace=app\nfrom ledger.Ledger import make, amount\nfrom ledger.Policy import Fee, credit\ndef main():\n    @handler(Fee)\n    class fees:\n        def value():\n            return 2\n    return fees(lambda: print(amount(make(credit(40)))))\n",
    ]),
    ("unrelated/model", [
        "namespace ledger;\nmodule Ledger { export type Entry = { amount: int }\nexport fn make(value) = Entry { amount: value }\nexport fn amount(value) = value.amount }\n",
        "namespace ledger\nmodule Ledger\n    export type Entry =\n        amount: int\n    export make value = Entry(amount = value)\n    export amount value = value.amount\n",
        "# osprey: namespace=ledger\n# osprey: module=Ledger\n@export\nclass Entry:\n    amount: int\n@export\ndef make(value):\n    return Entry(amount=value)\n@export\ndef amount(value):\n    return value.amount\n",
    ]),
    ("another/place/policy", [
        "namespace ledger;\nmodule Policy { export effect Fee { value: fn() -> int }\nexport fn credit(value) = satAdd(value, perform Fee.value()) }\n",
        "namespace ledger\nmodule Policy\n    export effect Fee\n        value : Unit => int\n    export credit value = satAdd (value, perform Fee.value ())\n",
        "# osprey: namespace=ledger\n# osprey: module=Policy\n@export\n@effect\nclass Fee:\n    def value() -> int: ...\n@export\ndef credit(value):\n    return satAdd(value, perform(Fee.value()))\n",
    ]),
];

/// Public names and opacity, checked source contracts, and complete LLVM text.
#[derive(Debug, PartialEq)]
struct GraphResult {
    exports: std::collections::BTreeMap<String, bool>,
    program: String,
    ir: String,
}

fn ledger_result(choices: [usize; 3]) -> Result<GraphResult, String> {
    let flavors = [
        (Flavor::Default, "osp"),
        (Flavor::Ml, "ospml"),
        (Flavor::Python, "py"),
    ];
    let mut files = Vec::new();
    for ((stem, variants), choice) in LEDGER_GRAPH.iter().zip(choices) {
        let &(flavor, extension) = flavors.get(choice).ok_or("flavor index")?;
        let source = variants.get(choice).ok_or("source variant")?;
        files.push(parsed_source(
            source,
            flavor,
            &format!("{stem}.{extension}"),
        )?);
    }
    observe_graph(&files)
}

fn observe_graph(files: &[SourceFile]) -> Result<GraphResult, String> {
    let mut config = osprey_project::ProjectConfig::for_root(Path::new("app"));
    config.entry = Some(files.first().ok_or("missing entry")?.path.clone());
    let project = osprey_project::assemble(&config, files).map_err(|e| format!("{e:?}"))?;
    let errors = osprey_types::check_program(&project.program);
    assert!(errors.is_empty(), "{errors:?}");
    let ir = compile_program(project.backend_program()).map_err(|e| format!("{e:?}"))?;
    Ok(GraphResult {
        program: osprey_ast::canonical::without_positions(&project.program),
        exports: project.public_api,
        ir,
    })
}

/// [FLAVOR-PYTHON-SELECT] [FLAVOR-PYTHON-VERIFICATION]
#[test]
fn mixed_flavor_project_graphs_emit_identical_ir() -> Result<(), String> {
    let expected = ledger_result([0, 0, 0])?;
    for main in 0..3 {
        for model in 0..3 {
            for policy in 0..3 {
                let choices = [main, model, policy];
                let actual = ledger_result(choices)?;
                assert_eq!(actual.exports, expected.exports, "exports for {choices:?}");
                assert_eq!(actual.program, expected.program, "contract for {choices:?}");
                assert_eq!(actual.ir, expected.ir, "LLVM for {choices:?}");
            }
        }
    }
    Ok(())
}
