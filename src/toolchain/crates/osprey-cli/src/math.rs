//! `osprey math` — print the Osprey module generated from a Lean math source
//! ([MATH-CODE-COMMANDS], `docs/specs/tooling/MathToCode.md`).
//!
//! The command writes nothing: a project build generates and stores the same
//! module itself. It exists so the generated code of one formula can be read.

use std::process::ExitCode;

const USAGE: &str = "usage: osprey math <file.lean>";

/// Entry point for `osprey math`; `args` excludes the `math` subcommand word.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let [file] = args else {
        return crate::usage_error(USAGE);
    };
    let source = match std::fs::read_to_string(file) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{file}: {error}");
            return ExitCode::FAILURE;
        }
    };
    match osprey_math::generate(file, &source) {
        Ok(module) => {
            print!("{module}");
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            eprintln!("{file}:{refusal}");
            ExitCode::FAILURE
        }
    }
}
