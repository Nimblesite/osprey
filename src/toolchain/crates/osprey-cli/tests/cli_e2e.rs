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

#[cfg(unix)]
#[path = "cases/doctest_harness.rs"]
mod doctest_harness;

#[path = "cases/memory_backend.rs"]
mod memory_backend;

#[path = "cases/suite_scratch.rs"]
mod suite_scratch;

/// Repo root: `src/toolchain/crates/osprey-cli` -> `../../../..`. The C runtime archives `osprey`
/// links at `--run`/`--compile` time live under `src/toolchain/compiler/bin/` there, and
/// `find_runtime_lib` resolves them relative to the process cwd — so every
/// child runs from here.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
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

/// Best-effort `chmod` on a test fixture: a missing file or a platform that
/// refuses is not this helper's failure to report.
#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;

    if let Ok(metadata) = std::fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(mode);
        let _ = std::fs::set_permissions(path, permissions);
    }
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

#[path = "cases/api_docs_extra.rs"]
mod api_docs_extra;

#[path = "cases/api_docs_packages.rs"]
mod api_docs_packages;

/// The captured result of one invocation.
struct Out {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Out {
    /// The run exited with `status`, printed every string in `expected` on
    /// stdout and none of the strings in `forbidden`.
    fn expect_stdout(&self, status: i32, expected: &[&str], forbidden: &[&str]) {
        self.expect(status, &self.stdout, "stdout", expected, forbidden);
    }

    /// The run exited with `status`, printed every string in `expected` on
    /// stderr and none of the strings in `forbidden`.
    fn expect_stderr(&self, status: i32, expected: &[&str], forbidden: &[&str]) {
        self.expect(status, &self.stderr, "stderr", expected, forbidden);
    }

    /// The run was REJECTED — a non-zero exit, whichever code the gate chose —
    /// and `stderr` carries a diagnostic. `expected` are the strings that
    /// diagnostic must name; a rejection with no message would pass a
    /// code-only check, which is the silent failure this guards.
    fn expect_rejected(&self, expected: &[&str]) {
        assert_ne!(self.code, Some(0), "stdout={}", self.stdout);
        assert!(!self.stderr.is_empty(), "stdout={}", self.stdout);
        for wanted in expected {
            assert!(
                self.stderr.contains(wanted),
                "missing {wanted:?} in stderr={}",
                self.stderr
            );
        }
    }

