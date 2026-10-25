//! Running KLEE [SYMEX-KLEE-INVOCATION] (docs/specs/tooling/SymbolicExecution.md).
//! The engine is KLEE (klee-se.org); every run uses the one fixed command
//! below, and nothing from the user reaches it.

use super::process::wait_bounded;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Cap on one solver query [SYMEX-KLEE-INVOCATION]: `--max-time` is checked
/// between instructions, so an unbounded query could outlive the budget.
const SOLVER_TIME: &str = "--max-solver-time=30s";
/// Loading, linking and optimising a large module happens before the budget
/// starts; this much extra is allowed before the engine is killed.
const ENGINE_GRACE: Duration = Duration::from_secs(120);

/// What KLEE's `info` file says about the run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Exploration {
    pub(crate) paths: u64,
    pub(crate) completed: u64,
    pub(crate) instructions: u64,
    pub(crate) elapsed: String,
}

pub(crate) fn arguments(
    runtime: &Path,
    out_dir: &Path,
    budget: (u64, u64),
    module: &Path,
) -> Vec<String> {
    let (max_time, max_memory) = budget;
    [
        "--libc=klee",
        "--ubsan-runtime",
        "--external-calls=none",
        "--external-call-warnings=all",
        &format!("-link-llvm-lib={}", runtime.display()),
        "--optimize",
        "--kdalloc",
        "--rng-initial-seed=5489",
        "--search=dfs",
        "--check-div-zero",
        "--check-overshift",
        "--emit-all-errors",
        SOLVER_TIME,
        &format!("--max-time={max_time}s"),
        "--watchdog",
        &format!("--max-memory={max_memory}"),
        &format!("--output-dir={}", out_dir.display()),
        &module.display().to_string(),
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

/// How the engine ended: on its own, or killed by its own watchdog once the
/// budget was spent while a solver query was still running. A halted run has
/// still written the states it dumped on the way out, so it is read like any
/// other and reported as cut at the budget, not as a tool failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ending {
    Finished,
    Halted,
}

/// The reason a halted run's report carries under "incomplete".
pub(crate) const HALTED_REASON: &str =
    "KLEE's watchdog killed the run at the budget while a solver query was still running";

/// KLEE exits 0 whether or not it found errors; a non-zero exit is the engine
/// itself failing, reported with what it said, unless its own watchdog ended
/// the run at the budget. Its stderr goes to `stderr_path` (a pipe could fill
/// and stall it), and the engine is killed if it outlives the budget plus the
/// loading grace.
pub(crate) fn run(
    klee: &Path,
    arguments: &[String],
    stderr_path: &Path,
    max_time: u64,
) -> Result<Ending, String> {
    let stderr_file = std::fs::File::create(stderr_path)
        .map_err(|error| format!("cannot create {}: {error}", stderr_path.display()))?;
    let mut child = Command::new(klee)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr_file)
        .spawn()
        .map_err(|error| format!("could not run {}: {error}", klee.display()))?;
    let limit = Duration::from_secs(max_time).saturating_add(ENGINE_GRACE);
    let status = wait_bounded(&mut child, limit)?.ok_or_else(|| {
        format!(
            "klee did not finish within {}s of its {max_time}s budget",
            limit.as_secs()
        )
    })?;
    let stderr = std::fs::read_to_string(stderr_path).unwrap_or_default();
    let crashed = stderr.contains("llvm::sys::PrintStackTrace");
    if status.success() && !crashed {
        return Ok(Ending::Finished);
    }
    if halted_by_watchdog(&stderr) && !crashed {
        return Ok(Ending::Halted);
    }
    let reason = if crashed {
        "interpreter crashed".to_string()
    } else {
        status.to_string()
    };
    Err(format!(
        "klee failed ({reason}):\n{}",
        excerpt(&stderr).join("\n")
    ))
}

/// KLEE's `--watchdog` reports the kill it is about to make with this line.
pub(crate) fn halted_by_watchdog(stderr: &str) -> bool {
    stderr.contains("WATCHDOG: time expired")
}

/// What the engine said, head and tail: LLVM prints a fatal error before its
/// stack trace, so the last lines alone would show only frames.
pub(crate) fn excerpt(stderr: &str) -> Vec<&str> {
    let lines: Vec<&str> = stderr
        .lines()
        .filter(|line| !is_noise(line) && !line.trim().is_empty())
        .collect();
    if lines.len() <= 18 {
        return lines;
    }
    let mut shown: Vec<&str> = lines.iter().take(6).copied().collect();
    shown.push("...");
    shown.extend(tail(stderr, 12));
    shown
}

/// The last `count` lines that are not target-mismatch noise: the module and
/// runtime are x86-64 Linux bitcode on purpose, whatever the host.
pub(crate) fn tail(stderr: &str, count: usize) -> Vec<&str> {
    let lines: Vec<&str> = stderr.lines().filter(|line| !is_noise(line)).collect();
    let skip = lines.len().saturating_sub(count);
    lines.into_iter().skip(skip).collect()
}

