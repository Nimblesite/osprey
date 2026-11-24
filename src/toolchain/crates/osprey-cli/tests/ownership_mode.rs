//! `--memory=ownership` [MEM-OWNERSHIP-STATIC]
//! (docs/specs/compiler/0018-MemoryManagement.md): the compiler proves one
//! owner for every managed value or refuses the program. An accepted program
//! prints what it prints under the collector and leaves nothing allocated; a
//! refused one names the value that needs a second owner and where.

#[path = "common/repo_root.rs"]
mod repo_root;
use repo_root::repo_root;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The exit report of the ownership runtime, which is not ARC's: it keeps no
/// reference counts and frees each value at its one release.
const CLEAN_EXIT: &str = "[osp-ownership] exit: 0 live objects, ";
/// No block was handed a second reference at run time.
const NONE_SHARED: &str = "[osp-ownership]   shared: 0 blocks kept to exit";
/// Programs every value of which has one provable owner.
const ACCEPTED: &str = "src/osprey/tests/regressions/ownership";
/// Programs the collector runs and ownership refuses, each beside the exact
/// diagnostics it must get.
const REFUSED: &str = "src/osprey/examples/failscompilation/ownership";

fn osprey(program: &Path, arguments: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(program)
        .args(arguments)
        .env("OSPREY_OWNERSHIP_DEBUG", "1")
        // Freed memory is overwritten, so reading a freed value shows.
        .env("MallocScribble", "1")
        .output()
}

/// Every `.osp` program directly inside the repository folder `folder`, sorted.
fn programs(folder: &str) -> std::io::Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(repo_root().join(folder))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "osp"))
        .collect();
    found.sort();
    Ok(found)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `program` prints its golden under the collector and under ownership, and
/// the ownership run ends with no block left allocated.
fn assert_accepted(program: &Path) -> std::io::Result<()> {
    let name = program.display();
    let golden = std::fs::read_to_string(format!("{name}.expectedoutput"))?;
    let collected = osprey(program, &["--run", "--memory=gc"])?;
    let owned = osprey(program, &["--run", "--memory=ownership"])?;
    let stderr = text(&owned.stderr);
    assert!(collected.status.success(), "{name} under gc");
    assert!(owned.status.success(), "{name}: {stderr}");
    assert_eq!(text(&collected.stdout), golden, "{name} under gc");
    assert_eq!(text(&owned.stdout), golden, "{name} under ownership");
    assert!(stderr.contains(CLEAN_EXIT), "{name} left blocks: {stderr}");
    assert!(
        stderr.contains(NONE_SHARED),
        "{name} shared blocks: {stderr}"
    );
    assert!(
        !stderr.contains("[osp-arc]"),
        "{name} linked the ARC runtime: {stderr}"
    );
    Ok(())
}

/// The collector runs `program`; ownership refuses it with exactly the
/// diagnostics in its golden and builds nothing.
fn assert_refused(program: &Path) -> std::io::Result<()> {
    let name = program.display();
    let golden = std::fs::read_to_string(format!("{name}.expectedoutput"))?;
    let collected = osprey(program, &["--run", "--memory=gc"])?;
    assert!(collected.status.success(), "{name} under gc");
    let owned = osprey(program, &["--run", "--memory=ownership"])?;
    assert!(!owned.status.success(), "{name} was accepted");
    assert!(owned.stdout.is_empty(), "{name} ran");
    let said = text(&owned.stderr).replacen(&format!("{name}: "), "", 1);
    assert_eq!(said, golden, "{name}");
    assert!(golden.contains("[MEM-OWNERSHIP-STATIC]"), "{name}");
    Ok(())
}

#[test]
fn programs_whose_values_each_have_one_owner_run_and_free_everything() -> std::io::Result<()> {
    let accepted = programs(ACCEPTED)?;
    assert!(!accepted.is_empty(), "no accepted programs in {ACCEPTED}");
    accepted
        .iter()
        .try_for_each(|program| assert_accepted(program))
}

#[test]
fn a_value_that_needs_a_second_owner_is_refused_by_name_and_place() -> std::io::Result<()> {
    let refused = programs(REFUSED)?;
    assert!(
        refused.len() >= 5,
        "refused programs missing from {REFUSED}"
    );
    refused
        .iter()
        .try_for_each(|program| assert_refused(program))
}

#[test]
fn an_unknown_backend_names_ownership_among_the_choices() -> std::io::Result<()> {
    let program = repo_root().join(ACCEPTED).join("single_owner.test.osp");
    let output = osprey(&program, &["--run", "--memory=bogus"])?;
    let stderr = text(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("available: gc, arc, ownership"), "{stderr}");
    Ok(())
}
