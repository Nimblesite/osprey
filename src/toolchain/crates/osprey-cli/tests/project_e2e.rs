//! End-to-end coverage for mixed-flavor project CLI entry points.
#![expect(
    clippy::expect_used,
    reason = "subprocess and fixture setup failures must fail these end-to-end tests immediately"
)]

#[path = "common/launcher.rs"]
mod launcher;
use launcher::{osprey, repo_root};

use std::path::{Path, PathBuf};
use std::process::Command;

/// Fixture setup and cleanup failures end these tests through `?`.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A rewritten fixture copy: its directory, the original text and the new text.
type Rewritten = (PathBuf, String, String);

fn fixture() -> PathBuf {
    repo_root().join("src/osprey/examples/talon/backend")
}

struct Output {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run(args: &[String]) -> Output {
    finish(osprey().args(args))
}

fn run_in(directory: &Path, args: &[&str]) -> Output {
    let mut command = osprey();
    let _ = command.current_dir(directory).args(args);
    finish(&mut command)
}

fn finish(command: &mut Command) -> Output {
    let result = command.output().expect("run osprey");
    Output {
        code: result.status.code(),
        stdout: String::from_utf8_lossy(&result.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
    }
}

fn arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Asserts a pipeline invocation succeeded, returning its output.
fn checked(output: Output) -> Output {
    assert_eq!(output.code, Some(0), "stderr={}", output.stderr);
    output
}

/// Asserts the `flag` pipeline stage on `path` succeeded, its stdout
/// containing every `wanted` and no `banned` fragment.
fn flag_stdout(path: &Path, flag: &str, wanted: &[&str], banned: &[&str]) -> String {
    let stdout = checked(run(&[arg(path), flag.to_string()])).stdout;
    for fragment in wanted {
        assert!(stdout.contains(fragment), "{stdout}");
    }
    for fragment in banned {
        assert!(!stdout.contains(fragment), "{stdout}");
    }
    stdout
}

/// Checks `project` quietly, asserting the flat project pipeline accepted it.
fn check_passes(project: &Path) {
    let output = run(&[arg(project), "--check".into(), "--quiet".into()]);
    assert_eq!(output.code, Some(0), "stderr={}", output.stderr);
}

/// A process-scoped temporary path plus the namespace its directory name spells.
fn temp_project(name: &str) -> (PathBuf, String) {
    let path = std::env::temp_dir().join(format!("osprey_{name}_e2e_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    let namespace = path.file_name().and_then(|name| name.to_str());
    let namespace = namespace
        .expect("temporary directory has an identifier name")
        .to_owned();
    (path, namespace)
}

const FIXTURE_SOURCES: [&str; 9] = [
    "osprey.toml",
    "main.ospml",
    "lib/sqlite.ospml",
    "domain/money.ospml",
    "domain/accounts.ospml",
    "domain/json.ospml",
    "store/ledger.ospml",
    "store/metrics.ospml",
    "api/routes.ospml",
];

/// The module the project-layout tests import their `answer` from.
const ANSWERS_MODULE: &str = "module Answers {\n    export fn answer() -> int = 42\n}\n";

fn copy_fixture(name: &str) -> PathBuf {
    let root = temp_project(&format!("fixture_{name}")).0;
    for relative in FIXTURE_SOURCES {
        let destination = root.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).expect("create project directories");
        }
        let _ =
            std::fs::copy(fixture().join(relative), &destination).expect("copy project fixture");
    }
    root
}

fn expected_output() -> String {
    std::fs::read_to_string(fixture().join("expectedoutput")).expect("read expectedoutput fixture")
}

/// Copies the bank fixture under `name`, rewriting `relative` from `from` to `to`.
fn rewritten_fixture(name: &str, relative: &str, from: &str, to: &str) -> Rewritten {
    let project = copy_fixture(name);
    let file = project.join(relative);
    let original = std::fs::read_to_string(&file).expect("read fixture source");
    let rewritten = original.replace(from, to);
    std::fs::write(&file, &rewritten).expect("write rewritten fixture source");
    (project, original, rewritten)
}

/// Runs `project` under the GC and ARC backends, returning each run's stderr.
fn run_under_each_backend(project: &Path, expected: &str) -> Vec<String> {
    let mut stderrs = Vec::new();
    for backend in ["gc", "arc"] {
        let output = run(&[arg(project), "--run".into(), format!("--memory={backend}")]);
        assert_eq!(output.code, Some(0), "{backend}: {}", output.stderr);
        assert_eq!(output.stdout, expected, "{backend}");
        stderrs.push(output.stderr);
    }
    stderrs
}

/// The Talon Bank demo binds one fixed HTTP port and one `SQLite` file, so
/// the tests that actually execute it must not overlap in time.
fn demo_run_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    let guard = match LOCK.get_or_init(std::sync::Mutex::default).lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    assert!(
        demo_available(Path::new("/tmp/talon_bank.hold")),
        "bank demo active"
    );
    guard
}

fn demo_available(marker: &Path) -> bool {
    !marker.exists()
}
#[test]
fn demo_preparation_preserves_another_sessions_hold_marker() {
    let directory = copy_fixture("foreign_hold");
    let marker = directory.join("hold");
    std::fs::write(&marker, "another session").expect("create foreign hold");
    let available = demo_available(&marker);
    let retained = std::fs::read_to_string(&marker);
    std::fs::remove_dir_all(directory).expect("clean fixture");
    assert!(!available, "an active demo must be refused, not stopped");
    assert_eq!(
        retained.expect("foreign hold must survive"),
        "another session"
    );
}

#[test]
fn flat_project_pipeline_covers_scripts_directories_and_manifests() {
    let project = fixture();
    let manifest = project.join("osprey.toml");
    for path in [&project, &manifest] {
        check_passes(path);
    }

    let _ = flag_stdout(
        &project,
        "--ast",
        &[],
        &["Import(", "Namespace {", "Module {"],
    );

    let _ = flag_stdout(&project, "--llvm", &["define"], &[]);

    let symbols = flag_stdout(&project, "--symbols", &["bank::Api::route"], &["__osp_"]);
    assert_project_main_symbol_is_local(&symbols, &project.join("main.ospml"));

    // An ordinary script keeps its own names: no project name mangling.
    let source = temp_project("plain_script").0.with_extension("osp");
    std::fs::write(
        &source,
        "fn historicalName(value: int) -> int = value\nprint(historicalName(7))\n",
    )
    .expect("write ordinary script");
    let _ = flag_stdout(&source, "--llvm", &["@historicalName("], &["__osp_"]);
    let _ = std::fs::remove_file(source);
}

fn assert_project_main_symbol_is_local(json: &str, source: &Path) {
    let value = serde_json::from_str::<serde_json::Value>(json).expect("valid symbols JSON");
    let main = value.as_array().and_then(|entries| {
        entries.iter().find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("bank::main")
        })
    });
    let expected_line = std::fs::read_to_string(source)
        .expect("read entry source")
        .lines()
        .position(|line| line.trim_start().starts_with("main () ="))
        .and_then(|line| u64::try_from(line.saturating_add(1)).ok());
    let line = main
        .and_then(|entry| entry.get("line"))
        .and_then(serde_json::Value::as_u64);
    assert_eq!(line, expected_line);
    let path = main
        .and_then(|entry| entry.get("path"))
        .and_then(serde_json::Value::as_str);
    let canonical = osprey_project::paths::canonical(source).ok();
    assert_eq!(path, canonical.as_deref().and_then(Path::to_str));
}

