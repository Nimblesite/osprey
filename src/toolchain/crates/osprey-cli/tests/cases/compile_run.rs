//! Check, compile and run through the public CLI: exit codes, diagnostics, sandbox gates and C compiler failures.

use super::{
    repo_root, run_args, run_file, run_file_cc, run_source, temp_osp, BRANCHWISE_RESUME,
    CODEGEN_REJECTED, GENERIC_AS_VALUE, HELLO, MULTISHOT_RESUME, RESUME_EFFECT,
};

#[test]
fn unknown_flag_exits_two_with_usage() {
    let o = run_source("flag", HELLO, &["--bogus"]);
    o.expect_stderr(2, &["unknown flag --bogus"], &[]);
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
    o.expect_stderr(2, &["cannot read"], &[]);
}

#[test]
fn check_parse_error_is_reported() {
    let o = run_source("parse", "fn = = =\n", &["--check"]);
    assert_ne!(o.code, Some(0));
    assert_ne!(o.stderr, "");
}

#[test]
fn flavor_marker_conflict_exits_two() {
    let o = run_source(
        "flavor_conflict",
        "// osprey: flavor=ml\nprint 1\n",
        &["--check"],
    );
    o.expect_stderr(2, &["flavor marker"], &[]);
}

#[test]
fn check_type_error_is_reported() {
    let o = run_source("typed", "let y = 1 + \"oops\" - true\n", &["--check"]);
    o.expect_rejected(&[]);
}

#[test]
fn llvm_emits_ir_and_rejects_ill_typed() {
    let good = run_source("llok", HELLO, &["--llvm"]);
    good.expect_stdout(0, &["define"], &[]);
    let rejected = run_source("llbad", "let y = 1 + true\n", &["--llvm"]);
    assert_ne!(rejected.code, Some(0));
}

#[test]
fn ast_and_symbols_modes() {
    let prog = temp_osp("astsym", HELLO);
    let ast = run_file(&prog, &["--ast"]);
    assert_eq!(ast.code, Some(0));
    assert_ne!(ast.stdout, "");
    let sym = run_file(&prog, &["--symbols"]);
    sym.expect_stdout(0, &["\"name\""], &[]);
}

#[test]
fn run_compiles_links_and_executes() {
    let o = run_source("run", HELLO, &["--run"]);
    o.expect_stdout(0, &["v=hi"], &[]);
}

#[test]
fn explicit_resume_runs_the_performer_continuation() {
    // [EFFECTS-RESUME]
    let o = run_source("resume_effect", RESUME_EFFECT, &["--run"]);
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
    // The runtime guard in `src/toolchain/compiler/runtime/effects_coro.c` stays as a
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
    // program `src/osprey/tests/regressions/effects/abort_vs_resume.test.osp` is the same
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
    for (name, call) in [
        ("readFile", "readFile(\"x.txt\")"),
        ("listDir", "listDir(\".\")"),
        ("kindOf", "kindOf(\"/\")"),
        ("makeDir", "makeDir(\"made\")"),
    ] {
        let o = run_source(name, &format!("let c = {call}\n"), &["--llvm", "--no-fs"]);
        o.expect_rejected(&[&format!("security: `{name}` is disabled by --no-fs")]);
    }
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
        o.expect_rejected(&[expected]);
        assert!(!o.stderr.contains("expects exactly"), "{}", o.stderr);
    }
}

#[test]
fn no_ffi_rejects_foreign_declarations() {
    // [SECURITY-FFI-GATE]
    let o = run_source(
        "blocked_ffi",
        "extern fn foreignCall(value: int) -> int\nlet value = foreignCall(1)\n",
        &["--check", "--no-ffi"],
    );
    o.expect_rejected(&["security: extern function `foreignCall` is disabled by --no-ffi"]);
}

#[test]
fn websocket_language_surface_links_to_runtime() {
    // [BUILTIN-WEBSOCKET] This crosses the language/codegen/archive boundary;
    // an accidental camel-case external symbol fails at link time.
    let o = run_source(
        "websocket_runtime",
        "let server = websocketCreateServer(19093, \"127.0.0.1\", \"/chat\")\n\
         print(server > 0)\n",
        &["--run"],
    );
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert_eq!(o.stdout, "true\n");
}

#[test]
fn quiet_suppresses_the_ok_line() {
    let o = run_source("quiet", HELLO, &["--check", "--quiet"]);
    assert_eq!(o.code, Some(0), "stderr={}", o.stderr);
    assert!(o.stdout.trim().is_empty(), "{}", o.stdout);
}

#[test]
fn llvm_reports_a_codegen_error() {
    let o = run_source("cgllvm", CODEGEN_REJECTED, &["--llvm"]);
    o.expect_rejected(&["codegen"]);
}

#[test]
fn run_reports_a_codegen_error() {
    let o = run_source("cgrun", CODEGEN_REJECTED, &["--run"]);
    o.expect_rejected(&["codegen"]);
}

