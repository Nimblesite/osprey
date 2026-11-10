//! Bound diagnostic proof work without a wall-clock timeout.
use std::{
    fmt::{self, Write},
    sync::atomic::{AtomicUsize, Ordering},
};

use osprey_ast::{Program, Stmt};
use osprey_syntax::{parse_program_with_flavor, Flavor};

use crate::redundant_sites::{sites, Site};

const SENTINEL: &str = "annotation_cost_sentinel";
const PARALLEL_SENTINEL: &str = "annotation_parallelism_sentinel";
const MEMO_SENTINEL: &str = "annotation_memo_sentinel";
const FLIGHT_SENTINEL: &str = "annotation_flight_sentinel";
const DENSE_SENTINEL: &str = "annotation_dense_cost_sentinel";
static CALLS: AtomicUsize = AtomicUsize::new(0);
static MEMO_CALLS: AtomicUsize = AtomicUsize::new(0);
static FLIGHT_CALLS: AtomicUsize = AtomicUsize::new(0);
static DENSE_CALLS: AtomicUsize = AtomicUsize::new(0);
const SPACED_SENTINEL: &str = "annotation_spaced_cost_sentinel";
static SPACED_CALLS: AtomicUsize = AtomicUsize::new(0);
const BLAME_SENTINEL: &str = "annotation_blame_sentinel";
static BLAME_CALLS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
type CostExample = (String, usize, Vec<usize>);
const GREETING: &str = "(name: string) -> string = \"hi \" + name";

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
        (SPACED_SENTINEL, &SPACED_CALLS),
        (BLAME_SENTINEL, &BLAME_CALLS),
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
    let program = program(&format!(
        "fn {MEMO_SENTINEL}() = 0\nfn pin(x: int) -> int = x\n"
    ));
    let written = sites(&program);
    assert_eq!(written.len(), 2);
    MEMO_CALLS.store(0, Ordering::Relaxed);
    let baseline = super::published(&program).ok_or("fixture must typecheck")?;
    let removable = super::accumulate(&program, &baseline, &written, 2);
    assert_eq!(removable.len(), 1, "joint removal changes the type");
    let calls = MEMO_CALLS.load(Ordering::Relaxed);
    assert_eq!(calls, 4, "distinct programs need one full proof each");
    let calls = directed_search(&MEMO_CALLS, &program, 2, &[0]);
    assert_eq!(
        calls, 5,
        "the directed search judges each erasure once, then runs its final proof"
    );
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
        assert_eq!(sites(&program).len(), site_count);
        assert_cost_windows(&program, &expected)?;
    }
    Ok(())
}

fn cost_examples() -> Result<[CostExample; 5], fmt::Error> {
    let greetings = definitions(64, "greet", GREETING)?;
    let pins = definitions(32, "pin", "(x: int) -> int = x")?;
    let necessary = definitions(32, "required", "(x: int) = 0")?;
    let sparse = format!("fn pin(x: int) -> int = x\n{greetings}");
    let isolated = format!("fn required(x: int) = 0\n{greetings}");
    let periodic = periodic_definitions()?;
    Ok([
        (pins, 64, (0..64).step_by(2).collect()),
        (necessary, 32, Vec::new()),
        (sparse, 130, (0..130).filter(|i| *i != 1).collect()),
        (isolated, 129, (1..129).collect()),
        (periodic, 48, (0..48).filter(|i| i % 3 != 0).collect()),
    ])
}

fn periodic_definitions() -> Result<String, fmt::Error> {
    (0..16).try_fold(String::new(), |mut text, index| {
        writeln!(text, "fn required{index}(x: int) = 0\nfn greet{index}(name: string) -> string = \"hi \" + name")?;
        Ok(text)
    })
}

