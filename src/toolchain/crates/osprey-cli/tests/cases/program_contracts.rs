//! Whole-program contracts through the CLI: written type arguments, exit statuses, JSON stdin, the source graph, warning flags and hunt.

use super::{run_args, run_args_with_stdin, run_file, run_source, temp_dir, temp_osp};
use std::path::Path;
use std::process::Command;

/// The whole call-site type-application pipeline — parse, check, lower, emit,
/// link, run — for the shape that has no other spelling: a binder appearing in
/// no parameter position. [TYPE-GENERICS-APPLY]
#[test]
fn a_written_type_argument_pins_an_instantiation_end_to_end() {
    let o = run_source(
        "turbofish_run",
        "fn identity<T>(x: T) -> T = x\n\
         fn emptyOf<T>() -> List<T> = []\n\
         fn pickOf<T, U>(first: T, second: U) -> T = first\n\
         let n = identity<int>(5)\n\
         let s = identity<string>(\"os\")\n\
         let nested = length(identity<List<int>>([1, 2]))\n\
         let empty = length(emptyOf<int>())\n\
         let kept = pickOf<int, string>(7, \"seven\")\n\
         print(\"n=${n} s=${s} nested=${nested} empty=${empty} kept=${kept}\")\n",
        &["--run"],
    );
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert_eq!(o.stdout, "n=5 s=os nested=2 empty=0 kept=7\n");
}

/// The ML twin of the same program prints the same bytes ([FLAVOR-IR-EQUIV]).
#[test]
fn the_ml_written_type_argument_prints_the_same_bytes() {
    let path = std::env::temp_dir().join("osprey_cli_e2e_turbofish_run_ml.ospml");
    let _ = std::fs::write(
        &path,
        "identity<T> : T -> T\n\
         identity x = x\n\
         emptyOf<T> : Unit -> List<T>\n\
         emptyOf () = []\n\
         pickOf<T, U> : (T, U) -> T\n\
         pickOf (first, second) = first\n\
         n = identity<int> 5\n\
         s = identity<string> \"os\"\n\
         nested = length (identity<List<int>> [1, 2])\n\
         empty = length (emptyOf<int> ())\n\
         kept = pickOf<int, string> (7, \"seven\")\n\
         print \"n=${n} s=${s} nested=${nested} empty=${empty} kept=${kept}\"\n",
    );
    let o = run_file(&path, &["--run"]);
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert_eq!(o.stdout, "n=5 s=os nested=2 empty=0 kept=7\n");
}

/// A written list that misses the declared binder count is rejected before
/// anything is emitted, naming both counts. [TYPE-GENERICS-APPLY]
#[test]
fn a_written_type_argument_count_mismatch_is_rejected_by_the_cli() {
    let o = run_source(
        "turbofish_arity",
        "fn identity<T>(x: T) -> T = x\n\
         print(\"${identity<int, string>(5)}\")\n",
        &["--check"],
    );
    o.expect_rejected(&["function `identity` takes 1 type argument(s), got 2"]);
}

/// A written argument contradicting the value argument is a type error, not a
/// silently ignored annotation. [TYPE-GENERICS-APPLY]
#[test]
fn a_contradicting_written_type_argument_is_rejected_by_the_cli() {
    let o = run_source(
        "turbofish_contradiction",
        "fn identity<T>(x: T) -> T = x\n\
         print(\"${identity<int>(\\\"text\\\")}\")\n",
        &["--check"],
    );
    o.expect_rejected(&["cannot unify int with string"]);
}

