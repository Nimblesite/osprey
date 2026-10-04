//! Cross-flavor **LLVM IR** equivalence ([FLAVOR-IR-EQUIV],
//! docs/specs/0023-LanguageFlavors.md). The headline guarantee of the
//! many-CSTs-one-AST design: a program written in the ML flavor (`.ospml`) and
//! its Default-flavor twin (`.osp`) lower to the SAME canonical AST, therefore
//! the codegen backend emits **byte-identical LLVM IR** for both. Currying is a
//! pure lowering, so `add x y = x + y` (ML) and `fn add(x) = fn(y) => x + y`
//! (Default) are indistinguishable past the `[FLAVOR-BOUNDARY]`.
//!
//! This is the enforcement layer the differential golden harness cannot give:
//! the harness proves the two flavors *run* the same; this proves they *compile*
//! to the same IR, which is a far stronger structural claim.
//!
//! Data-driven: every `.ospml` under the root `tests`
//! corpus MUST have an in-place Default `.osp` twin unless explicitly exempt.
//! The test compiles both through
//! `osprey_codegen::compile_program` (in-process — no built binary required) and
//! asserts the emitted IR text is identical.
//!
//! Exception: a handful of examples are an *intentionally ML-only surface* with
//! no Default-flavor equivalent — the pure three-state `Verdict` testing model
//! ([TESTING-VERDICT], docs/specs/0027) is the case in point: the Default flavor
//! stays imperative (`fn() -> Unit` firing soft assertions), so a Verdict case
//! has no twin to be IR-identical to. Those stems are listed in
//! [`ML_ONLY_STEMS`] and skipped by both tests below.

#[path = "cross_flavor_ir_equiv/gpu.rs"]
mod gpu;

use std::path::{Path, PathBuf};

use osprey_ast::Program;
use osprey_codegen::compile_program;
use osprey_project::SourceFile;
use osprey_syntax::{parse_program_with_flavor, Flavor};

/// Stems that are an intentionally ML-only surface (no Default twin, exempt from
/// IR equivalence). Keep this list tiny and each entry justified — it is a hole
/// in the headline `[FLAVOR-IR-EQUIV]` guarantee, warranted only when the ML
/// program genuinely has no Default-flavor spelling.
const ML_ONLY_STEMS: &[&str] = &[
    // The pure `Verdict` testing model is ML-only by design [TESTING-VERDICT].
    "verdict.test",
];

/// Whether `path`'s stem is an allowlisted ML-only surface.
fn is_ml_only(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| ML_ONLY_STEMS.contains(&stem))
}

/// Flavor-pair roots, resolved from the crate manifest directory so the test
/// runs unchanged locally and in CI. Moved assertion suites retain the same IR
/// equivalence guarantee as golden examples.
///
/// `benchmarks` is a root because its twins were otherwise unguarded and rotted:
/// nothing type-checked a `.ospml` under it — this test walked only `tests/`, the
/// formatter corpus test only *parses*, and `benchmarks/run.sh` compiles the
/// `.osp` side alone. `binarytrees.ospml` sat with four
/// `cannot unify int with Result<int, MathError>` errors under the old
/// [plan 0019](../../../docs/plans/0019-ml-elegance.md) advertised it as the
/// proof that ML reaches 6 lines at IR parity with its Default twin.
fn flavor_roots() -> Vec<PathBuf> {
    // canonicalize() resolves the `../../`; fall back to the joined path when it
    // is unavailable rather than expect()-panicking outside a `#[test]` (the
    // workspace denies clippy::expect_used in non-test code).
    ["../../tests", "../../benchmarks"]
        .iter()
        .map(|relative| {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
            dir.canonicalize().unwrap_or(dir)
        })
        .collect()
}

/// Parse `source` under `flavor` and emit LLVM IR text, surfacing parse errors
/// loudly so a malformed example fails the test instead of silently lowering a
/// partial AST.
fn ir_for(source: &str, flavor: Flavor, label: &str) -> Result<String, String> {
    let source_file = parsed_source(source, flavor, label)?;
    let program = assembled_if_module_aware(source_file)?;
    let ir = compile_program(&program).map_err(|e| format!("{label}: codegen failed: {e:?}"))?;
    gpu::check_extraction_floor(label, &ir)?;
    Ok(ir)
}