#[test]
fn bank_project_runs_and_builds_byte_exact_against_expectedoutput() {
    let _serialized = demo_run_lock();

    let project = fixture();
    let output = checked(run(&[arg(&project), "--run".to_string()]));
    assert_eq!(output.stdout, expected_output());

    let project = copy_fixture("build");
    let artifact = project.join("talon-bank");
    let _ = checked(run_in(&project, &["build", "--quiet"]));
    assert!(artifact.is_file(), "missing {}", artifact.display());

    let executed = Command::new(&artifact).output().expect("run built project");
    assert!(executed.status.success());
    assert_eq!(String::from_utf8_lossy(&executed.stdout), expected_output());
    let _ = std::fs::remove_dir_all(project);
}

// [MODULES-SIGNATURE], [MODULES-OPAQUE-TYPES]: exercise the shipped bank source.
#[test]
fn bank_money_signature_is_opaque_from_both_flavors() -> TestResult {
    for (extension, source) in [
        (
            "osp",
            "fn main() = { let amount = Money::fromCents(420005)\n print(\"${Money::show(amount)}|${wrapAdd(Money::toCents(amount), 1)}|${Money::positive(amount)}|${Money::show(Money::fromCents(-9223372036854775808))}\") }\n",
        ),
        (
            "ospml",
            "main () =\n    amount = Money::fromCents 420005\n    print \"${Money::show amount}|${wrapAdd (Money::toCents amount) 1}|${Money::positive amount}|${Money::show (Money::fromCents (-9223372036854775808))}\"\n",
        ),
    ] {
        let project = money_client_fixture(extension, source)?;
        let expected = "$4,200.05|420006|true|-$92,233,720,368,547,758.08\n";
        for stderr in run_under_each_backend(&project, expected) {
            assert!(!stderr.contains("redundant"), "{stderr}");
        }
        std::fs::remove_dir_all(project)?;
    }
    for &(extension, source, error) in MONEY_REJECTIONS {
        let project = money_client_fixture(extension, source)?;
        let output = run(&[arg(&project), "--check".into(), "--quiet".into()]);
        assert_eq!(output.code, Some(1), "{source}: {}", output.stderr);
        assert!(output.stderr.contains(error), "{source}: {}", output.stderr);
        std::fs::remove_dir_all(project)?;
    }
    Ok(())
}

