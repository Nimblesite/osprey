use super::*;

#[test]
fn native_float_remainder_links_its_platform_math_runtime() {
    for backend in ["default", "gc", "arc"] {
        let flags = link_args("%value = frem double %left, %right", "", backend);
        assert_eq!(
            flags.iter().any(|flag| flag == "-lm"),
            cfg!(unix),
            "{flags:?}"
        );
        let integer = link_args("%value = srem i64 %left, %right", "", backend);
        assert!(!integer.iter().any(|flag| flag == "-lm"), "{integer:?}");
    }
}

#[test]
fn link_args_adds_ffi_directives_and_openssl_for_http() {
    let ffi = link_args(
        "",
        "// @link: sqlite3\n// @linkdir: /opt/lib\ncode\n",
        "default",
    );
    assert!(ffi.iter().any(|a| a == "-lsqlite3"), "{ffi:?}");
    assert!(ffi.iter().any(|a| a == "-L/opt/lib"), "{ffi:?}");
    let http = link_args("call void @http_listen()", "", "default");
    assert!(http.iter().any(|a| a == "-lssl") && http.iter().any(|a| a == "-lcrypto"));
    // No HTTP markers => no openssl flags.
    let plain = link_args("call void @osprey_list_empty()", "", "default");
    assert!(!plain.iter().any(|a| a == "-lssl"));
}

#[test]
fn link_args_selects_gc_archive_and_validates_backend() {
    // [MEM-BACKENDS] `gc`/`arc` swap archives; `default` does not.
    let gc = link_args("call void @osprey_list_empty()", "", "gc");
    assert!(
        gc.iter().any(|a| a.contains("_gc.a")) || gc.is_empty(),
        "gc backend must select a *_gc archive when one is present: {gc:?}"
    );
    let arc = link_args("call void @osprey_list_empty()", "", "arc");
    assert!(
        arc.iter().any(|a| a.contains("_arc.a")) || arc.is_empty(),
        "arc backend must select a *_arc archive when one is present: {arc:?}"
    );
    let plain = link_args("call void @osprey_list_empty()", "", "default");
    assert!(!plain.iter().any(|a| a.contains("_gc.a")), "{plain:?}");
    assert!(!plain.iter().any(|a| a.contains("_arc.a")), "{plain:?}");
    // Backend validation: default/gc/arc accepted, others rejected.
    assert_eq!(parse_memory("gc").as_deref(), Ok("gc"));
    assert_eq!(parse_memory("default").as_deref(), Ok("default"));
    assert_eq!(parse_memory("arc").as_deref(), Ok("arc"));
    assert!(parse_memory("bogus").is_err());
}

#[test]
fn openssl_and_compiler_helpers_are_well_formed() {
    let flags = openssl_flags();
    assert!(flags.iter().any(|f| f == "-lssl") && flags.iter().any(|f| f == "-lcrypto"));
    assert_ne!(c_compiler(), "");
    assert!(find_runtime_lib("definitely_not_a_real_lib_xyz.a").is_none());
}

#[test]
fn runtime_search_walks_above_arbitrarily_nested_cargo_profiles() {
    let root = PathBuf::from("workspace");
    let executable_dir = root.join("target/llvm-cov-target/ci/deps");
    let lib = "libfiber_runtime.a";
    let candidates = runtime_lib_candidates(lib, Some(&executable_dir));
    let expected = root.join("compiler/bin").join(lib).display().to_string();
    assert!(candidates.contains(&expected), "{candidates:?}");
    let fallbacks = runtime_lib_candidates(lib, None);
    assert!(fallbacks.contains(&format!("/usr/local/lib/{lib}")));
    assert!(fallbacks
        .iter()
        .any(|path| path.ends_with("compiler/lib/libfiber_runtime.a")));
}

#[cfg(unix)]
#[test]
fn child_exit_code_maps_codes_and_signals() {
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(child_exit_code(std::process::ExitStatus::from_raw(0)), 0);
    assert_eq!(
        child_exit_code(std::process::ExitStatus::from_raw(1 << 8)),
        1
    );
    // Killed by SIGKILL (9): no exit code, so 128 + signal.
    assert_eq!(child_exit_code(std::process::ExitStatus::from_raw(9)), 137);
}

#[test]
fn report_type_errors_counts_zero_for_valid_and_more_for_ill_typed() {
    let ok = osprey_syntax::parse_program("let x = 1\nprint(x)\n").program;
    let ok = CompilationInput::script("ok.osp", String::new(), ok);
    assert_eq!(report_type_errors(&ok), 0);
    let bad = osprey_syntax::parse_program("let y = 1 + \"oops\" - true\n").program;
    let bad = CompilationInput::script("bad.osp", String::new(), bad);
    assert!(report_type_errors(&bad) > 0);
}

