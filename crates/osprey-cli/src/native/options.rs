//! The native compiler's complete per-invocation build policy.
use crate::debug_options::{DebugOptimization, DebugOptions};
use crate::invocation::Cli;
use osprey_debug::BuildKind;

#[derive(Clone, Copy)]
pub(crate) struct NativeOptions<'a> {
    pub(crate) memory: &'a str,
    pub(crate) kind: BuildKind,
    pub(crate) debug: DebugOptions,
}

impl<'a> NativeOptions<'a> {
    pub(crate) fn new(memory: &'a str, kind: BuildKind) -> Self {
        Self {
            memory,
            kind,
            debug: DebugOptions::default(),
        }
    }

    pub(crate) fn from_cli(cli: &'a Cli) -> Self {
        Self {
            memory: &cli.memory,
            kind: crate::pipeline::build_kind(cli),
            debug: cli.debug_options,
        }
    }

    pub(crate) fn optimization(self) -> String {
        if self.debug.optimization == DebugOptimization::Unoptimized {
            "-O0".into()
        } else {
            super::opt_flag(self.kind)
        }
    }

    pub(crate) fn cacheable(self) -> bool {
        matches!(self.kind, BuildKind::Release | BuildKind::Coverage)
            && !self.debug.keeps_artifacts()
            && self.debug.optimization == DebugOptimization::Environment
    }
}