const MONEY_REJECTIONS: &[(&str, &str, &str)] = &[
    (
        "osp",
        "fn main() = Money::Amount { cents: 1 }\n",
        "cannot be constructed outside module",
    ),
    (
        "ospml",
        "main () = Money::Amount(cents = 1)\n",
        "cannot be constructed outside module",
    ),
    (
        "osp",
        "fn main() = { let amount = Money::fromCents(1)\n amount.cents }\n",
        "is hidden outside module",
    ),
    (
        "ospml",
        "main () =\n    amount = Money::fromCents 1\n    amount.cents\n",
        "is hidden outside module",
    ),
    ("osp", "fn main() = Money::show(1)\n", "type mismatch"),
    ("ospml", "main () = Money::show 1\n", "type mismatch"),
];

fn money_client_fixture(
    extension: &str,
    source: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let project = temp_project(&format!("money_signature_{extension}_{}", source.len())).0;
    std::fs::create_dir_all(project.join("src"))?;
    let _ = std::fs::copy(
        fixture().join("domain/money.ospml"),
        project.join("src/money.ospml"),
    )?;
    std::fs::write(project.join(format!("src/main.{extension}")), source)?;
    std::fs::write(
        project.join("osprey.toml"),
        format!(
            "[project]\nname = \"money-contract\"\nsource_roots = [\"src\"]\ndefault_namespace = \"bank\"\nentry = \"src/main.{extension}\"\n"
        ),
    )?;
    Ok(project)
}

