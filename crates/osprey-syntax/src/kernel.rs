//! What the two flavors' `kernel` lowerings share.
//!
//! A `kernel` region names its effect per arm, because one region answers
//! several device dialects where a `handle` answers exactly one
//! ([STAGE-GPU-KERNEL]). Turning those arms into the nested regions they mean
//! starts the same way in both surfaces — group by effect, preserving the order
//! the source first names each — so it is written once here and parameterised
//! by the arm type each flavor's CST uses.

/// Arms grouped by the effect each names, in first-mention order.
pub(crate) fn group_by_effect<Arm>(
    arms: impl IntoIterator<Item = (String, Arm)>,
) -> Vec<(String, Vec<Arm>)> {
    let mut grouped: Vec<(String, Vec<Arm>)> = Vec::new();
    for (effect, arm) in arms {
        match grouped.iter_mut().find(|(name, _)| *name == effect) {
            Some((_, existing)) => existing.push(arm),
            None => grouped.push((effect, vec![arm])),
        }
    }
    grouped
}

/// Whether the region at `index` of `total` nested regions is the OUTERMOST —
/// the one boundary the source wrote, and so the one that carries the offload
/// obligation. Implements [STAGE-GPU-LEGAL].
pub(crate) const fn is_boundary(index: usize) -> bool {
    index == 0
}