/// Checks the removals and the proof budget of every search window, then the default search.
fn assert_cost_windows(program: &Program, expected: &[usize]) -> Result<(), String> {
    let baseline = super::published(program).ok_or("fixture must typecheck")?;
    let written = sites(program);
    for window in [1, 2, 4, 8, 14, 32] {
        DENSE_CALLS.store(0, Ordering::Relaxed);
        assert_eq!(
            super::accumulate(program, &baseline, &written, window),
            expected
        );
        let budget = written.len() + window + 2;
        let calls = DENSE_CALLS.load(Ordering::Relaxed);
        assert_proof_budget(written.len(), calls, budget, &format!("window {window}"));
    }
    DENSE_CALLS.store(0, Ordering::Relaxed);
    assert_default_sites(program, &written, expected);
    let calls = DENSE_CALLS.load(Ordering::Relaxed);
    let budget = default_cost_budget(written.len(), expected.len());
    assert_proof_budget(written.len(), calls, budget, "default search");
    Ok(())
}

/// Asserts the directed search over `count` written sites reports `expected` and counts its solves.
fn directed_search(
    counter: &AtomicUsize,
    program: &Program,
    count: usize,
    expected: &[usize],
) -> usize {
    let written = sites(program);
    assert_eq!(written.len(), count);
    counter.store(0, Ordering::Relaxed);
    assert_default_sites(program, &written, expected);
    counter.load(Ordering::Relaxed)
}

/// The directed redundancy search, judging every written annotation site.
fn all_site_removals(program: &Program) -> Vec<super::RedundantAnnotation> {
    super::redundant_annotation_sites_for(program, |_| true, &|_| false)
}

/// Asserts the directed search reports exactly the `expected` written sites.
fn assert_default_sites(program: &Program, written: &[Site], expected: &[usize]) {
    let positions: Vec<_> = all_site_removals(program)
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
    let complaint = format!("{search}: {calls} full solves exceeds {budget}");
    assert!(calls <= budget, "{complaint}");
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
    let written = sites(program);
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
    let written = sites(&program);
    let proofs = super::proofs::Proofs::new(&program, &baseline);
    FLIGHT_CALLS.store(0, Ordering::Relaxed);
    concurrent_erasure(&proofs, &written);
    assert_eq!(FLIGHT_CALLS.load(Ordering::Relaxed), 1);
    Ok(())
}

/// Runs four concurrent erasures, asserting every one reports the annotation as load bearing.
fn concurrent_erasure(proofs: &super::proofs::Proofs<'_>, written: &[Site]) {
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for index in 0..4 {
            let chosen: &[usize] = if index % 2 == 0 { &[0] } else { &[0, 0] };
            handles.push(scope.spawn(move || proofs.keeps(written, chosen)));
        }
        for handle in handles {
            assert!(matches!(handle.join(), Ok(true)));
        }
    });
}

fn fixture_with(count: usize, marker: &str) -> Result<Program, fmt::Error> {
    let greetings = definitions(count, "greeting", GREETING)?;
    let source = format!(
        "{}\nfn {marker}() = 0\n{greetings}",
        crate::redundant_window_tests::MIXED
    );
    Ok(program(&source))
}

