//! In-process diagnostics.
//!
//! The TypeScript server wrote each edit to a temp file, shelled out to the
//! `osprey` binary, and scraped stderr with a wall of regexes. Here the
//! compiler front-end is called directly: [`osprey_syntax::parse_program`] for
//! syntax errors and [`osprey_types::check_program`] for type errors, mapped to
//! the [`lspkit_server::Diagnostic`] the diagnostics bus fans out.
//! Implements [LSP-DIAGNOSTICS].

use lspkit_server::{Diagnostic, Severity};
use lspkit_vfs::PositionEncoding;
use osprey_ast::{Position, Program};
use osprey_project::{AssembledProject, ProjectError};
use std::path::{Path, PathBuf};

// URI/path resolution and project discovery live once, in `workspace`: the
// editor's idea of which files form a program must be the compiler's.
use crate::workspace::{file_path, project_root, same_path};

const SOURCE: &str = "osprey";

/// Compute diagnostics for `source`. The document `path` selects the flavor
/// (`.ospml` ⇒ ML), so a layout-flavor file is parsed by its own frontend
/// instead of misreported as broken Default syntax. Syntax errors are reported
/// alone (an unparsable file is not type-checked, matching the CLI gate); a clean
/// parse is then type-checked.
#[must_use]
pub(crate) fn compute(source: &str, path: &str, encoding: PositionEncoding) -> Vec<Diagnostic> {
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
    let mut diagnostics = project_diagnostics(source, path, &parsed.program, encoding)
        .or_else(|| standalone_diagnostics(source, path, flavor, &parsed.program, encoding))
        .unwrap_or_else(|| type_diagnostics(source, &parsed.program, encoding));
    diagnostics.extend(skip_diagnostics(source, &parsed.program, encoding));
    diagnostics
}

