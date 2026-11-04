//! Stamp the commit the compiler is built from into `osprey --version`, so a
//! stale binary is visible ([SWR-VERSION-BUILD-STAMPING]).
//!
//! A release or clean cut names its commit in `OSPREY_COMMIT`; CI's checkout
//! is `GITHUB_SHA`. Anything else is built from a working tree that may hold
//! uncommitted changes, and says so with a `-worktree` suffix.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=OSPREY_COMMIT\ncargo:rerun-if-env-changed=GITHUB_SHA");
    let commit = stamped("OSPREY_COMMIT")
        .or_else(|| stamped("GITHUB_SHA").map(|sha| sha.chars().take(8).collect()))
        .or_else(worktree_head)
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=OSPREY_COMMIT={commit}");
}

fn stamped(variable: &str) -> Option<String> {
    std::env::var(variable)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// The checkout's HEAD, marked as a working-tree build.
fn worktree_head() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()?;
    let head = String::from_utf8(output.stdout).ok()?;
    let head = head.trim();
    if !output.status.success() || head.is_empty() {
        return None;
    }
    if let Ok(git_head) = Command::new("git")
        .args(["rev-parse", "--git-path", "HEAD"])
        .output()
    {
        let path = String::from_utf8_lossy(&git_head.stdout).trim().to_owned();
        println!("cargo:rerun-if-changed={path}");
    }
    Some(format!("{head}-worktree"))
}
