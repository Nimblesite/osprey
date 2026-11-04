//! Bounded execution for the complete package/backend regression matrix.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(crate) const MATRIX_WORKERS: usize = 4;

/// Runs `run` over every job, claiming each exactly once from an atomic cursor
/// with at most `workers` in flight; results return indexed by submission
/// position so output never depends on timing, and panics propagate at join.
pub(crate) fn schedule<J: Copy + Sync, F: Fn(J) -> Vec<String> + Sync>(
    jobs: &[J],
    workers: usize,
    run: F,
) -> Vec<String> {
    // Quarantine invalid scheduling: zero workers silently discarded every job.
    assert!(workers > 0, "the package matrix needs at least one worker");
    let next = AtomicUsize::new(0);
    let slots = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers.min(jobs.len()) {
            let _ = scope.spawn(|| run_worker(jobs, &next, &slots, &run));
        }
    });
    ordered_problems(slots)
}

fn ordered_problems(slots: Mutex<Vec<(usize, Vec<String>)>>) -> Vec<String> {
    let mut results = slots
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    results.sort_by_key(|(index, _)| *index);
    results
        .into_iter()
        .flat_map(|(_, problems)| problems)
        .collect()
}

fn run_worker<J: Copy + Sync, F: Fn(J) -> Vec<String>>(
    jobs: &[J],
    next: &AtomicUsize,
    slots: &Mutex<Vec<(usize, Vec<String>)>>,
    run: &F,
) {
    loop {
        let index = next.fetch_add(1, Ordering::Relaxed);
        let Some(job) = jobs.get(index).copied() else {
            return;
        };
        let problems = run(job);
        let mut guard = slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.push((index, problems));
    }
}

/// Records the visit, waits for a second in-flight probe job so peak overlap
/// is only observed when workers genuinely run concurrently, then lingers
/// longest as the earliest job so completion order inverts submission order.
fn overlap_job(
    index: u8,
    visited: &Mutex<Vec<u8>>,
    active: &AtomicUsize,
    peak: &AtomicUsize,
) -> Vec<String> {
    record_job(index, visited, active, peak);
    let deadline = Instant::now() + Duration::from_secs(2);
    while active.load(Ordering::Relaxed) < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    std::thread::sleep(Duration::from_millis(u64::from(8 - index) * 5));
    let _ = active.fetch_sub(1, Ordering::Relaxed);
    vec![format!("problem-{index}")]
}

fn record_job(index: u8, visited: &Mutex<Vec<u8>>, active: &AtomicUsize, peak: &AtomicUsize) {
    visited
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(index);
    let _ = peak.fetch_max(
        active.fetch_add(1, Ordering::Relaxed) + 1,
        Ordering::Relaxed,
    );
}

fn observations(jobs: &[u8]) -> (Vec<u8>, usize, Vec<String>) {
    let (visited, active, peak) = (
        Mutex::new(Vec::new()),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    );
    let problems = schedule(jobs, MATRIX_WORKERS, |index| {
        overlap_job(index, &visited, &active, &peak)
    });
    let mut ran = visited
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    ran.sort_unstable();
    (ran, peak.load(Ordering::Relaxed), problems)
}

fn verify_visits_and_order(ran: &[u8], problems: &[String]) {
    assert_eq!(
        ran,
        (0..8).collect::<Vec<_>>(),
        "every job must run exactly once"
    );
    let expected = (0..8)
        .map(|index| format!("problem-{index}"))
        .collect::<Vec<_>>();
    assert_eq!(
        problems, expected,
        "problems must return in submission order"
    );
}

fn verify_overlap(peak: usize) {
    assert!(
        peak >= 2,
        "scheduler ran serially: peak in-flight jobs {peak}"
    );
    assert!(
        peak <= MATRIX_WORKERS,
        "overlap exceeded the worker bound: {peak}"
    );
}

/// A serial schedule stalls the overlap probe at peak 1; a correct bounded
/// schedule genuinely overlaps within the worker bound, visits each job
/// exactly once, returns problems in submission order and propagates panics.
#[test]
fn the_matrix_scheduler_overlaps_jobs_and_propagates_panics() {
    let jobs: Vec<u8> = (0..8).collect();
    let (ran, peak, problems) = observations(&jobs);
    verify_visits_and_order(&ran, &problems);
    verify_overlap(peak);
    let outcome = std::panic::catch_unwind(|| {
        schedule(&jobs, 2, |index| {
            assert!(index != 2, "propagate");
            Vec::new()
        })
    });
    assert!(outcome.is_err(), "a panicking job must fail the schedule");
}

#[test]
fn zero_workers_cannot_silently_discard_every_job() {
    let result = std::panic::catch_unwind(|| schedule(&[1, 2], 0, |job| vec![job.to_string()]));
    assert!(
        result.is_err(),
        "zero workers silently discarded the package matrix"
    );
}

#[test]
fn an_empty_matrix_starts_no_jobs() {
    let jobs: &[u8] = &[];
    let problems = schedule(jobs, MATRIX_WORKERS, |_| panic!("unexpected job"));
    assert_eq!(problems, Vec::<String>::new());
}