fn parsed_source(source: &str, flavor: Flavor, label: &str) -> Result<SourceFile, String> {
    let parsed = parse_program_with_flavor(source, flavor);
    if !parsed.errors.is_empty() {
        return Err(format!(
            "{label}: unexpected {flavor} parse errors: {:?}",
            parsed.errors
        ));
    }
    Ok(SourceFile {
        path: PathBuf::from(label),
        flavor,
        source: source.to_owned(),
        program: parsed.program,
    })
}

/// A module-bearing source is not a program until the project layer resolves its
/// paths and flattens the graph — `Tax::add` and an import alias like `U::pack`
/// have no meaning before that. Lowering the raw parse would compare IR neither
/// flavor ever runs, so this reproduces the CLI's own single-source path
/// ([MODULES-MODEL]) for both flavors alike, leaving ordinary scripts untouched.
fn assembled_if_module_aware(source: SourceFile) -> Result<Program, String> {
    if !osprey_project::needs_assembly(&source.program) {
        return Ok(source.program);
    }
    osprey_project::assemble_one(source)
        .map(|assembled| assembled.program)
        .map_err(|errors| format!("project assembly failed: {errors:?}"))
}

/// Every `.ospml` file anywhere under `dir`, found by a recursive walk and
/// sorted for deterministic output. Twins live in place next to their `.osp`
/// counterparts throughout `tests`, not in one folder.
fn ml_stems(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_ospml(dir, &mut out);
    out.sort();
    out
}

/// Recurse into `dir`, pushing every `.ospml` path into `out`.
fn collect_ospml(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ospml(&path, out);
        } else if path.extension().is_some_and(|x| x == "ospml") {
            out.push(path);
        }
    }
}

/// Every ML example must have a Default twin (same stem, `.osp`). A missing twin
/// is a hard failure — the pairing is the whole point of the flavor system.
#[test]
fn every_ml_example_has_a_default_twin() {
    let missing: Vec<String> = flavor_roots()
        .iter()
        .flat_map(|dir| ml_stems(dir))
        .filter(|p| !is_ml_only(p) && !p.with_extension("osp").exists())
        .map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string()
        })
        .collect();
    assert!(
        missing.is_empty(),
        "every <stem>.ospml needs a Default twin <stem>.osp for IR equivalence; \
         missing twins for: {missing:?}"
    );
}

