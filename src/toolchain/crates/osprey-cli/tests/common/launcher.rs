use std::process::Command;

#[path = "repo_root.rs"]
mod root;
pub(crate) use root::repo_root;

/// The built compiler, rooted where its runtime archives can be found.
pub(crate) fn osprey() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = command.current_dir(repo_root());
    command
}
