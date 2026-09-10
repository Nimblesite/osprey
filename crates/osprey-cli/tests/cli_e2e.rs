//! End-to-end CLI tests that drive the real `osprey` binary.
//!
//! `main`, `run_lsp`, and the full compile -> link -> run pipeline are process
//! entry points the in-process twins (the `src` unit tests and
//! `tests/examples_compile.rs`) can never reach. Spawning the built binary does
//! reach them, and because `cargo llvm-cov` instruments `CARGO_BIN_EXE_osprey`
//! too, each child's coverage is merged back into the report — so these tests
//! count toward the per-crate gate.

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

#[path = "cases/doctests.rs"]
mod doctests;

#[path = "cases/api_docs.rs"]
mod api_docs;

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

/// Type-clean but codegen-rejected: an FFI callback slot is a raw C code
/// pointer, so a capturing lambda has nowhere to carry its environment
/// ([FFI-CALLBACKS]). It passes the type gate, so every compiling mode reaches
/// codegen and fails there — exercising the `Err` arms `compile_program` feeds.
///
/// `(x + base) ?: 0` discharges the arithmetic `Result`; without it the lambda
/// is `(int) -> Result<int, MathError>` and the type gate rejects the call
/// before codegen sees the capture.
const CODEGEN_REJECTED: &str = concat!(
    "extern fn registerCallback(cb: fn(int) -> int) -> int\n",
    "let base = 10\n",
    "let r = registerCallback(fn(x) => (x + base) ?: 0)\n",
    "print(\"${r}\")\n",
);

/// A still-generic lambda used as a bare VALUE: no ABI to fix and no call site
/// to specialise against ([TYPE-GENERICS-FN]).
///
/// This drove the two codegen-error tests until the checker learned to reject
/// it, which is a strictly better place to catch it. It stays here to pin
/// WHERE it is rejected, so the move cannot happen again unnoticed.
const GENERIC_AS_VALUE: &str = "fn mk<T>(x: T) = |y| => x\nprint(\"${mk(1)}\")\n";

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

/// A handler arm that resumes its continuation TWICE on ONE control path.
/// `Choose.pick` carries no multiplicity keyword, so it is `once`, and the arm
/// is rejected before it can run ([MULTI-HANDLE-ONCE], [MULTI-COMPAT]).
///
/// This lived as `examples/failscompilation/multishot_resume_rejected.ospo`,
/// which was a category error while the rejection was a RUNTIME one: the
/// program was well formed, so a must-reject fixture could never observe the
/// abort. It "passed" only because `x + 1` and `a + b` lacked the `?:` that
/// `[ARITH-CHECKED]` requires, and the corpus recorded that unrelated type
/// error as the expected rejection. Multiplicity moves the verdict back to
/// compile time, where the arm has always been visible.
const MULTISHOT_RESUME: &str = r#"
effect Choose {
    pick: fn() -> int
}

fn both() -> int !Choose = {
    let x = perform Choose.pick()
    x + 1 ?: 0
}

fn main() = {
    let total = handle Choose
        pick => {
            let a = resume(10)
            let b = resume(20)
            a + b ?: 0
        }
    in both()
    print("total=" + toString(total))
}
"#;

/// The same effect answered by an arm with one `resume` per `match` branch.
/// Two `resume` SITES, at most one per control path — the shape
/// [MULTI-HANDLE-ONCE] must keep accepting.
const BRANCHWISE_RESUME: &str = r#"
effect Choose {
    pick: fn() -> int
}

fn both() -> int !Choose = {
    let x = perform Choose.pick()
    x + 1 ?: 0
}

fn main() = {
    let total = handle Choose
        pick => match true {
            true => resume(29)
            false => resume(0)
        }
    in both()
    print("total=" + toString(total))
}
"#;

