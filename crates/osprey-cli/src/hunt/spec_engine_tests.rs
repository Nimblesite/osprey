//! The fixed KLEE command, its `info` and stderr, and the module it loads,
//! permuted against docs/specs/tooling/SymbolicExecution.md
//! [SYMEX-KLEE-INVOCATION] [SYMEX-CODEGEN] [SYMEX-MOCK-STREAM].

#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: a missing element is a test failure, not a production panic"
)]

use super::engine::{self, Exploration};
use super::module;
use super::spec_tests::permutations;
use super::{clear_run_directory, run, RUN_MARKER, RUN_MARKER_CONTENTS};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::ExitCode;

// ---- [SYMEX-KLEE-INVOCATION] ----------------------------------------------

/// The command as the spec writes it, with the spec's placeholders filled in:
/// the document is the one source of the table, so a flag added to one and
/// not the other fails here.
pub(super) fn spec_command(
    runtime: &str,
    out: &str,
    budget: (u64, u64),
    module: &str,
) -> Vec<String> {
    let (seconds, megabytes) = budget;
    let spec = include_str!("../../../../docs/specs/tooling/SymbolicExecution.md");
    let block = spec
        .split("```text\nklee ")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("the spec's klee command block");
    block
        .replace("<run dir>", "\u{1}")
        .split_whitespace()
        .map(|token| match token {
            "-link-llvm-lib=compiler/bin/libosprey_runtime_klee_gc.bc" => {
                format!("-link-llvm-lib={runtime}")
            }
            "--max-time=<S>s" => format!("--max-time={seconds}s"),
            "--max-memory=<MB>" => format!("--max-memory={megabytes}"),
            "--output-dir=\u{1}" => format!("--output-dir={out}"),
            "<program.ll>" => module.to_string(),
            other => other.to_string(),
        })
        .collect()
}

#[test]
fn the_engine_command_is_the_spec_command_for_every_budget_and_path() {
    let budgets = [(1, 256), (60, 2000), (3600, 65_536), (u64::MAX, u64::MAX)];
    let places = [
        ("rt.bc", "out", "p.ll"),
        (
            "compiler/bin/libosprey_runtime_klee_gc.bc",
            "/tmp/x-hunt-klee",
            "/tmp/x-hunt.ll",
        ),
        ("my rt/rt.bc", "out dir/run 1", "a b.ll"),
        ("ünï/rt.bc", "-out", "--json.ll"),
    ];
    for budget in budgets {
        for (runtime, out, module) in places {
            let actual = engine::arguments(
                Path::new(runtime),
                Path::new(out),
                budget,
                Path::new(module),
            );
            assert_eq!(actual, spec_command(runtime, out, budget, module));
            assert_eq!(actual.iter().collect::<BTreeSet<_>>().len(), actual.len());
        }
    }
}

/// The macOS package builds neither uclibc nor the POSIX runtime, and a
/// fixed-size symbolic stdin is what the mock replaces [SYMEX-INSTALL].
#[test]
fn the_engine_never_gets_linux_only_or_user_options() {
    let actual = engine::arguments(Path::new("r"), Path::new("o"), (1, 256), Path::new("m"));
    for banned in [
        "uclibc",
        "posix-runtime",
        "sym-stdin",
        "sym-files",
        "sym-arg",
        "seed",
    ] {
        assert!(
            actual.iter().all(|argument| !argument.contains(banned) || argument.contains("rng-initial-seed")),
            "{banned}: {actual:?}"
        );
    }
    assert_eq!(actual.len(), 17);
    assert_eq!(
        actual.last().map(String::as_str),
        Some("m"),
        "the module is last"
    );
}

#[test]
fn every_kind_of_target_noise_is_dropped_and_order_is_kept() {
    let noise = [
        "KLEE: WARNING: unable to select target: x86_64",
        "warning: Linking two modules of different target triples",
        "target triples do not match: 'arm64-apple-macosx' vs 'x86_64'",
        "warning: Linking two modules of different data layouts",
        "different data layouts!",
    ];
    for line in noise {
        assert!(engine::is_noise(line), "{line}");
    }
    for line in ["KLEE: ERROR: real", "", "target", "Linking", "data layout"] {
        assert!(!engine::is_noise(line), "{line}");
    }
    let real: Vec<String> = (0..20).map(|n| format!("KLEE: line {n}")).collect();
    let mut mixed = Vec::new();
    for (at, line) in real.iter().enumerate() {
        mixed.push(line.as_str());
        mixed.push(noise[at % noise.len()]);
    }
    let stderr = mixed.join("\r\n");
    for count in [0, 1, 12, 20, 21, 1000] {
        let tail = engine::tail(&stderr, count);
        let keep = count.min(real.len());
        let expected: Vec<&str> = real[real.len() - keep..]
            .iter()
            .map(String::as_str)
            .collect();
        assert_eq!(tail, expected, "count {count}");
    }
    assert_eq!(engine::tail("", 12), Vec::<&str>::new());
}

