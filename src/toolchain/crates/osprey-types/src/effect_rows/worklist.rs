//! Which functions a fixed-point sweep must re-analyse.
//!
//! A function's analysis reads other functions only through
//! [`super::Analyzer::function_value`], so recording those reads gives its
//! exact inputs. A function none of whose inputs moved would recompute the
//! very row and return it already holds; re-analysing only the moved
//! functions and their readers reaches the same fixed point as sweeping all.
use super::BTreeSet;
use std::cell::RefCell;

thread_local! {
    /// The functions read by the analysis [`recording`] is collecting.
    static READS: RefCell<Option<BTreeSet<usize>>> = const { RefCell::new(None) };
}

/// Note that the analysis in progress read function `id`'s row and return.
pub(super) fn record_read(id: usize) {
    READS.with(|reads| {
        if let Some(reads) = reads.borrow_mut().as_mut() {
            let _ = reads.insert(id);
        }
    });
}

/// Run `analysis`, returning its result and every function it read. A
/// recording already in progress resumes afterwards, unaffected.
pub(super) fn recording<T>(analysis: impl FnOnce() -> T) -> (T, BTreeSet<usize>) {
    let enclosing = READS.with(|reads| reads.replace(Some(BTreeSet::new())));
    let result = analysis();
    let read = READS.with(|reads| reads.replace(enclosing));
    (result, read.unwrap_or_default())
}

/// The functions the next sweep must re-analyse after `moved` moved: each
/// moved function, whose settled history changed, and every function whose
/// last analysis read one. The file scope seeds every body, so a moved
/// function it read makes every function's input change.
pub(super) fn affected(
    moved: &BTreeSet<usize>,
    file_reads: &BTreeSet<usize>,
    reads: &[BTreeSet<usize>],
) -> BTreeSet<usize> {
    if !moved.is_disjoint(file_reads) {
        return (0..reads.len()).collect();
    }
    let readers = reads.iter().enumerate();
    readers
        .filter(|(id, read)| moved.contains(id) || !read.is_disjoint(moved))
        .map(|(id, _)| id)
        .collect()
}

#[cfg(test)]
#[path = "worklist_tests.rs"]
mod tests;