fn definitions(count: usize, prefix: &str, signature: &str) -> Result<String, fmt::Error> {
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
fn closed_functions_do_not_rescan_the_global_environment() -> Result<(), fmt::Error> {
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
    let phantom_main = "fn main() = wrapAdd(phantom<int>(), phantom<string>())";
    let control = source.replace("fn main() = 0", phantom_main) + "fn phantom<T>() = 0\n";
    assert!(
        environment_marker_visits(&control) > 0,
        "phantom binders must still use the environment oracle"
    );
    Ok(())
}

#[test]
fn full_program_proofs_have_bounded_parallelism() -> Result<(), fmt::Error> {
    let program = fixture_with(64, PARALLEL_SENTINEL)?;
    let baseline = super::published(&program).expect("valid annotation proof fixture");
    let written = sites(&program);
    PEAK.store(0, Ordering::SeqCst);
    let removable = super::individually_removable(&program, &baseline, &written);
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
    PEAK.store(0, Ordering::SeqCst);
    let warnings = all_site_removals(&program);
    assert!(
        !warnings.is_empty(),
        "directed search must still report warnings"
    );
    assert_eq!(
        PEAK.load(Ordering::SeqCst),
        1,
        "a directed search must complete one inference trial at a time"
    );
    Ok(())
}

#[test]
fn isolated_load_bearing_sites_do_not_force_every_annotation_through_a_full_solve(
) -> Result<(), fmt::Error> {
    let program = fixture_with(256, SENTINEL)?;
    let count = sites(&program).len();
    let workers = super::judges().min(count).max(1);
    CALLS.store(0, Ordering::Relaxed);
    let warnings = all_site_removals(&program);
    assert!(
        !warnings.is_empty(),
        "fixture must contain removable annotations"
    );
    let calls = CALLS.load(Ordering::Relaxed);
    let budget = count.div_ceil(workers) + 3 * workers + 64;
    assert!(
        calls <= budget,
        "{calls} full solves for {count} annotations exceeds {budget}"
    );
    Ok(())
}

/// [TYPE-ANNOTATION-REDUNDANT] Necessary annotations spread through a large
/// application must not permanently turn later independent proofs into singles.
#[test]
fn spaced_necessary_annotations_keep_independent_proofs_batched() -> Result<(), fmt::Error> {
    for position in [0, 7, 15] {
        let mut source = format!("fn {SPACED_SENTINEL}() = 0\n");
        for group in 0..8 {
            for item in 0..16 {
                if item == position {
                    writeln!(source, "fn required{group}(x: int) = 0")?;
                } else {
                    writeln!(source, "fn greeting{group}_{item}() -> string = \"hi\"")?;
                }
            }
        }
        let program = program(&source);
        let expected: Vec<_> = (0..128).filter(|index| index % 16 != position).collect();
        let calls = directed_search(&SPACED_CALLS, &program, 128, &expected);
        assert_proof_budget(128, calls, 96, &format!("position {position}"));
    }
    for required in [8, 16] {
        assert_dense_prefix_cost(required)?;
    }
    Ok(())
}

/// [TYPE-ANNOTATION-REDUNDANT] A failed solve names what changed, so proof work
/// follows the annotations at fault and not the number of annotations written.
#[test]
fn proof_work_follows_the_annotations_at_fault_not_the_annotations_written(
) -> Result<(), fmt::Error> {
    let safe = definitions(32, "safe", "() -> string = \"hi\"")?;
    let all_safe = program(&format!("fn {BLAME_SENTINEL}() = 0\n{safe}"));
    let expected: Vec<_> = (0..32).collect();
    let calls = directed_search(&BLAME_CALLS, &all_safe, 32, &expected);
    assert_eq!(
        calls, 4,
        "joint success needs one trial, followed by the unchanged final oracle"
    );
    for required in [1, 2, 4, 8, 16, 32] {
        let mut source = format!("fn {BLAME_SENTINEL}() = 0\n");
        for group in 0..required {
            writeln!(source, "fn required{group}(x: int) = 0")?;
            for item in 0..3 {
                writeln!(source, "fn kept{group}_{item}() -> string = \"hi\"")?;
            }
        }
        let program = program(&source);
        let expected: Vec<_> = (0..required * 4).filter(|i| i % 4 != 0).collect();
        let calls = directed_search(&BLAME_CALLS, &program, required * 4, &expected);
        let search = format!("{required} necessary");
        assert_proof_budget(required * 4, calls, required + 6, &search);
    }
    Ok(())
}

fn assert_dense_prefix_cost(required: usize) -> Result<(), fmt::Error> {
    let necessary = definitions(required, "required", "(x: int) = 0")?;
    let independent = definitions(128, "independent", "() -> string = \"hi\"")?;
    let source = format!("fn {SPACED_SENTINEL}() = 0\n{necessary}{independent}");
    let program = program(&source);
    let expected: Vec<_> = (required..required + 128).collect();
    let calls = directed_search(&SPACED_CALLS, &program, required + 128, &expected);
    let search = format!("dense prefix {required}");
    assert_proof_budget(required + 128, calls, 64, &search);
    Ok(())
}