#[test]
fn version_plain_and_json() {
    // [SWR-VERSION-BUILD-STAMPING] [SWR-VERSION-CLI-OUTPUT]
    let plain = run_args(&["--version"]);
    assert_eq!(plain.code, Some(0));
    assert_eq!(plain.stdout, "osprey 0.0.0-dev\n");

    let json = run_args(&["--version", "--json"]);
    assert_eq!(json.code, Some(0));
    assert_eq!(
        json.stdout,
        "{\"manifestVersion\":1,\"name\":\"osprey\",\"version\":\"0.0.0-dev\",\"kind\":\"cli\",\"product\":\"osprey\"}\n"
    );
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
fn deps_refuses_a_file_that_did_not_parse() {
    // [STAGE-SIGNALS-EXACT] A dependency set is only exact if it is derived
    // from a program that exists. Parsing is best-effort, so a broken file
    // still yields a partial tree, and reading dependencies off it prints
    // FEWER than the source asks for — with no way to tell that apart from a
    // view that genuinely reads nothing. A wrongly empty dirty set is a
    // subtree that never rebuilds, which is the one bug [STAGE-SIGNALS-DIRTY]
    // exists to remove, so `--deps` must refuse rather than under-report.
    let broken = temp_osp(
        "deps_unparsed",
        "static effect Signal<T> { read: fn( -> T }\n",
    );
    let out = run_file(&broken, &["--deps"]);
    assert_eq!(
        out.code,
        Some(1),
        "a file that did not parse must not report a dependency set; stdout={} stderr={}",
        out.stdout,
        out.stderr
    );
    assert!(
        out.stderr.contains("syntax error"),
        "the refusal must name the syntax error; stderr={}",
        out.stderr
    );
    assert!(
        out.stdout.trim().is_empty(),
        "no dependency line may be printed from a partial tree; stdout={}",
        out.stdout
    );

    // The positive control: the same shape, parsing, still reports and exits 0.
    let good = temp_osp(
        "deps_parsed",
        "type Count = { value: int }\nstatic effect Signal<T> { read: fn() -> T }\n         fn counterLabel() = \"count: ${(perform Signal<Count>.read()).value}\"\n",
    );
    let ok = run_file(&good, &["--deps"]);
    assert_eq!(ok.code, Some(0), "stderr={}", ok.stderr);
    assert!(
        ok.stdout.contains("counterLabel: Signal<Count>.read"),
        "stdout={}",
        ok.stdout
    );
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
    // [EFFECTS-RESUME]
    let prog = temp_osp("resume_effect", RESUME_EFFECT);
    let o = run_file(&prog, &["--run"]);
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert_eq!(
        o.stdout,
        "after parse: answer=3\nafter load: answer=3\ntotal=3\n"
    );
}

#[test]
fn a_second_resume_on_one_path_is_rejected_at_compile_time() {
    // [MULTI-HANDLE-ONCE] An arm for a `once` operation may use `resume` at most
    // once on every control path, and `Choose.pick` is `once` because it carries
    // no multiplicity keyword ([MULTI-COMPAT]). The arm is visible at compile
    // time, so the rejection is too: this program aborted at run time with
    // `fatal: continuation already resumed` before multiplicity existed, and
    // [MULTI-COMPAT] narrows exactly that program to a compile-time error — the
    // same program rejected earlier, not a program that stops working.
    //
    // The runtime guard in `compiler/runtime/effects_coro.c` stays as a
    // defensive backstop for invalid compiler output, in the same sense as the
    // generic handler-key null lookup ([EFFECTS-GENERIC-RUNTIME]). It is no
    // longer reachable from a source program, which is why this test asserts
    // the diagnostic instead of the abort.
    let prog = temp_osp("multishot_resume", MULTISHOT_RESUME);
    let check = run_file(&prog, &["--check"]);
    assert_ne!(
        check.code,
        Some(0),
        "two `resume` sites on one path must not type-check; stdout={} stderr={}",
        check.stdout,
        check.stderr
    );
    let diagnostic = format!("{}{}", check.stdout, check.stderr);
    for fragment in ["Choose.pick", "may resume more than once", "once"] {
        assert!(
            diagnostic.contains(fragment),
            "expected the [MULTI-HANDLE-ONCE] diagnostic to name {fragment:?}; got {diagnostic}"
        );
    }
    // Rejected at the checker, so nothing is emitted and nothing runs.
    let run = run_file(&prog, &["--run"]);
    assert_ne!(run.code, Some(0), "stdout={}", run.stdout);
    assert!(
        !run.stdout.contains("total="),
        "the program must not reach its print; stdout={}",
        run.stdout
    );
}

#[test]
fn two_resume_sites_on_different_branches_stay_legal() {
    // [MULTI-HANDLE-ONCE] is affine per PATH, not per arm: only one branch of a
    // `match` runs, so an arm with a `resume` in each branch resumes at most
    // once. Osprey has no loop construct ([BUILTIN-ITER]), so branch and
    // sequence are the only two shapes, and this is the positive control that
    // keeps the check from degenerating into `contains_resume`. The corpus
    // program `tests/regressions/effects/abort_vs_resume.test.osp` is the same
    // shape end to end; this pins the CLI verdict beside its negative.
    let prog = temp_osp("branchwise_resume", BRANCHWISE_RESUME);
    let check = run_file(&prog, &["--check"]);
    assert_eq!(
        check.code,
        Some(0),
        "one `resume` per branch is at most one per path; stderr={}",
        check.stderr
    );
    let run = run_file(&prog, &["--run"]);
    assert_eq!(run.code, Some(0), "stderr={}", run.stderr);
    assert!(
        run.stdout.contains("total=30"),
        "expected the resumed answer; stdout={}",
        run.stdout
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
    // [SECURITY-CAPABILITY-GATES]
    let prog = temp_osp("fs", "let c = readFile(\"x.txt\")\n");
    let o = run_file(&prog, &["--llvm", "--no-fs"]);
    assert_ne!(o.code, Some(0), "stdout={}", o.stdout);
    assert!(
        o.stderr
            .contains("security: `readFile` is disabled by --no-fs"),
        "{}",
        o.stderr
    );
}

#[test]
fn network_gates_run_before_type_checking() {
    // [SECURITY-CAPABILITY-GATES] Both calls have the wrong arity. The security
    // diagnostic wins because policy enforcement precedes type checking.
    for (name, source, flag, expected) in [
        (
            "blocked_http",
            "let r = httpGet()\n",
            "--no-http",
            "security: `httpGet` is disabled by --no-http",
        ),
        (
            "blocked_websocket",
            "let r = websocketConnect()\n",
            "--no-websocket",
            "security: `websocketConnect` is disabled by --no-websocket",
        ),
    ] {
        let prog = temp_osp(name, source);
        let o = run_file(&prog, &["--llvm", flag]);
        assert_ne!(o.code, Some(0), "stdout={}", o.stdout);
        assert!(o.stderr.contains(expected), "{}", o.stderr);
        assert!(!o.stderr.contains("expects exactly"), "{}", o.stderr);
    }
}

#[test]
fn no_ffi_rejects_foreign_declarations() {
    // [SECURITY-FFI-GATE]
    let prog = temp_osp(
        "blocked_ffi",
        "extern fn foreignCall(value: int) -> int\nlet value = foreignCall(1)\n",
    );
    let o = run_file(&prog, &["--check", "--no-ffi"]);
    assert_ne!(o.code, Some(0), "stdout={}", o.stdout);
    assert!(
        o.stderr
            .contains("security: extern function `foreignCall` is disabled by --no-ffi"),
        "{}",
        o.stderr
    );
}

#[test]
fn websocket_language_surface_links_to_runtime() {
    // [BUILTIN-WEBSOCKET] This crosses the language/codegen/archive boundary;
    // an accidental camel-case external symbol fails at link time.
    let prog = temp_osp(
        "websocket_runtime",
        "let server = websocketCreateServer(19093, \"127.0.0.1\", \"/chat\")\n\
         print(server > 0)\n",
    );
    let o = run_file(&prog, &["--run"]);
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert_eq!(o.stdout, "true\n");
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
    let prog = temp_osp("cgllvm", CODEGEN_REJECTED);
    let o = run_file(&prog, &["--llvm"]);
    assert_ne!(o.code, Some(0));
    assert!(o.stderr.contains("codegen"), "{}", o.stderr);
}

#[test]
fn run_reports_a_codegen_error() {
    let prog = temp_osp("cgrun", CODEGEN_REJECTED);
    let o = run_file(&prog, &["--run"]);
    assert_ne!(o.code, Some(0));
    assert!(o.stderr.contains("codegen"), "{}", o.stderr);
}

#[test]
fn a_generic_closure_value_is_rejected_by_the_type_gate() {
    // The two tests above assert a CODEGEN failure, so they go quiet the moment
    // their input starts being rejected earlier — which is exactly what happened
    // to this program. Pinning the checker's message here means a future move of
    // the gate fails a test that names the gate, instead of silently draining
    // the codegen arms of coverage.
    let prog = temp_osp("genval", GENERIC_AS_VALUE);
    let o = run_file(&prog, &["--llvm"]);
    assert_ne!(o.code, Some(0));
    assert!(
        o.stderr
            .contains("a closure value with a still-generic type cannot be interpolated"),
        "{}",
        o.stderr
    );
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

// --- testing framework (docs/specs/0027-TestingFramework.md) ---------------

const PASSING_TESTS: &str = "test(\"adds\", fn() => expect(1 + 1, 2))\n\
test(\"labeled\", fn() => check(\"sum\", 4, 2 + 2))\n";

const FAILING_TESTS: &str = "test(\"bad math\", fn() => expect(1 + 1, 3))\n\
test(\"good math\", fn() => expect(2 + 2, 4))\n";

/// Write `body` to `<dir>/<name>` and return the path.
fn write_in(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    let _ = std::fs::write(&path, body);
    path
}

#[cfg(unix)]
fn executable_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let script = write_in(dir, name, body);
    if let Ok(metadata) = std::fs::metadata(&script) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        let _ = std::fs::set_permissions(&script, permissions);
    }
    script
}

#[cfg(unix)]
const CONCURRENCY_PROBE_SCRIPT: &str = "#!/bin/sh\nmarker=\"$OSPREY_PARALLEL_PROBE/worker-$$\"\n\
         : > \"$marker\"\n\
         count=$(find \"$OSPREY_PARALLEL_PROBE\" -name 'worker-*' | wc -l)\n\
         if [ \"$count\" -gt 1 ]; then : > \"$OSPREY_PARALLEL_PROBE/overlap\"; fi\n\
         sleep 1\nrm -f \"$marker\"\nexit 1\n";

#[cfg(unix)]
fn parallel_probe_fixture(
    root: &Path,
    tests: &Path,
    bin: &Path,
    driver_name: &str,
) -> (PathBuf, PathBuf) {
    let probe = root.join("probe");
    for dir in [tests, bin, &probe] {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = write_in(tests, "one.test.osp", PASSING_TESTS);
    let _ = write_in(tests, "two.test.osp", PASSING_TESTS);
    let driver = executable_script(bin, driver_name, CONCURRENCY_PROBE_SCRIPT);
    (probe, driver)
}

#[cfg(unix)]
fn run_concurrency_probe(dir: &Path, driver: &Path, jobs: Option<&str>) -> Out {
    let mut cmd = osprey();
    let _ = cmd
        .args(["test", dir.to_string_lossy().as_ref(), "--quiet"])
        .env("OSPREY_CC", driver)
        .env("OSPREY_PARALLEL_PROBE", dir.join("probe"));
    if let Some(value) = jobs {
        let _ = cmd.env("OSPREY_TEST_JOBS", value);
    }
    finish(cmd)
}

#[cfg(unix)]
fn run_conformance_probe(root: &Path, jobs: Option<&str>) -> Out {
    let mut cmd = Command::new("zsh");
    let _ = cmd
        .arg(repo_root().join("crates/run_test_corpus.sh"))
        .arg("default")
        .current_dir(repo_root())
        .env("OSPREY_ROOT", root)
        .env("OSPREY_PARALLEL_PROBE", root.join("probe"));
    if let Some(value) = jobs {
        let _ = cmd.env("OSPREY_TEST_JOBS", value);
    }
    finish(cmd)
}

#[cfg(unix)]
fn assert_parallel_default_has_serial_escape(
    probe: &Path,
    mut run: impl FnMut(Option<&str>) -> Out,
    failure: &str,
) {
    let parallel = run(None);
    assert_eq!(parallel.code, Some(1), "{}", parallel.stderr);
    let overlap = probe.join("overlap");
    assert!(overlap.exists(), "{failure}");
    let _ = std::fs::remove_file(&overlap);
    let serial = run(Some("1"));
    assert_eq!(serial.code, Some(1), "{}", serial.stderr);
    assert!(!overlap.exists(), "OSPREY_TEST_JOBS=1 was not serial");
}

#[cfg(unix)]
fn run_cache_probe(dir: &Path, driver: &Path) -> Out {
    let mut cmd = osprey();
    let _ = cmd
        .args(["test", dir.to_string_lossy().as_ref(), "--quiet"])
        .env("OSPREY_CC", driver)
        .env("OSPREY_CC_COUNT", dir.join("cc-count"))
        .env("OSPREY_TEST_CACHE_DIR", dir.join("cache"));
    finish(cmd)
}

// [TESTING-BUILTIN-TEST][TESTING-BUILTIN-EXPECT][TESTING-BUILTIN-CHECK]
// [TESTING-RUNTIME][TESTING-TAP][TESTING-EXIT] a test binary reports TAP and
// exits by outcome.
#[test]
fn test_builtins_emit_tap_and_exit_status() {
    let pass = temp_osp("tap_pass", PASSING_TESTS);
    let o = run_file(&pass, &["--run"]);
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    assert!(o.stdout.contains("ok 1 - adds"), "{}", o.stdout);
    assert!(o.stdout.contains("1..2"), "{}", o.stdout);
    assert!(
        o.stdout.contains("# tests=2 passed=2 failed=0"),
        "{}",
        o.stdout
    );

    let fail = temp_osp("tap_fail", FAILING_TESTS);
    let o = run_file(&fail, &["--run"]);
    assert_eq!(o.code, Some(1), "{}", o.stdout);
    assert!(
        o.stdout.contains("# expect failed: expected 3, got 2"),
        "{}",
        o.stdout
    );
    assert!(o.stdout.contains("not ok 1 - bad math"), "{}", o.stdout);
    assert!(o.stdout.contains("ok 2 - good math"), "{}", o.stdout);
    assert!(
        o.stdout.contains("# tests=2 passed=1 failed=1"),
        "{}",
        o.stdout
    );
}

// [TESTING-FILTER] the env var exact-matches one case; others skip silently.
#[test]
fn test_filter_env_selects_one_case() {
    let prog = temp_osp("tap_filter", FAILING_TESTS);
    let mut cmd = osprey();
    let _ = cmd
        .arg(&prog)
        .arg("--run")
        .env("OSPREY_TEST_FILTER", "good math");
    let o = finish(cmd);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(o.stdout.contains("ok 1 - good math"), "{}", o.stdout);
    assert!(!o.stdout.contains("bad math"), "{}", o.stdout);
    assert!(o.stdout.contains("1..1"), "{}", o.stdout);
}

// [TESTING-LIST] static discovery lists literal names with positions and
// skips dynamic names; a testless file lists as [].
#[test]
fn list_tests_reports_literal_cases_as_json() {
    let prog = temp_osp(
        "list_tests",
        "let name = \"dyn\"\ntest(\"first\", fn() => expect(1, 1))\ntest(name, fn() => expect(1, 1))\n",
    );
    let o = run_file(&prog, &["--list-tests"]);
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    assert_eq!(
        o.stdout.trim(),
        "[{\"name\":\"first\",\"line\":2,\"column\":1}]"
    );

    let none = temp_osp("list_none", HELLO);
    let o = run_file(&none, &["--list-tests"]);
    assert_eq!(o.stdout.trim(), "[]");
}

// [TESTING-DOC] a `///` block above a case travels through `--list-tests` as
// `summary` (the Test Explorer's inline description) and `doc` (the hover
// markdown); an undocumented case emits neither key.
#[test]
fn list_tests_carries_case_documentation() {
    let prog = temp_osp(
        "list_tests_docs",
        "fn add(a, b) = a + b\n\
         /// Addition is commutative.\n\
         ///\n\
         /// # Parameters\n\
         /// - left: the first addend\n\
         ///\n\
         /// # Since\n\
         /// 0.3\n\
         test(\"commutes\", fn() => expect(add(1, 2), add(2, 1)))\n\
         test(\"bare\", fn() => expect(1, 1))\n",
    );
    let o = run_file(&prog, &["--list-tests"]);
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    let out = o.stdout.trim();
    assert!(
        out.contains("\"summary\":\"Addition is commutative.\""),
        "{out}"
    );
    assert!(out.contains("**Parameters**"), "sections render: {out}");
    assert!(
        out.contains("- `left` \\u2014 the first addend")
            || out.contains("- `left` — the first addend"),
        "{out}"
    );
    assert!(out.contains("**Since**"), "{out}");
    // The case's own line, not the doc block's first line.
    assert!(out.contains("\"name\":\"commutes\",\"line\":9"), "{out}");
    // The undocumented case keeps the bare three-key shape.
    assert!(
        out.contains("{\"name\":\"bare\",\"line\":10,\"column\":1}"),
        "{out}"
    );
    // Whatever the content, the whole array must be a single JSON line the
    // extension can JSON.parse.
    assert!(!out.trim_end().contains('\n'), "one line of JSON: {out}");

    // The ML twin lowers the `(** … *)` form to the same wire shape.
    let ml_dir = temp_dir("list_tests_docs_ml");
    let ml = write_in(
        &ml_dir,
        "docs.test.ospml",
        "add a b = a + b\n\
         (** Addition is commutative. *)\n\
         test \"commutes\" (\\() => check \"c\" (add 1 2) (add 2 1))\n",
    );
    let o = run_file(&ml, &["--list-tests"]);
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    assert!(
        o.stdout
            .contains("\"summary\":\"Addition is commutative.\""),
        "{}",
        o.stdout
    );
}

// [TESTING-CLI-RUN][TESTING-FILE-CONVENTION] the runner discovers
// *.test.osp{,ml} under a directory, streams TAP under headers, and aggregates
// the exit status.
/// [TESTING-SKIP-WARNING] a skipped case is never silent: `osprey test` echoes
/// every TAP `# SKIP` directive as a stderr warning naming the case and reason.
#[test]
fn test_subcommand_warns_on_every_skipped_case() {
    let dir = temp_dir("skip-warns");
    let _ = write_in(
        &dir,
        "skips.test.osp",
        "type Verdict = Pass | Fail(string) | Skip(string)\n\
         test(\"parked case\", fn() => Skip(\"blocked on #123\"))\n\
         test(\"live case\", fn() => expect(1, 1))\n",
    );
    let o = run_args(&["test", dir.to_string_lossy().as_ref()]);
    assert_eq!(o.code, Some(0), "{}\n{}", o.stdout, o.stderr);
    assert!(o.stdout.contains("# SKIP blocked on #123"), "{}", o.stdout);
    assert!(
        o.stderr
            .contains("warning: test 'parked case' skipped: blocked on #123"),
        "{}",
        o.stderr
    );
    assert!(!o.stderr.contains("live case"), "{}", o.stderr);
}

/// [TESTING-VERDICT] `test` reports the states the program's OWN `Verdict`
/// declares. A payload-free `Skip`/`Fail` reports with no reason rather than
/// failing codegen on a binder the declaration never named, and a reasonless
/// skip still prints a bare `# SKIP` directive that the diagnostic path sees.
///
/// [TESTING-SKIP-REASON] and that reasonless skip is an ERROR: the TAP is
/// unchanged — the case still reports skipped, not failed — but the runner
/// fails the suite, because a hole in coverage nobody wrote a cause for is a
/// defect rather than a debt someone chose.
#[test]
fn a_verdict_declaring_payload_free_states_reports_each_of_them() {
    let dir = temp_dir("verdict-bare");
    let _ = write_in(
        &dir,
        "bare.test.osp",
        "type Verdict = Pass | Fail | Skip\n\
         test(\"parked\", fn() => Skip)\n\
         test(\"green\", fn() => Pass)\n",
    );
    let o = run_args(&["test", dir.to_string_lossy().as_ref()]);
    assert_eq!(o.code, Some(1), "{}\n{}", o.stdout, o.stderr);
    assert!(o.stdout.contains("ok 1 - parked # SKIP\n"), "{}", o.stdout);
    assert!(o.stdout.contains("ok 2 - green\n"), "{}", o.stdout);
    assert!(
        o.stdout.contains("# tests=2 passed=1 failed=0 skipped=1"),
        "{}",
        o.stdout
    );
    assert!(
        o.stderr
            .contains("error: test 'parked' skipped with no reason; every skip must name one"),
        "{}",
        o.stderr
    );
    assert!(
        !o.stderr.contains("warning: test 'parked'"),
        "an unexplained skip is an error, never also a warning: {}",
        o.stderr
    );
}

/// [TESTING-SKIP-REASON] a skip that NAMES a reason stays a warning and keeps
/// the suite green — the two strengths are decided by the reason alone, not by
/// whether the `Verdict` declaration gives `Skip` a payload at all.
#[test]
fn a_reasoned_skip_warns_while_an_unexplained_one_fails_the_suite() {
    let reasoned = temp_dir("skip-reasoned");
    let _ = write_in(
        &reasoned,
        "ok.test.osp",
        "type Verdict = Pass | Fail(string) | Skip(string)\n\
         test(\"parked\", fn() => Skip(\"blocked on #123\"))\n",
    );
    let green = run_args(&["test", reasoned.to_string_lossy().as_ref()]);
    assert_eq!(green.code, Some(0), "{}\n{}", green.stdout, green.stderr);
    assert!(
        green
            .stderr
            .contains("warning: test 'parked' skipped: blocked on #123"),
        "{}",
        green.stderr
    );

    // The SAME declaration with an empty reason is the error case: the author
    // had somewhere to write the cause and left it blank.
    let blank = temp_dir("skip-blank");
    let _ = write_in(
        &blank,
        "blank.test.osp",
        "type Verdict = Pass | Fail(string) | Skip(string)\n\
         test(\"parked\", fn() => Skip(\"\"))\n",
    );
    let red = run_args(&["test", blank.to_string_lossy().as_ref()]);
    assert_eq!(red.code, Some(1), "{}\n{}", red.stdout, red.stderr);
    assert!(
        red.stderr
            .contains("error: test 'parked' skipped with no reason"),
        "{}",
        red.stderr
    );
    assert!(
        red.stdout.contains("# suites: 0 passed, 1 failed"),
        "{}",
        red.stdout
    );
}

/// [TESTING-VERDICT] the payload is read per state, not per union: a `Verdict`
/// that gives `Fail` a reason and `Skip` none reports both correctly.
#[test]
fn a_verdict_may_give_one_state_a_reason_and_another_none() {
    let dir = temp_dir("verdict-mixed");
    let _ = write_in(
        &dir,
        "mixed.test.osp",
        "type Verdict = Pass | Fail(string) | Skip\n\
         test(\"parked\", fn() => Skip)\n\
         test(\"broken\", fn() => Fail(\"expected 3\"))\n",
    );
    let o = run_args(&["test", dir.to_string_lossy().as_ref()]);
    assert_eq!(o.code, Some(1), "{}\n{}", o.stdout, o.stderr);
    assert!(o.stdout.contains("ok 1 - parked # SKIP\n"), "{}", o.stdout);
    assert!(o.stdout.contains("# fail: expected 3"), "{}", o.stdout);
    assert!(o.stdout.contains("not ok 2 - broken"), "{}", o.stdout);
}

/// [TESTING-VERDICT] a `Verdict` state `test` has no report primitive for is
/// rejected by name, not by a binder error from a pattern nobody wrote.
#[test]
fn a_verdict_state_test_cannot_report_is_rejected_by_name() {
    let prog = temp_osp(
        "verdict_extra_state",
        "type Verdict = Pass | Fail(string) | Skip(string) | Todo(string)\n\
         test(\"case\", fn() => Pass)\n",
    );
    let o = run_file(&prog, &["--run"]);
    assert_ne!(o.code, Some(0), "{}", o.stdout);
    assert!(o.stderr.contains("Todo"), "{}", o.stderr);
    assert!(o.stderr.contains("Verdict"), "{}", o.stderr);
}

#[test]
fn test_subcommand_runs_directories_and_files() {
    let dir = temp_dir("suite");
    let _ = write_in(&dir, "pass.test.osp", PASSING_TESTS);
    let _ = write_in(
        &dir,
        "fail.test.ospml",
        "test \"ml bad\" (\\() => check \"v\" 3 (1 + 1))\n",
    );
    let _ = write_in(&dir, "ignored.osp", HELLO);

    let o = run_args(&["test", dir.to_string_lossy().as_ref()]);
    assert_eq!(o.code, Some(1), "{}", o.stdout);
    assert!(o.stdout.contains("# file:"), "{}", o.stdout);
    assert!(o.stdout.contains("ok 1 - adds"), "{}", o.stdout);
    assert!(
        o.stdout.contains("# check 'v' failed: expected 3, got 2"),
        "{}",
        o.stdout
    );
    assert!(
        o.stdout.contains("# suites: 1 passed, 1 failed"),
        "{}",
        o.stdout
    );
    assert!(
        !o.stdout.contains("v=hi"),
        "plain .osp files must be ignored"
    );

    // A single file runs regardless of naming; --filter reaches the children.
    let single = write_in(&dir, "single.osp", FAILING_TESTS);
    let o = run_args(&[
        "test",
        single.to_string_lossy().as_ref(),
        "--filter",
        "good math",
        "--quiet",
    ]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(
        o.stdout.contains("# suites: 1 passed, 0 failed"),
        "{}",
        o.stdout
    );
    assert!(!o.stdout.contains("# file:"), "--quiet drops headers");
}

// [TESTING-PARALLEL] Independent suites compile concurrently by default, while
// configuration can force serial execution for constrained or diagnostic runs.
#[cfg(unix)]
#[test]
fn test_subcommand_parallel_default_has_serial_escape_hatch() {
    let dir = temp_dir("suite_parallel");
    let (probe, driver) = parallel_probe_fixture(&dir, &dir, &dir, "probe-cc.sh");
    assert_parallel_default_has_serial_escape(
        &probe,
        |jobs| run_concurrency_probe(&dir, &driver, jobs),
        "test suites compiled serially by default",
    );
}

#[cfg(unix)]
#[test]
fn conformance_corpus_parallel_default_has_serial_escape_hatch() {
    let root = temp_dir("conformance_parallel");
    let tests = root.join("tests");
    let bin = root.join("target/release");
    let (probe, _) = parallel_probe_fixture(&root, &tests, &bin, "osprey");
    assert_parallel_default_has_serial_escape(
        &probe,
        |jobs| run_conformance_probe(&root, jobs),
        "conformance suites ran serially by default",
    );
}

#[cfg(unix)]
#[test]
fn test_subcommand_reuses_unchanged_compiled_suites() {
    // [TESTING-NATIVE-CACHE] the second identical run never invokes clang.
    let dir = temp_dir("suite_cache");
    let _ = write_in(&dir, "cached.test.osp", PASSING_TESTS);
    let _ = write_in(
        &dir,
        "cached_http.test.osp",
        "let client = httpCreateClient(\"http://127.0.0.1:1\", 1)\n\
         let closed = httpCloseClient(client)\n\
         test(\"http client\", fn() => expect(closed, 0))\n",
    );
    let linked = format!("// @link: m\n{PASSING_TESTS}");
    let _ = write_in(&dir, "cached_link.test.osp", &linked);
    let custom_search = format!("// @linkdir: {}\n{PASSING_TESTS}", dir.display());
    let _ = write_in(&dir, "uncached_linkdir.test.osp", &custom_search);
    let driver = executable_script(
        &dir,
        "counting-cc.sh",
        "#!/bin/sh\nprintf x >> \"$OSPREY_CC_COUNT\"\nexec clang \"$@\"\n",
    );

    let first = run_cache_probe(&dir, &driver);
    assert_eq!(first.code, Some(0), "{}", first.stderr);
    let second = run_cache_probe(&dir, &driver);
    assert_eq!(second.code, Some(0), "{}", second.stderr);
    assert_eq!(read_text(&dir.join("cc-count")), "xxxxx");
}

#[cfg(unix)]
#[test]
fn run_removes_temporary_native_artifacts() {
    let dir = temp_dir("native_cleanup");
    let scratch = dir.join("scratch");
    let _ = std::fs::create_dir_all(&scratch);
    let suite = write_in(&dir, "cleanup.test.osp", PASSING_TESTS);
    let mut cmd = osprey();
    let _ = cmd
        .args([suite.to_string_lossy().as_ref(), "--run", "--quiet"])
        .env("TMPDIR", &scratch)
        .env_remove("OSPREY_TEST_CACHE_DIR");
    let output = finish(cmd);
    assert_eq!(output.code, Some(0), "{}", output.stderr);
    let leaked = std::fs::read_dir(&scratch)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert!(leaked.is_empty(), "temporary artifacts leaked: {leaked:?}");
}

// A compile-error suite fails the run; an empty discovery set is loud.
#[test]
fn test_subcommand_reports_broken_and_missing_suites() {
    let dir = temp_dir("suite_bad");
    let _ = write_in(&dir, "broken.test.osp", "test(\"x\", fn() => expect(1)\n");
    let o = run_args(&["test", dir.to_string_lossy().as_ref()]);
    assert_eq!(o.code, Some(1), "{}", o.stdout);
    assert!(
        o.stdout.contains("# suites: 0 passed, 1 failed"),
        "{}",
        o.stdout
    );

    let empty = temp_dir("suite_empty");
    let o = run_args(&["test", empty.to_string_lossy().as_ref()]);
    assert_eq!(o.code, Some(1), "{}", o.stderr);
    assert!(o.stderr.contains("no test files found"), "{}", o.stderr);

    let o = run_args(&["test", "--filter"]);
    assert_eq!(o.code, Some(2), "{}", o.stderr);
    let o = run_args(&["test", "--nonsense"]);
    assert_eq!(o.code, Some(2), "{}", o.stderr);
    let o = run_args(&["test", "a", "b"]);
    assert_eq!(o.code, Some(2), "{}", o.stderr);
}

// [TESTING-COVERAGE-CLI][TESTING-COVERAGE-JSON] --coverage instruments each
// suite and prints per-file and total line rates; --coverage-json also writes
// the merged machine-readable report the editor integration consumes.
#[test]
fn test_subcommand_coverage_reports_lines_and_writes_json() {
    let dir = temp_dir("suite_cov");
    let _ = write_in(&dir, "pass.test.osp", PASSING_TESTS);
    let json = dir.join("cov.json");
    let o = run_args(&[
        "test",
        dir.to_string_lossy().as_ref(),
        "--coverage-json",
        json.to_string_lossy().as_ref(),
    ]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(o.stdout.contains("# coverage: "), "{}", o.stdout);
    assert!(o.stdout.contains("# coverage total: "), "{}", o.stdout);
    let report = std::fs::read_to_string(&json).expect("coverage json");
    assert!(report.starts_with("{\"files\":{"), "{report}");
    assert!(report.contains("pass.test.osp"), "{report}");
    assert!(report.contains("\"lines\":{"), "{report}");

    // --quiet drops the per-file rows but keeps the aggregate.
    let o = run_args(&[
        "test",
        dir.to_string_lossy().as_ref(),
        "--coverage",
        "--quiet",
    ]);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(!o.stdout.contains("# coverage: "), "{}", o.stdout);
    assert!(o.stdout.contains("# coverage total: "), "{}", o.stdout);
}

// A type-error suite fails before execution, so it produces no dump; the run
// keeps going, and an unwritable JSON path is reported without aborting.
#[test]
fn test_subcommand_coverage_survives_type_errors_and_bad_json_paths() {
    let dir = temp_dir("suite_cov_bad");
    let _ = write_in(&dir, "pass.test.osp", PASSING_TESTS);
    let _ = write_in(&dir, "typebad.test.osp", "let n: int = \"nope\"\n");
    let bogus = dir.join("no-such-dir").join("cov.json");
    let o = run_args(&[
        "test",
        dir.to_string_lossy().as_ref(),
        "--coverage-json",
        bogus.to_string_lossy().as_ref(),
        "--quiet",
    ]);
    assert_eq!(o.code, Some(1), "{}", o.stdout);
    assert!(
        o.stdout.contains("# suites: 1 passed, 1 failed"),
        "{}",
        o.stdout
    );
    assert!(o.stderr.contains("type mismatch"), "{}", o.stderr);
    assert!(o.stderr.contains("no coverage dump"), "{}", o.stderr);
    assert!(
        o.stderr.contains("cannot write coverage json"),
        "{}",
        o.stderr
    );
}

// A nested test() must not silently reshuffle counters: it fails the
// enclosing case loudly [TESTING-BUILTIN-TEST], and a zero-case run still
// prints its plan so a matchless filter is visible [TESTING-TAP].
#[test]
fn nested_tests_fail_loudly_and_zero_case_runs_keep_the_plan() {
    let nested = temp_osp(
        "tap_nested",
        "test(\"outer\", fn() => {\n    expect(1, 2)\n    test(\"inner\", fn() => expect(3, 3))\n})\n",
    );
    let o = run_file(&nested, &["--run"]);
    assert_eq!(o.code, Some(1), "{}", o.stdout);
    assert!(
        o.stdout.contains("# nested test 'inner' skipped"),
        "{}",
        o.stdout
    );
    assert!(o.stdout.contains("not ok 1 - outer"), "{}", o.stdout);

    let prog = temp_osp("tap_nomatch", PASSING_TESTS);
    let mut cmd = osprey();
    let _ = cmd
        .arg(&prog)
        .arg("--run")
        .env("OSPREY_TEST_FILTER", "matches nothing");
    let o = finish(cmd);
    assert_eq!(o.code, Some(0), "{}", o.stdout);
    assert!(o.stdout.contains("1..0"), "{}", o.stdout);
    assert!(
        o.stdout.contains("# tests=0 passed=0 failed=0"),
        "{}",
        o.stdout
    );
}

// [TESTING-EQUALITY] an Error operand is a visible mismatch, never a blind
// payload read.
#[test]
fn error_result_assertions_render_the_error() {
    let prog = temp_osp(
        "tap_err_result",
        "test(\"div\", fn() => expect(intDiv(1, 0), 2))\n",
    );
    let o = run_file(&prog, &["--run"]);
    assert_eq!(o.code, Some(1), "{}", o.stdout);
    assert!(
        o.stdout
            .contains("# expect failed: expected 2, got Error(division by zero)"),
        "{}",
        o.stdout
    );
    assert!(o.stdout.contains("not ok 1 - div"), "{}", o.stdout);
}

/// The whole call-site type-application pipeline — parse, check, lower, emit,
/// link, run — for the shape that has no other spelling: a binder appearing in
/// no parameter position. [TYPE-GENERICS-APPLY]
#[test]
fn a_written_type_argument_pins_an_instantiation_end_to_end() {
    let prog = temp_osp(
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
    );
    let o = run_file(&prog, &["--run"]);
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
    let prog = temp_osp(
        "turbofish_arity",
        "fn identity<T>(x: T) -> T = x\n\
         print(\"${identity<int, string>(5)}\")\n",
    );
    let o = run_file(&prog, &["--check"]);
    assert_ne!(o.code, Some(0), "stdout={}", o.stdout);
    assert!(
        o.stderr
            .contains("function `identity` takes 1 type argument(s), got 2"),
        "stderr={}",
        o.stderr
    );
}

/// A written argument contradicting the value argument is a type error, not a
/// silently ignored annotation. [TYPE-GENERICS-APPLY]
#[test]
fn a_contradicting_written_type_argument_is_rejected_by_the_cli() {
    let prog = temp_osp(
        "turbofish_contradiction",
        "fn identity<T>(x: T) -> T = x\n\
         print(\"${identity<int>(\\\"text\\\")}\")\n",
    );
    let o = run_file(&prog, &["--check"]);
    assert_ne!(o.code, Some(0), "stdout={}", o.stdout);
    assert!(
        o.stderr.contains("cannot unify int with string"),
        "stderr={}",
        o.stderr
    );
}
