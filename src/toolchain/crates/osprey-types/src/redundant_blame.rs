//! Failure-directed annotation search ([TYPE-ANNOTATION-REDUNDANT]).
//!
//! A solve that rejects an erasure also says what it changed: which published
//! labels moved and where errors landed. That names the statements at fault, so
//! every annotation elsewhere is settled by one joint solve and only the suspects
//! are judged one at a time. Blame only chooses what to try next: every reported
//! set still passes the full oracle, and a set it rejects falls back to the
//! source-order search.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use osprey_ast::stmt_meta::statement_position;
use osprey_ast::{Position, Program, Stmt};

use super::proofs::{member_key, ErasureMemo};
use super::{equivalent, same_types, tried, Proofs, Published, Renaming, Tried};
use crate::redundant_sites::{erase, Site};

/// Sites with no recorded position share one group, judged in source order.
const UNPLACED: usize = usize::MAX;
const TRIAL_MEMO_LIMIT: usize = 256;

/// What a rejected trial points at.
#[derive(Default)]
struct Blame {
    positions: Vec<Position>,
    functions: BTreeSet<String>,
}

/// A top-level declaration and the source lines it owns.
struct Leaf {
    name: Option<String>,
    start: Position,
    end: Option<Position>,
}

impl Leaf {
    fn holds(&self, position: Position) -> bool {
        let at = (position.line, position.column);
        (self.start.line, self.start.column) <= at
            && self.end.is_none_or(|end| at < (end.line, end.column))
    }
}

struct Trials<'a> {
    proofs: &'a Proofs<'a>,
    written: &'a [Site],
    baseline: Tried,
    leaves: Vec<Leaf>,
    owners: Vec<Vec<usize>>,
    verdicts: ErasureMemo<Result<(), Arc<Blame>>>,
}

/// Select the jointly removable set, or `None` when the failure-directed search
/// cannot prove one and the source-order search has to decide.
pub(super) fn directed(proofs: &Proofs<'_>, written: &[Site]) -> Option<Vec<usize>> {
    let baseline = tried(proofs.program()).ok()?;
    let leaves = leaves(proofs.program());
    let owners = written.iter().map(|site| owners(&leaves, site)).collect();
    let trials = Trials {
        proofs,
        written,
        baseline,
        leaves,
        owners,
        verdicts: ErasureMemo::new(TRIAL_MEMO_LIMIT),
    };
    let all: Vec<usize> = (0..written.len()).collect();
    let (accepted, suspects) = trials.separate(&all);
    let decided = trials.decide(&accepted, &suspects);
    // An unchanged set has already passed the joint trial.
    let chosen = if decided.len() == accepted.len() {
        accepted
    } else {
        trials.reconcile(&decided)
    };
    (chosen.is_empty() || proofs.keeps(written, &chosen)).then_some(chosen)
}

impl Trials<'_> {
    /// Move blamed sites out of the joint set until the rest survive together.
    fn separate(&self, sites: &[usize]) -> (Vec<usize>, Vec<usize>) {
        let mut accepted = sites.to_vec();
        let mut suspects = Vec::new();
        while !accepted.is_empty() {
            let Err(blame) = self.judge(&accepted) else {
                break;
            };
            let blamed = self.blamed(&blame);
            let (fresh, rest): (Vec<_>, Vec<_>) =
                accepted.iter().partition(|site| blamed.contains(*site));
            if fresh.is_empty() {
                let mut kept = Vec::new();
                self.bisect(&accepted, &mut kept, &mut suspects);
                return (kept, suspects);
            }
            accepted = rest;
            suspects.extend(fresh);
        }
        (accepted, suspects)
    }

    /// Declarations that each held alone can still pin one another, as mutually
    /// recursive functions do. The sites the joint trial blames go in source order.
    fn reconcile(&self, decided: &[usize]) -> Vec<usize> {
        let (accepted, contested) = self.separate(decided);
        let mut chosen = self.decide_group(&accepted, &contested);
        chosen.extend(accepted);
        chosen.sort_unstable();
        chosen
    }

    /// A change no label ties to a site leaves group testing: halves that hold
    /// join the set, and a lone site that does not becomes a suspect.
    fn bisect(&self, sites: &[usize], kept: &mut Vec<usize>, suspects: &mut Vec<usize>) {
        let trial: Vec<usize> = kept.iter().chain(sites).copied().collect();
        if self.judge(&trial).is_ok() {
            kept.extend_from_slice(sites);
        } else if let [site] = sites {
            suspects.push(*site);
        } else {
            let (left, right) = sites.split_at(sites.len() / 2);
            self.bisect(left, kept, suspects);
            self.bisect(right, kept, suspects);
        }
    }

    fn blamed(&self, blame: &Blame) -> BTreeSet<usize> {
        let leaves: BTreeSet<usize> = self
            .leaves
            .iter()
            .enumerate()
            .filter(|(_, leaf)| {
                blame.positions.iter().any(|at| leaf.holds(*at))
                    || leaf
                        .name
                        .as_ref()
                        .is_some_and(|name| blame.functions.contains(name))
            })
            .map(|(index, _)| index)
            .collect();
        self.owners
            .iter()
            .enumerate()
            .filter(|(_, owners)| owners.is_empty() || owners.iter().any(|o| leaves.contains(o)))
            .map(|(index, _)| index)
            .collect()
    }

    /// Greedy in source order within a declaration, then checked jointly.
    /// When checking a single Agent project, concurrent full inference trials
    /// consumed more CPU and took longer than running them in order.
    fn decide(&self, accepted: &[usize], suspects: &[usize]) -> Vec<usize> {
        let mut chosen = accepted.to_vec();
        for group in self.grouped(suspects) {
            chosen.extend(self.decide_group(accepted, &group));
        }
        chosen
    }

    fn grouped(&self, suspects: &[usize]) -> Vec<Vec<usize>> {
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for site in suspects {
            let owner = self
                .owners
                .get(*site)
                .and_then(|owners| owners.first())
                .copied()
                .unwrap_or(UNPLACED);
            groups.entry(owner).or_default().push(*site);
        }
        groups.into_values().collect()
    }

    fn decide_group(&self, accepted: &[usize], group: &[usize]) -> Vec<usize> {
        let mut kept = Vec::new();
        let mut sorted = group.to_vec();
        sorted.sort_unstable();
        for site in sorted {
            let trial: Vec<usize> = accepted
                .iter()
                .chain(&kept)
                .chain(std::iter::once(&site))
                .copied()
                .collect();
            if self.judge(&trial).is_ok() {
                kept.push(site);
            }
        }
        kept
    }

    /// `Ok` is provisional until the full oracle solves the final set. `Err` is
    /// final: without an open handler the skipped effect proof cannot move a
    /// type, and with one the full oracle answers instead.
    fn judge(&self, chosen: &[usize]) -> Result<(), Arc<Blame>> {
        let members = member_key(self.written, chosen);
        self.verdicts.compute(&members, || {
            self.judge_new(chosen, &members).map_err(Arc::new)
        })
    }

    fn judge_new(&self, chosen: &[usize], members: &[usize]) -> Result<(), Blame> {
        let erasing: BTreeSet<usize> = members.iter().copied().collect();
        let candidate = erase(self.proofs.program(), &mut |slot| !erasing.contains(&slot));
        let trial = tried(&candidate).map_err(|errors| Blame {
            positions: errors.iter().filter_map(|error| error.position).collect(),
            functions: BTreeSet::new(),
        })?;
        let (base, other) = (&self.baseline.snapshot, &trial.snapshot);
        let same = base.methods == other.methods && same_types(&base.types, &other.types);
        let open = self.baseline.open_handlers || trial.open_handlers;
        if same || (open && self.proofs.keeps(self.written, chosen)) {
            return Ok(());
        }
        Err(blame_labels(&base.types, &other.types))
    }
}

