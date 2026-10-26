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
        memory: "gc".to_string(),
        target: "native".to_string(),
        exports: MobileExports::All,
        output: None,
        debug: false,
        debug_options: debug_options::DebugOptions::default(),
        profile: false,
        flavor: None,
    }
}

/// Assemble one source with the production project resolver for ABI tests.
pub(crate) fn assembled_program(source: &str, root: &str, file: &str) -> osprey_ast::Program {
    osprey_project::assemble(
        &osprey_project::ProjectConfig::for_root(std::path::Path::new(root)),
        &[osprey_project::SourceFile {
            path: PathBuf::from(file),
            flavor: Flavor::Default,
            source: source.to_string(),
            program: osprey_syntax::parse_program(source).program,
        }],
    )
    .expect("assemble fixture")
    .program
}
