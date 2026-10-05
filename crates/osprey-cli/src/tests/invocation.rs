use super::*;

#[test]
fn parse_args_defaults_to_check_with_full_capabilities() {
    let cli = parse_args(&args(&["prog.osp"])).expect("parses");
    assert_eq!(cli.path, "prog.osp");
    assert_eq!(cli.mode, "--check");
    assert!(!cli.quiet);
    assert!(cli.policy.http && cli.policy.websocket && cli.policy.fs && cli.policy.ffi);
}

#[test]
fn parse_args_build_defaults_to_current_project_and_compile() {
    let default = parse_args(&args(&["build"])).expect("build parses");
    assert_eq!(default.path, ".");
    assert_eq!(default.mode, "--compile");
    let explicit =
        parse_args(&args(&["build", "apps/demo", "--quiet"])).expect("explicit project parses");
    assert_eq!(explicit.path, "apps/demo");
    assert!(explicit.quiet);
    assert!(parse_args(&args(&["build", ".", "--check"])).is_err());
}

#[test]
fn parse_args_accepts_flavor_flag_in_both_spellings() {
    // No flag ⇒ unset, so resolution falls through to marker/extension.
    assert_eq!(parse_args(&args(&["f.osp"])).expect("ok").flavor, None);
    // Spaced and `=` spellings both set the explicit flavor.
    for spelling in [
        &["--flavor", "ml", "f.osp"][..],
        &["--flavor=ml", "f.osp"][..],
    ] {
        let cli = parse_args(&args(spelling)).expect("ok");
        assert_eq!(cli.flavor, Some(Flavor::Ml));
    }
    assert_eq!(
        parse_args(&args(&["--flavor=default", "f.osp"]))
            .expect("ok")
            .flavor,
        Some(Flavor::Default)
    );
    // A bogus value and a missing value both fail loudly.
    assert!(parse_args(&args(&["--flavor=fsharp", "f.osp"])).is_err());
    assert!(parse_args(&args(&["f.osp", "--flavor"])).is_err());
}

#[test]
fn osprey_build_rejects_every_mode_flag() {
    for flag in [
        "--ast",
        "--check",
        "--llvm",
        "--compile",
        "--run",
        "--symbols",
        "--list-tests",
        "--hover",
    ] {
        assert!(
            parse_args(&args(&["build", ".", flag])).is_err(),
            "build must reject {flag}"
        );
    }
}

#[test]
fn parse_args_last_mode_wins_and_quiet_sets() {
    let cli = parse_args(&args(&["--ast", "f.osp", "--llvm", "--run", "--quiet"])).expect("ok");
    assert_eq!(cli.mode, "--run");
    assert_eq!(cli.path, "f.osp");
    assert!(cli.quiet);
}

#[test]
fn parse_args_each_sandbox_flag_clears_one_capability() {
    let cli = parse_args(&args(&["f.osp", "--no-http"])).expect("ok");
    assert!(!cli.policy.http && cli.policy.websocket && cli.policy.fs && cli.policy.ffi);
    let cli = parse_args(&args(&["f.osp", "--no-websocket"])).expect("ok");
    assert!(cli.policy.http && !cli.policy.websocket);
    let cli = parse_args(&args(&["f.osp", "--no-fs"])).expect("ok");
    assert!(!cli.policy.fs && cli.policy.ffi);
    let cli = parse_args(&args(&["f.osp", "--no-ffi"])).expect("ok");
    assert!(!cli.policy.ffi && cli.policy.fs);
    let cli = parse_args(&args(&["--sandbox", "f.osp"])).expect("ok");
    assert!(!cli.policy.http && !cli.policy.websocket && !cli.policy.fs && !cli.policy.ffi);
}