#[test]
fn answers_project_layouts_are_path_spelling_independent() {
    // A single file carrying its own module; the sibling stays unloaded.
    let (directory, namespace) = temp_project("single_module");
    std::fs::create_dir_all(&directory).expect("create single-source directory");
    let source = directory.join("main.osp");
    std::fs::write(
        &source,
        format!(
            "{ANSWERS_MODULE}import {namespace}::Answers::{{answer}}\nfn main() = print(\"answer=${{answer()}}\")\n"
        ),
    )
    .expect("write single module source");
    std::fs::write(directory.join("broken.osp"), "fn = = =\n").expect("write ignored sibling");
    let output = checked(run(&[arg(&source), "--run".to_string()]));
    assert_eq!(output.stdout, "answer=42\n");
    let _ = checked(run_in(&directory, &["main.osp", "--check", "--quiet"]));
    let _ = std::fs::remove_dir_all(directory);

    // A manifest-free project under `src`, addressed absolutely and relatively.
    let (directory, namespace) = temp_project("manifest_free");
    let sources = directory.join("src");
    std::fs::create_dir_all(&sources).expect("create manifest-free source root");
    std::fs::write(sources.join("values.osp"), ANSWERS_MODULE).expect("write module source");
    std::fs::write(
        sources.join("main.osp"),
        format!("import {namespace}::Answers::{{answer}}\nfn main() = print(\"${{answer()}}\")\n"),
    )
    .expect("write entry source");
    check_passes(&directory);
    let _ = checked(run_in(&directory, &[".", "--check", "--quiet"]));
    let _ = std::fs::remove_dir_all(directory);
}

#[path = "cases/project_diagnostics.rs"]
mod project_diagnostics;

/// [FLAVOR-TS-INTEROP]: a TypeScript-flavor module imports Default, ML and
/// Python-flavor modules of its namespace, and the project prints the same
/// line under both memory backends.
#[test]
fn a_project_imports_between_all_four_flavors() {
    let project = repo_root().join("src/osprey/examples/projects/four_flavors");
    let expected =
        std::fs::read_to_string(project.join("expectedoutput")).expect("expected output");
    for memory in ["--memory=gc", "--memory=arc"] {
        let output = checked(run(&[
            arg(&project),
            "--run".into(),
            memory.into(),
            "--quiet".into(),
        ]));
        assert_eq!(output.stdout, expected, "{memory}: {}", output.stderr);
    }
}

/// Runs the built daw renderer with `args`, from the repository root.
fn daw(binary: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(binary);
    let _ = command.current_dir(repo_root()).args(args);
    finish(&mut command)
}

/// Asserts the renderer refused a request: a non-zero status and the reason on its output.
fn refused(output: &Output, reason: &str) {
    assert_eq!(
        output.code,
        Some(1),
        "stdout={} stderr={}",
        output.stdout,
        output.stderr
    );
    assert!(output.stdout.contains(reason), "stdout={}", output.stdout);
    assert!(
        !output.stdout.contains("daw: wrote"),
        "stdout={}",
        output.stdout
    );
}

