//! Bound diagnostic proof work without a wall-clock timeout.
use std::{
    fmt::Write,
    sync::atomic::{AtomicUsize, Ordering},
};

use osprey_ast::{Program, Stmt};
use osprey_syntax::{parse_program_with_flavor, Flavor};

const SENTINEL: &str = "annotation_cost_sentinel";
const PARALLEL_SENTINEL: &str = "annotation_parallelism_sentinel";
static CALLS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

pub(super) fn record(program: &Program) -> ProofGuard {
    if contains_marker(program, SENTINEL) {
        let _previous = CALLS.fetch_add(1, Ordering::Relaxed);
    }
    let tracked = contains_marker(program, PARALLEL_SENTINEL);
    if tracked {
        let active = ACTIVE.fetch_add(1, Ordering::SeqCst) + 1;
        let _previous = PEAK.fetch_max(active, Ordering::SeqCst);
    }
    ProofGuard(tracked)
}

pub(super) struct ProofGuard(bool);

impl Drop for ProofGuard {
    fn drop(&mut self) {
        if self.0 {
            let _previous = ACTIVE.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

fn contains_marker(program: &Program, marker: &str) -> bool {
    program
        .statements
        .iter()
        .any(|statement| matches!(statement, Stmt::Function { name, .. } if name == marker))
}

fn fixture() -> Result<Program, std::fmt::Error> {
    fixture_with(256, SENTINEL)
}

fn fixture_with(count: usize, marker: &str) -> Result<Program, std::fmt::Error> {
    let greetings = definitions(
        count,
        "greeting",
        "(name: string) -> string = \"hi \" + name",
    )?;
    let source = format!(
        "{}\nfn {marker}() = 0\n{greetings}",
        crate::redundant_window_tests::MIXED
    );
    Ok(parse_program_with_flavor(&source, Flavor::Default).program)
}

fn definitions(count: usize, prefix: &str, signature: &str) -> Result<String, std::fmt::Error> {
    (0..count).try_fold(String::new(), |mut text, index| {
        writeln!(text, "fn {prefix}{index}{signature}")?;
        Ok(text)
    })
}

fn environment_marker_visits(source: &str) -> usize {
    let parsed = parse_program_with_flavor(source, Flavor::Default);
    assert_eq!(parsed.errors, Vec::new());
    crate::env::tests::MARKER_VISITS.with(|visits| visits.set(0));
    let errors = crate::check::check_program(&parsed.program);
    assert_eq!(errors, Vec::new());
    crate::env::tests::MARKER_VISITS.with(std::cell::Cell::get)
}

#[test]
fn closed_functions_do_not_rescan_the_global_environment() -> Result<(), std::fmt::Error> {
    let marker = crate::env::tests::ENVIRONMENT_MARKER;
    let definitions = definitions(64, "closed", "() = 0")?;
    let source = format!("type {marker} = {{ value: int }}\nlet marker = {marker} {{ value: 0 }}\n{definitions}\nfn main() = 0\n");
    assert_eq!(
        environment_marker_visits(&source),
        0,
        "closed functions have no declared binders to quantify"
    );
    let control = source.replace(
        "fn main() = 0",
        "fn main() = wrapAdd(phantom<int>(), phantom<string>())",
    ) + "fn phantom<T>() = 0\n";
    assert!(
        environment_marker_visits(&control) > 0,
        "phantom binders must still use the environment oracle"
    );
    Ok(())
}

#[test]
fn full_program_proofs_have_bounded_parallelism() -> Result<(), std::fmt::Error> {
    let program = fixture_with(64, PARALLEL_SENTINEL)?;
    let baseline = super::published(&program).expect("valid annotation proof fixture");
    let sites = crate::redundant_sites::sites(&program);
    PEAK.store(0, Ordering::SeqCst);
    let removable = super::individually_removable(&program, &baseline, &sites);
    assert!(
        !removable.is_empty(),
        "fixture must exercise successful proofs"
    );
    let peak = PEAK.load(Ordering::SeqCst);
    assert!(peak > 0, "observer must see actual full-program proofs");
    assert!(
        peak <= 4,
        "{peak} simultaneous full-program proofs exceeds the resource ceiling of 4"
    );
    Ok(())
}

#[test]
fn isolated_load_bearing_sites_do_not_force_every_annotation_through_a_full_solve(
) -> Result<(), std::fmt::Error> {
    let program = fixture()?;
    let sites = crate::redundant_sites::sites(&program).len();
    let workers = super::judges().min(sites).max(1);
    CALLS.store(0, Ordering::Relaxed);
    let warnings = super::redundant_annotation_sites_for(&program, |_| true, &|_| false);
    assert!(
        !warnings.is_empty(),
        "fixture must contain removable annotations"
    );
    let calls = CALLS.load(Ordering::Relaxed);
    let budget = sites.div_ceil(workers) + 3 * workers + 64;
    assert!(
        calls <= budget,
        "{calls} full solves for {sites} annotations exceeds {budget}"
    );
    Ok(())
}
