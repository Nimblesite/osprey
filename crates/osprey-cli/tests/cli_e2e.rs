//! End-to-end CLI tests that drive the real `osprey` binary.
//!
//! `main`, `run_lsp`, and the full compile -> link -> run pipeline are process
//! entry points the in-process twins (the `src` unit tests and
//! `tests/examples_compile.rs`) can never reach. Spawning the built binary does
//! reach them, and because `cargo llvm-cov` instruments `CARGO_BIN_EXE_osprey`
//! too, each child's coverage is merged back into the report — so these tests
//! count toward the per-crate gate. [TEST-RULES][COVERAGE-THRESHOLDS-JSON]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Repo root: `crates/osprey-cli` -> `../..`. The C runtime archives `osprey`
/// links at `--run`/`--compile` time live under `compiler/bin/` there, and
/// `find_runtime_lib` resolves them relative to the process cwd — so every
/// child runs from here.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// A `Command` for the built `osprey`, rooted at the repo so the runtime
/// archives resolve.
fn osprey() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = cmd.current_dir(repo_root());
    cmd
}

/// Write `body` to a uniquely-named temp `.osp` (the name doubles as the file
/// stem, which `--compile` turns into the output executable's name).
fn temp_osp(name: &str, body: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("osprey_cli_e2e_{name}.osp"));
    // A failed write surfaces as a downstream "cannot read"/parse failure that
    // trips the test's own assertions — so no panic is needed here.
    let _ = std::fs::write(&path, body);
    path
}

fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("osprey_cli_e2e_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

fn read_text(path: &Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => format!("read failed: {e}"),
    }
}

const HELLO: &str = "let g = \"hi\"\nprint(\"v=${g}\")\n";