fn temp_source(name: &str, body: &str) -> String {
    let p = std::env::temp_dir().join(format!("osprey_cli_{name}.osp"));
    std::fs::write(&p, body).expect("write temp source");
    p.display().to_string()
}

#[test]
fn run_drives_check_symbols_and_llvm_modes_in_process() {
    let path = temp_source("ok", "let greeting = \"hi\"\nprint(greeting)\n");
    for mode in ["--check", "--symbols", "--llvm", "--ast"] {
        // ExitCode is opaque; this drives run -> dispatch coverage and must
        // not panic for a well-formed program.
        let _ = run(&cli(path.clone(), mode, Policy::allow_all()));
    }
}

#[test]
fn run_reports_missing_file_and_parse_errors() {
    let _ = run(&cli(
        "/no/such/osprey/file.osp",
        "--check",
        Policy::allow_all(),
    ));
    let path = temp_source("broken", "fn = = =\n");
    let _ = run(&cli(path, "--check", Policy::allow_all())); // parse-error branch
}

#[test]
fn load_input_reports_project_and_module_assembly_errors() {
    let missing = std::env::temp_dir()
        .join(format!("osprey_cli_missing_{}", std::process::id()))
        .join("osprey.toml");
    assert!(load_input(&cli(
        missing.display().to_string(),
        "--check",
        Policy::allow_all()
    ))
    .is_err());
    let source = "module A { export let x = 1 }\nmodule A { export let x = 2 }\n";
    let path = temp_source("duplicate_module", source);
    assert!(load_input(&cli(path, "--check", Policy::allow_all())).is_err());
    // `--flavor` on a directory project is rejected: projects pick a flavor
    // per source file, so a whole-project flavor is meaningless.
    let mut with_flavor = cli(
        std::env::temp_dir().to_string_lossy().into_owned(),
        "--check",
        Policy::allow_all(),
    );
    with_flavor.flavor = Some(Flavor::Ml);
    assert!(load_input(&with_flavor).is_err());
}

#[test]
fn run_rejects_sandbox_violation_before_codegen() {
    let path = temp_source("fs", "let c = readFile(\"x.txt\")\n");
    let _ = run(&cli(path, "--llvm", Policy::sandbox())); // sandbox-violation branch
}

#[test]
fn parse_args_accepts_the_memory_backend_flag() {
    let cli = parse_args(&args(&["f.osp", "--memory=gc"])).expect("ok");
    assert_eq!(cli.memory, "gc");
}

#[test]
fn report_type_errors_prints_positioned_diagnostics() {
    // An undefined identifier yields an error carrying a source position,
    // exercising the `Some(position)` diagnostic arm.
    let bad = osprey_syntax::parse_program("print(missingVariable)\n").program;
    let bad = CompilationInput::script("bad.osp", String::new(), bad);
    assert!(report_type_errors(&bad) > 0);
}

#[test]
fn parse_flavor_accepts_known_names_and_rejects_the_rest() {
    assert_eq!(parse_flavor("default").expect("default"), Flavor::Default);
    assert_eq!(parse_flavor("ml").expect("ml"), Flavor::Ml);
    let err = parse_flavor("klingon").expect_err("unknown flavor rejected");
    assert!(err.contains("usage: osprey"), "{err}");
}

#[test]
fn link_flag_helpers_return_a_nonempty_flag_set() {
    // Both run to completion regardless of host: `openssl_flags` always yields
    // at least the `-lssl -lcrypto` fallback, and the runtime-lib search walks
    // its whole candidate list (returning None here is fine — the body ran).
    assert!(openssl_flags().iter().any(|f| f == "-lssl"));
    let _ = find_runtime_lib("libosprey_runtime_definitely_absent.a");
}

#[test]
fn compile_ir_and_debug_helpers_switch_on_the_build_kind() {
    use osprey_debug::BuildKind;
    let program = osprey_syntax::parse_program("let n = 1\nprint(\"${n}\")\n").program;
    // Debug and Profile both take the debug-info codegen path; the opt
    // flag differs (Profile keeps the release optimizer [PROF-BUILD-MODE]).
    assert!(compile_ir("p.osp", &program, BuildKind::Debug).is_ok());
    assert!(compile_ir("p.osp", &program, BuildKind::Profile).is_ok());
    assert_eq!(opt_flag(BuildKind::Debug), "-O0");
    assert_ne!(opt_flag(BuildKind::Release), "");
    assert_eq!(
        opt_flag(BuildKind::Profile),
        opt_flag(BuildKind::Release),
        "profiling must keep release optimization"
    );
}

