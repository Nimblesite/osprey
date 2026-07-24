//! In-process diagnostics.
//!
//! The TypeScript server wrote each edit to a temp file, shelled out to the
//! `osprey` binary, and scraped stderr with a wall of regexes. Here the
//! compiler front-end is called directly: [`osprey_syntax::parse_program`] for
//! syntax errors and [`osprey_types::check_program`] for type errors, mapped to
//! the [`lspkit_server::Diagnostic`] the diagnostics bus fans out.

use lspkit_server::{Diagnostic, Severity};
use lspkit_vfs::PositionEncoding;
use osprey_ast::{Position, Program};
use osprey_project::{AssembledProject, ProjectError};
use std::path::{Path, PathBuf};

const SOURCE: &str = "osprey";

/// Compute diagnostics for `source`. The document `path` selects the flavor
/// (`.ospml` ⇒ ML), so a layout-flavor file is parsed by its own frontend
/// instead of misreported as broken Default syntax. Syntax errors are reported
/// alone (an unparsable file is not type-checked, matching the CLI gate); a clean
/// parse is then type-checked.
#[must_use]
pub fn compute(source: &str, path: &str, encoding: PositionEncoding) -> Vec<Diagnostic> {
    // [FLAVOR-SELECT] makes a marker/extension disagreement a hard error, and
    // the CLI refuses to build such a file. Resolve FIRST and report the
    // conflict as the document's only finding: guessing a flavor would parse
    // the file with the wrong frontend and bury the real fault under a cascade
    // of phantom syntax and type errors.
    let flavor = match osprey_syntax::resolve_flavor(None, path, source) {
        Ok(flavor) => flavor,
        Err(message) => {
            return vec![diagnostic(
                source,
                marker_position(source),
                &message,
                "flavor-error",
                encoding,
            )]
        }
    };
    let parsed = osprey_syntax::parse_program_with_flavor(source, flavor);
    if !parsed.errors.is_empty() {
        return parsed
            .errors
            .iter()
            .map(|e| diagnostic(source, e.position, &e.message, "syntax-error", encoding))
            .collect();
    }
    if let Some(diagnostics) = project_diagnostics(source, path, &parsed.program, encoding) {
        return diagnostics;
    }
    if let Some(d) = standalone_diagnostics(source, path, flavor, &parsed.program, encoding) {
        return d;
    }
    type_diagnostics(source, &parsed.program, encoding)
}

/// Where to underline a flavor conflict: the `// osprey: flavor=` marker line
/// itself (1-based), since that is the half of the disagreement the author can
/// edit in this buffer. Falls back to line 1 when no marker is present — which
/// `resolve_flavor` only errors without if the marker names an unknown flavor.
fn marker_position(source: &str) -> Position {
    let line = source
        .lines()
        .position(|l| l.trim_start().starts_with("//") && l.contains("osprey: flavor="))
        .and_then(|i| u32::try_from(i + 1).ok())
        .unwrap_or(1);
    Position { line, column: 0 }
}

fn type_diagnostics(
    source: &str,
    program: &Program,
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    osprey_types::check_program(program)
        .iter()
        .map(|e| {
            let pos = e.position.unwrap_or(Position { line: 1, column: 0 });
            diagnostic(source, pos, &e.message, "type-error", encoding)
        })
        .collect()
}

/// Single-source assembly for a module-bearing file that no project claims —
/// a standalone script or a file outside the manifest's `source_roots` (test
/// suites in `test/`). Mirrors the CLI's `--check`/`osprey test` path so the
/// editor never reports `unknown identifier` for the file's own modules.
/// `None` for ordinary module-free scripts, which skip assembly entirely.
fn standalone_diagnostics(
    source: &str,
    uri: &str,
    flavor: osprey_syntax::Flavor,
    program: &Program,
    encoding: PositionEncoding,
) -> Option<Vec<Diagnostic>> {
    if !osprey_project::needs_assembly(program) {
        return None;
    }
    let file = file_path(uri).unwrap_or_else(|| PathBuf::from(uri));
    let source_file = osprey_project::SourceFile {
        path: file.clone(),
        flavor,
        source: source.to_string(),
        program: program.clone(),
    };
    Some(assembly_diagnostics(
        osprey_project::assemble_one(source_file),
        source,
        &file,
        encoding,
    ))
}

