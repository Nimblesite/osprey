use super::*;
use crate::invocation::*;
use crate::linking::*;
use crate::native::*;
use crate::pipeline::*;
use crate::profiling::*;
use crate::project::CompilationInput;
use crate::sandbox::Policy;
use std::path::PathBuf;

mod execution;
mod invocation;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn cli(path: impl Into<String>, mode: &str, policy: Policy) -> Cli {
    Cli {
        path: path.into(),
        mode: mode.to_string(),
        quiet: true,
        policy,
        memory: "default".to_string(),
        target: "native".to_string(),
        exports: MobileExports::All,
        output: None,
        debug: false,
        profile: false,
        flavor: None,
    }
}
