//! No live attach: Osprey's profiler is POSIX-only [PROF-ACTIVATE-ENV], so on
//! this platform no program serves an endpoint, none is recognised, and there
//! is nothing to attach a CPU session to.

use std::path::PathBuf;

use lsp_profiler::{Classification, CpuBackend, ProcessCandidate};

#[derive(Debug)]
pub(crate) struct LiveAttach;

impl LiveAttach {
    pub(crate) fn new(_socket_dir: Option<PathBuf>) -> Self {
        Self
    }

    #[expect(
        clippy::unused_self,
        reason = "one signature with attach.rs, whose backend reads its state"
    )]
    pub(crate) fn classify(
        &self,
        _candidates: &[ProcessCandidate],
    ) -> Vec<(usize, Classification)> {
        Vec::new()
    }

    #[expect(
        clippy::unused_self,
        reason = "one signature with attach.rs, whose backend reads its state"
    )]
    pub(crate) fn cpu(&self) -> Option<&dyn CpuBackend> {
        None
    }
}
