//! Exact oracle reuse within one immutable annotation-diagnostic invocation.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use osprey_ast::Program;

use super::Snapshot;
use crate::redundant_sites::Site;

type Verdicts<T> = HashMap<Vec<usize>, Arc<OnceLock<T>>>;

/// Exact reuse for one immutable program and one kind of proof.
pub(super) struct ErasureMemo<T> {
    entries: Mutex<Verdicts<T>>,
    limit: usize,
}

impl<T: Clone> ErasureMemo<T> {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            limit,
        }
    }

    pub(super) fn compute(&self, members: &[usize], solve: impl FnOnce() -> T) -> T {
        match self.entry(members) {
            Some(verdict) => verdict.get_or_init(solve).clone(),
            None => solve(),
        }
    }

    // Distinct proofs can run in parallel after the map lock is released.
    fn entry(&self, members: &[usize]) -> Option<Arc<OnceLock<T>>> {
        let mut entries = self.entries.lock().ok()?;
        if let Some(verdict) = entries.get(members) {
            return Some(Arc::clone(verdict));
        }
        (entries.len() < self.limit)
            .then(|| Arc::clone(entries.entry(members.to_vec()).or_default()))
    }
}

pub(super) struct Proofs<'a> {
    program: &'a Program,
    baseline: &'a Snapshot,
    verdicts: ErasureMemo<bool>,
}

impl<'a> Proofs<'a> {
    pub(super) fn new(program: &'a Program, baseline: &'a Snapshot) -> Self {
        Self {
            program,
            baseline,
            verdicts: ErasureMemo::new(usize::MAX),
        }
    }

    pub(super) fn program(&self) -> &'a Program {
        self.program
    }

    pub(super) fn keeps(&self, written: &[Site], chosen: &[usize]) -> bool {
        let members = member_key(written, chosen);
        self.verdicts.compute(&members, || {
            super::preserves(self.program, self.baseline, &members)
        })
    }
}

// Site subsets are reindexed during prefiltering; only original member IDs
// identify the erased program. Exact vector equality also guards hash collisions.
pub(super) fn member_key(written: &[Site], chosen: &[usize]) -> Vec<usize> {
    let mut members: Vec<_> = chosen
        .iter()
        .filter_map(|index| written.get(*index))
        .flat_map(|site| site.members.iter().copied())
        .collect();
    members.sort_unstable();
    members.dedup();
    members
}
