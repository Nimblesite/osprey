//! The agent is the largest Osprey application in the tree, and `run.sh` compiles
//! it before opening a window. A checker that stops finishing on it leaves the
//! user with no window and no error, so a run past the budget is a failure here.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Far beyond any healthy type check of about 4,000 lines, in an unoptimized compiler.
const BUDGET: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

fn agent_project(name: &str) -> PathBuf {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../osprey/examples/agent");
    examples.join(name)
}

fn start_check(project: &Path) -> std::io::Result<Child> {
    Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(project)
        .args(["--check", "--quiet"])
        .stdout(Stdio::null())
        .spawn()
}

/// What became of one check when the shared deadline arrived: `None` when it passed.
fn finish(child: &mut Child, deadline: Instant) -> std::io::Result<Option<String>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((!status.success()).then(|| format!("exited with {status}")));
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    child.kill()?;
    let _ = child.wait()?;
    Ok(Some(format!("still checking after {BUDGET:?}")))
}

/// Every project `run.sh` builds must finish type checking.
#[test]
fn every_agent_project_finishes_type_checking_within_the_budget() -> std::io::Result<()> {
    let names = ["", "test", "test-app"];
    let deadline = Instant::now() + BUDGET;
    let mut checks = names
        .map(|name| start_check(&agent_project(name)))
        .into_iter()
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut failed = Vec::new();
    for (name, child) in names.iter().zip(&mut checks) {
        if let Some(reason) = finish(child, deadline)? {
            failed.push(format!("examples/agent/{name}: {reason}"));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
    Ok(())
}

/// How many times longer a program four times the size may take to lower. Time
/// linear in the program gives 4; a table copied at every call gives 16 or more.
const GROWTH_LIMIT: u32 = 10;
const SMALL_PROGRAM: usize = 1000;
const LOWERING_RUNS: usize = 3;

/// `functions` functions that each call one generic helper.
fn generic_callers(functions: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Write;
    let mut source = b"fn same(x) = x\n".to_vec();
    for i in 0..functions {
        writeln!(source, "fn f{i}() = same({i})")?;
    }
    source.extend_from_slice(b"print(\"${f0()}\")\n");
    Ok(source)
}

/// Writes a program of `functions` generic callers and returns its path.
fn callers_program(functions: usize) -> std::io::Result<PathBuf> {
    let source = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("callers{functions}.osp"));
    std::fs::write(&source, generic_callers(functions)?)?;
    Ok(source)
}

/// One lowering of `source` to LLVM IR, timed.
fn lowering_time(source: &Path) -> std::io::Result<Duration> {
    let started = Instant::now();
    let status = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(source)
        .args(["--llvm", "--no-warnings"])
        .stdout(Stdio::null())
        .status()?;
    assert!(status.success(), "{}: {status}", source.display());
    Ok(started.elapsed())
}

/// A generic call specializes the callee's types, never a copy of every table
/// in the program: the build time of a large application is otherwise the
/// square of its size ([DAW-PERF-BUILD], [TYPE-GENERICS-APPLY]).
#[test]
fn lowering_time_grows_with_the_program_and_not_with_its_square() -> std::io::Result<()> {
    let (small_program, large_program) = (
        callers_program(SMALL_PROGRAM)?,
        callers_program(SMALL_PROGRAM * 4)?,
    );
    // Interleaved, so a load spike from a sibling test lands on both sizes,
    // and the fastest of each: the ratio measures the compiler, not the runner.
    let (mut small, mut large) = (Duration::MAX, Duration::MAX);
    for _ in 0..LOWERING_RUNS {
        small = small.min(lowering_time(&small_program)?);
        large = large.min(lowering_time(&large_program)?);
    }
    assert!(
        large < small * GROWTH_LIMIT,
        "{SMALL_PROGRAM} generic callers lowered in {small:?}, four times as many in {large:?}"
    );
    Ok(())
}
