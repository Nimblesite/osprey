//! The Osprey runtime for lspkit's shared profiler [PROF-RUNTIME].
//!
//! Every native Osprey program serves a live-attach endpoint
//! ([PROF-ATTACH-ENDPOINT] in `docs/specs/tooling/0028-Profiler.md`), so this
//! runtime recognises an Osprey process by its socket, attaches a CPU session
//! through it, and turns the streamed raw stacks into symbolized
//! [`lsp_profiler::SampleBatch`]es, one per sampler tick. Everything above
//! that — sessions, hotspots, exports, the JSON-RPC surface and the editor
//! UI — is lspkit's and is the same for every language.

#[cfg(unix)]
mod attach;
#[cfg(not(unix))]
#[path = "attach_none.rs"]
mod attach;
#[cfg(unix)]
mod cpu;
#[cfg(unix)]
mod endpoint;
mod frames;
#[cfg(unix)]
mod process;
#[cfg(unix)]
mod protocol;
#[cfg(unix)]
mod relay;
#[cfg(unix)]
mod session;

use std::path::PathBuf;

use lsp_profiler::{
    Classification, CpuBackend, FrameFilter, MemoryBackend, ProcessCandidate, Runtime,
};

use crate::attach::LiveAttach;
use crate::frames::OspreyFrames;

/// How the Osprey runtime finds endpoints.
#[derive(Debug, Clone, Default)]
pub struct OspreyConfig {
    /// The directory holding `<pid>.sock` endpoints. `None` uses the one every
    /// Osprey program opens: `$TMPDIR/osprey-diagnostics-<uid>`.
    pub socket_dir: Option<PathBuf>,
}

/// Recognises, samples and names Osprey programs.
#[derive(Debug)]
pub struct OspreyRuntime {
    attach: LiveAttach,
    frames: OspreyFrames,
}

impl OspreyRuntime {
    /// A runtime that finds endpoints where `config` says.
    #[must_use]
    pub fn new(config: OspreyConfig) -> Self {
        Self {
            attach: LiveAttach::new(config.socket_dir),
            frames: OspreyFrames,
        }
    }
}

impl Runtime for OspreyRuntime {
    fn id(&self) -> &'static str {
        "osprey"
    }

    fn display_name(&self) -> &'static str {
        "Osprey"
    }

    fn classify(&self, candidates: &[ProcessCandidate]) -> Vec<(usize, Classification)> {
        self.attach.classify(candidates)
    }

    fn frame_filter(&self) -> &dyn FrameFilter {
        &self.frames
    }

    fn cpu(&self) -> Option<&dyn CpuBackend> {
        self.attach.cpu()
    }

    fn memory(&self) -> Option<&dyn MemoryBackend> {
        None
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn the_runtime_describes_itself_and_samples_cpu_only() {
        let runtime = OspreyRuntime::new(OspreyConfig::default());
        let info = runtime.info();
        assert_eq!(
            (info.id.as_str(), info.display_name.as_str()),
            ("osprey", "Osprey")
        );
        assert!(!info.source_entries);
        let cpu = info.cpu.expect("Osprey samples CPU");
        assert!(cpu.live_progress && cpu.line_level && cpu.native_frames && !cpu.cooperative);
        assert!(info.memory.is_none());
    }
}
