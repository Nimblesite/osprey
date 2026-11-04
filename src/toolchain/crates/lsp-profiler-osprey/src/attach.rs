//! Live attach where programs serve an endpoint [PROF-ATTACH-ENDPOINT]: the
//! classifier and the CPU backend share one endpoint directory.

use std::path::PathBuf;

use lsp_profiler::{Classification, CpuBackend, ProcessCandidate};

use crate::cpu::OspreyCpu;
use crate::endpoint::Endpoints;
use crate::process;

#[derive(Debug)]
pub(crate) struct LiveAttach {
    endpoints: Endpoints,
    cpu: OspreyCpu,
}

impl LiveAttach {
    pub(crate) fn new(socket_dir: Option<PathBuf>) -> Self {
        let endpoints = Endpoints::new(socket_dir);
        Self {
            cpu: OspreyCpu::new(endpoints.clone()),
            endpoints,
        }
    }

    pub(crate) fn classify(&self, candidates: &[ProcessCandidate]) -> Vec<(usize, Classification)> {
        process::classify(&self.endpoints, candidates)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "one signature with attach_none.rs, where no platform backend exists"
    )]
    pub(crate) fn cpu(&self) -> Option<&dyn CpuBackend> {
        Some(&self.cpu)
    }
}