pub(crate) fn is_noise(line: &str) -> bool {
    [
        "unable to select target",
        "target triples do not match",
        "different data layouts",
        "Linking two modules",
    ]
    .iter()
    .any(|noise| line.contains(noise))
}

pub(crate) fn exploration(info: &str) -> Exploration {
    let count = |key: &str| {
        info.lines()
            .find_map(|line| {
                line.strip_prefix("KLEE: done: ")?
                    .strip_prefix(key)?
                    .strip_prefix(" = ")
            })
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0)
    };
    Exploration {
        paths: count("explored paths"),
        completed: count("completed paths"),
        instructions: count("total instructions"),
        elapsed: info
            .lines()
            .find_map(|line| line.strip_prefix("Elapsed: "))
            .unwrap_or("")
            .trim()
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_is_the_fixed_spec_invocation() {
        let args = arguments(
            Path::new("rt.bc"),
            Path::new("out"),
            (7, 4000),
            Path::new("p.ll"),
        );
        assert!(args.contains(&"--max-memory=4000".to_string()));
        assert_eq!(args.first().map(String::as_str), Some("--libc=klee"));
        assert!(args.contains(&"-link-llvm-lib=rt.bc".to_string()));
        assert!(args.contains(&"--max-time=7s".to_string()));
        assert!(args.contains(&"--max-solver-time=30s".to_string()));
        assert!(args.contains(&"--output-dir=out".to_string()));
        assert!(
            args.contains(&"--kdalloc".to_string())
                && args.contains(&"--emit-all-errors".to_string())
        );
        assert_eq!(args.last().map(String::as_str), Some("p.ll"));
    }

    #[test]
    fn depth_first_search_finishes_each_symbolic_path_before_moving_on() {
        let args = arguments(
            Path::new("rt.bc"),
            Path::new("out"),
            (30, 2000),
            Path::new("p.ll"),
        );
        assert!(args.contains(&"--search=dfs".to_string()));
    }

    #[test]
    fn info_lines_become_exploration_counts() {
        let info = "Elapsed: 00:01:00\nKLEE: done: explored paths = 488\nKLEE: done: total instructions = 650279\nKLEE: done: completed paths = 106\nKLEE: done: partially completed paths = 382\n";
        let exploration = exploration(info);
        assert_eq!(
            exploration,
            Exploration {
                paths: 488,
                completed: 106,
                instructions: 650_279,
                elapsed: "00:01:00".into()
            }
        );
        assert_eq!(super::exploration(""), Exploration::default());
    }

    #[test]
    fn target_noise_is_dropped_from_the_tail() {
        let stderr = "KLEE: WARNING: unable to select target\nKLEE: ERROR: real problem\nLinking two modules of different data layouts\nKLEE: ERROR: last\n";
        assert_eq!(tail(stderr, 1), vec!["KLEE: ERROR: last"]);
        assert_eq!(
            tail(stderr, 5),
            vec!["KLEE: ERROR: real problem", "KLEE: ERROR: last"]
        );
    }

    fn stderr_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "osprey-hunt-engine-{name}-{}.stderr",
            std::process::id()
        ))
    }

    #[test]
    fn a_missing_engine_is_an_error_naming_it() {
        let path = stderr_path("missing");
        let error = run(Path::new("/nonexistent/klee"), &[], &path, 1).expect_err("missing klee");
        crate::testkit::shows(&error, &["could not run /nonexistent/klee"]);
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn a_crashed_interpreter_is_not_hidden_by_its_watchdog_status() {
        let path = stderr_path("watchdog-crash");
        for status in [0, 1, 137] {
            let script = format!(
                "printf '%s\\n' 'KLEE: WARNING: KLEE: WATCHDOG: time expired' ' #0 llvm::sys::PrintStackTrace(llvm::raw_ostream&, int)' >&2; exit {status}"
            );
            let result = run(Path::new("/bin/sh"), &["-c".into(), script], &path, 1);
            assert!(
                result.is_err(),
                "interpreter crash with watchdog status {status}: {result:?}"
            );
            assert!(result.expect_err("crash").contains("PrintStackTrace"));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_engine_reports_its_last_words() {
        let path = stderr_path("failing");
        let error = run(
            Path::new("/bin/sh"),
            &["-c".into(), "echo bad >&2; exit 3".into()],
            &path,
            1,
        )
        .expect_err("failure");
        assert!(
            error.contains("klee failed") && error.contains("bad"),
            "{error}"
        );
        assert!(run(
            Path::new("/bin/sh"),
            &["-c".into(), "exit 0".into()],
            &path,
            1
        )
        .is_ok());
        let _ = std::fs::remove_file(&path);
    }
}
