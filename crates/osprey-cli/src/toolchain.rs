//! Spawning the external toolchain a cross-target backend drives (clang,
//! `wasm-ld`, `xcrun`). Shared by the wasm and iOS drivers so an overridable
//! tool name, a failed exit and a missing program are reported one way.

use std::process::{Command, ExitCode};

/// The tool to invoke for `env`, defaulting to `default` when unset.
pub(crate) fn tool(env: &str, default: &str) -> String {
    std::env::var(env).unwrap_or_else(|_| default.to_string())
}

/// Spawn `prog args`, mapping a non-zero exit or spawn failure to a CLI
/// failure. `hint` names the toolchain a spawn failure suggests installing.
pub(crate) fn run_tool(prog: &str, args: &[String], hint: &str) -> Result<(), ExitCode> {
    match Command::new(prog).args(args).status() {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err(fail(&format!("{prog} failed"))),
        Err(e) => Err(fail(&format!("could not invoke {prog}: {e} — {hint}"))),
    }
}

/// Print a backend build error and yield the failure exit code.
pub(crate) fn fail(msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_falls_back_to_default_when_env_unset() {
        assert_eq!(
            tool("OSPREY_WASM_CC_DEFINITELY_UNSET_XYZ", "clang"),
            "clang"
        );
    }

    #[test]
    fn run_tool_reports_success_failure_and_a_missing_program() {
        // A program that exits 0 succeeds; a non-zero exit and a missing program
        // are both mapped to a CLI failure (exercising `run_tool` + `fail`).
        assert!(run_tool("true", &[], "install it").is_ok());
        assert!(run_tool("false", &[], "install it").is_err());
        assert!(run_tool("/no/such/tool/osprey_xyz", &[], "install it").is_err());
    }
}