/// The captured result of one invocation.
struct Out {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn finish(mut cmd: Command) -> Out {
    // A spawn failure becomes an empty, code-less `Out`; every caller asserts on
    // an expected code/stdout, so the failure reports loudly through them.
    match cmd.output() {
        Ok(out) => Out {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Err(e) => Out {
            code: None,
            stdout: String::new(),
            stderr: format!("spawn failed: {e}"),
        },
    }
}

/// Run with literal args (no source file): `--version`, `--hover`, etc.
fn run_args(args: &[&str]) -> Out {
    let mut cmd = osprey();
    let _ = cmd.args(args);
    finish(cmd)
}

fn run_args_with_stdin(args: &[&str], input: &str) -> Out {
    use std::io::Write;

    let mut cmd = osprey();
    let _ = cmd
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            return Out {
                code: None,
                stdout: String::new(),
                stderr: format!("spawn failed: {e}"),
            };
        }
    };
    if let Some(stdin) = child.stdin.as_mut() {
        let _ = stdin.write_all(input.as_bytes());
    }
    match child.wait_with_output() {
        Ok(out) => Out {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Err(e) => Out {
            code: None,
            stdout: String::new(),
            stderr: format!("wait failed: {e}"),
        },
    }
}

/// Run against a source `path` plus extra flags — the common compiling shape.
fn run_file(path: &Path, extra: &[&str]) -> Out {
    let mut cmd = osprey();
    let _ = cmd.arg(path).args(extra);
    finish(cmd)
}

/// Run a single `mode` against `path` with `OSPREY_CC` overridden — used to
/// drive the C-driver failure branches of `build_executable`.
fn run_file_cc(path: &Path, mode: &str, cc: &str) -> Out {
    let mut cmd = osprey();
    let _ = cmd.arg(path).arg(mode).env("OSPREY_CC", cc);
    finish(cmd)
}

/// Type-clean but codegen-rejected: a still-generic lambda returned from a
/// generic function has no concrete cell ABI (`let f = id` and slot-typed
/// uses now specialise instead — [TYPE-GENERICS-FN]). It passes the type
/// gate, so every compiling mode reaches codegen and fails there —
/// exercising the `Err` arms `compile_program` feeds.
const GENERIC_AS_VALUE: &str =
    "fn mk<T>(x: T) = |y| => x\nlet f = mk(1)\nprint(\"${f(0)}\")\n";

/// Explicit effect resume must run the rest of the handled computation and then
/// return the handled computation's answer to the arm.
const RESUME_EFFECT: &str = r#"
effect Audit {
    step: fn(string) -> int
}

fn pipeline() -> int !Audit = {
    let a = perform Audit.step("load")
    let b = perform Audit.step("parse")
    match a + b {
        Success { value } => value
        Error { message } => 0
    }
}

fn main() = {
    mut n = 0
    let total = handle Audit
        step label => {
            n = match n + 1 {
                Success { value } => value
                Error { message } => n
            }
            let answer = resume(n)
            print("after " + label + ": answer=" + toString(answer))
            answer
        }
    in pipeline()
    print("total=" + toString(total))
}
"#;

#[test]
fn version_plain_and_json() {
    let plain = run_args(&["--version"]);
    assert_eq!(plain.code, Some(0));
    assert!(plain.stdout.contains("osprey"), "{}", plain.stdout);
    let json = run_args(&["--version", "--json"]);
    assert_eq!(json.code, Some(0));
    assert!(json.stdout.contains("\"kind\":\"cli\""), "{}", json.stdout);
}

#[test]
fn lsp_exits_cleanly_on_closed_stdin() {
    let status = osprey()
        .arg("lsp")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn lsp");
    assert!(status.success(), "lsp should exit 0 at EOF");
}

#[test]
fn hover_prints_known_builtin_and_is_silent_for_unknown() {
    let known = run_args(&["--hover", "print"]);
    assert_eq!(known.code, Some(0));
    assert!(known.stdout.contains("print"), "{}", known.stdout);
    let unknown = run_args(&["--hover", "__definitely_not_a_builtin__"]);
    assert_eq!(unknown.code, Some(0));
    assert!(unknown.stdout.trim().is_empty(), "{}", unknown.stdout);
}

#[test]
fn fmt_stdout_check_and_rewrite_modes() {
    let prog = temp_osp("fmt_modes", "fn main() = {\nprint(1)\n}\n");
    let path = prog.to_string_lossy().into_owned();
    let shown = run_args(&["fmt", "--stdout", "--flavor", "default", &path]);
    assert_eq!(shown.code, Some(0), "stderr={}", shown.stderr);
    assert!(shown.stdout.contains("    print(1)"), "{}", shown.stdout);
    let unchanged = read_text(&prog);
    assert!(unchanged.contains("\nprint(1)\n"), "{unchanged}");

    let checked = run_args(&["fmt", "--check", &path]);
    assert_ne!(checked.code, Some(0));
    assert!(
        checked.stdout.contains("would reformat"),
        "{}",
        checked.stdout
    );

    let quiet = run_args(&["fmt", "--quiet", &path]);
    assert_eq!(quiet.code, Some(0), "stderr={}", quiet.stderr);
    assert!(quiet.stdout.trim().is_empty(), "{}", quiet.stdout);
    let rewritten = read_text(&prog);
    assert!(rewritten.contains("    print(1)"), "{rewritten}");

    let clean = temp_osp("fmt_clean", "fn main() = {\n    print(1)\n}\n");
    let clean_path = clean.to_string_lossy().into_owned();
    let no_change = run_args(&["fmt", &clean_path]);
    assert_eq!(no_change.code, Some(0), "stderr={}", no_change.stderr);
    assert!(no_change.stdout.trim().is_empty(), "{}", no_change.stdout);

    let loud = temp_osp("fmt_loud", "fn main() = {\nprint(2)\n}\n");
    let loud_path = loud.to_string_lossy().into_owned();
    let rewritten_loud = run_args(&["fmt", &loud_path]);
    assert_eq!(
        rewritten_loud.code,
        Some(0),
        "stderr={}",
        rewritten_loud.stderr
    );
    assert!(
        rewritten_loud.stdout.contains("formatted"),
        "{}",
        rewritten_loud.stdout
    );
}

#[test]
fn fmt_recurses_directories_and_formats_stdin() {
    let dir = temp_dir("fmt_tree");
    let nested = dir.join("nested");
    let _ = std::fs::create_dir_all(&nested);
    let osp = dir.join("a.osp");
    let ml = nested.join("b.ospml");
    let ignored = nested.join("ignored.txt");
    let _ = std::fs::write(&osp, "fn main() = {\nprint(1)\n}\n");
    let _ = std::fs::write(&ml, "print 2\n");
    let _ = std::fs::write(&ignored, "not osprey\n");

    let dir_arg = dir.to_string_lossy().into_owned();
    let shown = run_args(&["fmt", "--stdout", &dir_arg]);
    assert_eq!(shown.code, Some(0), "stderr={}", shown.stderr);
    assert!(shown.stdout.contains("print(1)"), "{}", shown.stdout);
    assert!(shown.stdout.contains("print 2"), "{}", shown.stdout);
    assert!(!shown.stdout.contains("not osprey"), "{}", shown.stdout);

    let piped = run_args_with_stdin(&["fmt", "--flavor=ml", "-"], "print 3\n");
    assert_eq!(piped.code, Some(0), "stderr={}", piped.stderr);
    assert!(piped.stdout.contains("print 3"), "{}", piped.stdout);

    let bad_stdin = run_args_with_stdin(&["fmt", "-"], "fn main( = {\n");
    assert_ne!(bad_stdin.code, Some(0));
    assert!(bad_stdin.stderr.contains("stdin:"), "{}", bad_stdin.stderr);
}

#[test]
fn fmt_reports_usage_parse_and_read_errors() {
    let no_paths = run_args(&["fmt"]);
    assert_eq!(no_paths.code, Some(2));
    assert!(
        no_paths.stderr.contains("usage: osprey fmt"),
        "{}",
        no_paths.stderr
    );

    let missing_flavor = run_args(&["fmt", "--flavor"]);
    assert_eq!(missing_flavor.code, Some(2));
    assert!(
        missing_flavor.stderr.contains("--flavor requires"),
        "{}",
        missing_flavor.stderr
    );

    let bad_flavor = run_args(&["fmt", "--flavor=bogus", "x.osp"]);
    assert_eq!(bad_flavor.code, Some(2));
    assert!(
        bad_flavor.stderr.contains("unknown flavor"),
        "{}",
        bad_flavor.stderr
    );

    let bad_flag = run_args(&["fmt", "--bogus", "x.osp"]);
    assert_eq!(bad_flag.code, Some(2));
    assert!(
        bad_flag.stderr.contains("unknown flag --bogus"),
        "{}",
        bad_flag.stderr
    );

    let missing = run_args(&["fmt", "/no/such/osprey_fmt_missing.osp"]);
    assert_ne!(missing.code, Some(0));
    assert!(missing.stderr.contains("cannot read"), "{}", missing.stderr);

    let broken = temp_osp("fmt_broken", "fn main( = {\n");
    let path = broken.to_string_lossy().into_owned();
    let parsed = run_args(&["fmt", &path]);
    assert_ne!(parsed.code, Some(0));
    assert!(!parsed.stderr.is_empty());
}

#[cfg(unix)]
#[test]
fn fmt_reports_write_errors() {
    use std::os::unix::fs::PermissionsExt;

    let blocked_dir = temp_dir("fmt_blocked");
    if let Ok(metadata) = std::fs::metadata(&blocked_dir) {
        let mut perms = metadata.permissions();
        perms.set_mode(0o000);
        let _ = std::fs::set_permissions(&blocked_dir, perms);
    }
    let blocked_arg = blocked_dir.to_string_lossy().into_owned();
    let blocked = run_args(&["fmt", &blocked_arg]);
    if let Ok(metadata) = std::fs::metadata(&blocked_dir) {
        let mut perms = metadata.permissions();
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(&blocked_dir, perms);
    }
    assert_eq!(blocked.code, Some(0), "stderr={}", blocked.stderr);

    let prog = temp_osp("fmt_readonly", "fn main() = {\nprint(1)\n}\n");
    let path = prog.to_string_lossy().into_owned();
    if let Ok(metadata) = std::fs::metadata(&prog) {
        let mut perms = metadata.permissions();
        perms.set_mode(0o444);
        let _ = std::fs::set_permissions(&prog, perms);
    }

    let out = run_args(&["fmt", &path]);
    if let Ok(metadata) = std::fs::metadata(&prog) {
        let mut perms = metadata.permissions();
        perms.set_mode(0o644);
        let _ = std::fs::set_permissions(&prog, perms);
    }
    assert_ne!(out.code, Some(0));
    assert!(out.stderr.contains("cannot write"), "{}", out.stderr);
}

#[test]
fn unknown_flag_exits_two_with_usage() {
    let prog = temp_osp("flag", HELLO);
    let o = run_file(&prog, &["--bogus"]);
    assert_eq!(o.code, Some(2));
    assert!(o.stderr.contains("unknown flag --bogus"), "{}", o.stderr);
}

#[test]
fn check_ok_reports_statement_count() {
    let prog = temp_osp("ok", HELLO);
    let o = run_file(&prog, &[]); // default mode is --check
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert!(o.stdout.contains("ok"), "{}", o.stdout);
}

#[test]
fn check_missing_file_exits_two() {
    let o = run_args(&["/no/such/osprey_e2e_missing.osp"]);
    assert_eq!(o.code, Some(2));
    assert!(o.stderr.contains("cannot read"), "{}", o.stderr);
}

#[test]
fn check_parse_error_is_reported() {
    let prog = temp_osp("parse", "fn = = =\n");
    let o = run_file(&prog, &["--check"]);
    assert_ne!(o.code, Some(0));
    assert!(!o.stderr.is_empty());
}

#[test]
fn flavor_marker_conflict_exits_two() {
    let prog = temp_osp("flavor_conflict", "// osprey: flavor=ml\nprint 1\n");
    let o = run_file(&prog, &["--check"]);
    assert_eq!(o.code, Some(2));
    assert!(o.stderr.contains("flavor marker"), "{}", o.stderr);
}

#[test]
fn check_type_error_is_reported() {
    let prog = temp_osp("typed", "let y = 1 + \"oops\" - true\n");
    let o = run_file(&prog, &["--check"]);
    assert_ne!(o.code, Some(0));
    assert!(!o.stderr.is_empty(), "{}", o.stderr);
}

#[test]
fn llvm_emits_ir_and_rejects_ill_typed() {
    let ok = temp_osp("llok", HELLO);
    let good = run_file(&ok, &["--llvm"]);
    assert_eq!(good.code, Some(0), "stderr={}", good.stderr);
    assert!(good.stdout.contains("define"), "{}", good.stdout);
    let bad = temp_osp("llbad", "let y = 1 + true\n");
    let rejected = run_file(&bad, &["--llvm"]);
    assert_ne!(rejected.code, Some(0));
}

#[test]
fn ast_and_symbols_modes() {
    let prog = temp_osp("astsym", HELLO);
    let ast = run_file(&prog, &["--ast"]);
    assert_eq!(ast.code, Some(0));
    assert!(!ast.stdout.is_empty());
    let sym = run_file(&prog, &["--symbols"]);
    assert_eq!(sym.code, Some(0));
    assert!(sym.stdout.contains("\"name\""), "{}", sym.stdout);
}

#[test]
fn run_compiles_links_and_executes() {
    let prog = temp_osp("run", HELLO);
    let o = run_file(&prog, &["--run"]);
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert!(o.stdout.contains("v=hi"), "{}", o.stdout);
}

#[test]
fn explicit_resume_runs_the_performer_continuation() {
    let prog = temp_osp("resume_effect", RESUME_EFFECT);
    let o = run_file(&prog, &["--run"]);
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert_eq!(
        o.stdout,
        "after parse: answer=3\nafter load: answer=3\ntotal=3\n"
    );
}

#[test]
fn compile_writes_executable_to_cwd() {
    let prog = temp_osp("compile", HELLO);
    // `compile_program_to_disk` names the output after the source stem, in cwd.
    let artifact = repo_root().join("osprey_cli_e2e_compile");
    let _ = std::fs::remove_file(&artifact);
    let o = run_file(&prog, &["--compile"]);
    let produced = artifact.exists();
    let _ = std::fs::remove_file(&artifact);
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert!(produced, "expected executable at {}", artifact.display());
    assert!(o.stdout.contains("osprey_cli_e2e_compile"), "{}", o.stdout);
}

#[test]
fn sandbox_blocks_filesystem_capability() {
    let prog = temp_osp("fs", "let c = readFile(\"x.txt\")\n");
    let o = run_file(&prog, &["--llvm", "--no-fs"]);
    assert_ne!(o.code, Some(0), "stdout={}", o.stdout);
    assert!(!o.stderr.is_empty());
}

#[test]
fn quiet_suppresses_the_ok_line() {
    let prog = temp_osp("quiet", HELLO);
    let o = run_file(&prog, &["--check", "--quiet"]);
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert!(o.stdout.trim().is_empty(), "{}", o.stdout);
}

#[test]
fn llvm_reports_a_codegen_error() {
    let prog = temp_osp("cgllvm", GENERIC_AS_VALUE);
    let o = run_file(&prog, &["--llvm"]);
    assert_ne!(o.code, Some(0));
    assert!(o.stderr.contains("codegen"), "{}", o.stderr);
}

#[test]
fn run_reports_a_codegen_error() {
    let prog = temp_osp("cgrun", GENERIC_AS_VALUE);
    let o = run_file(&prog, &["--run"]);
    assert_ne!(o.code, Some(0));
    assert!(o.stderr.contains("codegen"), "{}", o.stderr);
}

#[test]
fn compile_reports_a_failing_c_compiler() {
    // `false` runs and exits non-zero -> the "cc failed to compile" branch.
    let prog = temp_osp("ccfail", HELLO);
    let o = run_file_cc(&prog, "--compile", "false");
    let _ = std::fs::remove_file(repo_root().join("osprey_cli_e2e_ccfail"));
    assert_ne!(o.code, Some(0));
    assert!(!o.stderr.is_empty(), "{}", o.stderr);
}

#[test]
fn run_reports_an_uninvokable_c_compiler() {
    // A missing driver can't be spawned at all -> the "could not invoke" branch.
    let prog = temp_osp("ccmiss", HELLO);
    let o = run_file_cc(&prog, "--run", "osprey_no_such_cc_zzz");
    assert_ne!(o.code, Some(0));
    assert!(!o.stderr.is_empty(), "{}", o.stderr);
}
