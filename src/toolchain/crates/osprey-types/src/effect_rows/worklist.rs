//! Which functions a fixed-point sweep must re-analyse.
//!
//! A function's analysis reads other functions only through
//! [`super::Analyzer::function_value`], so recording those reads gives its
//! exact inputs. A function none of whose inputs moved would recompute the
//! very row and return it already holds; re-analysing only the moved
//! functions and their readers reaches the same fixed point as sweeping all.
use super::{BTreeSet, CallableEnv};
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

/// The file scope seeds every body. Only a different computed environment
/// requires all the bodies to run again.
pub(super) fn invalidate_scope(
    previous: Option<&CallableEnv>,
    current: &CallableEnv,
    dirty: &mut BTreeSet<usize>,
    function_count: usize,
) {
    if previous.is_some_and(|scope| scope != current) {
        eprintln!("SCOPE-INVALIDATE all {function_count} (dirty before {})", dirty.len());
        dirty.extend(0..function_count);
    }
}

/// Re-analyse moved functions and the functions whose last analysis read one.
pub(super) fn affected(moved: &BTreeSet<usize>, reads: &[BTreeSet<usize>]) -> BTreeSet<usize> {
    let readers = reads.iter().enumerate();
    readers
        .filter(|(id, read)| moved.contains(id) || !read.is_disjoint(moved))
        .map(|(id, _)| id)
        .collect()
}

#[cfg(test)]
#[path = "worklist_tests.rs"]
mod tests;
