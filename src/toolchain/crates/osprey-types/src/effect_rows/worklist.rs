//! Which functions a fixed-point sweep must re-analyse.
//!
//! A function's analysis reads other functions only through
//! [`super::Analyzer::function_value`] and file-scope bindings only through
//! [`super::scope_values::ScopeValues`], so recording those reads gives its
//! exact inputs. A function none of whose inputs moved would recompute the
//! very row and return it already holds; re-analysing only the moved
//! functions and their readers reaches the same fixed point as sweeping all.
use super::{BTreeSet, CallableEnv};
use std::cell::RefCell;
use std::thread::LocalKey;

/// The reads of one kind the analysis in progress is collecting.
type Reads<K> = RefCell<Option<BTreeSet<K>>>;

thread_local! {
    /// The functions read by the analysis [`recording`] is collecting.
    static READS: Reads<usize> = const { RefCell::new(None) };
    /// The file-scope names read by the analysis [`scope_recording`] collects.
    static SCOPE_READS: Reads<String> = const { RefCell::new(None) };
}

fn note<K: Ord>(reads: &'static LocalKey<Reads<K>>, key: impl FnOnce() -> K) {
    reads.with(|reads| {
        if let Some(reads) = reads.borrow_mut().as_mut() {
            let _ = reads.insert(key());
        }
    });
}

/// Run `analysis` collecting into `reads`; a recording already in progress
/// resumes afterwards, unaffected.
fn collect<K, T>(
    reads: &'static LocalKey<Reads<K>>,
    analysis: impl FnOnce() -> T,
) -> (T, BTreeSet<K>) {
    let enclosing = reads.with(|reads| reads.replace(Some(BTreeSet::new())));
    let result = analysis();
    let read = reads.with(|reads| reads.replace(enclosing));
    (result, read.unwrap_or_default())
}

/// Note that the analysis in progress read function `id`'s row and return.
pub(super) fn record_read(id: usize) {
    note(&READS, || id);
}

/// Run `analysis`, returning its result and every function it read.
pub(super) fn recording<T>(analysis: impl FnOnce() -> T) -> (T, BTreeSet<usize>) {
    collect(&READS, analysis)
}

/// Note that the analysis in progress read the file-scope binding `name`.
pub(super) fn record_scope_read(name: &str) {
    note(&SCOPE_READS, || name.to_owned());
}

/// Run `analysis`, returning its result and every file-scope name it read.
pub(super) fn scope_recording<T>(analysis: impl FnOnce() -> T) -> (T, BTreeSet<String>) {
    collect(&SCOPE_READS, analysis)
}

/// What each function's last analysis read: other functions and file-scope
/// names.
pub(super) struct Inputs {
    pub(super) functions: Vec<BTreeSet<usize>>,
    pub(super) scope: Vec<BTreeSet<String>>,
}

impl Inputs {
    pub(super) fn new(count: usize) -> Inputs {
        Inputs {
            functions: vec![BTreeSet::new(); count],
            scope: vec![BTreeSet::new(); count],
        }
    }

    /// Replace function `id`'s reads with those of its latest analysis.
    pub(super) fn set(&mut self, id: usize, functions: BTreeSet<usize>, scope: BTreeSet<String>) {
        if let (Some(f), Some(s)) = (self.functions.get_mut(id), self.scope.get_mut(id)) {
            *f = functions;
            *s = scope;
        }
    }
}

/// Run `analysis`, returning its result, the functions and the file-scope
/// names it read.
pub(super) fn recorded_inputs<T>(
    analysis: impl FnOnce() -> T,
) -> ((T, BTreeSet<usize>), BTreeSet<String>) {
    scope_recording(|| recording(analysis))
}

/// The file scope seeds every body. A file-scope value that changed re-runs
/// only the bodies that read it; any other change to the environment re-runs
/// them all.
pub(super) fn invalidate_scope(
    previous: Option<&CallableEnv>,
    current: &CallableEnv,
    dirty: &mut BTreeSet<usize>,
    scope_reads: &[BTreeSet<String>],
) {
    let Some(previous) = previous.filter(|scope| *scope != current) else {
        return;
    };
    let readers = scope_reads.iter().enumerate();
    match previous.changed_values(current) {
        Some(names) => dirty.extend(
            readers
                .filter(|(_, read)| !read.is_disjoint(&names))
                .map(|(id, _)| id),
        ),
        None => dirty.extend(readers.map(|(id, _)| id)),
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