fn blame_labels(left: &Published, right: &Published) -> Blame {
    let mut blame = Blame::default();
    for (label, ty) in left {
        let moved = right
            .get(label)
            .is_none_or(|other| !equivalent(ty, other, &mut Renaming::default()));
        if moved {
            blame.add(label);
        }
    }
    for label in right.keys().filter(|label| !left.contains_key(*label)) {
        blame.add(label);
    }
    blame
}

impl Blame {
    fn add(&mut self, label: &str) {
        let mut words = label.split(' ');
        let (Some(kind), Some(subject)) = (words.next(), words.next()) else {
            return;
        };
        if matches!(kind, "fn" | "binder" | "constraint") {
            let _ = self.functions.insert(subject.to_owned());
        } else if let Some(position) = position(subject) {
            self.positions.push(position);
        }
    }
}

fn position(text: &str) -> Option<Position> {
    let (line, column) = text.split_once(':')?;
    Some(Position {
        line: line.parse().ok()?,
        column: column.parse().ok()?,
    })
}

/// Declarations in program order. Merged files restart line numbers, so a span
/// ending at a restart stays open and a position may land in several files.
fn leaves(program: &Program) -> Vec<Leaf> {
    let mut starts = Vec::new();
    collect_leaves(&program.statements, &mut starts);
    let ends: Vec<Option<Position>> = starts
        .iter()
        .skip(1)
        .map(|(start, _): &(Position, Option<String>)| Some(*start))
        .chain(std::iter::once(None))
        .collect();
    starts
        .into_iter()
        .zip(ends)
        .map(|((start, name), end)| Leaf {
            name,
            start,
            end: end.filter(|end| (end.line, end.column) > (start.line, start.column)),
        })
        .collect()
}

fn collect_leaves(statements: &[Stmt], out: &mut Vec<(Position, Option<String>)>) {
    for statement in statements {
        match statement {
            Stmt::Namespace { body, .. } => collect_leaves(body, out),
            Stmt::Module { body, .. } => {
                for item in body {
                    collect_leaves(std::slice::from_ref(&*item.declaration), out);
                }
            }
            _ => {
                if let Some(start) = statement_position(statement) {
                    let name = match statement {
                        Stmt::Function { name, .. } => Some(name.clone()),
                        _ => None,
                    };
                    out.push((start, name));
                }
            }
        }
    }
}

/// The declarations a site answers for. A signature is lowered onto the function
/// written under it and keeps its own line, so the function's name decides.
fn owners(leaves: &[Leaf], site: &Site) -> Vec<usize> {
    let named: Vec<usize> = site.slot.owner().map_or_else(Vec::new, |owner| {
        leaves
            .iter()
            .enumerate()
            .filter(|(_, leaf)| leaf.name.as_deref() == Some(owner))
            .map(|(index, _)| index)
            .collect()
    });
    if !named.is_empty() {
        return named;
    }
    site.position.or(site.source).map_or_else(Vec::new, |at| {
        leaves
            .iter()
            .enumerate()
            .filter(|(_, leaf)| leaf.holds(at))
            .map(|(index, _)| index)
            .collect()
    })
}
