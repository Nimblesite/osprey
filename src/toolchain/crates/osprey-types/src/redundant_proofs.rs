//! Exact oracle reuse within one immutable annotation-diagnostic invocation.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use osprey_ast::Program;

use super::Snapshot;
use crate::redundant_sites::Site;

type Verdict = Arc<OnceLock<bool>>;

pub(super) struct Proofs<'a> {
    program: &'a Program,
    baseline: &'a Snapshot,
    verdicts: Mutex<HashMap<Vec<usize>, Verdict>>,
}

impl<'a> Proofs<'a> {
    pub(super) fn new(program: &'a Program, baseline: &'a Snapshot) -> Self {
        Self {
            program,
            baseline,
            verdicts: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn keeps(&self, written: &[Site], chosen: &[usize]) -> bool {
        let members = member_key(written, chosen);
        let compute = || super::preserves(self.program, self.baseline, &members);
        match self.entry(&members) {
            Some(verdict) => *verdict.get_or_init(compute),
            None => compute(),
        }
    }

    // Only lookup/insertion holds the map lock; distinct full proofs stay parallel.
    fn entry(&self, members: &[usize]) -> Option<Verdict> {
        let Ok(mut verdicts) = self.verdicts.lock() else {
            // Cache failure must never substitute a verdict for the full oracle.
            return None;
        };
        Some(Arc::clone(verdicts.entry(members.to_vec()).or_default()))
    }
}

// Site subsets are reindexed during prefiltering; only original member IDs
// identify the erased program. Exact vector equality also guards hash collisions.
fn member_key(written: &[Site], chosen: &[usize]) -> Vec<usize> {
    let mut members: Vec<_> = chosen
        .iter()
        .filter_map(|index| written.get(*index))
        .flat_map(|site| site.members.iter().copied())
        .collect();
    members.sort_unstable();
    members.dedup();
    members
}