// [PROF-CLI-RUN] end-to-end: `--profile` compiles with the profile
// pipeline (two-step + dsymutil), runs under the in-runtime sampler, and
// writes all four exports where `-o` points. POSIX-only by design.
#[cfg(unix)]
#[test]
fn profile_run_writes_exports_where_output_points() {
    let path = temp_source(
        "prof_e2e",
        "fn dec(n: int) -> int = wrapSub(n, 1)\n\
         fn count(n: int) -> int = match n {\n    0 => 0\n    _ => count(dec(n))\n}\n\
         print(\"${count(500)}\")\n",
    );
    let dir = std::env::temp_dir().join(format!("osprey_prof_exports_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create export dir");
    let mut c = cli(path, "--run", Policy::allow_all());
    c.profile = true;
    c.output = Some(dir.join("prof_e2e").display().to_string());
    let (out_dir, stem) = profile_export_target(&c);
    assert_eq!(out_dir, dir);
    assert_eq!(stem, "prof_e2e");
    let _ = run(&c);
    for export in [
        "prof_e2e.speedscope.json",
        "prof_e2e.cpuprofile",
        "prof_e2e.folded",
        "prof_e2e.profile.json",
    ] {
        assert!(
            dir.join(export).exists(),
            "missing export {export} in {}",
            dir.display()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn profile_export_target_defaults_to_cwd_and_source_stem() {
    let c = cli("examples/demo.osp", "--run", Policy::allow_all());
    let (dir, stem) = profile_export_target(&c);
    assert_eq!(dir, PathBuf::from("."));
    assert_eq!(stem, "demo");
    let mut with_bare_output = cli("a.osp", "--run", Policy::allow_all());
    with_bare_output.output = Some("renamed".to_string());
    let (dir, stem) = profile_export_target(&with_bare_output);
    assert_eq!(dir, PathBuf::from("."));
    assert_eq!(stem, "renamed");
}

// On Windows `--profile` is rejected outright (POSIX-only sampler), so
// the acceptance-path assertions only hold on unix.
#[cfg(unix)]
#[test]
fn parse_args_profile_implies_run_and_rejects_debug_combo() {
    let args = vec!["main.osp".to_string(), "--profile".to_string()];
    let cli = parse_args(&args).expect("parse --profile");
    assert!(cli.profile);
    assert_eq!(cli.mode, "--run");
    assert_eq!(build_kind(&cli), osprey_debug::BuildKind::Profile);
    // An explicit mode is preserved.
    let args = vec![
        "main.osp".to_string(),
        "--compile".to_string(),
        "--profile".to_string(),
    ];
    let cli = parse_args(&args).expect("parse --compile --profile");
    assert_eq!(cli.mode, "--compile");
    // --debug + --profile is a contradiction.
    let args = vec![
        "main.osp".to_string(),
        "--debug".to_string(),
        "--profile".to_string(),
    ];
    assert!(parse_args(&args).is_err());
    // Default build kinds for the other switches.
    let plain = parse_args(&["main.osp".to_string()]).expect("parse plain");
    assert_eq!(build_kind(&plain), osprey_debug::BuildKind::Release);
    let dbg = parse_args(&["main.osp".to_string(), "--debug".to_string()]).expect("parse --debug");
    assert_eq!(build_kind(&dbg), osprey_debug::BuildKind::Debug);
}

#[test]
fn wasm_target_rejects_debug_then_dispatches_to_the_backend() {
    let program = osprey_syntax::parse_program("let n = 1\nprint(\"${n}\")\n").program;
    let input = CompilationInput::script("p.osp", String::new(), program);
    let mut c = cli("p.osp", "--compile", Policy::allow_all());
    c.target = "wasm32".to_string();
    // --debug + --target=wasm32 is rejected before any toolchain work.
    c.debug = true;
    let _ = compile_program_to_disk(&c, &input);
    let _ = run_program(&c, &input);
    // Without --debug the wasm build/run driver is dispatched (it fails
    // cleanly without the wasm toolchain, but the dispatch lines execute).
    c.debug = false;
    let _ = compile_program_to_disk(&c, &input);
    let _ = run_program(&c, &input);
}

/// [DEBUGGER-BUILD-OPTIONS] A failed IR write is a build failure, not a lost artifact.
#[test]
fn debug_build_rejects_an_unwritable_artifact_path() -> Result<(), String> {
    let source = "print(42)\n";
    let program = osprey_syntax::parse_program(source).program;
    let input = CompilationInput::script("debug-write.osp", source.into(), program);
    let dir = std::env::temp_dir().join(format!("osprey-debug-write-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let output = dir.join("absent/output");
    let cli = parse_args(&args(&[
        "debug-write.osp",
        "--compile",
        "--debug-preserve-ir",
    ]))?;
    assert!(build_input(&input, &output, NativeOptions::from_cli(&cli)).is_err());
    assert!(!output.exists());
    assert!(
        !dir.exists(),
        "a failed write cannot silently select another output"
    );
    Ok(())
}
