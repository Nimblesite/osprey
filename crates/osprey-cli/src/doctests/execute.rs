//! Documentation examples use the ordinary compiler gates and backends.
//! Implements [DOC-DOCTEST-HARNESS].

use crate::project::CompilationInput;
use crate::{build_kind, native_executable, report_type_errors, sandbox, target_error, Cli};
use osprey_ast::DocExample;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Output};

pub(super) fn check(
    cli: &Cli,
    input: &CompilationInput,
    example: &DocExample,
) -> Result<(), String> {
    if report_type_errors(input) != 0 {
        return Err("documentation example failed compilation".into());
    }
    let input = CompilationInput::script(
        input.display_path(),
        input.source().to_string(),
        super::synthesize::reachable(input.program().clone()),
    );
    validate(cli, &input)?;
    if !example.run {
        return Ok(());
    }
    let (mut command, temporary) = executable(cli, &input)?;
    if cli.memory == "arc" {
        let _ = command.env("OSPREY_ARC_DEBUG", "1");
    }
    let output = super::process::capture(&mut command);
    if let Some(path) = temporary {
        let _ = std::fs::remove_file(path);
    }
    compare(&output?, example, &cli.memory)
}

fn validate(cli: &Cli, input: &CompilationInput) -> Result<(), String> {
    let violations = sandbox::violations(input.program(), cli.policy);
    if !violations.is_empty() {
        return Err(violations.join("\n"));
    }
    if target_error(cli, input).is_some() {
        return Err("documentation example failed compilation".into());
    }
    Ok(())
}

fn executable(cli: &Cli, input: &CompilationInput) -> Result<(Command, Option<PathBuf>), String> {
    if cli.target == "wasm32" {
        return wasm_executable(input);
    }
    if cli.target != "native" {
        return Err(format!(
            "runnable doctests require native or wasm32, found {}",
            cli.target
        ));
    }
    let (path, temporary) =
        native_executable(input, &cli.memory, build_kind(cli)).map_err(failed)?;
    Ok((Command::new(&path), temporary.then_some(path)))
}

fn wasm_executable(input: &CompilationInput) -> Result<(Command, Option<PathBuf>), String> {
    let path = std::env::temp_dir().join(format!(
        "{}.wasm",
        crate::scratch_stem(input.display_path())
    ));
    crate::wasm::build(input.debug_path(), input.program(), &path).map_err(failed)?;
    let runner = crate::toolchain::tool("OSPREY_WASM_RUN", "wasmtime");
    let mut command = Command::new(runner);
    let _ = command.arg(&path);
    Ok((command, Some(path)))
}

fn failed(_: ExitCode) -> String {
    "documentation example could not be compiled".into()
}

fn compare(output: &Output, example: &DocExample, memory: &str) -> Result<(), String> {
    if !output.status.success() {
        return Err(format!(
            "example exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let expected = expected_bytes(example)?;
    if output.stdout != expected {
        return Err(format!(
            "stdout mismatch\nexpected: {:?}\nactual: {:?}",
            String::from_utf8_lossy(&expected),
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    check_arc(&output.stderr, memory)
}

fn expected_bytes(example: &DocExample) -> Result<Vec<u8>, String> {
    match example.expected_output.as_deref() {
        Some("") => Ok(Vec::new()),
        Some(text) => Ok(format!("{text}\n").into_bytes()),
        None => Err("runnable documentation example has no expected output".into()),
    }
}

fn check_arc(stderr: &[u8], memory: &str) -> Result<(), String> {
    if memory != "arc" {
        return Ok(());
    }
    let text = String::from_utf8_lossy(stderr);
    let counts: Vec<_> = text
        .lines()
        .filter_map(|line| {
            line.strip_prefix("[osp-arc] exit: ")?
                .split_once(" live objects")
                .map(|(count, _)| count)
        })
        .collect();
    if counts.as_slice() != ["0"] {
        return Err(format!(
            "expected one ARC exit sentinel with zero live objects: {text}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_arc, expected_bytes};
    use osprey_ast::DocExample;

    #[test]
    fn arc_requires_one_zero_object_exit_record() {
        assert!(check_arc(b"[osp-arc] exit: 0 live objects\n", "arc").is_ok());
        for stderr in [
            "",
            "ordinary output\n",
            "[osp-arc] exit: 1 live objects\n",
            "[osp-arc] exit: 0 live objects\n[osp-arc] exit: 0 live objects\n",
            "[osp-arc] exit: unknown live objects\n",
        ] {
            assert!(check_arc(stderr.as_bytes(), "arc").is_err(), "{stderr}");
        }
        assert!(check_arc(b"", "default").is_ok());
        assert!(check_arc(b"", "gc").is_ok());
    }

    #[test]
    fn expected_output_preserves_empty_and_unicode_whitespace() {
        for (output, bytes) in [
            ("", b"".as_slice()),
            ("hello \t", b"hello \t\n"),
            ("naïve\nsecond", "naïve\nsecond\n".as_bytes()),
        ] {
            let example = DocExample {
                code: String::new(),
                expected_output: Some(output.into()),
                run: true,
            };
            assert_eq!(expected_bytes(&example).as_deref(), Ok(bytes));
        }
        let example = DocExample {
            code: String::new(),
            expected_output: None,
            run: true,
        };
        assert!(expected_bytes(&example).is_err());
    }
}
