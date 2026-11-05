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