/// Map an assembly outcome to diagnostics for the open `file`: type errors on
/// success, project errors on failure.
fn assembly_diagnostics(
    assembled: Result<AssembledProject, Vec<ProjectError>>,
    source: &str,
    file: &Path,
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    match assembled {
        Ok(project) => assembled_type_errors(source, file, &project, encoding),
        Err(errors) => project_errors(source, file, &errors, encoding),
    }
}

fn project_diagnostics(
    source: &str,
    uri: &str,
    program: &Program,
    encoding: PositionEncoding,
) -> Option<Vec<Diagnostic>> {
    let file = file_path(uri)?;
    let root = project_root(&file)?;
    let (config, mut sources) = match osprey_project::load(&root) {
        Ok(loaded) => loaded,
        Err(errors) => return Some(project_errors(source, &file, &errors, encoding)),
    };
    let source_file = sources
        .iter_mut()
        .find(|candidate| same_path(&candidate.path, &file))?;
    source_file.source = source.to_string();
    source_file.program = program.clone();
    Some(assembly_diagnostics(
        osprey_project::assemble(&config, &sources),
        source,
        &file,
        encoding,
    ))
}

fn assembled_type_errors(
    source: &str,
    file: &Path,
    project: &AssembledProject,
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    osprey_types::check_program(&project.program)
        .iter()
        .filter_map(|error| {
            let position = if let Some(global) = error.position {
                let (owner, line) = project.source_at_line(global.line)?;
                same_path(&owner.path, file).then_some(Position {
                    line,
                    column: global.column,
                })?
            } else {
                let is_entry = project
                    .entry()
                    .is_some_and(|entry| same_path(&entry.path, file));
                is_entry.then_some(Position { line: 1, column: 0 })?
            };
            Some(diagnostic(
                source,
                position,
                &error.message,
                "type-error",
                encoding,
            ))
        })
        .collect()
}

fn project_errors(
    source: &str,
    file: &Path,
    errors: &[ProjectError],
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    errors
        .iter()
        .filter(|error| {
            error
                .path
                .as_deref()
                .is_none_or(|path| same_path(path, file))
        })
        .map(|error| {
            let position = Position {
                line: error
                    .line
                    .and_then(|line| u32::try_from(line).ok())
                    .unwrap_or(1),
                column: error
                    .column
                    .and_then(|column| u32::try_from(column).ok())
                    .unwrap_or(0),
            };
            diagnostic(source, position, &error.message, "project-error", encoding)
        })
        .collect()
}

fn project_root(file: &Path) -> Option<PathBuf> {
    file.parent()?
        .ancestors()
        .find(|directory| directory.join("osprey.toml").is_file())
        .map(Path::to_path_buf)
}

fn same_path(left: &Path, right: &Path) -> bool {
    let normalize =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    normalize(left) == normalize(right)
}

fn file_path(uri: &str) -> Option<PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    let decoded = percent_decode(encoded)?;
    #[cfg(windows)]
    let decoded = decoded
        .strip_prefix('/')
        .filter(|path| path.as_bytes().get(1) == Some(&b':'))
        .unwrap_or(&decoded)
        .to_string();
    Some(PathBuf::from(decoded))
}

fn percent_decode(encoded: &str) -> Option<String> {
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte == b'%' {
            let high = hex(*bytes.get(index.saturating_add(1))?)?;
            let low = hex(*bytes.get(index.saturating_add(2))?)?;
            decoded.push((high << 4) | low);
            index = index.saturating_add(3);
        } else {
            decoded.push(byte);
            index = index.saturating_add(1);
        }
    }
    String::from_utf8(decoded).ok()
}

const fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Build one error diagnostic spanning the offending line from `pos` onward.
fn diagnostic(
    source: &str,
    pos: Position,
    message: &str,
    code: &str,
    encoding: PositionEncoding,
) -> Diagnostic {
    let line = pos.line.saturating_sub(1);
    let line_text = nth_line(source, line);
    // `pos.column` is a tree-sitter byte offset; the wire range is in the
    // negotiated encoding, so re-measure the line prefix in those units.
    let start = byte_col_to_encoding(line_text, pos.column, encoding);
    let end = line_text
        .map_or(0, |l| crate::text::measure(l, encoding))
        .max(start.saturating_add(1));
    Diagnostic::new(Severity::Error, message, (line, start, line, end))
        .with_source(SOURCE)
        .with_code(code)
}

