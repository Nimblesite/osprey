//! The window search behind [`accumulate`]: it grows one jointly removable
//! annotation set for the whole program ([TYPE-ANNOTATION-REDUNDANT]).
//!
//! This module holds the search only; the [`super::preserves`] oracle and the
//! [`super::Snapshot`] it compares against stay in the parent module beside
//! the diagnostics they serve.

use std::collections::VecDeque;

use osprey_ast::Program;

use super::{preserves, Snapshot};
use crate::redundant_sites::Site;

/// Grow the reported set in source order, keeping only the annotations that
/// stay removable alongside everything already kept.
///
/// A reader acts on the whole list, so the whole list has to be safe. Judging
/// each annotation against the untouched program answers a question nobody
/// asked: mutually-pinning annotations each look removable while the others
/// hold the type down, and a reader who deletes all of them gets a different
/// program. Every set this returns has been solved for as a set.
///
/// Each window of `window` undecided candidates first costs one whole-program
/// solve of the entire window against the set kept so far. Erasing more
/// annotations only removes constraints: it never restores a type another
/// erasure changed, nor repairs an error. So when that one solve preserves
/// the program, the one-at-a-time search would have kept every candidate in
/// turn, and the whole window is accepted at once. When it fails, the
/// candidates are judged at once on separate threads, each against the set
/// kept so far: a candidate rejected against the kept set stays rejected as
/// the set grows, and when the window's accepted candidates also preserve
/// the program together, that combined set is solved once; when it fails,
/// only its first member is kept and the rest are judged again against the
/// larger set.
struct Search<'a> {
    program: &'a Program,
    baseline: &'a Snapshot,
    written: &'a [Site],
}

pub(crate) fn accumulate(
    program: &Program,
    baseline: &Snapshot,
    written: &[Site],
    window: usize,
) -> Vec<usize> {
    let search = Search {
        program,
        baseline,
        written,
    };
    let mut kept = Vec::new();
    let mut pending: VecDeque<_> = (0..written.len()).collect();
    while !pending.is_empty() {
        let candidates = pending.drain(..pending.len().min(window.max(1))).collect();
        accept(&search, &mut kept, candidates, &mut pending);
    }
    kept
}

fn keeps(search: &Search<'_>, chosen: &[usize]) -> bool {
    preserves(search.program, search.baseline, search.written, chosen)
}

// The entire batch receives the same whole-program proof as a single erasure.
fn accept(
    search: &Search<'_>,
    kept: &mut Vec<usize>,
    candidates: Vec<usize>,
    pending: &mut VecDeque<usize>,
) {
    let together: Vec<_> = kept.iter().chain(&candidates).copied().collect();
    if keeps(search, &together) {
        kept.extend(candidates);
        return;
    }
    let accepted = judged_all(search, kept, &candidates);
    settle(search, kept, accepted, pending);
}

fn judged_all(search: &Search<'_>, kept: &[usize], candidates: &[usize]) -> Vec<usize> {
    judged(search, kept, candidates)
        .into_iter()
        .zip(candidates)
        .filter_map(|(keep, index)| keep.then_some(*index))
        .collect()
}

// A failed joint proof keeps only the first proven member, then retries the rest.
fn settle(
    search: &Search<'_>,
    kept: &mut Vec<usize>,
    accepted: Vec<usize>,
    pending: &mut VecDeque<usize>,
) {
    let Some((&first, rest)) = accepted.split_first() else {
        return;
    };
    let together: Vec<_> = kept.iter().chain(&accepted).copied().collect();
    if rest.is_empty() || keeps(search, &together) {
        kept.extend(accepted);
        return;
    }
    kept.push(first);
    for index in rest.iter().rev() {
        pending.push_front(*index);
    }
}

// Inference needs the main thread's headroom rather than the 2 MiB default.
const JUDGE_STACK: usize = 64 * 1024 * 1024;

fn trial(search: &Search<'_>, kept: &[usize], index: usize) -> bool {
    let mut chosen = kept.to_vec();
    chosen.push(index);
    keeps(search, &chosen)
}

fn judged(search: &Search<'_>, kept: &[usize], candidates: &[usize]) -> Vec<bool> {
    if let [index] = candidates {
        return vec![trial(search, kept, *index)];
    }
    std::thread::scope(|scope| {
        let judges = candidates
            .iter()
            .map(|&index| (index, spawn_trial(scope, search, kept, index)))
            .collect();
        verdicts(search, kept, judges)
    })
}

fn spawn_trial<'scope, 'env>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    search: &'scope Search<'env>,
    kept: &'scope [usize],
    index: usize,
) -> std::io::Result<std::thread::ScopedJoinHandle<'scope, bool>> {
    std::thread::Builder::new()
        .stack_size(JUDGE_STACK)
        .spawn_scoped(scope, move || trial(search, kept, index))
}

fn verdicts(
    search: &Search<'_>,
    kept: &[usize],
    judges: Vec<(
        usize,
        std::io::Result<std::thread::ScopedJoinHandle<'_, bool>>,
    )>,
) -> Vec<bool> {
    judges
        .into_iter()
        .map(|(index, judge)| verdict(judge).unwrap_or_else(|| trial(search, kept, index)))
        .collect()
}

// A spawn failure retries locally; a solver panic still propagates to its caller.
fn verdict(judge: std::io::Result<std::thread::ScopedJoinHandle<'_, bool>>) -> Option<bool> {
    judge.ok().map(|handle| {
        handle
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}