#[test]
fn parse_args_rejects_unknown_flag_missing_path_and_extra_positional() {
    let e = parse_args(&args(&["f.osp", "--bogus"])).expect_err("unknown flag");
    assert!(e.contains("unknown flag --bogus"));
    let e = parse_args(&args(&["--check"])).expect_err("no path");
    assert!(e.contains("usage:"));
    let e = parse_args(&args(&["a.osp", "b.osp"])).expect_err("two paths");
    assert!(e.contains("unexpected argument b.osp"));
}

#[test]
fn parse_args_handles_target_and_output() {
    let cli = parse_args(&args(&[
        "f.osp",
        "--target=wasm32",
        "--debug",
        "--compile",
        "-o",
        "out/f.wasm",
    ]))
    .expect("ok");
    assert_eq!(cli.target, "wasm32");
    assert!(cli.debug);
    assert_eq!(cli.output.as_deref(), Some("out/f.wasm"));
    // default target is native, no output.
    let cli = parse_args(&args(&["f.osp"])).expect("ok");
    assert_eq!(cli.target, "native");
    assert!(!cli.debug);
    assert!(cli.output.is_none());
    // -o with no following value, and an unknown target, are errors.
    assert!(parse_args(&args(&["f.osp", "-o"])).is_err());
    assert!(parse_args(&args(&["f.osp", "--target=riscv"])).is_err());
}

#[test]
fn entry_only_requires_a_mobile_archive() -> Result<(), String> {
    for target in ["ios", "ios-sim", "android-arm64", "android-x64"] {
        let flag = format!("--target={target}");
        let cli = parse_args(&args(&["f.osp", &flag, "--entry-only"]))?;
        assert_eq!(cli.exports, MobileExports::EntryOnly);
    }
    let Err(error) = parse_args(&args(&["f.osp", "--entry-only"])) else {
        return Err("native target accepted --entry-only".to_string());
    };
    assert!(error.contains("mobile C ABI target"), "{error}");
    Ok(())
}

#[test]
fn parse_target_accepts_known_and_rejects_unknown() {
    assert_eq!(parse_target("native").as_deref(), Ok("native"));
    assert_eq!(parse_target("wasm32").as_deref(), Ok("wasm32"));
    assert_eq!(parse_target("ios").as_deref(), Ok("ios"));
    assert_eq!(parse_target("ios-sim").as_deref(), Ok("ios-sim"));
    assert!(parse_target("x86").is_err());
}

#[test]
fn output_path_defaults_by_target_and_honours_dash_o() {
    assert_eq!(output_path("a/b.osp", None, "native"), PathBuf::from("b"));
    assert_eq!(
        output_path("a/b.osp", None, "wasm32"),
        PathBuf::from("b.wasm")
    );
    assert_eq!(
        output_path("a/b.osp", Some("custom.wasm"), "wasm32"),
        PathBuf::from("custom.wasm")
    );
}

#[test]
fn debug_wasm_rejection_is_centralized() {
    let mut c = cli("p.osp", "--run", Policy::allow_all());
    assert!(reject_debug_cross_target(&c).is_none());
    c.debug = true;
    assert!(reject_debug_cross_target(&c).is_some());
    c.debug = false;
    c.profile = true;
    assert!(reject_debug_cross_target(&c).is_some());
}

#[test]
fn wasm_rejects_native_only_options_in_ir_and_check_modes() {
    let parsed = osprey_syntax::parse_program("print(1)\n");
    let input = CompilationInput::script("app.osp", "print(1)\n".to_string(), parsed.program);
    for mode in ["--llvm", "--check"] {
        for flag in ["--memory=gc", "--memory=arc", "--debug"] {
            let args = ["app.osp", "--target=wasm32", mode, flag].map(str::to_string);
            let cli = parse_args(&args).expect("valid arguments");
            assert!(
                target_error(&cli, &input).is_some(),
                "{mode} accepted {flag}"
            );
        }
    }
}

#[test]
fn stem_of_handles_dirs_and_missing_extension() {
    assert_eq!(stem_of("examples/demo.osp"), "demo");
    assert_eq!(stem_of("/a/b/c.osp"), "c");
    assert_eq!(stem_of("noext"), "noext");
}