#[test]
fn info_counts_read_in_any_order_and_bad_values_are_zero() {
    let lines = [
        "KLEE: done: explored paths = 488",
        "KLEE: done: completed paths = 106",
        "KLEE: done: total instructions = 650279",
        "Elapsed: 00:01:00  ",
        "KLEE: done: partially completed paths = 382",
        "KLEE: done: generated tests = 9",
    ];
    let expected = Exploration {
        paths: 488,
        completed: 106,
        instructions: 650_279,
        elapsed: "00:01:00".into(),
    };
    for order in permutations(&lines) {
        assert_eq!(
            engine::exploration(&order.join("\n")),
            expected,
            "{order:?}"
        );
    }
    for (info, paths) in [
        ("KLEE: done: explored paths = abc", 0),
        ("KLEE: done: explored paths = -4", 0),
        ("KLEE: done: explored paths = 18446744073709551616", 0),
        ("KLEE: done: explored paths =  5  ", 5),
        ("KLEE: done: explored paths=5", 0),
        ("explored paths = 5", 0),
        ("  KLEE: done: explored paths = 5", 0),
        (
            "KLEE: done: explored paths = 1\nKLEE: done: explored paths = 2",
            1,
        ),
        (
            "KLEE: done: explored paths = 18446744073709551615",
            u64::MAX,
        ),
    ] {
        assert_eq!(engine::exploration(info).paths, paths, "{info:?}");
    }
}

// ---- [SYMEX-CODEGEN] [SYMEX-MOCK-STREAM]: the module KLEE loads -------------

#[test]
fn the_module_is_x86_64_linux_with_one_strong_cap_for_every_cap() {
    for cap in [0, 1, 64, 4095, 4096] {
        for ir in [
            "",
            "define i32 @main() {\n  ret i32 0\n}",
            "; only a comment\n",
        ] {
            let module = module::wrap(ir, cap);
            let lines: Vec<&str> = module.lines().collect();
            assert!(
                lines[0].starts_with("target datalayout = \"e-m:e-"),
                "{module}"
            );
            assert_eq!(lines[1], "target triple = \"x86_64-unknown-linux-gnu\"");
            assert_eq!(
                lines.last().copied(),
                Some(format!("@osp_hunt_input_bytes = global i64 {cap}").as_str())
            );
            assert_eq!(module.matches("target triple").count(), 1);
            assert_eq!(module.matches("@osp_hunt_input_bytes").count(), 1);
            assert!(module.contains(ir));
            for host in ["aarch64", "arm64", "apple", "windows", "msvc"] {
                assert!(!module.contains(host), "{host}");
            }
            crate::testkit::contents(&module, &[], &["weak"]);
        }
    }
}

/// Every program in `tests/hunt/` lowers for the engine at every cap.
#[test]
fn the_hunt_corpus_lowers_symbolically_at_every_cap() {
    let corpus = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/hunt");
    let mut programs: Vec<String> = std::fs::read_dir(corpus)
        .expect("tests/hunt")
        .filter_map(Result::ok)
        .map(|entry| entry.path().display().to_string())
        .filter(|path| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "osp" || extension == "ospml")
        })
        .collect();
    programs.sort();
    assert!(programs.len() >= 4, "{programs:?}");
    for path in programs {
        let cli = crate::invocation::parse_args(std::slice::from_ref(&path)).expect("args");
        let input = crate::pipeline::load_input(&cli).expect("load");
        let name = Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        for cap in [0, 64, 4096] {
            let module = module::symbolic_module(&input, cap).expect("symbolic build");
            assert!(module.starts_with("target datalayout"), "{path}");
            assert!(module.ends_with(&format!("@osp_hunt_input_bytes = global i64 {cap}\n")));
            crate::testkit::shows(&module, &["!DILocation"]);
            assert!(
                module.contains(name.as_deref().unwrap_or_default()),
                "{path}"
            );
            crate::testkit::contents(&module, &[], &["optnone"]);
        }
    }
}

