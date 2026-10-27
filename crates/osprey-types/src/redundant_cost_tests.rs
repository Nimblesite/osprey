//! Bound diagnostic proof work without a wall-clock timeout.
use std::{
    fmt::Write,
    sync::atomic::{AtomicUsize, Ordering},
};

use osprey_ast::{Program, Stmt};
use osprey_syntax::{parse_program_with_flavor, Flavor};

const SENTINEL: &str = "annotation_cost_sentinel";
const PARALLEL_SENTINEL: &str = "annotation_parallelism_sentinel";
const MEMO_SENTINEL: &str = "annotation_memo_sentinel";
const FLIGHT_SENTINEL: &str = "annotation_flight_sentinel";
const DENSE_SENTINEL: &str = "annotation_dense_cost_sentinel";
static CALLS: AtomicUsize = AtomicUsize::new(0);
static MEMO_CALLS: AtomicUsize = AtomicUsize::new(0);
static FLIGHT_CALLS: AtomicUsize = AtomicUsize::new(0);
static DENSE_CALLS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
type CostExample = (String, usize, Vec<usize>);

pub(super) fn record(program: &Program) -> ProofGuard {
    count_calls(program);
    let tracked = contains_marker(program, PARALLEL_SENTINEL);
    if tracked {
        let active = ACTIVE.fetch_add(1, Ordering::SeqCst) + 1;
        let _previous = PEAK.fetch_max(active, Ordering::SeqCst);
    }
    ProofGuard(tracked)
}