/// The headline guarantee: ML and Default twins emit byte-identical IR. Collects
/// every mismatch before failing so one run reports all drift, not just the first.
#[test]
fn ml_and_default_twins_emit_identical_ir() {
    let mut mismatches: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for ml_path in flavor_roots().iter().flat_map(|dir| ml_stems(dir)) {
        let def_path = ml_path.with_extension("osp");
        if !def_path.exists() {
            continue; // covered by `every_ml_example_has_a_default_twin`
        }
        let stem = ml_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();

        let ml_src = std::fs::read_to_string(&ml_path).expect("read .ospml");
        let def_src = std::fs::read_to_string(&def_path).expect("read .osp");

        let ml_ir = ir_for(&ml_src, Flavor::Ml, &format!("{stem}.ospml")).expect("ml codegen");
        let def_ir =
            ir_for(&def_src, Flavor::Default, &format!("{stem}.osp")).expect("default codegen");

        checked += 1;
        if ml_ir != def_ir {
            mismatches.push(format!(
                "  {stem}: IR differs\n{}",
                first_diff(&def_ir, &ml_ir)
            ));
        }
    }

    assert!(checked > 0, "no flavor pairs found under tests");
    assert!(
        mismatches.is_empty(),
        "ML and Default twins MUST emit identical LLVM IR; {} pair(s) drifted:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// First differing line between two IR texts, with a little context, so a
/// failure points at the exact divergence instead of dumping two full modules.
fn first_diff(default_ir: &str, ml_ir: &str) -> String {
    for (i, (d, m)) in default_ir.lines().zip(ml_ir.lines()).enumerate() {
        if d != m {
            return format!(
                "    line {}:\n      default: {d}\n      ml:      {m}",
                i + 1
            );
        }
    }
    format!(
        "    one IR is a prefix of the other (default {} lines, ml {} lines)",
        default_ir.lines().count(),
        ml_ir.lines().count()
    )
}

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
    let mut config = osprey_project::ProjectConfig::for_root(Path::new("app"));
    config.entry = Some(PathBuf::from("main.osp"));
    let project = osprey_project::assemble(&config, &project_sources(mask)?)
        .map_err(|errors| format!("project {mask}: {errors:?}"))?;
    let errors = osprey_types::check_program(&project.program);
    if !errors.is_empty() {
        return Err(format!("project {mask}: {errors:?}"));
    }
    compile_program(&project.program).map_err(|error| format!("project {mask}: {error:?}"))
}

/// [MODULES-FLAVOR-PROJECTION] [MODULES-ABI] [FLAVOR-IR-EQUIV]
/// Abstract and manifest types, split namespaces, imports and effect dispatch keep
/// identical IR under all eight flavor assignments to this three-file graph.
#[test]
fn mixed_flavor_project_graphs_emit_identical_ir() -> Result<(), String> {
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

/// [MODULES-ABI] Debugger names are source identities; linkage stays encoded.
#[test]
fn module_debug_frames_keep_source_names_in_both_flavors() -> Result<(), String> {
    for (source, flavor) in [
        ("namespace \"billing/api\";\nmodule Tax { export fn add(n) = satAdd(n, 1) }\nfn main() = print(Tax::add(41))", Flavor::Default),
        ("namespace \"billing/api\"\nmodule Tax\n    export add n = satAdd n 1\nmain () = print (Tax::add 41)", Flavor::Ml),
    ] {
        let program = assembled_if_module_aware(parsed_source(source, flavor, "debug.osp")?)?;
        let ir = osprey_codegen::compile_program_debug(&program, osprey_codegen::DebugSource::from_path("debug.osp"))
            .map_err(|error| format!("{flavor}: {error}"))?;
        let linkage = osprey_ast::symbol::mangle(["billing/api", "Tax", "add"]);
        assert!(ir.contains("!DISubprogram(name: \"billing/api::Tax::add\", scope:"), "{flavor}: {ir}");
        assert!(ir.contains(&format!("define i64 @{linkage}(")), "preserve native ABI");
    }
    Ok(())
}

const CAPTURED_LAMBDAS: [(&str, Flavor); 2] = [
    ("fn makeAdder(n) = fn(x) => {\n    let sum = wrapAdd(x, n)\n    sum\n}\nfn main() = {\n    let add = makeAdder(2)\n    print(add(40))\n}\n", Flavor::Default),
    ("makeAdder n = \\x =>\n    sum = wrapAdd x n\n    sum\nmain () =\n    add = makeAdder 2\n    print (add 40)\n", Flavor::Ml),
];

/// [DEBUGGER-SOURCE-MAP] Source lambdas need scopes, body lines and variables.
#[test]
fn captured_lambda_bodies_keep_debug_scopes_in_both_flavors() -> Result<(), String> {
    for (source, flavor) in CAPTURED_LAMBDAS {
        let ir = lambda_debug_ir(source, flavor)?;
        for expected in [
            "!DISubprogram(name: \"__closure_fn_",
            "!DILocation(line: 2,",
            "!DILocation(line: 3,",
            "!DILocalVariable(name: \"x\", arg: 2,",
            "!DILocalVariable(name: \"sum\"",
            "!DILocalVariable(name: \"n\"",
        ] {
            assert!(ir.contains(expected), "{flavor}: missing {expected}");
        }
        assert_lambda_variables(&ir, "__closure_fn_", &["x", "n", "sum"])?;
    }
    Ok(())
}

fn lambda_debug_ir(source: &str, flavor: Flavor) -> Result<String, String> {
    let program = parsed_source(source, flavor, "lambda.osp")?.program;
    let errors = osprey_types::check_program(&program);
    assert!(errors.is_empty(), "{flavor}: {errors:?}");
    osprey_codegen::compile_program_debug(
        &program,
        osprey_codegen::DebugSource::from_path("lambda.osp"),
    )
    .map_err(|error| format!("{flavor}: {error}"))
}

fn assert_lambda_variables(ir: &str, name: &str, variables: &[&str]) -> Result<(), String> {
    let prefix = format!("!DISubprogram(name: \"{name}");
    let (scope, _) = ir
        .lines()
        .find(|line| line.contains(&prefix))
        .and_then(|line| line.split_once(" = "))
        .ok_or_else(|| format!("missing lambda scope {name}"))?;
    for variable in variables {
        assert!(
            ir.lines().any(
                |line| line.contains(&format!("!DILocalVariable(name: \"{variable}\""))
                    && line.contains(&format!("scope: {scope},"))
            ),
            "{variable} must belong to {name}'s scope {scope}"
        );
    }
    Ok(())
}

/// [DEBUGGER-SOURCE-MAP] Every materialized source-lambda path has a native scope.
#[test]
fn bound_argument_and_ffi_lambdas_keep_their_debug_scopes() -> Result<(), String> {
    let paths = [
        ("fn main() = {\n let f = fn(x) => wrapMul(x, 2)\n print(f(21))\n}", Flavor::Default, "__closure_fn_", 2),
        ("main () =\n    f = \\x => wrapMul x 2\n    print (f 21)", Flavor::Ml, "__closure_fn_", 2),
        ("fn apply(f, n) = f(n)\nfn main() = print(apply(fn(x) => wrapMul(x, 2), 21))", Flavor::Default, "__closure_fn_", 2),
        ("apply f n = f n\nmain () = print (apply (\\x => wrapMul x 2) 21)", Flavor::Ml, "__closure_fn_", 2),
        ("extern fn invoke(f: (int) -> int, n: int) -> int\nfn main() = print(invoke(fn(x) => wrapMul(x, 2), 21))", Flavor::Default, "__callback_", 1),
        ("extern invoke (f : int -> int) (n : int) -> int\nmain () = print (invoke (\\x => wrapMul x 2) 21)", Flavor::Ml, "__callback_", 1),
    ];
    for (source, flavor, name, arg) in paths {
        let ir = lambda_debug_ir(source, flavor)?;
        assert_lambda_variables(&ir, name, &["x"])?;
        assert!(ir.contains(&format!("!DILocalVariable(name: \"x\", arg: {arg},")));
    }
    Ok(())
}

/// [DEBUGGER-BLOCK-SCOPES] Inner bindings have a scope separate from function locals.
#[test]
fn nested_source_blocks_keep_distinct_variable_scopes() -> Result<(), String> {
    for (source, flavor) in [
        ("fn choose(input) = {\n let value = 100\n let selected = {\n  let value = input\n  let observed = wrapAdd(value, 1)\n  observed\n }\n let outside = wrapAdd(value, selected)\n outside\n}\nprint(choose(2))\n", Flavor::Default),
        ("choose input =\n    value = 100\n    selected =\n        value = input\n        observed = wrapAdd value 1\n        observed\n    outside = wrapAdd value selected\n    outside\nprint (choose 2)\n", Flavor::Ml),
    ] {
        let normal = ir_for(source, flavor, "block.osp")?;
        assert!(!normal.contains("br label"), "scope boundaries add no ordinary control flow");
        assert!(!normal.contains("store volatile"), "debug markers stay out of ordinary builds");
        let ir = lambda_debug_ir(source, flavor)?;
        assert_eq!(ir.matches("%__osprey_debug_scope = alloca i8").count(), 1);
        let inside = local_scope(&ir, "observed")?;
        let outside = local_scope(&ir, "outside")?;
        assert_ne!(inside, outside, "{flavor}: nested locals need their own scope");
        assert!(ir.contains(&format!("{inside} = distinct !DILexicalBlock(scope: {outside},")));
        assert!(ir.lines().any(|line| line.contains("!DILocation(line: 6,") && line.contains(&format!("scope: {inside})"))), "{flavor}: the block return must remain inside its scope");
    }
    Ok(())
}

fn local_scope<'a>(ir: &'a str, name: &str) -> Result<&'a str, String> {
    ir.lines()
        .find(|line| line.contains(&format!("!DILocalVariable(name: \"{name}\",")))
        .and_then(|line| line.split_once("scope: "))
        .and_then(|(_, scope)| scope.split(',').next())
        .ok_or_else(|| format!("missing scope for {name}"))
}