/// [ENTRY-EXIT-STATUS] [SYNTH-ARRANGEMENT-PLAYER] A render that fails says why and exits 1, so a
/// script or the app never mistakes a refused project for a written song; a written song exits 0.
#[test]
fn the_daw_renderer_exits_one_on_every_failure_and_zero_when_it_writes() -> TestResult {
    let (root, _) = temp_project("daw_render");
    std::fs::create_dir_all(&root)?;
    let binary = root.join("daw");
    let built = run(&[
        "build".into(),
        "src/osprey/examples/daw/render".into(),
        "--memory=arc".into(),
        "--no-warnings".into(),
        "-o".into(),
        arg(&binary),
    ]);
    assert_eq!(built.code, Some(0), "stderr={}", built.stderr);
    let out = root.join("out");
    let folder = arg(&out);

    refused(&daw(&binary, &[]), "usage: daw");
    refused(&daw(&binary, &["only-a-project.json"]), "usage: daw");
    refused(
        &daw(
            &binary,
            &[
                "src/osprey/examples/daw/tracks/elise-motif.json",
                &folder,
                "x",
                "4",
            ],
        ),
        "usage: daw",
    );
    refused(
        &daw(
            &binary,
            &[
                "src/osprey/examples/daw/tracks/elise-motif.json",
                &folder,
                "0",
                "-4",
            ],
        ),
        "usage: daw",
    );
    refused(
        &daw(&binary, &["/nonexistent/osprey-song.json", &folder]),
        "daw: ",
    );

    let old = root.join("old.json");
    std::fs::write(
        &old,
        r#"{"title":"Old","tempo":120,"sampleRate":48000,"tracks":[{"name":"glow","instrument":{"kind":"operator"},"gain":0.5,"pan":0,"reverb":0,"notes":[]}]}"#,
    )?;
    refused(&daw(&binary, &[&arg(&old), &folder]), "daw: ");
    let broken = root.join("broken.json");
    std::fs::write(&broken, "{ not json")?;
    refused(&daw(&binary, &[&arg(&broken), &folder]), "daw: ");
    // A valid project reaches the synth before its out-of-range brightness is refused.
    let bell = root.join("bell.json");
    let project = serde_json::json!({
        "title": "Bell", "tempo": 120, "sampleRate": 48000, "buses": [],
        "tracks": [{"name": "bell", "gain": 1, "pan": 0, "bus": "", "sends": [], "positions": [],
            "instrument": {"kind": "BellInstrument", "decay": 0.05, "brightness": 2},
            "notes": [{"pitch": 60, "start": 0, "length": 0.125, "velocity": 0.8}]}]
    });
    std::fs::write(&bell, serde_json::to_vec(&project)?)?;
    let detail = "bell: Invalid modal rate, frequency, decay, brightness, strike or mode count.";
    refused(&daw(&binary, &[&arg(&bell), &folder]), detail);
    let live = daw(&binary, &[&arg(&bell), &folder, "0", "0.125", "live"]);
    assert_eq!(live.code, Some(1), "stderr={}", live.stderr);
    assert!(
        live.stdout.is_empty(),
        "a refused first block emitted audio"
    );
    assert_eq!(std::fs::read_to_string(out.join("fault.txt"))?, detail);
    assert!(
        !out.join("song.wav").exists(),
        "a refused render wrote a song"
    );

    assert_rendered_excerpt(&binary, &folder)?;
    let mut playable = project;
    *playable
        .pointer_mut("/tracks/0/instrument/brightness")
        .ok_or("missing bell brightness")? = 0.5.into();
    std::fs::write(&bell, serde_json::to_vec(&playable)?)?;
    assert_repairs_truncated_note(&binary, &bell, &out)?;
    assert_device_drain(&binary, &bell, &out)?;
    assert_late_note_pickup(&binary, &root.join("late-notes"))?;
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

/// Device playback ends with an acknowledged drain after its last nonempty WAV packet.
fn assert_device_drain(binary: &Path, song: &Path, folder: &Path) -> TestResult {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    let controls = folder.join("controls");
    std::fs::write(&controls, "0")?;
    let mut child = Command::new(binary)
        .current_dir(repo_root())
        .args([
            &arg(song),
            &arg(folder),
            "0",
            "0.125",
            "live",
            &arg(&controls),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut receipt = child.stdin.take().ok_or("missing receipt pipe")?;
    let mut packets = 0;
    let mut drained = false;
    for line in BufReader::new(child.stdout.take().ok_or("missing audio pipe")?).lines() {
        let line = line?;
        assert!(!drained, "data after the final drain");
        drained = line == "!drain";
        if !drained {
            assert!(line.starts_with("52494646"), "expected a RIFF packet");
            packets += 1;
        }
        writeln!(receipt, "next")?;
    }
    assert!(child.wait()?.success());
    assert!(packets > 0 && drained, "playback ended without its drain");
    Ok(())
}

const CODEX_SCORE: &str = "src/osprey/examples/daw/tracks/codex-version.json";

/// Hold the actual 160-frame producer at each device receipt so cache publication is deterministic.
fn opening_packets(
    binary: &Path,
    folder: &Path,
    publish: bool,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    let clock = folder.join("controls");
    std::fs::write(&clock, "0")?;
    let mut child = Command::new(binary)
        .current_dir(repo_root())
        .args([CODEX_SCORE, &arg(folder), "0", "2", "live", &arg(&clock)])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let result = (|| {
        let mut lines = BufReader::new(child.stdout.take().ok_or("missing audio pipe")?).lines();
        let mut receipt = child.stdin.take().ok_or("missing receipt pipe")?;
        let mut packets = Vec::new();
        for number in 1..=64 {
            packets.push(lines.next().ok_or("audio ended early")??);
            if publish && number == 8 {
                std::fs::rename(folder.join("staged"), folder.join("notes"))?;
            }
            writeln!(receipt, "next")?;
        }
        Ok(packets)
    })();
    let _ = child.kill();
    let _status = child.wait()?;
    result
}

/// [DAW-TRANSPORT-LIVE-SPEED] Prepared audio joins a sounding note at its current sample.
fn assert_late_note_pickup(binary: &Path, folder: &Path) -> TestResult {
    std::fs::create_dir_all(folder)?;
    let made = daw(
        binary,
        &[CODEX_SCORE, &arg(folder), "0", "2", "warm", "0", "1"],
    );
    assert_eq!(made.code, Some(0), "{}", made.stderr);
    let warm = opening_packets(binary, folder, false)?;
    std::fs::rename(folder.join("notes"), folder.join("staged"))?;
    let cold = opening_packets(binary, folder, false)?;
    let joined = opening_packets(binary, folder, true)?;
    assert_eq!(
        cold.get(..8).ok_or("missing cold audio packets")?,
        joined.get(..8).ok_or("missing joined audio packets")?,
        "publishing later changed the attack"
    );
    assert_ne!(
        cold.get(27..).ok_or("missing cold audio packets")?,
        warm.get(27..).ok_or("missing warm audio packets")?,
        "the fixture must distinguish live doubles from cached F32"
    );
    assert_eq!(
        joined.get(27..).ok_or("missing joined audio packets")?,
        warm.get(27..).ok_or("missing warm audio packets")?,
        "the prepared note restarted or missed its handoff"
    );
    Ok(())
}

fn assert_rendered_excerpt(binary: &Path, folder: &str) -> TestResult {
    let written = daw(
        binary,
        &[
            "src/osprey/examples/daw/tracks/elise-motif.json",
            folder,
            "0",
            "1",
        ],
    );
    assert_eq!(
        written.code,
        Some(0),
        "stdout={} stderr={}",
        written.stdout,
        written.stderr
    );
    assert!(
        written.stdout.contains("daw: wrote"),
        "stdout={}",
        written.stdout
    );
    let path = written
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("daw: wrote "))
        .expect("the renderer names what it wrote");
    assert!(
        std::fs::metadata(path.trim())?.len() > 44,
        "the written song has audio: {path}"
    );
    Ok(())
}

// [DAW-TRANSPORT-LIVE-SPEED] An interrupted writer must not force synthesis at every subsequent Play.
fn assert_repairs_truncated_note(binary: &Path, song: &Path, out: &Path) -> TestResult {
    let song = arg(song);
    let folder = arg(out);
    let warm = || daw(binary, &[&song, &folder, "0", "1", "warm", "0", "1"]);
    let made = warm();
    assert_eq!(made.code, Some(0), "{}", made.stderr);
    let dir = std::fs::read_dir(out.join("notes"))?
        .next()
        .ok_or("no note directory")??
        .path();
    let note = std::fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "st"))
        .ok_or("no rendered note")?;
    let original = std::fs::read(&note)?;
    assert!(original.len() > 128);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&note)?
        .set_len(128)?;
    let repaired = warm();
    assert_eq!(repaired.code, Some(0), "{}", repaired.stderr);
    let restored = std::fs::read(&note)?;
    assert_eq!(restored.len(), original.len(), "a truncated note was kept");
    assert_eq!(restored, original);
    let label = dir.join("key.txt");
    std::fs::remove_file(&label)?;
    std::fs::create_dir(&label)?;
    refused(&warm(), "key.txt");
    assert_eq!(std::fs::read(note)?, original);
    Ok(())
}
