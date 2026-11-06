//! A test program's runaway costs the whole machine, so each run sits in its own
//! process group and the group dies at its memory or time budget.
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MEMORY_BUDGET_MB: u64 = 1024;
const RUN_DEADLINE: Duration = Duration::from_secs(120);

pub(crate) struct Bounded {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) code: Option<i32>,
    /// Why the group was killed, when it was.
    pub(crate) stopped: Option<String>,
}

/// Resident memory of every process in `group`, in MiB.
fn group_rss_mb(group: u32) -> io::Result<u64> {
    let listing = Command::new("ps").args(["-axo", "pgid=,rss="]).output()?;
    let text = String::from_utf8_lossy(&listing.stdout).into_owned();
    let kib: u64 = text
        .lines()
        .filter_map(|line| line.trim().split_once(char::is_whitespace))
        .filter(|row| row.0.parse() == Ok(group))
        .filter_map(|row| row.1.trim().parse::<u64>().ok())
        .sum();
    Ok(kib / 1024)
}

/// Runs `source` as `<name>.osp` with `args`; stderr goes to a file so a chatty
/// program can't fill a pipe nobody drains while it is being watched.
pub(crate) fn run_within_budget(
    name: &str,
    source: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> io::Result<Bounded> {
    let stem = format!("osprey_bounded_{}_{name}", std::process::id());
    let path = std::env::temp_dir().join(format!("{stem}.osp"));
    let err_path = std::env::temp_dir().join(format!("{stem}.stderr"));
    std::fs::write(&path, source)?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(&path)
        .args(args)
        .envs(envs.iter().copied())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&err_path)?)
        .process_group(0)
        .spawn()?;
    let started = Instant::now();
    let stopped = loop {
        if child.try_wait()?.is_some() {
            break None;
        }
        let rss = group_rss_mb(child.id())?;
        if rss > MEMORY_BUDGET_MB || started.elapsed() > RUN_DEADLINE {
            let group = format!("-{}", child.id());
            let _ = Command::new("kill").args(["-9", &group]).status()?;
            break Some(format!(
                "stopped at {rss} MiB after {:?}",
                started.elapsed()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let output = child.wait_with_output()?;
    let stderr = std::fs::read_to_string(&err_path)?;
    std::fs::remove_file(&path)?;
    std::fs::remove_file(&err_path)?;
    Ok(Bounded {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr,
        code: output.status.code(),
        stopped,
    })
}
