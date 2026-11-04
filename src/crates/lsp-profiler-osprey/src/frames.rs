//! Which frames are plumbing [PROF-RUNTIME]: the loader's and the thread
//! library's entry trampolines and the runtime's own thread starters. They sit
//! at the root of every stack and say nothing about the program, so they never
//! appear as hotspots or flame-graph roots.

use lsp_profiler::wire::StackFrame;
use lsp_profiler::FrameFilter;

/// Native entry points every Osprey thread starts in. A frame with a source
/// file is never scaffolding: an Osprey function may be called `start` too.
const SCAFFOLDING: &[&str] = &[
    "start",
    "_start",
    "__libc_start_main",
    "__libc_start_call_main",
    "_pthread_start",
    "thread_start",
    "start_thread",
    "start_wqthread",
    "clone",
    "clone3",
    "__clone",
    "fiber_thread_func",
];

/// The Osprey frame filter.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OspreyFrames;

impl FrameFilter for OspreyFrames {
    fn is_scaffolding(&self, frame: &StackFrame) -> bool {
        frame.file.is_none() && SCAFFOLDING.contains(&frame.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(name: &str, file: Option<&str>) -> StackFrame {
        StackFrame {
            name: name.to_owned(),
            file: file.map(str::to_owned),
            line: None,
            module: None,
        }
    }

    #[test]
    fn thread_trampolines_are_scaffolding_and_program_code_never_is() {
        assert!(OspreyFrames.is_scaffolding(&frame("_pthread_start", None)));
        assert!(OspreyFrames.is_scaffolding(&frame("fiber_thread_func", None)));
        assert!(!OspreyFrames.is_scaffolding(&frame("start", Some("/src/app.osp"))));
        assert!(!OspreyFrames.is_scaffolding(&frame("osp_alloc", None)));
    }
}