fn count_calls(program: &Program) {
    for (marker, calls) in [
        (SENTINEL, &CALLS),
        (MEMO_SENTINEL, &MEMO_CALLS),
        (FLIGHT_SENTINEL, &FLIGHT_CALLS),
        (DENSE_SENTINEL, &DENSE_CALLS),
    ] {
        if contains_marker(program, marker) {
            let _previous = calls.fetch_add(1, Ordering::Relaxed);
        }
    }
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

#[test]
fn mutually_pinning_annotations_solve_each_candidate_once() -> Result<(), String> {
    let source = format!("fn {MEMO_SENTINEL}() = 0\nfn pin(x: int) -> int = x\n");
    let program = program(&source);
    let written = crate::redundant_sites::sites(&program);
    assert_eq!(written.len(), 2);
    MEMO_CALLS.store(0, Ordering::Relaxed);
    let baseline = super::published(&program).ok_or("fixture must typecheck")?;
    let removable = super::accumulate(&program, &baseline, &written, 2);
    assert_eq!(removable.len(), 1, "joint removal changes the type");
    let calls = MEMO_CALLS.load(Ordering::Relaxed);
    assert_eq!(calls, 4, "distinct programs need one full proof each");
    Ok(())
}

fn program(source: &str) -> Program {
    let parsed = parse_program_with_flavor(source, Flavor::Default);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    parsed.program
}

#[test]
fn dense_mutually_pinning_sites_bound_full_typechecker_work() -> Result<(), String> {
    for (source, site_count, expected) in cost_examples().map_err(|error| error.to_string())? {
        let program = program(&format!("fn {DENSE_SENTINEL}() = 0\n{source}"));
        assert_eq!(crate::redundant_sites::sites(&program).len(), site_count);
        assert_cost_windows(&program, &expected)?;
    }
    Ok(())
}

fn cost_examples() -> Result<[CostExample; 5], std::fmt::Error> {
    let greetings = definitions(64, "greet", "(name: string) -> string = \"hi \" + name")?;
    let pins = definitions(32, "pin", "(x: int) -> int = x")?;
    let necessary = definitions(32, "required", "(x: int) = 0")?;
    let sparse = format!("fn pin(x: int) -> int = x\n{greetings}");
    let isolated = format!("fn required(x: int) = 0\n{greetings}");
    Ok([
        (pins, 64, (0..64).step_by(2).collect()),
        (necessary, 32, Vec::new()),
        (sparse, 130, (0..130).filter(|i| *i != 1).collect()),
        (isolated, 129, (1..129).collect()),
        (
            periodic_definitions()?,
            48,
            (0..48).filter(|i| i % 3 != 0).collect(),
        ),
    ])
}

fn periodic_definitions() -> Result<String, std::fmt::Error> {
    (0..16).try_fold(String::new(), |mut text, index| {
        writeln!(text, "fn required{index}(x: int) = 0\nfn greet{index}(name: string) -> string = \"hi \" + name")?;
        Ok(text)
    })
}

fn assert_cost_windows(program: &Program, expected: &[usize]) -> Result<(), String> {
    let baseline = super::published(program).ok_or("fixture must typecheck")?;
    let written = crate::redundant_sites::sites(program);
    for window in [1, 2, 4, 8, 14, 32] {
        assert_window_cost(program, &baseline, &written, expected, window);
    }
    assert_default_cost(program, &written, expected);
    Ok(())
}

fn assert_window_cost(
    program: &Program,
    baseline: &super::Snapshot,
    written: &[crate::redundant_sites::Site],
    expected: &[usize],
    window: usize,
) {
    DENSE_CALLS.store(0, Ordering::Relaxed);
    assert_eq!(
        super::accumulate(program, baseline, written, window),
        expected
    );
    let calls = DENSE_CALLS.load(Ordering::Relaxed);
    let budget = written.len() + window + 2;
    assert_proof_budget(written.len(), calls, budget, &format!("window {window}"));
}

fn assert_default_cost(
    program: &Program,
    written: &[crate::redundant_sites::Site],
    expected: &[usize],
) {
    DENSE_CALLS.store(0, Ordering::Relaxed);
    assert_default_sites(program, written, expected);
    let calls = DENSE_CALLS.load(Ordering::Relaxed);
    let budget = default_cost_budget(written.len(), expected.len());
    assert_proof_budget(written.len(), calls, budget, "default search");
}

fn assert_default_sites(
    program: &Program,
    written: &[crate::redundant_sites::Site],
    expected: &[usize],
) {
    let found = super::redundant_annotation_sites_for(program, |_| true, &|_| false);
    let positions: Vec<_> = found
        .iter()
        .map(|annotation| Some(annotation.annotation_position))
        .collect();
    let expected_positions: Vec<_> = expected
        .iter()
        .map(|index| written.get(*index).map(|site| site.source))
        .collect();
    assert_eq!(positions, expected_positions);
}

fn default_cost_budget(sites: usize, kept: usize) -> usize {
    if kept + 1 == sites {
        sites.div_ceil(16) + 34
    } else {
        sites + 18
    }
}

fn assert_proof_budget(sites: usize, calls: usize, budget: usize, search: &str) {
    assert!(
        calls <= budget,
        "{search}: {calls} full solves exceeds {budget}"
    );
    eprintln!("{sites} sites, {search}: {calls} full solves");
}

#[test]
fn memoized_windows_keep_accepted_prefixes_distinct() -> Result<(), String> {
    for (body, expected) in [("x", vec![0, 2]), ("1", vec![1, 2])] {
        let program = program(&format!(
            "fn first(x: int) -> int = {body}\nfn second(x: int) -> int = x\n"
        ));
        assert_windows(&program, &expected)?;
    }
    Ok(())
}

fn assert_windows(program: &Program, expected: &[usize]) -> Result<(), String> {
    let baseline = super::published(program).ok_or("fixture must typecheck")?;
    let written = crate::redundant_sites::sites(program);
    assert_eq!(written.len(), 4);
    for window in [1, 2, 4] {
        assert_eq!(
            super::accumulate(program, &baseline, &written, window),
            expected
        );
    }
    Ok(())
}

#[test]
fn memoized_verdicts_do_not_cross_programs() {
    for (body, parameter) in [("x", true), ("1", false)] {
        let program = program(&format!("fn pin(x: int) -> int = {body}\n"));
        let found = super::redundant_annotation_sites(&program);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found.first().is_some_and(|annotation| matches!(
                annotation.target,
                super::RedundantTarget::Parameter { .. }
            )),
            parameter
        );
    }
}

#[test]
fn concurrent_identical_erasures_share_one_full_proof() -> Result<(), String> {
    let program = program(&format!(
        "fn {FLIGHT_SENTINEL}() = 0\nfn pin(x: int) -> int = x\n"
    ));
    let baseline = super::published(&program).ok_or("fixture must typecheck")?;
    let written = crate::redundant_sites::sites(&program);
    let proofs = super::proofs::Proofs::new(&program, &baseline);
    FLIGHT_CALLS.store(0, Ordering::Relaxed);
    concurrent_erasure(&proofs, &written);
    assert_eq!(FLIGHT_CALLS.load(Ordering::Relaxed), 1);
    Ok(())
}

fn concurrent_erasure(
    proofs: &super::proofs::Proofs<'_>,
    written: &[crate::redundant_sites::Site],
) {
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|index| {
                scope.spawn(move || {
                    let chosen: &[usize] = if index % 2 == 0 { &[0] } else { &[0, 0] };
                    proofs.keeps(written, chosen)
                })
            })
            .collect();
        for handle in handles {
            assert!(matches!(handle.join(), Ok(true)));
        }
    });
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
    Ok(program(&source))
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
    let source = format!(
        "type {marker} = {{ value: int }}\nlet marker = {marker} {{ value: 0 }}\n{definitions}\nfn main() = 0\n"
    );
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