/// Zero-based `line`'s text, or `None` if absent.
fn nth_line(source: &str, line: u32) -> Option<&str> {
    usize::try_from(line)
        .ok()
        .and_then(|i| source.lines().nth(i))
}

/// Convert a byte column within `line` into `encoding`'s character units.
fn byte_col_to_encoding(line: Option<&str>, byte_col: u32, encoding: PositionEncoding) -> u32 {
    let Some(line) = line else {
        return byte_col;
    };
    let idx = usize::try_from(byte_col).unwrap_or(usize::MAX);
    line.get(..idx)
        .map_or(byte_col, |prefix| crate::text::measure(prefix, encoding))
}

#[cfg(test)]
mod tests {
    use super::*;
    const U16: PositionEncoding = PositionEncoding::Utf16;

    const OSP: &str = "file:///a.osp";

    #[test]
    fn clean_program_has_no_diagnostics() {
        let diags = compute("fn main() -> Unit = print(\"hi\")\n", OSP, U16);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn ml_flavor_file_is_parsed_by_its_own_frontend() {
        // The exact editor regression: a layout, curry-by-default `.ospml` source
        // (bare `:` signature, `\` lambda, whitespace application) must parse
        // cleanly under the ML frontend rather than be flagged as broken Default
        // syntax. Selecting the flavor by the document path is what fixes it.
        let ml = "inc : int -> int\ninc x = x + 1\nmain () =\n    print \"v=${toString (inc 41)}\"\n    0\n";
        let clean = compute(ml, "file:///tour.ospml", U16);
        assert!(
            clean.is_empty(),
            "ML file should not syntax-error: {clean:?}"
        );
        // The same source under a `.osp` path is genuinely not Default syntax, so
        // the Default frontend still reports errors — proving the path drives the
        // flavor rather than the diagnostics silently accepting everything.
        let as_default = compute(ml, OSP, U16);
        assert!(
            !as_default.is_empty(),
            "ML source is not valid Default syntax"
        );
    }

    #[test]
    fn a_flavor_marker_that_fights_the_extension_is_reported_not_guessed() {
        // [FLAVOR-SELECT] makes a marker/extension disagreement a HARD error so
        // the editor and the CLI never read one file two ways. The CLI refuses
        // to build it; the editor used to fall back to Default and show the
        // file as green, so a `.ospml` mislabelled `flavor=default` looked fine
        // right up until the build failed. Report it instead of guessing.
        let src = "// osprey: flavor=default\ninc x = x + 1\n";
        let diags = compute(src, "file:///tour.ospml", U16);
        let first = diags.first().expect("the disagreement must be reported");
        assert_eq!(first.code.as_deref(), Some("flavor-error"), "{diags:?}");
        assert!(first.message.contains("disagree"), "{}", first.message);
        // The conflict is the ONLY finding: parsing under a guessed flavor
        // would bury it under a cascade of phantom syntax errors.
        assert_eq!(diags.len(), 1, "{diags:?}");
        // An agreeing marker stays silent and still selects the ML frontend.
        let agree = "// osprey: flavor=ml\ninc x = x + 1\n";
        assert!(compute(agree, "file:///tour.ospml", U16).is_empty());
    }

    #[test]
    fn syntax_error_is_reported_with_source_and_code() {
        let diags = compute("fn main( = 1\n", OSP, U16);
        assert!(!diags.is_empty());
        let first = diags.first().expect("diagnostic");
        assert_eq!(first.severity, Severity::Error);
        assert_eq!(first.source.as_deref(), Some("osprey"));
        assert_eq!(first.code.as_deref(), Some("syntax-error"));
    }

    #[test]
    fn type_error_surfaces_when_parse_is_clean() {
        // Referencing an unknown function type-checks but does not parse-fail.
        let diags = compute("fn main() -> int = nope(1)\n", OSP, U16);
        assert!(!diags.is_empty(), "an unknown call type-errors");
        assert!(
            diags
                .iter()
                .all(|d| d.code.as_deref() == Some("type-error")),
            "{diags:?}"
        );
        // Every diagnostic carries the osprey source, is an error, and spans a
        // non-empty range on its line.
        for d in &diags {
            assert_eq!(d.severity, Severity::Error);
            assert_eq!(d.source.as_deref(), Some("osprey"));
            let (sl, sc, el, ec) = d.range;
            assert_eq!(sl, el, "single-line span: {d:?}");
            assert!(ec > sc, "non-empty span: {d:?}");
            assert!(!d.message.is_empty());
        }
    }

    #[test]
    fn diagnostic_columns_are_remeasured_in_the_negotiated_encoding() {
        // A multi-byte identifier shifts the byte column; the wire range must be
        // re-measured so the same program reports a wider start under UTF-8 than
        // under UTF-16 when the error sits past a multi-byte char.
        let src = "fn café() -> int = nope(1)\n";
        let u16 = compute(src, OSP, PositionEncoding::Utf16);
        let u8 = compute(src, OSP, PositionEncoding::Utf8);
        // Both encodings find at least one diagnostic on the first line.
        assert!(!u16.is_empty() && !u8.is_empty(), "{u16:?} {u8:?}");
        assert!(u16.iter().all(|d| d.range.0 == 0));
        assert!(u8.iter().all(|d| d.range.0 == 0));
    }

    #[cfg(unix)]
    #[test]
    fn module_files_use_the_assembled_project_graph() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for relative in [
            "examples/projects/modules/src/main.ospml",
            "examples/projects/modules/src/web/pages.ospml",
        ] {
            let path = root.join(relative);
            let source = std::fs::read_to_string(&path).expect("read module example");
            let uri = format!("file://{}", path.display());
            let diagnostics = compute(&source, &uri, U16);
            assert!(diagnostics.is_empty(), "{relative}: {diagnostics:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn module_bearing_files_outside_project_roots_are_assembled_standalone() {
        // The editor regression: a self-contained test suite living in `test/`
        // (outside `source_roots = ["src"]`) defines `module Money` and calls
        // `Money::positive`. `osprey <file> --check` and `osprey test` accept it
        // via single-source assembly; the LSP must not spray
        // `unknown identifier `Money::positive`` by checking the raw AST.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = root.join("examples/projects/modules/test/accounts.test.ospml");
        let source = std::fs::read_to_string(&path).expect("read module test suite");
        let uri = format!("file://{}", path.display());
        let diagnostics = compute(&source, &uri, U16);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[cfg(unix)]
    #[test]
    fn project_diagnostics_map_resolution_and_type_errors_to_the_open_file() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let main_path = root.join("examples/projects/modules/src/main.ospml");
        let main = std::fs::read_to_string(&main_path).expect("read ML module example");
        let unresolved = main.replace(
            "import \"bank/web\" as web",
            "import \"missing/web\" as web",
        );
        let diagnostics = compute(&unresolved, &format!("file://{}", main_path.display()), U16);
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code.as_deref() == Some("project-error")),
            "{diagnostics:?}"
        );

        let ill_typed = main.replace("served = Metrics::track boot", "served = Metrics::track 42");
        let diagnostics = compute(&ill_typed, &format!("file://{}", main_path.display()), U16);
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code.as_deref() == Some("type-error")),
            "{diagnostics:?}"
        );

        let source = "print(missing)\n";
        let project = AssembledProject {
            program: osprey_syntax::parse_program(source).program,
            entry_prologue: Vec::new(),
            entry_source: 0,
            sources: vec![osprey_project::SourceMetadata {
                index: 0,
                path: PathBuf::from("entry.osp"),
                flavor: osprey_syntax::Flavor::Default,
                source: source.to_string(),
                global_line_start: 1,
                global_line_end: 1,
            }],
            source_name_by_mangled: std::collections::BTreeMap::new(),
        };
        let diagnostics = assembled_type_errors(
            source,
            Path::new("entry.osp"),
            &project,
            PositionEncoding::Utf16,
        );
        assert!(!diagnostics.is_empty(), "{diagnostics:?}");
        assert!(assembled_type_errors(
            source,
            Path::new("other.osp"),
            &project,
            PositionEncoding::Utf16,
        )
        .is_empty());
    }

    #[test]
    fn file_uri_decoding_is_strict_and_handles_spaces() {
        assert_eq!(
            file_path("file:///tmp/with%20space/a.osp"),
            Some(PathBuf::from("/tmp/with space/a.osp"))
        );
        assert_eq!(
            file_path("file:///tmp/with%2fslash.osp"),
            Some(PathBuf::from("/tmp/with/slash.osp"))
        );
        assert!(file_path("untitled:buffer").is_none());
        assert!(file_path("file:///tmp/bad%GG.osp").is_none());
        assert!(file_path("file:///tmp/truncated%.osp").is_none());
    }
}
