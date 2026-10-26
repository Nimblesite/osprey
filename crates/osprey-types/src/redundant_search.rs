//! The window search behind [`accumulate`]: it grows one jointly removable
//! annotation set for the whole program ([TYPE-ANNOTATION-REDUNDANT]).
//!
//! This module holds the search only; the [`super::preserves`] oracle and the
//! [`super::Snapshot`] it compares against stay in the parent module beside
//! the diagnostics they serve.

#[cfg(test)]
use osprey_ast::Program;

use super::Proofs;
#[cfg(test)]
use super::Snapshot;
use crate::redundant_sites::Site;

// Each proof owns a complete AST and inference state. Bound their simultaneous
// working sets independently of the source-order search window.
const MAX_PROOF_WORKERS: usize = 4;

struct Search<'a> {
    proofs: &'a Proofs<'a>,
    written: &'a [Site],
}

/// Test individual removals while retaining their original source identities.
#[cfg(test)]
pub(crate) fn individually_removable(
    program: &Program,
    baseline: &Snapshot,
    written: &[Site],
) -> Vec<usize> {
    let proofs = Proofs::new(program, baseline);
    individual_candidates(&search_input(&proofs, written))
}

fn individual_candidates(search: &Search<'_>) -> Vec<usize> {
    let mut preserved = vec![false; search.written.len()];
    if !preserved.is_empty() {
        judge_chunks(search, &mut preserved);
    }
    preserved
        .iter()
        .enumerate()
        .filter_map(|(index, removable)| removable.then_some(index))
        .collect()
}

fn judge_chunks(search: &Search<'_>, preserved: &mut [bool]) {
    let workers = super::judges()
        .min(MAX_PROOF_WORKERS)
        .min(preserved.len())
        .max(1);
    let width = preserved.len().div_ceil(workers);
    std::thread::scope(|scope| {
        for (chunk, results) in preserved.chunks_mut(width).enumerate() {
            let start = chunk * width;
            let _ = scope.spawn(move || {
                for (offset, result) in results.iter_mut().enumerate() {
                    *result = keeps(search, &[start + offset]);
                }
            });
        }
    });
}

/// Grow the reported set in source order, keeping only the annotations that
/// stay removable alongside everything already kept.
///
/// A reader acts on the whole list, so the whole list has to be safe. Judging
/// each annotation against the untouched program answers a question nobody
/// asked: mutually-pinning annotations each look removable while the others
/// hold the type down, and a reader who deletes all of them gets a different
/// program. Every set this returns has been solved for as a set.
///
/// Each window of `window` undecided candidates is a slice. One solve of the
/// whole slice against the set kept so far either proves every candidate in it
/// or proves its individually eligible sites together before trying them in
/// source order. Dense rejection or consecutive failed windows stop batching.
#[cfg(test)]
pub(crate) fn accumulate(
    program: &Program,
    baseline: &Snapshot,
    written: &[Site],
    window: usize,
) -> Vec<usize> {
    let candidates: Vec<usize> = (0..written.len()).collect();
    accumulate_candidates(program, baseline, written, &candidates, window)
}

/// [`accumulate`] over a source-ordered candidate subsequence. Candidates
/// already known to fail alone are excluded; the accepted set is unchanged
/// because every exclusion failed the oracle in isolation.
#[cfg(test)]
pub(crate) fn accumulate_candidates(
    program: &Program,
    baseline: &Snapshot,
    written: &[Site],
    candidates: &[usize],
    window: usize,
) -> Vec<usize> {
    let proofs = Proofs::new(program, baseline);
    accumulate_with(&proofs, written, candidates, window)
}

pub(super) fn accumulate_with(
    proofs: &Proofs<'_>,
    written: &[Site],
    candidates: &[usize],
    window: usize,
) -> Vec<usize> {
    let search = search_input(proofs, written);
    accumulate_search(&search, candidates, window)
}

fn accumulate_search(search: &Search<'_>, candidates: &[usize], window: usize) -> Vec<usize> {
    let mut width = window.max(1);
    let mut previous_failed = false;
    let mut kept = Vec::new();
    let mut remaining = candidates;
    while !remaining.is_empty() {
        let (slice, rest) = remaining.split_at(width.min(remaining.len()));
        let failure = accept_window(search, &mut kept, slice);
        if failure.is_some_and(|dense| dense || previous_failed) {
            width = 1;
        }
        previous_failed = failure.is_some();
        remaining = rest;
    }
    kept
}

fn search_input<'a>(proofs: &'a Proofs<'a>, written: &'a [Site]) -> Search<'a> {
    Search { proofs, written }
}

fn keeps(search: &Search<'_>, chosen: &[usize]) -> bool {
    search.proofs.keeps(search.written, chosen)
}

fn joined(kept: &[usize], candidates: &[usize]) -> Vec<usize> {
    kept.iter().chain(candidates).copied().collect()
}

fn eligible(search: &Search<'_>, candidates: &[usize]) -> Vec<usize> {
    let written: Vec<Site> = candidates
        .iter()
        .filter_map(|index| search.written.get(*index))
        .cloned()
        .collect();
    individual_candidates(&search_input(search.proofs, &written))
        .into_iter()
        .filter_map(|index| candidates.get(index).copied())
        .collect()
}

fn accept_window(search: &Search<'_>, kept: &mut Vec<usize>, candidates: &[usize]) -> Option<bool> {
    let before = kept.len();
    if keeps(search, &joined(kept, candidates)) {
        kept.extend_from_slice(candidates);
        return None;
    }
    if candidates.len() > 1 {
        let removable = eligible(search, candidates);
        accept_eligible(search, kept, &removable);
    }
    Some(kept.len() - before <= candidates.len() / 2)
}

fn accept_eligible(search: &Search<'_>, kept: &mut Vec<usize>, candidates: &[usize]) {
    if candidates.is_empty() {
        return;
    }
    if keeps(search, &joined(kept, candidates)) {
        kept.extend_from_slice(candidates);
    } else {
        accept_greedy(search, kept, candidates);
    }
}

// Every accepted singleton is checked together with the full accepted prefix.
fn accept_greedy(search: &Search<'_>, kept: &mut Vec<usize>, candidates: &[usize]) {
    for candidate in candidates {
        if keeps(search, &joined(kept, &[*candidate])) {
            kept.push(*candidate);
        }
    }
}
