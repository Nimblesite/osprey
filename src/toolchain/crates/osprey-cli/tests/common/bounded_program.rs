//! One Osprey program run from source under the program budget; see `bounded.rs`.
use crate::bounded::{run_command, Bounded, Budget};
use std::io;
use std::process::Command;
use std::time::Duration;

/// The budget for one test program.
const PROGRAM_BUDGET: Budget = Budget {
    memory_mb: 1024,
    deadline: Duration::from_secs(120),
};

/// Runs `source` as `<name>.osp` with `args` under [`PROGRAM_BUDGET`].
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