#[test]
fn arbitrary_info_entries_never_authorize_recursive_deletion() {
    for entry in ["info", "info/note", RUN_MARKER] {
        let dir = super::spec_findings_tests::Dir::new("unowned-info");
        dir.write("precious.txt", b"keep");
        dir.write(entry, b"ordinary user data");
        assert!(clear_run_directory(&dir.0).is_err(), "accepted {entry}");
        assert_eq!(
            std::fs::read(dir.0.join("precious.txt")).expect("preserved"),
            b"keep"
        );
        assert_eq!(
            std::fs::read(dir.0.join(entry)).expect("info preserved"),
            b"ordinary user data"
        );
    }
}

#[test]
fn only_a_klee_output_directory_is_cleared() {
    let dir = std::env::temp_dir().join(format!("osprey-hunt-clear-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(clear_run_directory(&dir).is_ok(), "absent is fine");
    std::fs::create_dir_all(&dir).expect("dir");
    assert!(clear_run_directory(&dir).is_ok(), "empty is fine");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("precious.txt"), "x").expect("file");
    let error = clear_run_directory(&dir).expect_err("unrelated directory");
    crate::testkit::shows(&error, &["refusing to clear"]);
    std::fs::write(dir.join("info"), "KLEE").expect("info");
    std::fs::write(dir.join(RUN_MARKER), RUN_MARKER_CONTENTS).expect("ownership marker");
    assert!(clear_run_directory(&dir).is_ok() && !dir.exists());
}

#[test]
fn run_rejects_bad_arguments_with_usage() {
    assert_eq!(run(&[]), ExitCode::from(2));
    assert_eq!(run(&["--bogus".to_string()]), ExitCode::from(2));
    assert_eq!(run(&["--help".to_string()]), ExitCode::SUCCESS);
    assert_eq!(run(&["-h".to_string()]), ExitCode::SUCCESS);
}

#[cfg(unix)]
#[test]
fn symlinked_output_directories_are_preserved() {
    let dir = super::spec_findings_tests::Dir::new("symlinked-output");
    dir.write("owned/info", b"KLEE");
    dir.write("owned/precious.txt", b"keep");
    dir.write(&format!("owned/{RUN_MARKER}"), RUN_MARKER_CONTENTS);
    let link = dir.0.join("alias");
    std::os::unix::fs::symlink(dir.0.join("owned"), &link).expect("directory symlink");
    assert!(clear_run_directory(&link).is_err());
    assert!(std::fs::symlink_metadata(&link)
        .expect("preserved symlink")
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read(link.join("precious.txt")).expect("preserved"),
        b"keep"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_ownership_markers_do_not_authorize_deletion() {
    let dir = super::spec_findings_tests::Dir::new("symlinked-marker");
    dir.write("elsewhere", RUN_MARKER_CONTENTS);
    dir.write("output/precious.txt", b"keep");
    let output = dir.0.join("output");
    std::os::unix::fs::symlink(dir.0.join("elsewhere"), output.join(RUN_MARKER))
        .expect("marker symlink");
    assert!(clear_run_directory(&output).is_err());
    assert_eq!(
        std::fs::read(output.join("precious.txt")).expect("preserved"),
        b"keep"
    );
    assert_eq!(
        std::fs::read(dir.0.join("elsewhere")).expect("external marker"),
        RUN_MARKER_CONTENTS
    );
}

/// A program the native build accepts lowers for the engine too: the
/// symbolic switch changes only the unhandled-effect guard [SYMEX-CODEGEN].
#[test]
fn a_fiber_program_lowers_symbolically() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/core/collections/nested_generic_collections_fibers.test.osp"
    )
    .to_string();
    let cli = crate::invocation::parse_args(std::slice::from_ref(&path)).expect("args");
    let input = crate::pipeline::load_input(&cli).expect("load");
    let symbolic = module::symbolic_module(&input, 64).expect("symbolic build");
    crate::testkit::shows(
        &symbolic,
        &[
            "target triple = \"x86_64-unknown-linux-gnu\"",
            "@osp_hunt_input_bytes = global i64 64",
        ],
    );
}