#[test]
fn a_generic_closure_value_is_rejected_by_the_type_gate() {
    // The two tests above assert a CODEGEN failure, so they go quiet the moment
    // their input starts being rejected earlier — which is exactly what happened
    // to this program. Pinning the checker's message here means a future move of
    // the gate fails a test that names the gate, instead of silently draining
    // the codegen arms of coverage.
    let o = run_source("genval", GENERIC_AS_VALUE, &["--llvm"]);
    o.expect_rejected(&["a closure value with a still-generic type cannot be interpolated"]);
}

#[test]
fn compile_reports_a_failing_c_compiler() {
    // `false` runs and exits non-zero -> the "cc failed to compile" branch.
    let prog = temp_osp("ccfail", HELLO);
    let o = run_file_cc(&prog, "--compile", "false");
    let _ = std::fs::remove_file(repo_root().join("osprey_cli_e2e_ccfail"));
    o.expect_rejected(&[]);
}

#[test]
fn run_reports_an_uninvokable_c_compiler() {
    // A missing driver can't be spawned at all -> the "could not invoke" branch.
    let prog = temp_osp("ccmiss", HELLO);
    let o = run_file_cc(&prog, "--run", "osprey_no_such_cc_zzz");
    o.expect_rejected(&[]);
}

/// A package whose entry defines `argument`, `open` and `event` builds for wasm32 as the shared
/// native boundary: one module exporting the three C entry points plus a WASI entry that runs the
/// top level once. [NATIVE-SHM-WASM]
#[test]
fn a_native_boundary_package_builds_one_wasm_module_exporting_the_boundary() {
    let out = std::env::temp_dir().join(format!("osprey_e2e_boundary_{}.wasm", std::process::id()));
    let editor = repo_root().join("target/downstream/osprey-packages/src/manager/editor");
    let mut command = super::osprey();
    let _ = command
        .arg("build")
        .arg(&editor)
        .args(["--target=wasm32", "--quiet", "--no-warnings", "-o"])
        .arg(&out);
    let built = super::finish(&mut command);
    assert_eq!(built.code, Some(0), "stderr={}", built.stderr);
    let module = std::fs::read(&out).unwrap_or_default();
    let _ = std::fs::remove_file(&out);
    assert_eq!(module.get(..4), Some(&b"\0asm"[..]), "not a wasm module");
    for export in [
        "osprey_app_argument",
        "osprey_app_open",
        "osprey_app_event",
        "__main_void",
    ] {
        assert!(
            module
                .windows(export.len())
                .any(|bytes| bytes == export.as_bytes()),
            "the module does not name {export}"
        );
    }
}

/// A Python library may not export a name the bridge itself defines, and must be written with
/// the host's shared-library extension. [PYTHON-HOST-ABI]
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn python_libraries_refuse_reserved_bridge_names_and_foreign_extensions() {
    let extension = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let foreign = if cfg!(target_os = "macos") {
        "so"
    } else {
        "dylib"
    };
    for reserved in [
        "osprey_python_call",
        "osprey_python_object",
        "osprey_python_install",
        "osprey_python_abi",
    ] {
        let out = std::env::temp_dir().join(format!("osprey_e2e_{reserved}.{extension}"));
        let result = run_source(
            &format!("python_{reserved}"),
            &format!("fn {reserved}(n) = wrapAdd(n, 1)\n"),
            &["--compile", "--target=python", "-o", &out.to_string_lossy()],
        );
        result.expect_stderr(
            1,
            &[&format!("Python bridge symbol `{reserved}` is reserved")],
            &[],
        );
        assert!(
            !out.exists(),
            "a refused library was written: {}",
            out.display()
        );
    }
    let wrong = std::env::temp_dir().join(format!("osprey_e2e_python_wrong.{foreign}"));
    let result = run_source(
        "python_wrong_extension",
        "fn twice(n) = wrapMul(n, 2)\n",
        &[
            "--compile",
            "--target=python",
            "-o",
            &wrong.to_string_lossy(),
        ],
    );
    result.expect_stderr(
        1,
        &[&format!("Python output must end in .{extension}")],
        &[],
    );
    assert!(
        !wrong.exists(),
        "a refused library was written: {}",
        wrong.display()
    );
}

/// `--signature` reads one source file: a project directory, an unreadable path and a file that
/// does not parse are each refused with their own message.
#[test]
fn signature_refuses_projects_unreadable_files_and_parse_errors() {
    let project = super::temp_dir("signature_project");
    std::fs::write(
        project.join("osprey.toml"),
        "[project]\nname = \"p\"\nsource_roots = [\"src\"]\n",
    )
    .expect("manifest");
    run_args(&[&project.to_string_lossy(), "--signature"]).expect_stderr(
        1,
        &["--signature applies to a single source file"],
        &[],
    );
    run_args(&["/no/such/osprey_signature_missing.osp", "--signature"]).expect_stderr(
        1,
        &["cannot read /no/such/osprey_signature_missing.osp"],
        &[],
    );
    let broken = temp_osp("signature_parse", "fn = = =\n");
    let name = broken.to_string_lossy().into_owned();
    run_file(&broken, &["--signature"]).expect_stderr(
        1,
        &[&format!("{name}:1:0: syntax error near `fn = = =`")],
        &[],
    );
}