/// One diagnostic per statically-skipped test case, on the `test` call's own
/// line ([TESTING-SKIP-WARNING-STATIC]): a skipped test must be loud in the
/// editor, so these ride alongside whatever errors the file already has. An
/// unparsable file never reaches here — it reports its syntax error alone.
///
/// A skip that names a reason is a Warning; one that names none is an Error
/// ([TESTING-SKIP-REASON]). Both carry the same `test-skipped` code, because
/// they are the same rule reported at two strengths.
fn skip_diagnostics(
    source: &str,
    program: &Program,
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    crate::testing::collect_tests(program)
        .iter()
        .filter_map(|case| {
            let report = case.skip_diagnostic()?;
            let pos = case.position.unwrap_or(Position { line: 1, column: 0 });
            let build = if report.unexplained {
                diagnostic
            } else {
                warning
            };
            Some(build(
                source,
                pos,
                &report.message,
                "test-skipped",
                encoding,
            ))
        })
        .collect()
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
    let mut diagnostics: Vec<_> = osprey_types::check_program(program)
        .iter()
        .map(|e| {
            let pos = e.position.unwrap_or(Position { line: 1, column: 0 });
            diagnostic(source, pos, &e.message, "type-error", encoding)
        })
        .collect();
    if diagnostics.is_empty() {
        diagnostics.extend(
            osprey_types::redundant_annotations(program)
                .into_iter()
                .map(|raised| {
                    warning(
                        source,
                        raised.position.unwrap_or(Position { line: 1, column: 0 }),
                        &raised.message,
                        raised.rule,
                        encoding,
                    )
                }),
        );
    }
    diagnostics
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
    let errors = osprey_types::check_program(&project.program);
    let mut diagnostics: Vec<_> = errors
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
        .collect();
    if errors.is_empty() {
        diagnostics.extend(
            osprey_types::redundant_annotations_where(&project.program, |position| {
                position
                    .and_then(|p| project.source_at_line(p.line))
                    .is_some_and(|(owner, _)| same_path(&owner.path, file))
            })
            .into_iter()
            .filter_map(|raised| {
                let global = raised.position?;
                let (owner, line) = project.source_at_line(global.line)?;
                same_path(&owner.path, file).then(|| {
                    warning(
                        source,
                        Position {
                            line,
                            column: global.column,
                        },
                        &raised.message,
                        raised.rule,
                        encoding,
                    )
                })
            }),
        );
    }
    diagnostics
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

/// Build one error diagnostic spanning the offending line from `pos` onward.
fn diagnostic(
    source: &str,
    pos: Position,
    message: &str,
    code: &str,
    encoding: PositionEncoding,
) -> Diagnostic {
    ranged(source, pos, Severity::Error, message, code, encoding)
}

/// Build one warning diagnostic spanning the offending line from `pos` onward.
fn warning(
    source: &str,
    pos: Position,
    message: &str,
    code: &str,
    encoding: PositionEncoding,
) -> Diagnostic {
    ranged(source, pos, Severity::Warning, message, code, encoding)
}

fn ranged(
    source: &str,
    pos: Position,
    severity: Severity,
    message: &str,
    code: &str,
    encoding: PositionEncoding,
) -> Diagnostic {
    let line = pos.line.saturating_sub(1);
    let line_text = nth_line(source, line);
    // `pos.column` is a tree-sitter byte offset; re-measure the line prefix in
    // the selected encoding before it crosses the wire. [LSP-ENCODING]
    let start = byte_col_to_encoding(line_text, pos.column, encoding);
    let end = line_text
        .map_or(0, |l| crate::text::measure(l, encoding))
        .max(start.saturating_add(1));
    Diagnostic::new(severity, message, (line, start, line, end))
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
pub(crate) fn assert_redundant_annotations(
    actual: &[Diagnostic],
    expected: &[(&str, crate::model::Span)],
) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (diagnostic, (message, range)) in actual.iter().zip(expected) {
        assert_eq!(diagnostic.severity, Severity::Warning, "{diagnostic:?}");
        assert_eq!(diagnostic.code.as_deref(), Some("redundant-annotation"));
        assert_eq!(diagnostic.source.as_deref(), Some("osprey"));
        assert_eq!(diagnostic.message, *message);
        assert_eq!(diagnostic.range, *range, "{diagnostic:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const U16: PositionEncoding = PositionEncoding::Utf16;

    const OSP: &str = "file:///a.osp";

    #[test]
    fn inferred_program_is_clean_and_redundant_return_is_a_warning() {
        assert!(compute("fn main() = print(\"hi\")\n", OSP, U16).is_empty());
        let diags = compute("fn main() -> Unit = print(\"hi\")\n", OSP, U16);
        assert_redundant_annotations(
            &diags,
            &[(
                "redundant return type annotation on `main`: inference derives `Unit` without it",
                (0, 3, 0, 31),
            )],
        );
    }

    #[test]
    fn ml_flavor_file_is_parsed_by_its_own_frontend() {
        // The exact editor regression: a layout, curry-by-default `.ospml` source
        // (bare `:` signature, `\` lambda, whitespace application) must parse
        // cleanly under the ML frontend rather than be flagged as broken Default
        // syntax. Selecting the flavor by the document path is what fixes it.
        let ml = "inc : int -> int\ninc x = (x + 1) ?: 0\nmain () =\n    print \"v=${toString (inc 41)}\"\n    0\n";
        let diagnostics = compute(ml, "file:///tour.ospml", U16);
        assert_redundant_annotations(
            &diagnostics,
            &[(
                "redundant type signature on `inc`: inference derives `(int) -> int` without it",
                (0, 0, 0, 16),
            )],
        );
        // The same source under a `.osp` path is genuinely not Default syntax, so
        // the Default frontend still reports errors — proving the path drives the
        // flavor rather than the diagnostics silently accepting everything.
        let as_default = compute(ml, OSP, U16);
        assert!(
            !as_default.is_empty(),
            "ML source is not valid Default syntax"
        );
        assert!(
            as_default.iter().all(|diagnostic| {
                diagnostic.severity == Severity::Error
                    && diagnostic.code.as_deref() == Some("syntax-error")
            }),
            "{as_default:?}"
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

    // ---------- [TESTING-SKIP-WARNING] ----------

    #[test]
    fn a_statically_skipped_test_raises_a_warning_diagnostic() {
        // A skipped test is never silent: the `Skip` verdict at the body's
        // result position warns on the `test` call's own line, in both flavors.
        let src = "type Verdict = Pass | Fail(string) | Skip(string)\n\n\
                   test(\"ignored case\", fn() => Skip(\"blocked on #123\"))\n\
                   test(\"live case\", fn() => expect(1, 1))\n";
        let diags = compute(src, OSP, U16);
        assert_eq!(diags.len(), 1, "{diags:?}");
        let warn = diags.first().expect("warning");
        assert_eq!(warn.severity, Severity::Warning);
        assert_eq!(warn.code.as_deref(), Some("test-skipped"));
        assert_eq!(warn.source.as_deref(), Some("osprey"));
        assert_eq!(warn.range.0, 2, "warning sits on the test call's line");
        assert!(
            warn.message
                .contains("test 'ignored case' is skipped: blocked on #123"),
            "{}",
            warn.message
        );

        let ml = "type Verdict = Pass | Fail string | Skip string\n\n\
                  test \"ml ignored\" (\\() => Skip \"later\")\n";
        let diags = compute(ml, "file:///skip.ospml", U16);
        assert_eq!(diags.len(), 1, "{diags:?}");
        let warn = diags.first().expect("ML warning");
        assert_eq!(warn.severity, Severity::Warning);
        assert!(
            warn.message.contains("test 'ml ignored' is skipped: later"),
            "{}",
            warn.message
        );
    }

    #[test]
    fn a_skip_with_no_reason_is_an_error_not_a_warning() {
        // [TESTING-SKIP-REASON] a reasoned skip is a debt someone can weigh, so
        // it warns; a skip that refuses to say why is a defect, so it errors.
        // Every reasonless spelling is caught: `Skip("")`, and the bare `Skip`
        // of a `Verdict` whose skip state declares no payload.
        let src = "type Verdict = Pass | Fail(string) | Skip(string)\n\n\
                   test(\"unexplained\", fn() => Skip(\"\"))\n";
        let diags = compute(src, OSP, U16);
        assert_eq!(diags.len(), 1, "{diags:?}");
        let err = diags.first().expect("error");
        assert_eq!(err.severity, Severity::Error);
        assert_eq!(err.code.as_deref(), Some("test-skipped"));
        assert_eq!(err.range.0, 2, "error sits on the test call's line");
        assert!(
            err.message
                .contains("test 'unexplained' is skipped with no reason"),
            "{}",
            err.message
        );

        let bare = "type Verdict = Pass | Fail | Skip\n\n\
                    test(\"bare skip\", fn() => Skip)\n";
        let diags = compute(bare, OSP, U16);
        assert!(
            diags
                .iter()
                .any(|d| d.severity == Severity::Error
                    && d.code.as_deref() == Some("test-skipped")),
            "{diags:?}"
        );
    }

    #[test]
    fn skip_warnings_ride_alongside_type_errors() {
        // The warning must not vanish because the file has other findings.
        let src = "type Verdict = Pass | Fail(string) | Skip(string)\n\
                   fn broken() -> int = nope(1)\n\
                   test(\"parked\", fn() => Skip(\"awaiting fix\"))\n";
        let diags = compute(src, OSP, U16);
        assert!(
            diags.iter().any(|d| d.severity == Severity::Error),
            "{diags:?}"
        );
        assert!(
            diags
                .iter()
                .any(|d| d.severity == Severity::Warning
                    && d.code.as_deref() == Some("test-skipped")),
            "{diags:?}"
        );
    }

    #[test]
    fn a_dynamically_guarded_test_does_not_warn_statically() {
        // `assume`-style skips are runtime outcomes; only a body that
        // LITERALLY results in `Skip` is statically an ignored test.
        let src = "type Verdict = Pass | Fail(string) | Skip(string)\n\
                   fn guard(n) = match n > 1 { true => Pass false => Skip(\"small\") }\n\
                   test(\"guarded\", fn() => guard(2))\n";
        let diags = compute(src, OSP, U16);
        assert!(
            diags
                .iter()
                .all(|d| d.code.as_deref() != Some("test-skipped")),
            "{diags:?}"
        );
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
    fn diagnostic_columns_are_remeasured_in_the_selected_encoding() {
        // [LSP-DIAGNOSTICS], [LSP-ENCODING]
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
        let main_warnings = [
            (
                "redundant type signature on `bank::fetch`: inference derives `(int) -> (string) -> string` without it",
                (20, 0, 20, 31),
            ),
            (
                "redundant type signature on `bank::drive`: inference derives `(int) -> Unit` without it",
                (32, 0, 32, 19),
            ),
            (
                "redundant return type annotation on `bank::hold`: inference derives `int` without it",
                (55, 0, 55, 9),
            ),
            (
                "redundant type signature on `bank::handleRequest`: inference derives `(string, string, string, string) -> HttpResponse` without it",
                (113, 0, 113, 68),
            ),
        ];
        for (relative, expected) in [
            (
                "examples/projects/modules/src/main.ospml",
                main_warnings.as_slice(),
            ),
            ("examples/projects/modules/src/web/pages.ospml", &[]),
        ] {
            let path = root.join(relative);
            let source = std::fs::read_to_string(&path).expect("read module example");
            let uri = format!("file://{}", path.display());
            let diagnostics = compute(&source, &uri, U16);
            assert_redundant_annotations(&diagnostics, expected);
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
        assert_redundant_annotations(
            &diagnostics,
            &[
                (
                    "redundant type signature on `test::Money::pennies`: inference derives `(int) -> string` without it",
                    (8, 4, 8, 27),
                ),
                (
                    "redundant type signature on `test::Money::triple`: inference derives `(int) -> string` without it",
                    (15, 4, 15, 26),
                ),
                (
                    "redundant type signature on `test::Money::group`: inference derives `(int) -> string` without it",
                    (22, 4, 22, 25),
                ),
                (
                    "redundant type signature on `test::Money::show`: inference derives `(int) -> string` without it",
                    (30, 11, 30, 31),
                ),
                (
                    "redundant type signature on `test::Money::positive`: inference derives `(int) -> bool` without it",
                    (33, 11, 33, 33),
                ),
                (
                    "redundant type signature on `test::Json::escape`: inference derives `(string) -> string` without it",
                    (37, 4, 37, 29),
                ),
                (
                    "redundant type signature on `test::Json::quoted`: inference derives `(string) -> string` without it",
                    (44, 4, 44, 29),
                ),
                (
                    "redundant type signature on `test::Json::strField`: inference derives `(string) -> (string) -> string` without it",
                    (49, 11, 49, 48),
                ),
                (
                    "redundant type signature on `test::Json::obj`: inference derives `(string) -> string` without it",
                    (55, 11, 55, 33),
                ),
                (
                    "redundant type signature on `test::Accounts::movable`: inference derives `(int) -> bool` without it",
                    (73, 11, 73, 32),
                ),
                (
                    "redundant type signature on `test::settle`: inference derives `(test::Outcome) -> string` without it",
                    (89, 0, 89, 26),
                ),
            ],
        );
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
