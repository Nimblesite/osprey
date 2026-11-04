//! Stack headroom for the recursive passes over a program's AST.

/// Stack kept in reserve before [`with_stack`] moves a pass onto a heap
/// segment. One debug-build nesting level of inference or effect analysis spans
/// several multi-kilobyte frames, so the reserve is generous.
const RED_ZONE: usize = 512 * 1024;

/// Size of each heap segment [`with_stack`] allocates once the reserve is hit.
const SEGMENT: usize = 8 * 1024 * 1024;

/// Run one level of a recursive pass with enough stack beneath it. A program's
/// nesting depth is then bounded by memory rather than by the calling thread's
/// stack — a 2 MiB test thread or a 1 MiB Windows main thread.
pub fn with_stack<R>(level: impl FnOnce() -> R) -> R {
    stacker::maybe_grow(RED_ZONE, SEGMENT, level)
}