    /// The exit status is `status` and `text` — the `stream` a failure names —
    /// carries every `expected` string and none of the `forbidden` ones.
    fn expect(&self, status: i32, text: &str, stream: &str, expected: &[&str], forbidden: &[&str]) {
        assert_eq!(
            self.code,
            Some(status),
            "stdout={} stderr={}",
            self.stdout,
            self.stderr
        );
        for wanted in expected {
            assert!(
                text.contains(wanted),
                "missing {wanted:?} in {stream}={text}"
            );
        }
        for unwanted in forbidden {
            assert!(
                !text.contains(unwanted),
                "unexpected {unwanted:?} in {stream}={text}"
            );
        }
    }
}

fn finish(cmd: &mut Command) -> Out {
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
    finish(&mut cmd)
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

/// [BUILTIN-STRING-JOIN] over a RUNTIME string list. `join` works on a list
/// literal and on an unbound `words`/`lines` result (both flat) and SEGFAULTS
/// (exit 139, no output) on every runtime list: one built by `listAppend`,
/// one returned by `toList`, and a `words` result bound to a name — which the
/// binding now settles into the runtime layout [BUILTIN-LIST-GET], so
/// `let ws = words(s)` then `join(ws, …)` crashes where it used to work.
/// This pins the crash; the fix belongs to `osp_string_join`'s lowering.
#[test]
fn join_accepts_every_string_list_representation() {
    let out = run_source(
        "join_runtime_lists",
        "fn main() = {\n\
             let appended = listAppend(listAppend(List(), \"a\"), \"b\")\n\
             let collected = range(0, 2) |> map(fn(i) => \"c${i}\") |> toList()\n\
             let bound = words(\"x y\")\n\
             print(\"${join(appended, \"-\")} ${join(collected, \"-\")} ${join(bound, \"-\")}\")\n\
         }\n",
        &["--run", "--quiet"],
    );
    assert_eq!(out.code, Some(0), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "a-b c0-c1 x-y\n");
}

/// Run against a source `path` plus extra flags — the common compiling shape.
/// Write `body` to the temp program `name` and run `osprey` on it with `extra`.
fn run_source(name: &str, body: &str, extra: &[&str]) -> Out {
    run_file(&temp_osp(name, body), extra)
}

fn run_file(path: &Path, extra: &[&str]) -> Out {
    let mut cmd = osprey();
    let _ = cmd.arg(path).args(extra);
    finish(&mut cmd)
}

/// Run a single `mode` against `path` with `OSPREY_CC` overridden — used to
/// drive the C-driver failure branches of `build_executable`.
fn run_file_cc(path: &Path, mode: &str, cc: &str) -> Out {
    let mut cmd = osprey();
    let _ = cmd.arg(path).arg(mode).env("OSPREY_CC", cc);
    finish(&mut cmd)
}

/// Type-clean but codegen-rejected: an FFI callback slot is a raw C code
/// pointer, so a capturing lambda has nowhere to carry its environment
/// ([FFI-CALLBACKS]). It passes the type gate, so every compiling mode reaches
/// codegen and fails there — exercising the `Err` arms `compile_program` feeds.
///
/// `satAdd` is total ([ARITH-EFFECT-TOTAL-HELPERS]), so the callback performs
/// nothing and the capture of `base` is the only defect. A plain `x + base`
/// would perform `Arith.overflow` on the C side of the boundary, where no
/// handler can discharge it, so the fixture must not depend on that verdict.
const CODEGEN_REJECTED: &str = concat!(
    "extern fn registerCallback(cb: fn(int) -> int) -> int\n",
    "let base = 10\n",
    "let r = registerCallback(fn(x) => satAdd(x, base))\n",
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
    control step: fn(string) -> int
}

fn pipeline() -> int !Audit = {
    let a = perform Audit.step("load")
    let b = perform Audit.step("parse")
    satAdd(a, b)
}

fn main() = {
    mut n = 0
    let total = {
        handle Audit {
            step label => {
                n = satAdd(n, 1)
                let answer = resume(n)
                print("after " + label + ": answer=" + toString(answer))
                answer
            }
        }
        pipeline()
    }
    print("total=" + toString(total))
}
"#;

/// A handler arm that resumes its continuation TWICE on ONE control path.
/// `Choose.pick` carries no multiplicity keyword, so it is `once`, and the arm
/// is rejected before it can run ([MULTI-HANDLE-ONCE], [MULTI-COMPAT]).
///
/// This lived as `src/osprey/examples/failscompilation/multishot_resume_rejected.ospo`,
/// which was a category error while the rejection was a RUNTIME one: the
/// program was well formed, so a must-reject fixture could never observe the
/// abort. It "passed" only because of an unrelated arithmetic type error,
/// which the corpus recorded as the expected rejection. Multiplicity moves the
/// verdict back to compile time, where the arm has always been visible; the
/// total `satAdd` keeps multiplicity the program's only defect.
const MULTISHOT_RESUME: &str = r#"
effect Choose {
    control pick: fn() -> int
}

fn both() -> int !Choose = {
    let x = perform Choose.pick()
    satAdd(x, 1)
}

fn main() = {
    let total = {
        handle Choose {
            pick => {
                let a = resume(10)
                let b = resume(20)
                satAdd(a, b)
            }
        }
        both()
    }
    print("total=" + toString(total))
}
"#;

/// The same effect answered by an arm with one `resume` per `match` branch.
/// Two `resume` SITES, at most one per control path — the shape
/// [MULTI-HANDLE-ONCE] must keep accepting.
const BRANCHWISE_RESUME: &str = r#"
effect Choose {
    control pick: fn() -> int
}

fn both() -> int !Choose = {
    let x = perform Choose.pick()
    satAdd(x, 1)
}

fn main() = {
    let total = {
        handle Choose {
            pick => match true {
                true => resume(29)
                false => resume(0)
            }
        }
        both()
    }
    print("total=" + toString(total))
}
"#;

#[test]
fn version_plain_and_json() {
    // [SWR-VERSION-BUILD-STAMPING] [SWR-VERSION-CLI-OUTPUT] The first line is
    // exactly `name version`, as a version probe parses it; the commit the
    // binary was built from follows, so a stale build is visible.
    let commit = env!("OSPREY_COMMIT");
    assert!(commit.len() >= 7, "{commit}");
    let plain = run_args(&["--version"]);
    assert_eq!(plain.code, Some(0));
    assert_eq!(plain.stdout, format!("osprey 0.0.0-dev\ncommit {commit}\n"));

    let json = run_args(&["--version", "--json"]);
    assert_eq!(json.code, Some(0));
    assert_eq!(
        json.stdout,
        format!(
            "{{\"manifestVersion\":1,\"name\":\"osprey\",\"version\":\"0.0.0-dev\",\"kind\":\"cli\",\"product\":\"osprey\",\"commit\":\"{commit}\"}}\n"
        )
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
    known.expect_stdout(0, &["print"], &[]);
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
    let out = run_source(
        "deps_unparsed",
        "static effect Signal<T> { read: fn( -> T }\n",
        &["--deps"],
    );
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

    let invalid = run_source(
        "deps_unknown_effect",
        "static effect Signal { read: fn() -> int }\nfn value() = perform Missing.read()\n",
        &["--deps"],
    );
    invalid.expect_stderr(1, &["unknown effect `Missing`"], &[]);
    assert!(
        invalid.stdout.trim().is_empty(),
        "stdout={}",
        invalid.stdout
    );

    // The positive control: the same shape, parsing, still reports and exits 0.
    let ok = run_source(
        "deps_parsed",
        "type Count = { value: int }\nstatic effect Signal<T> { read: fn() -> T }\n         fn counterLabel() = \"count: ${(perform Signal<Count>.read()).value}\"\n",
        &["--deps"],
    );
    ok.expect_stdout(0, &["counterLabel: Signal<Count>.read"], &[]);

    let open = run_source(
        "deps_open_callback",
        "static effect Signal { read: fn() -> int }\nfn invoke(callback) = callback()\nfn reading() = perform Signal.read()\nfn widget() = invoke(reading)\n",
        &["--deps"],
    );
    open.expect_stdout(0, &["invoke: <unknown>", "widget: Signal.read"], &[]);
}

#[path = "cases/formatting.rs"]
mod formatting;

#[path = "cases/signature_contracts.rs"]
mod signature_contracts;

#[path = "cases/compile_run.rs"]
mod compile_run;

#[path = "cases/test_runner.rs"]
mod test_runner;

#[path = "cases/test_suites.rs"]
mod test_suites;

#[path = "cases/program_contracts.rs"]
mod program_contracts;
