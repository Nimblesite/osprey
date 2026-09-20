//! The enclosing-region stack a static rewrite runs under: what each region
//! answers, how a runtime handler shadows it, and how a stack identifies a
//! specialization.

use crate::stage_rows::operation_name;
use crate::HandlerArm;

/// One `handle static` region on the enclosing stack.
pub(super) struct Region {
    pub(super) effect: String,
    pub(super) arms: Vec<HandlerArm>,
    pub(super) id: u32,
}

impl Region {
    pub(super) fn clone_region(&self) -> Self {
        Self {
            effect: self.effect.clone(),
            arms: self.arms.clone(),
            id: self.id,
        }
    }

    /// Whether this region answers `operation` (an `Effect.op` name).
    pub(super) fn answers(&self, operation: &str) -> bool {
        self.arms
            .iter()
            .any(|arm| operation_name(&self.effect, &arm.operation) == operation)
    }

    /// This region as seen from inside a runtime handler for its effect: the
    /// operations that handler supplies are shadowed, every other arm still
    /// answers, because an uncovered operation searches outward
    /// ([STAGE-LOWER-ORDER]). A region left with no arms answers nothing.
    pub(super) fn shadowed_by(&self, supplied: &[HandlerArm]) -> Option<Self> {
        let arms: Vec<HandlerArm> = self
            .arms
            .iter()
            .filter(|arm| {
                !supplied
                    .iter()
                    .any(|inner| inner.operation == arm.operation)
            })
            .cloned()
            .collect();
        (!arms.is_empty()).then(|| Self {
            effect: self.effect.clone(),
            arms,
            id: self.id,
        })
    }

    /// What identifies this region in a specialization key: a shadowed copy
    /// answers fewer operations than its original, so it specializes apart.
    pub(super) fn key(&self) -> String {
        let operations: Vec<&str> = self.arms.iter().map(|arm| arm.operation.as_str()).collect();
        format!("{}[{}]", self.id, operations.join(","))
    }
}

/// The regions a runtime handler's body sees: the same effect's regions with
/// the supplied operations shadowed, every other region unchanged.
pub(super) fn shadowed_regions(
    regions: &[Region],
    effect: &str,
    supplied: &[HandlerArm],
) -> Vec<Region> {
    regions
        .iter()
        .filter_map(|region| {
            if region.effect == effect {
                region.shadowed_by(supplied)
            } else {
                Some(region.clone_region())
            }
        })
        .collect()
}

/// Whether any enclosing region answers `effect`.
pub(super) fn answered(regions: &[Region], effect: &str) -> bool {
    regions.iter().any(|region| region.effect == effect)
}

/// The innermost arm answering one operation.
pub(super) fn innermost_arm(
    regions: &[Region],
    effect: &str,
    operation: &str,
) -> Option<(usize, HandlerArm)> {
    regions
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, region)| region.effect == effect)
        .find_map(|(index, region)| {
            region
                .arms
                .iter()
                .find(|arm| arm.operation == operation)
                .map(|arm| (index, arm.clone()))
        })
}

/// A specialization is identified by the function and the exact region stack it
/// was reached under, each region by the operations it still answers there.
pub(super) fn specialization_key(name: &str, regions: &[Region]) -> String {
    let keys: Vec<String> = regions.iter().map(Region::key).collect();
    format!("{name}|{}", keys.join("."))
}