#[test]
fn scratch_stems_disambiguate_equal_filenames_in_different_projects() {
    let left = scratch_stem("/apps/left/src/main.osp");
    let right = scratch_stem("/apps/right/src/main.osp");
    assert_ne!(left, right);
    assert!(left.starts_with("main-"));
}

#[test]
fn directive_parses_both_spellings_and_ignores_others() {
    // [FFI-LINK-DIRECTIVES]
    assert_eq!(directive("// @link: sqlite3", "link"), Some("sqlite3"));
    assert_eq!(
        directive("//@linkdir: /opt/lib ", "linkdir"),
        Some("/opt/lib")
    );
    assert_eq!(directive("  // @link:  pq  ", "link"), Some("pq"));
    assert_eq!(directive("let x = 1", "link"), None);
    assert_eq!(directive("// @link: sqlite3", "linkdir"), None);
}

// [DEBUGGER-BUILD-OPTIONS] Debug modifiers enable a native debug build.
#[test]
fn debug_controls_select_metadata_and_artifact_policies() -> Result<(), String> {
    for info in ["dwarf", "none"] {
        assert_debug_metadata_policy(info)?;
    }
    let cli = parse_args(&args(&[
        "main.osp",
        "--run",
        "--debug-out=chosen",
        "-o",
        "chosen",
        "--debug-preserve-ir",
        "--debug-preserve-symbols",
    ]))?;
    assert_eq!(cli.output.as_deref(), Some("chosen"));
    assert!(cli.debug_options.preserve_ir && cli.debug_options.preserve_symbols);
    assert!(cli.debug_options.keeps_artifacts());
    Ok(())
}

fn assert_debug_metadata_policy(info: &str) -> Result<(), String> {
    use osprey_debug::BuildKind;
    let flags = [
        "main.osp",
        "--llvm",
        &format!("--debug-info={info}"),
        "--debug-opt=none",
        "--debug-memory=off",
    ];
    let cli = parse_args(&args(&flags))?;
    assert!(cli.debug);
    let expected = if info == "dwarf" {
        BuildKind::Debug
    } else {
        BuildKind::DebugWithoutInfo
    };
    assert_eq!(build_kind(&cli), expected);
    let options = NativeOptions::from_cli(&cli);
    assert_eq!(options.optimization(), "-O0");
    assert!(
        !options.cacheable(),
        "explicit build controls cannot reuse incompatible cached artifacts"
    );
    Ok(())
}

#[test]
fn debug_controls_reject_unsupported_and_conflicting_requests() {
    for (flags, diagnostic) in INVALID_DEBUG_OPTIONS {
        let values = [vec!["main.osp"], flags.to_vec()].concat();
        assert!(
            parse_args(&args(&values)).is_err_and(|error| error.contains(diagnostic)),
            "{values:?}: expected {diagnostic}"
        );
    }
}

const INVALID_DEBUG_OPTIONS: &[(&[&str], &str)] = &[
    (&["--debug-opt=optimized"], "not implemented"),
    (&["--debug-opt=limited"], "not implemented"),
    (&["--debug-memory=object-graph"], "not implemented"),
    (&["--debug-memory=timeline"], "not implemented"),
    (&["--debug-info=codeview"], "supported: dwarf or none"),
    (&["--debug-preserve-ir"], "require --compile or --run"),
    (
        &["--run", "--debug-info=none", "--debug-preserve-symbols"],
        "requires --debug-info=dwarf",
    ),
    (
        &["--compile", "--debug-out", "a", "-o", "b"],
        "must name the same output",
    ),
    (
        &["--compile", "-o", "a", "--debug-out=b"],
        "must name the same output",
    ),
    (&["--compile", "--debug-out"], "requires a path"),
    (&["--compile", "--debug-out="], "cannot be empty"),
];
