//! A test program's runaway costs the whole machine, so each run sits in its own
//! process group and the group dies at its memory or time budget. Every run
//! records its peak group RSS and elapsed time, so a killed or over-cap run says
//! what it was doing (plan 0048).
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The memory and time a run may use before its process group is killed.
#[derive(Clone, Copy)]
pub(crate) struct Budget {
    pub(crate) memory_mb: u64,
    pub(crate) deadline: Duration,
}

/// The budget for one test program.
pub(crate) const PROGRAM_BUDGET: Budget = Budget {
    memory_mb: 1024,
    deadline: Duration::from_secs(120),
};

const POLL: Duration = Duration::from_millis(50);

pub(crate) struct Bounded {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) code: Option<i32>,
    /// Why the group was killed, when it was.
    pub(crate) stopped: Option<String>,
    #[allow(dead_code)] // Read by the package matrix through `summary`.
    pub(crate) peak_mb: u64,
    #[allow(dead_code)] // Read by the package matrix through `summary`.
    pub(crate) elapsed: Duration,
}

impl Bounded {
    /// One line naming the run's exit, peak memory and time.
    #[allow(dead_code)] // The program suites assert on fields, not this line.
    pub(crate) fn summary(&self) -> String {
        let exit = self
            .code
            .map_or_else(|| "killed by a signal".to_string(), |c| format!("exit {c}"));
        let stop = self
            .stopped
            .as_deref()
            .map_or_else(String::new, |why| format!(", {why}"));
        format!(
            "{exit}, peak {} MiB, {:.1?}{stop}",
            self.peak_mb, self.elapsed
        )
    }
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

/// Runs `command` in its own process group under `budget`. Both streams go to
/// files, so a chatty program can't fill a pipe nobody drains while it is watched.
pub(crate) fn run_command(mut command: Command, stem: &str, budget: Budget) -> io::Result<Bounded> {
    let out_path = std::env::temp_dir().join(format!("{stem}.stdout"));
    let err_path = std::env::temp_dir().join(format!("{stem}.stderr"));
    let mut child = command
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&out_path)?)
        .stderr(std::fs::File::create(&err_path)?)
        .process_group(0)
        .spawn()?;
    let started = Instant::now();
    let mut peak_mb = 0;
    let (status, stopped) = loop {
        if let Some(status) = child.try_wait()? {
            break (status, None);
        }
        let rss = group_rss_mb(child.id())?;
        peak_mb = peak_mb.max(rss);
        if rss > budget.memory_mb || started.elapsed() > budget.deadline {
            let group = format!("-{}", child.id());
            let _ = Command::new("kill").args(["-9", &group]).status()?;
            let why = format!(
                "stopped at {rss} MiB after {:.1?} (budget {} MiB, {:?})",
                started.elapsed(),
                budget.memory_mb,
                budget.deadline
            );
            break (child.wait()?, Some(why));
        }
        std::thread::sleep(POLL);
    };
    let stdout = std::fs::read_to_string(&out_path)?;
    let stderr = std::fs::read_to_string(&err_path)?;
    std::fs::remove_file(&out_path)?;
    std::fs::remove_file(&err_path)?;
    Ok(Bounded {
        stdout,
        stderr,
        code: status.code(),
        stopped,
        peak_mb,
        elapsed: started.elapsed(),
    })
}

/// Runs `source` as `<name>.osp` with `args` under [`PROGRAM_BUDGET`].
#[allow(dead_code)] // Used by the program suites; the package matrix uses only `run_command`.
pub(crate) fn run_within_budget(
    name: &str,
    source: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> io::Result<Bounded> {
    let stem = format!("osprey_bounded_{}_{name}", std::process::id());
    let path = std::env::temp_dir().join(format!("{stem}.osp"));
    std::fs::write(&path, source)?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = command.arg(&path).args(args).envs(envs.iter().copied());
    let run = run_command(command, &stem, PROGRAM_BUDGET);
    std::fs::remove_file(&path)?;
    run
}
