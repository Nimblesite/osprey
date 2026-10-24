//! End-to-end tests of `osprey hunt` against a stand-in `klee` executable
//! [SYMEX-CLI] [SYMEX-FINDINGS] [SYMEX-REPLAY]. The stand-in copies a
//! prepared KLEE output directory into `--output-dir`, so the whole pipeline
//! (compile, link, run, scan, replay, report, exit code) runs without the
//! engine, and the real engine is covered by `tests/hunt/` under `make`.

#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: a missing JSON key is a test failure, not a production panic"
)]

use super::toolchain::{KLEE_ENV, RUNTIME_ENV};
use super::*;
use std::sync::{Mutex, MutexGuard};

/// Environment variables are process-wide; tests that set them take turns.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(unix)]
const PTR_ERR: &str = "Error: memory error: out of bound pointer\n\
File: list_runtime.c\nLine: 88\nassembly.ll line: 4012\nState: 7\nStack: \n\
\t#000000031 in osp_list_get(ptr, i64) at list_runtime.c:88\n\
\t#000000024 in main() at PROGRAM:1\n";

/// A scratch directory holding the program, the fake engine and its canned
/// output; removed on drop.
struct Scene {
    root: PathBuf,
}

impl Scene {
    fn new(name: &str, program: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("osprey-hunt-e2e-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("canned")).expect("scene dir");
        std::fs::write(root.join("main.osp"), program).expect("program");
        std::fs::write(root.join("runtime.bc"), "BC").expect("runtime");
        Self { root }
    }

    fn program(&self) -> String {
        self.root.join("main.osp").display().to_string()
    }

    #[cfg(unix)]
    fn canned(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.root.join("canned").join(name), bytes).expect("canned file");
    }

    #[cfg(unix)]
    fn canned_finding(&self) {
        let err = PTR_ERR.replace("PROGRAM", &self.program());
        self.canned("test000001.ptr.err", err.as_bytes());
        let ktest = ktest::encode(&[("stdin", b"9\nzz"), ("stdin.len", &2i32.to_le_bytes())]);
        self.canned("test000001.ktest", &ktest);
        self.canned("test000002.early", b"max-time\n");
        self.canned("info", b"Elapsed: 00:00:02\nKLEE: done: explored paths = 4\nKLEE: done: completed paths = 3\nKLEE: done: total instructions = 99\n");
    }