/// A program ends by returning: `main`'s integer is its exit status in every
/// flavor, and every other shape of program exits 0. [ENTRY-EXIT-STATUS]
#[test]
fn main_returning_an_integer_sets_the_exit_status_in_every_flavor() {
    for (name, source, status, stdout) in [
        (
            "default_3",
            "fn main() -> int = {\n print(\"failing\")\n 3\n}\n",
            3,
            "failing\n",
        ),
        ("default_0", "fn main() -> int = 0\n", 0, ""),
        ("default_255", "fn main() = 255\n", 255, ""),
        ("default_unit", "fn main() = print(\"fine\")\n", 0, "fine\n"),
        ("default_string", "fn main() = \"text\"\n", 0, ""),
        ("default_bool", "fn main() = true\n", 0, ""),
        (
            "default_script",
            "print(\"script\")\nlet x = 4\nprint(\"${x}\")\n",
            0,
            "script\n4\n",
        ),
        (
            "default_computed",
            "fn fails(n) = n > 2\nfn main() = match fails(3) {\n true => 7\n false => 0\n}\n",
            7,
            "",
        ),
    ] {
        let program = temp_osp(&format!("exit_status_{name}"), source);
        let run = run_file(&program, &["--run", "--quiet", "--no-warnings"]);
        assert_eq!(run.code, Some(status), "{name}: stderr={}", run.stderr);
        assert_eq!(run.stdout, stdout, "{name}");
    }
    for (name, extension, source, status) in [
        (
            "ml",
            "ospml",
            "main () =\n    print \"failing\"\n    3\n",
            3,
        ),
        ("ml_unit", "ospml", "main () = print \"fine\"\n", 0),
        (
            "python",
            "py",
            "# osprey: flavor=python\n\ndef main():\n    print(\"failing\")\n    return 3\n",
            3,
        ),
        (
            "python_none",
            "py",
            "# osprey: flavor=python\n\ndef main():\n    print(\"fine\")\n",
            0,
        ),
    ] {
        let directory =
            std::env::temp_dir().join(format!("osprey_exit_{}_{name}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("create temp dir");
        let program = directory.join(format!("exit.{extension}"));
        std::fs::write(&program, source).expect("write source");
        let run = run_file(&program, &["--run", "--quiet", "--no-warnings"]);
        assert_eq!(run.code, Some(status), "{name}: stderr={}", run.stderr);
        std::fs::remove_dir_all(&directory).expect("remove temp dir");
    }
    // A failed test outranks whatever `main` returned; a passing one defers to it.
    let failed = temp_osp(
        "exit_status_failed_test",
        "fn main() = {\n test(\"no\", fn() => expect(1, 2))\n 0\n}\n",
    );
    assert_eq!(
        run_file(&failed, &["--run", "--quiet", "--no-warnings"]).code,
        Some(1)
    );
    let passed = temp_osp(
        "exit_status_passed_test",
        "fn main() = {\n test(\"yes\", fn() => expect(1, 1))\n 9\n}\n",
    );
    assert_eq!(
        run_file(&passed, &["--run", "--quiet", "--no-warnings"]).code,
        Some(9)
    );
}

/// The GitHub Action's contract: one JSON line on stdin, `key=value` lines on stdout,
/// and the exit status of `main`. [GHA-FERRY-STDIN] [GHA-IO-CONTRACT] [ENTRY-EXIT-STATUS]
#[test]
fn a_json_line_on_stdin_becomes_outputs_and_an_exit_status() {
    let program = temp_osp(
        "gha_ferry",
        "fn member(members, name, position) = match listGet(members, position) {\n\
             Success { value } => match value {\n\
                 JsonMember(key, json) => if key == name { json } else { member(members, name, position + 1) }\n\
             }\n\
             Error => JsonNull\n\
         }\n\
         fn answer(members) = match member(members, \"who\", 0) {\n\
             JsonString(who) => {\n\
                 print(\"greeting=hello ${who}\\nlength=${length(who)}\")\n\
                 0\n\
             }\n\
             _ => {\n\
                 print(\"error=who is required\")\n\
                 3\n\
             }\n\
         }\n\
         fn run() = match jsonDecode(input()) {\n\
             Success { value } => match value {\n\
                 JsonObject(members) => answer(members)\n\
                 _ => 2\n\
             }\n\
             Error => 1\n\
         }\n\
         fn main() = {\n\
             handle Arith { overflow _ _ _ wrapped => wrapped }\n\
             run()\n\
         }\n",
    );
    let path = program.to_str().unwrap_or_default();
    for (line, status, stdout) in [
        (
            "{\"who\":\"Osprey\"}\n",
            0,
            "greeting=hello Osprey\nlength=6\n",
        ),
        (
            "{\"other\":1,\"who\":\"🦅\"}",
            0,
            "greeting=hello 🦅\nlength=4\n",
        ),
        ("{\"other\":1}\n", 3, "error=who is required\n"),
        ("{\"who\":7}\n", 3, "error=who is required\n"),
        ("[1,2]\n", 2, ""),
        ("not json\n", 1, ""),
        ("", 1, ""),
    ] {
        let run = run_args_with_stdin(&[path, "--run", "--quiet", "--no-warnings"], line);
        assert_eq!(run.code, Some(status), "{line}: {}", run.stderr);
        assert_eq!(run.stdout, stdout, "{line}");
    }
}

/// The compiler answers which namespaces and modules each source declares, in both
/// flavors, nested and repeated across files. [EDITOR-MODULE-EXPLORER]
#[test]
fn the_source_graph_names_each_sources_namespaces_and_modules() {
    let dir = temp_dir("source_graph_declarations");
    std::fs::create_dir_all(dir.join("src")).expect("create project");
    for (file, text) in [
        (
            "osprey.toml",
            "[project]\nname = \"graph\"\nsource_roots = [\"src\"]\nentry = \"src/main.ospml\"\n",
        ),
        (
            "src/main.ospml",
            "namespace app\n\nmodule Outer\n    export module Inner\n        export one = 1\n\nprint \"${Outer::Inner::one}\"\n",
        ),
        (
            "src/more.osp",
            "namespace app;\n\nmodule Extra {\n    export fn two() = 2\n}\n",
        ),
    ] {
        std::fs::write(dir.join(file), text).expect("write project file");
    }
    let run = run_args(&[
        dir.to_str().unwrap_or_default(),
        "--source-graph",
        "--no-warnings",
    ]);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    let graph: serde_json::Value = serde_json::from_str(&run.stdout).expect("graph json");
    let found = graph
        .get("declarations")
        .and_then(serde_json::Value::as_array)
        .expect("declarations")
        .iter()
        .map(|entry| {
            let text = |key: &str| {
                entry
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
            };
            (
                text("kind").to_owned(),
                text("name").to_owned(),
                text("source")
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
                entry.get("line").is_some_and(serde_json::Value::is_u64),
            )
        })
        .collect::<Vec<_>>();
    for (kind, name, file) in [
        ("namespace", "app", "main.ospml"),
        ("module", "app::Outer", "main.ospml"),
        ("module", "app::Outer::Inner", "main.ospml"),
        ("namespace", "app", "more.osp"),
        ("module", "app::Extra", "more.osp"),
    ] {
        let want = (kind.to_owned(), name.to_owned(), file.to_owned(), true);
        assert!(found.contains(&want), "{want:?} missing from {found:?}");
    }
}

/// `--warnings-as-errors` keeps the warning text and fails; the default stays
/// advisory, `--no-warnings` stays silent, and errors fail as ever.
#[test]
fn warnings_as_errors_fails_only_on_opt_in_and_keeps_the_diagnostics() {
    let warned = temp_osp(
        "strict_warned",
        "fn choose(value, ignored) = value\nprint(choose(1, 2))\n",
    );
    let clean = temp_osp(
        "strict_clean",
        "fn shout(value) = \"${value}!\"\nprint(shout(\"x\"))\n",
    );
    let broken = temp_osp("strict_broken", "fn f() -> string = 1\nprint(f())\n");
    for mode in ["--check", "--run", "--llvm"] {
        let advisory = run_file(&warned, &[mode]);
        advisory.expect_stderr(0, &["unused parameter `ignored`"], &[]);

        let strict = run_file(&warned, &[mode, "--warnings-as-errors"]);
        strict.expect_rejected(&["unused parameter `ignored`", "1 warning (unused-parameter)"]);
        assert!(!strict.stdout.contains("ok ("), "{mode}: {}", strict.stdout);

        let skipped = run_file(&warned, &[mode, "--no-warnings"]);
        assert_eq!(skipped.code, Some(0), "{mode}: {}", skipped.stderr);
        assert!(
            !skipped.stderr.contains("warning"),
            "{mode}: {}",
            skipped.stderr
        );

        let fine = run_file(&clean, &[mode, "--warnings-as-errors"]);
        assert_eq!(fine.code, Some(0), "{mode}: {}", fine.stderr);

        let errors = run_file(&broken, &[mode, "--warnings-as-errors"]);
        errors.expect_rejected(&["cannot unify string with int"]);
    }
    let output = warned.with_extension("bin");
    let output = output.to_str().unwrap_or_default();
    let refused = run_file(
        &warned,
        &["--compile", "--warnings-as-errors", "-o", output],
    );
    assert_ne!(refused.code, Some(0), "{}", refused.stderr);
    assert!(
        !Path::new(output).exists(),
        "a refused build writes nothing"
    );
}

#[test]
fn bare_result_variants_compile_and_select_the_correct_arm() {
    for (index, source) in [
        "let a = match checkedAdd(0, 1) { Success { value } => value Error => 0 }\nlet b = match checkedAdd(9223372036854775807, 1) { Success => 7 Error => 9 }\nprint(\"${a},${b}\")\n",
        "type Outcome = Success | Error\nlet a = match Success { Success => 1 Error => 0 }\nlet b = match Error { Success => 7 Error => 9 }\nprint(\"${a},${b}\")\n",
        "let a = match 1 { Success { value } => value Error => 0 }\nlet b = match 9 { Success { value } => value Error => 0 }\nprint(\"${a},${b}\")\n",
        "let a = match checkedAdd(0, 1) { Error => 0 whole => match whole { Success { value } => value Error => 0 } }\nlet b = match checkedAdd(9223372036854775807, 1) { Success => 7 _ => 9 }\nprint(\"${a},${b}\")\n",
        "// osprey: flavor=ml\na = match checkedAdd (0, 1)\n    Success value => value\n    Error => 0\nb = match checkedAdd (9223372036854775807, 1)\n    Success => 7\n    Error => 9\nprint \"${a},${b}\"\n",
    ].iter().enumerate() {
        let source = temp_osp(&format!("bare_result_variants_{index}"), source);
        for memory in ["gc", "arc"] {
            let flavor = if index == 4 { "ml" } else { "default" };
            let output = run_file(&source, &[
                "--run", "--quiet", &format!("--memory={memory}"),
                &format!("--flavor={flavor}"),
            ]);
            assert_eq!(output.code, Some(0), "{}", output.stderr);
            assert_eq!(output.stdout, "1,9\n");
        }
    }
}

#[test]
fn quiet_silences_progress_and_keeps_warnings() {
    // `--quiet` is progress-only. A warning is a diagnostic: it names a defect
    // in the program, so it prints under `--quiet` exactly as it does without.
    let prog = temp_osp("quiet_warn", "let n: int = 42\nprint(\"v=${n}\")\n");
    let loud = run_file(&prog, &["--check"]);
    assert_eq!(loud.code, Some(0), "stderr={}", loud.stderr);
    assert!(loud.stderr.contains("redundant"), "{}", loud.stderr);
    assert!(loud.stdout.contains(": ok ("), "{}", loud.stdout);
    let o = run_file(&prog, &["--check", "--quiet"]);
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert!(o.stdout.trim().is_empty(), "{}", o.stdout);
    assert_eq!(o.stderr, loud.stderr, "--quiet changed the diagnostics");
}

#[test]
fn no_warnings_skips_the_warning_pass() {
    // The explicit opt-out drops advice: nothing on stderr, nothing computed.
    let o = run_source(
        "no_warn",
        "let n: int = 42\nprint(\"v=${n}\")\n",
        &["--check", "--quiet", "--no-warnings"],
    );
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert!(o.stdout.trim().is_empty(), "{}", o.stdout);
    assert!(o.stderr.trim().is_empty(), "{}", o.stderr);
}

#[test]
fn no_warnings_never_hides_type_errors() {
    // Errors are never advice: the opt-out drops warnings, not the reason a
    // build failed.
    let o = run_source(
        "no_warn_error",
        "fn broken(unused) = missing\n",
        &["--check", "--quiet", "--no-warnings"],
    );
    o.expect_rejected(&["missing"]);
}

#[test]
fn hunt_reports_a_compile_error_before_looking_for_the_engine() {
    // [SYMEX-CLI]: exit 1 for a program that does not compile, 2 for a
    // toolchain problem. The compile verdict comes first, so a machine without
    // KLEE still tells the author about the syntax error instead of about KLEE.
    let prog = temp_osp("hunt_broken", "fn broken( = \n");
    let out = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .args(["hunt", &prog.to_string_lossy(), "--quiet"])
        .env("OSPREY_KLEE", "/nonexistent/klee")
        .output()
        .expect("spawn osprey hunt");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("syntax error"), "{stderr}");
    assert!(!stderr.contains("klee was not found"), "{stderr}");
}
