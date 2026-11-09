//! What a user sees from the CLI when asking for what it can't do, or for the rarer
//! documentation forms: each answer is the real binary's, exit code and words.
//! Implements [DOC-EXPORT] and [DOC-DOCTEST-HARNESS].

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A directory of its own, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("osprey_paths_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(std::fs::create_dir_all(&dir).is_ok(), "{}", dir.display());
        Self(dir)
    }

    fn write(&self, file: &str, text: &str) -> PathBuf {
        let path = self.0.join(file);
        assert!(std::fs::write(&path, text).is_ok(), "{}", path.display());
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn osprey() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = command.current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.."));
    command
}

fn finished(command: &mut Command) -> (Option<i32>, String, String) {
    let output: std::io::Result<Output> = command.output();
    assert!(output.is_ok(), "{output:?}");
    let Ok(output) = output else {
        return (None, String::new(), String::new());
    };
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn assert_refused(command: &mut Command, code: i32, words: &str) {
    let (got, stdout, stderr) = finished(command);
    assert_eq!(got, Some(code), "stdout: {stdout}\nstderr: {stderr}");
    assert!(stderr.contains(words), "wanted {words:?} in: {stderr}");
}

/// [FAILURE-EXPLICIT]: hostile source must produce a diagnostic, including #362.
#[test]
fn deep_expressions_report_positioned_errors_without_aborting() {
    let scratch = Scratch::new("expression_depth");
    let cases = [
        (
            "group.ospml",
            format!("value = {}1{}", "(".repeat(50_000), ")".repeat(50_000)),
        ),
        (
            "chain.ospml",
            format!("value = {}", vec!["1"; 50_000].join(" + ")),
        ),
        (
            "list.osp",
            format!("let value = {}1{}", "[".repeat(5_000), "]".repeat(5_000)),
        ),
        (
            "group.osp",
            format!("let value = {}1{}", "(".repeat(50_000), ")".repeat(50_000)),
        ),
        (
            "chain.osp",
            format!("let value = {}", vec!["1"; 50_000].join(" + ")),
        ),
        (
            "list.ospml",
            format!("value = {}1{}", "[".repeat(5_000), "]".repeat(5_000)),
        ),
    ];
    let mut failures = Vec::new();
    for (name, source) in cases {
        let path = scratch.write(name, &format!("// source starts below\n{source}\n"));
        let (code, stdout, stderr) = finished(osprey().arg("--check").arg(&path));
        if code != Some(1) || !stderr.contains(":2:") || !stderr.contains("nesting limit") {
            failures.push(format!("{name}: {code:?}\n{stdout}\n{stderr}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const CONTRACTS: &str = "/// A cell.
effect Cell<T> {
    /// Reads it.
    read: fn() -> T
}

/// By name.
type Named = Named { value: int }

/// By position.
type Pair = Pair(int, string)

/// A contract.
signature Api {
    let limit: int
    fn fetch<T>(key: string) -> T ![Cell<T>]
    fn open<T>(key: string) -> T ![Cell<T> | r]
    type Hidden
    type Shown = int
}
";

// A signature's page lists each of its forms as written: values, functions under a
// closed and an open row of generic effects, opaque and manifest types.
#[test]
fn api_docs_render_every_form_a_contract_can_take() {
    let scratch = Scratch::new("contracts");
    let source = scratch.write("source.osp", CONTRACTS);
    let out = scratch.0.join("docs");
    let (code, _, stderr) = finished(
        osprey()
            .arg("--docs")
            .arg("--source")
            .arg(&source)
            .arg("--docs-dir")
            .arg(&out),
    );
    assert_eq!(code, Some(0), "{stderr}");
    let page = |name: &str| std::fs::read_to_string(out.join("api").join(name)).unwrap_or_default();
    let contract = page("api.md");
    for line in [
        "- `limit: int`",
        "- `fn fetch<T>(string) -> T ![Cell<T>]`",
        "- `fn open<T>(string) -> T ![Cell<T> | r]`",
        "- `type Hidden`",
        "- `type Shown = int`",
    ] {
        assert!(contract.contains(line), "wanted {line:?} in: {contract}");
    }
    let by_position = page("pair.md");
    assert!(by_position.contains("- `payload 1: int`"), "{by_position}");
    assert!(
        by_position.contains("- `payload 2: string`"),
        "{by_position}"
    );
    assert!(page("named.md").contains("- `value: int`"));
}

const EXAMPLE_WITH_OUTPUT: &str = "/// An example that must run.
/// # Examples
/// ```osprey
/// print(documented(\"x\"))
/// ```
/// ```output
/// x
/// ```
fn documented(n) = n
";

// An example with an expected output has to run, and only some targets can run here.
#[test]
fn doctests_say_which_targets_can_run_an_example() {
    let scratch = Scratch::new("doctargets");
    let source = scratch.write("source.osp", EXAMPLE_WITH_OUTPUT);
    for target in ["ios", "ios-sim", "android-arm64", "macos"] {
        let (code, stdout, stderr) = finished(
            osprey()
                .arg(&source)
                .arg("--doctests")
                .arg(format!("--target={target}")),
        );
        assert_eq!(code, Some(1), "{target}: {stdout}\n{stderr}");
        assert_eq!(
            stdout, "doctests: 0 passed, 1 failed\n",
            "{target}: {stderr}"
        );
        let reason = format!("runnable doctests require native or wasm32, found {target}");
        assert!(stderr.contains(&reason), "{target}: {stderr}");
    }
    let (code, stdout, stderr) = finished(osprey().arg(&source).arg("--doctests"));
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "doctests: 1 passed, 0 failed\n");
}

#[test]
fn the_test_job_count_must_be_a_positive_integer() {
    let scratch = Scratch::new("jobs");
    let suite = scratch.write("a.test.osp", "print(\"ran\")\n");
    for jobs in ["0", "-1", "abc", "", "1.5"] {
        assert_refused(
            osprey()
                .arg("test")
                .arg(&suite)
                .env("OSPREY_TEST_JOBS", jobs),
            2,
            "osprey test: OSPREY_TEST_JOBS must be a positive integer",
        );
    }
}

#[cfg(unix)]
#[test]
fn the_test_job_count_must_be_text() {
    use std::os::unix::ffi::OsStrExt;
    let scratch = Scratch::new("jobsbytes");
    let suite = scratch.write("a.test.osp", "print(\"ran\")\n");
    assert_refused(
        osprey()
            .arg("test")
            .arg(&suite)
            .env("OSPREY_TEST_JOBS", std::ffi::OsStr::from_bytes(&[0xff])),
        2,
        "osprey test: OSPREY_TEST_JOBS must be valid UTF-8",
    );
}

// Every way of asking for a target that doesn't exist, or a memory backend a target
// doesn't have, is told so rather than building something else.
#[test]
fn a_target_that_cannot_be_built_is_refused_by_name() {
    let scratch = Scratch::new("targets");
    let source = scratch.write("source.osp", "print(\"x\")\n");
    let out = scratch.0.join("out");
    for mode in ["--compile", "--llvm"] {
        assert_refused(
            osprey()
                .arg(&source)
                .arg(mode)
                .arg("--target=nonsense")
                .arg("-o")
                .arg(&out),
            2,
            "unknown target 'nonsense'",
        );
    }
    assert_refused(
        osprey()
            .arg(&source)
            .arg("--compile")
            .arg("--target=wasm32")
            .arg("--memory=gc")
            .arg("-o")
            .arg(&out),
        1,
        "wasm32 has no garbage-collecting runtime",
    );
}

// A program that doesn't compile is the author's problem, and a missing one the
// caller's, whether or not the symbolic engine is installed [SYMEX-CLI].
#[test]
fn hunt_answers_before_it_looks_for_its_engine() {
    let scratch = Scratch::new("hunt");
    let broken = scratch.write("broken.osp", "fn broken(\n");
    assert_refused(osprey().arg("hunt").arg(&broken), 1, "syntax error");
    let missing = scratch.0.join("missing.osp");
    assert_refused(osprey().arg("hunt").arg(&missing), 2, "cannot read");
    assert_refused(osprey().arg("hunt"), 2, "a source file is required");
}

// `--run` needs a WASI host: Node's, the one the wasm corpus runs under, is on
// every CI runner, wasmtime is not [WASM-TARGET].
fn node_wasi_host(scratch: &Scratch) -> PathBuf {
    let smoke =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../scripts/wasm/wasm-smoke.mjs");
    let host = scratch.write(
        "wasi-host",
        &format!("#!/bin/sh\nexec node '{}' \"$@\"\n", smoke.display()),
    );
    let executable = std::os::unix::fs::PermissionsExt::from_mode(0o755);
    let made = std::fs::set_permissions(&host, executable);
    assert!(made.is_ok(), "{made:?}");
    host
}

// Every wasm32 build lowers through a scratch `.wasm.ll` and `.wasm.o`, and
// `--run` through a scratch `.wasm`: none of them may outlive the command, or
// each corpus run leaves thousands of files in the temp directory.
#[test]
fn wasm32_builds_leave_no_scratch_files_behind() {
    let scratch = Scratch::new("wasm_scratch");
    let temp = scratch.0.join("tmp");
    assert!(std::fs::create_dir_all(&temp).is_ok(), "{}", temp.display());
    let source = scratch.write("hello.osp", "print(\"hi\")\n");
    let out = scratch.0.join("hello.wasm");
    let host = node_wasi_host(&scratch);
    let run = finished(
        osprey()
            .arg(&source)
            .arg("--target=wasm32")
            .arg("--run")
            .env("OSPREY_WASM_RUN", &host)
            .env("TMPDIR", &temp),
    );
    assert_eq!((run.0, run.1.as_str()), (Some(0), "hi\n"), "{}", run.2);
    let compile = finished(
        osprey()
            .arg(&source)
            .arg("--target=wasm32")
            .arg("--compile")
            .arg("-o")
            .arg(&out)
            .env("TMPDIR", &temp),
    );
    assert_eq!(compile.0, Some(0), "{}", compile.2);
    assert!(out.is_file(), "{}", out.display());
    let left: Vec<String> = std::fs::read_dir(&temp)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        left,
        Vec::<String>::new(),
        "scratch files left in {}",
        temp.display()
    );
}
