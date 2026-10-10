//! Shared corpus compilation and exact diagnostic formatting.
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn repo_root() -> PathBuf {
    // `crates/osprey-cli` -> repo root. Left un-canonicalized (no fallible call):
    // the `..` segments resolve fine for `read_dir`, and `strip_prefix` below
    // uses this same prefix.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Every file with extension `ext` under `dir`, recursively, sorted for stable
/// failure output.
pub(super) fn sources(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(dir, ext, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// Compile one source the way `osprey --run` does: resolve the flavor from the
/// path and any `// osprey: flavor=…` marker, parse, gate on type errors, then
/// lower to IR. `Ok(ir_len)` on success, else the failing stage + reason.
///
/// The flavor must come from the path, not a hardwired `Flavor::Default`:
/// `failscompilation/ml_*.ospo` are ML-flavor negatives selected by a leading
/// marker, and grading them with the brace grammar would pin the wrong
/// rejection path. Implements [FLAVOR-SELECT] (docs/specs/0023).
pub(super) fn compile(path: &Path, source: &str) -> Result<usize, String> {
    let parsed = osprey_syntax::parse_program_for_path(&path.to_string_lossy(), source);
    if !parsed.errors.is_empty() {
        let messages = parsed
            .errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>();
        return Err(format!("parse: {}", messages.join("; ")));
    }
    let (program, backend) =
        assemble_if_needed(path, source, parsed.program).map_err(|errors| {
            let first = errors.first().map(|error| error.message.as_str());
            format!(
                "project: {}",
                first.unwrap_or("assembly failed with no diagnostic")
            )
        })?;
    let type_errors = osprey_types::check_program(&program);
    if let Some(first) = type_errors.first() {
        return Err(format!("typecheck: {first:?}"));
    }
    osprey_codegen::compile_program(backend.as_ref().unwrap_or(&program))
        .map(|ir| ir.len())
        .map_err(|e| format!("codegen: {e:?}"))
}

/// Resolve a module-bearing source through the project layer before grading it.
///
/// A namespace, module, import or signature is not a program until
/// `osprey_project` resolves its paths and flattens the graph ([MODULES-MODEL]);
/// `Tax::add` is an unknown identifier until then. Grading the raw parse would
/// report every module program as broken while the CLI runs it perfectly — so
/// this reproduces the CLI's own single-source path. Ordinary scripts skip it
/// and keep exactly the IR and symbol names they had. The second program is
/// the one the backend lowers, when it differs ([MODULES-OPAQUE-TYPES]).
fn assemble_if_needed(
    path: &Path,
    source: &str,
    program: osprey_ast::Program,
) -> Result<Assembled, Vec<osprey_project::ProjectError>> {
    if !osprey_project::needs_assembly(&program) {
        return Ok((program, None));
    }
    let source_file = osprey_project::SourceFile {
        path: path.to_path_buf(),
        flavor: osprey_syntax::resolve_flavor(None, &path.to_string_lossy(), source)
            .unwrap_or(osprey_syntax::Flavor::Default),
        source: source.to_string(),
        program,
    };
    osprey_project::assemble_one(source_file)
        .map(|assembled| (assembled.program, assembled.backend))
}

type Assembled = (osprey_ast::Program, Option<osprey_ast::Program>);

/// Every diagnostic the CLI would print for one rejected source, path prefix
/// stripped so the golden is location-independent. Mirrors the two shapes
/// `main.rs` emits: `{path}:{line}:{col}: {msg}` for a located error and
/// `{path}: {msg}` for one with no position.
pub(super) fn rejection_diagnostics(path: &Path, source: &str) -> String {
    let parsed = osprey_syntax::parse_program_for_path(&path.to_string_lossy(), source);
    if !parsed.errors.is_empty() {
        return diagnostic_lines(
            parsed
                .errors
                .iter()
                .map(|e| located(Some(e.position.line), Some(e.position.column), &e.message)),
        );
    }
    // A module-bearing fixture is rejected by the project layer before the
    // checker ever sees it, exactly as the CLI's single-source path does.
    let (program, backend) = match assemble_if_needed(path, source, parsed.program) {
        Ok(assembled) => assembled,
        Err(errors) => {
            return diagnostic_lines(errors.iter().map(|e| located(e.line, e.column, &e.message)));
        }
    };
    let out = diagnostic_lines(osprey_types::check_program(&program).iter().map(|e| {
        located(
            e.position.map(|p| p.line),
            e.position.map(|p| p.column),
            &e.message,
        )
    }));
    if !out.is_empty() {
        return out;
    }
    // A program the frontend accepts can still be rejected at lowering (the
    // CLI prints these as `{path}: {msg}` too) — e.g. a recursive function
    // whose signature never became concrete enough to emit.
    osprey_codegen::compile_program(backend.as_ref().unwrap_or(&program))
        .err()
        .map(|e| format!("{e}\n"))
        .unwrap_or_default()
}

/// One golden line: `line:column: message`, or the bare message when the
/// rejecting stage recorded no position.
fn located(
    line: Option<impl std::fmt::Display>,
    column: Option<impl std::fmt::Display>,
    message: &str,
) -> String {
    match (line, column) {
        (Some(line), Some(column)) => format!("{line}:{column}: {message}"),
        _ => message.to_string(),
    }
}

fn diagnostic_lines(lines: impl Iterator<Item = String>) -> String {
    lines.map(|line| line + "\n").collect()
}