    /// A `klee` that records its arguments in `args.txt` and the module it
    /// was given in `module.ll`, copies the canned directory to
    /// `--output-dir` and exits with `code`.
    #[cfg(unix)]
    fn fake_klee(&self, code: u8) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = self.root.join("klee");
        let body = format!(
            "#!/bin/sh\nroot='{root}'\nprintf '%s\\n' \"$@\" > \"$root/args.txt\"\n\
             for a in \"$@\"; do last=\"$a\"; case \"$a\" in --output-dir=*) out=\"${{a#--output-dir=}}\";; esac; done\n\
             cp \"$last\" \"$root/module.ll\"\n\
             mkdir -p \"$out\" && cp \"$root\"/canned/* \"$out\"/ 2>/dev/null\necho engine-said-this >&2\nexit {code}\n",
            root = self.root.display()
        );
        std::fs::write(&script, body).expect("fake klee");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        script
    }

    /// What the fake engine was invoked with, one argument per line.
    #[cfg(unix)]
    fn engine_args(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("args.txt"))
            .unwrap_or_default()
            .lines()
            .map(ToString::to_string)
            .collect()
    }

    fn with_toolchain<T>(&self, klee: &Path, run: impl FnOnce() -> T) -> T {
        let _guard = lock();
        std::env::set_var(KLEE_ENV, klee);
        std::env::set_var(RUNTIME_ENV, self.root.join("runtime.bc"));
        let result = run();
        std::env::remove_var(KLEE_ENV);
        std::env::remove_var(RUNTIME_ENV);
        result
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn options(scene: &Scene, extra: &[&str]) -> Options {
    let mut args = vec![scene.program()];
    args.extend(extra.iter().map(ToString::to_string));
    options::parse(&args).expect("options")
}

#[cfg(unix)]
#[test]
fn a_finding_without_replay_fails_and_is_reported_as_json() {
    let scene = Scene::new("json", "print(\"hi\")\n");
    scene.canned_finding();
    let klee = scene.fake_klee(0);
    let out = scene.root.join("kout");
    let opts = options(
        &scene,
        &["--no-replay", "--json", "--out", &out.display().to_string()],
    );
    let (rendered, code) = scene.with_toolchain(&klee, || hunt(&opts)).expect("hunt");
    assert_eq!(code, ExitCode::FAILURE);
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("json");
    assert_eq!(value["engine"], "KLEE");
    assert_eq!(value["explored"]["paths"], 4);
    assert_eq!(value["findings"][0]["class"], "memory");
    assert_eq!(value["findings"][0]["line"], 1);
    assert_eq!(value["findings"][0]["stdin"], "9\\n");
    assert_eq!(value["findings"][0]["replay"], "not-run");
    assert_eq!(value["incomplete"][0]["reason"], "max-time");
    assert!(out.join("info").exists(), "--out keeps KLEE's directory");
}

#[cfg(unix)]
#[test]
fn a_finding_the_native_build_does_not_reproduce_is_klee_only() {
    let scene = Scene::new("replay", "print(\"hi\")\n");
    scene.canned_finding();
    let klee = scene.fake_klee(0);
    let opts = options(&scene, &["--quiet"]);
    let (rendered, code) = scene.with_toolchain(&klee, || hunt(&opts)).expect("hunt");
    assert_eq!(code, ExitCode::SUCCESS, "{rendered}");
    assert!(
        rendered.contains("symbolic execution by KLEE"),
        "{rendered}"
    );
    assert!(rendered.contains("replay   klee-only"), "{rendered}");
    assert!(
        rendered.contains("explored 4 paths (3 complete), 99 instructions, 00:00:02"),
        "{rendered}"
    );
}

#[cfg(unix)]
#[test]
fn a_clean_run_reports_no_findings() {
    let scene = Scene::new("clean", "print(\"hi\")\n");
    scene.canned("info", b"KLEE: done: explored paths = 1\n");
    let klee = scene.fake_klee(0);
    let opts = options(&scene, &["--quiet"]);
    let (rendered, code) = scene.with_toolchain(&klee, || hunt(&opts)).expect("hunt");
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(rendered.contains("findings: none"), "{rendered}");
}

#[cfg(unix)]
#[test]
fn a_failing_engine_is_a_tool_error() {
    let scene = Scene::new("engine", "print(\"hi\")\n");
    let klee = scene.fake_klee(3);
    let opts = options(&scene, &["--quiet"]);
    let code = scene
        .with_toolchain(&klee, || hunt(&opts))
        .expect_err("engine failure");
    assert_eq!(code, ExitCode::from(2));
}

#[test]
fn a_missing_engine_is_a_tool_error() {
    let scene = Scene::new("missing", "print(\"hi\")\n");
    let opts = options(&scene, &[]);
    let missing = scene.root.join("no-klee-here");
    let code = scene
        .with_toolchain(&missing, || hunt(&opts))
        .expect_err("missing klee");
    assert_eq!(code, ExitCode::from(2));
}

#[cfg(unix)]
#[test]
fn a_program_that_does_not_type_check_fails_before_the_engine_runs() {
    let scene = Scene::new("typeerr", "let x: int = \"text\"\n");
    let klee = scene.fake_klee(0);
    let opts = options(&scene, &["--quiet"]);
    let code = scene
        .with_toolchain(&klee, || hunt(&opts))
        .expect_err("type error");
    assert_eq!(code, ExitCode::FAILURE);
}

#[test]
fn run_rejects_bad_arguments_with_usage() {
    assert_eq!(run(&[]), ExitCode::from(2));
    assert_eq!(run(&["--bogus".to_string()]), ExitCode::from(2));
    assert_eq!(run(&["--help".to_string()]), ExitCode::SUCCESS);
    assert_eq!(run(&["-h".to_string()]), ExitCode::SUCCESS);
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
    assert!(error.contains("refusing to clear"));
    std::fs::write(dir.join("info"), "KLEE").expect("info");
    assert!(clear_run_directory(&dir).is_ok() && !dir.exists());
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
    let cli = parse_args(std::slice::from_ref(&path)).expect("args");
    let input = load_input(&cli).expect("load");
    let symbolic = module::symbolic_module(&input, 64).expect("symbolic build");
    assert!(symbolic.contains("target triple = \"x86_64-unknown-linux-gnu\""));
    assert!(symbolic.contains("@osp_hunt_input_bytes = global i64 64"));
}

/// The value of `--output-dir=` the engine was given.
#[cfg(unix)]
fn output_dir(args: &[String]) -> PathBuf {
    PathBuf::from(
        args.iter()
            .find_map(|argument| argument.strip_prefix("--output-dir="))
            .unwrap_or_default(),
    )
}

/// The engine is invoked with exactly the spec command, the toolchain's
/// runtime and the run's own module, at every budget and stream cap; the
/// module it loads is x86-64 Linux carrying the cap [SYMEX-KLEE-INVOCATION]
/// [SYMEX-CODEGEN] [SYMEX-MOCK-STREAM].
#[cfg(unix)]
#[test]
fn the_engine_receives_exactly_the_spec_command_and_module() {
    let scene = Scene::new("command", "print(\"hi\")\n");
    scene.canned("info", b"KLEE: done: explored paths = 1\n");
    let klee = scene.fake_klee(0);
    let out = scene.root.join("kout");
    let runtime = scene.root.join("runtime.bc").display().to_string();
    for (seconds, megabytes, cap) in [
        (1u64, 256u64, 0u64),
        (7, 300, 8),
        (60, 2000, 64),
        (3600, 65_536, 4096),
    ] {
        let flags = [
            "--max-time".to_string(),
            seconds.to_string(),
            "--max-memory".into(),
            megabytes.to_string(),
            "--input-bytes".into(),
            cap.to_string(),
            "--out".into(),
            out.display().to_string(),
            "--quiet".into(),
        ];
        let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
        let opts = options(&scene, &flags);
        let (_, code) = scene.with_toolchain(&klee, || hunt(&opts)).expect("hunt");
        assert_eq!(code, ExitCode::SUCCESS);
        let args = scene.engine_args();
        let module = args.last().cloned().unwrap_or_default();
        let expected = super::spec_engine_tests::spec_command(
            &runtime,
            &out.display().to_string(),
            (seconds, megabytes),
            &module,
        );
        assert_eq!(args, expected);
        assert!(module.ends_with("-hunt.ll"), "{module}");
        assert!(
            !Path::new(&module).exists(),
            "the module is removed after the run"
        );
        let loaded = std::fs::read_to_string(scene.root.join("module.ll")).expect("module copy");
        assert!(loaded.contains("target triple = \"x86_64-unknown-linux-gnu\""));
        assert!(
            loaded.ends_with(&format!("@osp_hunt_input_bytes = global i64 {cap}\n")),
            "{cap}"
        );
        assert!(
            loaded.contains("main.osp"),
            "debug locations name the program"
        );
    }
}

/// Without `--out` the run directory is temporary; with it, it is kept.
#[cfg(unix)]
#[test]
fn the_run_directory_is_temporary_unless_out_is_given() {
    let scene = Scene::new("tempdir", "print(\"hi\")\n");
    scene.canned("info", b"KLEE: done: explored paths = 1\n");
    let klee = scene.fake_klee(0);
    let opts = options(&scene, &["--quiet"]);
    let (rendered, _) = scene.with_toolchain(&klee, || hunt(&opts)).expect("hunt");
    let temporary = output_dir(&scene.engine_args());
    assert!(
        temporary.starts_with(std::env::temp_dir()),
        "{}",
        temporary.display()
    );
    assert!(!temporary.exists(), "a temporary run directory is deleted");
    assert!(!rendered.contains("KLEE output kept in"), "{rendered}");
    let out = scene.root.join("kept");
    let opts = options(&scene, &["--quiet", "--out", &out.display().to_string()]);
    let (rendered, _) = scene.with_toolchain(&klee, || hunt(&opts)).expect("hunt");
    assert_eq!(output_dir(&scene.engine_args()), out);
    assert!(out.join("info").exists());
    assert!(
        rendered.contains(&format!("KLEE output kept in {}", out.display())),
        "{rendered}"
    );
}

/// A non-empty `--out` that is not a KLEE run is refused before the engine
/// starts, and nothing in it is touched; a previous run is replaced.
#[cfg(unix)]
#[test]
fn only_a_previous_run_is_cleared_from_out() {
    let scene = Scene::new("outdir", "print(\"hi\")\n");
    scene.canned("info", b"KLEE: done: explored paths = 1\n");
    let klee = scene.fake_klee(0);
    let out = scene.root.join("mine");
    std::fs::create_dir_all(&out).expect("dir");
    std::fs::write(out.join("precious.txt"), "keep").expect("file");
    let opts = options(&scene, &["--quiet", "--out", &out.display().to_string()]);
    let code = scene
        .with_toolchain(&klee, || hunt(&opts))
        .expect_err("refused");
    assert_eq!(code, ExitCode::from(2));
    assert!(out.join("precious.txt").exists());
    assert!(scene.engine_args().is_empty(), "the engine never ran");
    std::fs::write(out.join("info"), "a previous run").expect("info");
    let (_, code) = scene.with_toolchain(&klee, || hunt(&opts)).expect("hunt");
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(
        !out.join("precious.txt").exists(),
        "a previous run is cleared"
    );
    assert!(out.join("info").exists());
}

/// A failed engine is a tool error and still leaves no temporary files.
#[cfg(unix)]
#[test]
fn a_failed_engine_cleans_up_after_itself() {
    let scene = Scene::new("failclean", "print(\"hi\")\n");
    for status in [1, 2, 3, 127, 255] {
        let klee = scene.fake_klee(status);
        let opts = options(&scene, &["--quiet"]);
        let code = scene
            .with_toolchain(&klee, || hunt(&opts))
            .expect_err("engine failed");
        assert_eq!(code, ExitCode::from(2), "klee exit {status}");
        let args = scene.engine_args();
        assert!(
            !Path::new(args.last().map_or("", String::as_str)).exists(),
            "module removed"
        );
        assert!(!output_dir(&args).exists(), "run directory removed");
    }
}

/// `--json` reports the budget and cap it ran with, and incomplete paths
/// alone never fail the command [SYMEX-CLI] [SYMEX-FINDINGS].
#[cfg(unix)]
#[test]
fn incomplete_paths_alone_pass_with_or_without_replay() {
    let scene = Scene::new("incomplete", "print(\"hi\")\n");
    scene.canned("info", b"KLEE: done: explored paths = 9\n");
    for n in 1..=5 {
        scene.canned(&format!("test{n:06}.early"), b"Execution halting.\n");
    }
    scene.canned("test000006.solver.err", b"Error: Query timed out\n");
    let klee = scene.fake_klee(0);
    for extra in [&["--json"][..], &["--json", "--no-replay"][..]] {
        let mut flags = vec!["--max-time", "9", "--input-bytes", "0"];
        flags.extend_from_slice(extra);
        let opts = options(&scene, &flags);
        let (rendered, code) = scene.with_toolchain(&klee, || hunt(&opts)).expect("hunt");
        assert_eq!(code, ExitCode::SUCCESS, "{rendered}");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("json");
        assert_eq!(value["budgetSeconds"], 9);
        assert_eq!(value["inputBytes"], 0);
        assert_eq!(value["findings"].as_array().map(Vec::len), Some(0));
        assert_eq!(value["incomplete"].as_array().map(Vec::len), Some(6));
        assert!(value["program"]
            .as_str()
            .is_some_and(|p| p.ends_with("main.osp")));
    }
}

/// An unmodelled finding names where the mock stops; after replay it does
/// not fail the command, without replay it does [SYMEX-MOCK-UNSUPPORTED].
#[cfg(unix)]
#[test]
fn an_unmodelled_finding_fails_only_without_replay() {
    let scene = Scene::new("unmodelled", "print(\"hi\")\n");
    scene.canned("info", b"KLEE: done: explored paths = 1\n");
    let stack = format!(
        "Error: unmodelled: fork\nStack: \n\t#000000001 in fork() at hunt_unsupported.c:20\n\t#000000002 in main() at {}:1\n",
        scene.program()
    );
    scene.canned("test000001.unmodelled.err", stack.as_bytes());
    let klee = scene.fake_klee(0);
    for (extra, expected) in [
        (&["--quiet"][..], ExitCode::SUCCESS),
        (&["--quiet", "--no-replay"][..], ExitCode::FAILURE),
    ] {
        let opts = options(&scene, extra);
        let (rendered, code) = scene.with_toolchain(&klee, || hunt(&opts)).expect("hunt");
        assert_eq!(code, expected, "{extra:?}: {rendered}");
        assert!(rendered.contains("unmodelled"), "{rendered}");
        assert!(rendered.contains("main.osp:1"), "{rendered}");
    }
}
