use super::*;

/// A file of this repository, named relative to its root: its path, for a
/// `file://` URI, and its text.
fn repo_file(relative: &str) -> (PathBuf, String) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join(relative);
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    (path, source)
}

#[test]
fn module_files_use_the_assembled_project_graph() {
    let (path, source) = repo_file("src/osprey/examples/talon/backend/main.ospml");
    let uri = format!("file://{}", path.display());
    let diagnostics = compute(&source, &uri, U16);
    assert_warnings(&diagnostics, &bank_warnings());
    // `bank` spans five folders by design, and talon's `osprey.toml` sets
    // `namespace-folder-drift = "allow"` ([LINT-LEVELS]).
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("namespace-folder-drift")),
        "talon's lint levels allow namespace-folder-drift: {diagnostics:?}"
    );
}

fn bank_warnings() -> Vec<(&'static str, &'static str, crate::model::Span)> {
    vec![
        (
            "unused-pattern-binding",
            "unused pattern binding `message`",
            (14, 22, 14, 29),
        ),
        (
            "unused-variable",
            "unused variable `freed`",
            (15, 12, 15, 17),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `message`",
            (17, 14, 17, 21),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `message`",
            (43, 14, 43, 21),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `value`",
            (44, 16, 44, 21),
        ),
        (
            "unused-variable",
            "unused variable `slept`",
            (45, 12, 45, 17),
        ),
        ("unused-variable", "unused variable `held`", (68, 4, 68, 8)),
        (
            "unused-variable",
            "unused variable `stopped`",
            (69, 4, 69, 11),
        ),
        (
            "unused-variable",
            "unused variable `closed`",
            (70, 4, 70, 10),
        ),
        (
            "unused-variable",
            "unused variable `stopped`",
            (74, 4, 74, 11),
        ),
        (
            "unused-variable",
            "unused variable `closed`",
            (75, 4, 75, 10),
        ),
        (
            "unused-variable",
            "unused variable `report`",
            (76, 4, 76, 10),
        ),
        (
            "unused-variable",
            "unused variable `closed`",
            (82, 8, 82, 14),
        ),
        (
            "unused-variable",
            "unused variable `report`",
            (83, 8, 83, 14),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `listening`",
            (87, 16, 87, 25),
        ),
        ("unused-variable", "unused variable `made`", (94, 4, 94, 8)),
        (
            "unused-parameter",
            "unused parameter `headers`",
            (102, 29, 102, 36),
        ),
        (
            "unused-variable",
            "unused variable `seen`",
            (103, 4, 103, 8),
        ),
        (
            "unused-variable",
            "unused variable `reported`",
            (110, 12, 110, 20),
        ),
        (
            "repeated-literal",
            "the literal `\"/api/accounts\"` is written 4 times in this file; name it once as a constant",
            (20, 4, 20, 82),
        ),
        (
            "repeated-literal",
            "the literal `\"/api/deposit\"` is written 3 times in this file; name it once as a constant",
            (23, 4, 23, 131),
        ),
        (
            "consecutive-print",
            "4 `print` calls in a row: join them into one interpolated `print`",
            (29, 4, 29, 141),
        ),
    ]
}

#[cfg(unix)]
#[test]
fn module_bearing_files_outside_project_roots_are_assembled_standalone() {
    // The bank's suites are their own project rooted at `test/`, importing
    // the real `bank::Accounts` from `../src/domain`: the LSP must assemble
    // that graph, not check the raw AST and spray unknown identifiers. The
    // inferred implementations carry no redundant annotations, every export
    // is documented, and the `_` handler parameters leave nothing unused.
    let (path, source) = repo_file("src/osprey/examples/talon/backend/test/accounts.test.ospml");
    let diagnostics = compute(&source, &format!("file://{}", path.display()), U16);
    assert_warnings(&diagnostics, &[]);
    // The editor regression: a self-contained suite with no manifest above it
    // defines `module Money` and calls `Money::positive`. `osprey <file>
    // --check` and `osprey test` accept it via single-source assembly; the
    // LSP must not spray `unknown identifier `Money::positive``.
    let uri = "file:///osprey-standalone-suite/money.test.ospml";
    // This edited buffer retains one intentional redundant signature, on a
    // private helper, so warning identity and source mapping stay exact. The
    // exported member's signature is API and is never reported
    // ([TYPE-ANNOTATION-REDUNDANT]).
    let source = "namespace test\n\n(** Amounts. *)\nmodule Money\n    (** Whether `cents` is above zero.\n    # Examples\n    ```osprey\n    print (Money::positive 1)\n    ```\n    *)\n    export positive : int -> bool\n    positive cents = above cents\n    above : int -> bool\n    above cents = cents > 0\n\ntest \"positive\" (\\() => expect (Money::positive 1) true)\n";
    assert_redundant_annotations(
        &compute(source, uri, U16),
        &[(
            "redundant type signature on `test::Money::above`: inference derives `(int) -> bool` without it",
            (12, 4, 12, 23),
        )],
    );
    let inferred = source
        .replace("    export positive : int -> bool\n", "")
        .replace("    positive cents =", "    export positive cents =")
        .replace("    above : int -> bool\n", "");
    assert_warnings(&compute(&inferred, uri, U16), &[]);
}

/// `compute` answers at least one diagnostic carrying `code` for `source` at `uri`.
fn assert_code(source: &str, uri: &str, code: &str) {
    let diagnostics = compute(source, uri, U16);
    assert!(
        diagnostics
            .iter()
            .any(|item| item.code.as_deref() == Some(code)),
        "{diagnostics:?}"
    );
}

#[cfg(unix)]
#[test]
fn project_diagnostics_map_resolution_and_type_errors_to_the_open_file() {
    let (main_path, main) = repo_file("src/osprey/examples/talon/backend/main.ospml");
    let unresolved = main.replace(
        "namespace bank\n",
        "namespace bank\nimport \"missing/web\" as web\n",
    );
    assert_code(
        &unresolved,
        &format!("file://{}", main_path.display()),
        "project-error",
    );

    let ill_typed = main.replace(
        "served = Metrics::track (\\() => boot port)",
        "served = Metrics::track 42",
    );
    assert_code(
        &ill_typed,
        &format!("file://{}", main_path.display()),
        "type-error",
    );

    let source = "print(missing)\n";
    let project = AssembledProject {
        source_dependencies: Vec::new(),
        method_candidates: Vec::new(),
        warnings: Vec::new(),
        state_boundaries: Vec::new(),
        program: osprey_syntax::parse_program(source).program,
        backend: None,
        entry_prologue: Vec::new(),
        entry_source: 0,
        sources: vec![osprey_project::SourceMetadata {
            index: 0,
            path: PathBuf::from("entry.osp"),
            flavor: osprey_syntax::Flavor::Default,
            source: source.to_string(),
            global_line_start: 1,
            global_line_end: 1,
            discipline: None,
        }],
        source_name_by_mangled: std::collections::BTreeMap::new(),
        public_api: std::collections::BTreeMap::new(),
        documentation_bindings: Vec::new(),
    };
    let errors = osprey_types::check_program(&project.program);
    let warnings = crate::warning_actions::Warnings::none();
    let diagnostics = assembled_type_errors(
        source,
        Path::new("entry.osp"),
        &project,
        &errors,
        &warnings,
        PositionEncoding::Utf16,
    );
    assert!(!diagnostics.diagnostics.is_empty(), "{diagnostics:?}");
    assert!(assembled_type_errors(
        source,
        Path::new("other.osp"),
        &project,
        &errors,
        &warnings,
        PositionEncoding::Utf16,
    )
    .diagnostics
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

#[test]
fn graphics_examples_discharge_arithmetic_effects() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../osprey/examples/graphics");
    for scene in ["character", "kali", "opal"] {
        let path = root.join(scene).join("scene.osp");
        let source = std::fs::read_to_string(&path)?;
        let uri = format!("file://{}", path.display());
        let errors: Vec<_> = compute(&source, &uri, U16)
            .into_iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{scene}: {errors:?}");
    }
    Ok(())
}

// Implements [LSP-DIAGNOSTICS]: an unknown name belongs to its physical source,
// including helpers written in a different syntax flavor from the entry file.
#[test]
fn unknown_names_in_cross_flavor_helpers_keep_their_physical_owner(
) -> Result<(), Box<dyn std::error::Error>> {
    for (extension, source) in [
        ("osp", "fn increment(value) = missingName\n"),
        ("ospml", "increment value = missingName\n"),
        (
            "py",
            "# osprey: flavor=python\ndef increment(value):\n    return missingName\n",
        ),
    ] {
        let fixture = crate::test_support::ProjectFixture::new("osp")?;
        let entry = "let total = increment(3)\n";
        let entry_uri = fixture.write("main", entry)?;
        let helper = fixture.root.join(format!("src/helper.{extension}"));
        std::fs::write(&helper, source)?;
        let helper_uri = lspkit_server::uri::path_to_uri(&helper)?;
        let helper_errors = compute(source, &helper_uri, U16);
        assert!(
            helper_errors
                .iter()
                .any(|error| error.severity == Severity::Error
                    && error.message.contains("unknown identifier `missingName`")),
            "{extension} helper lost its own error: {helper_errors:?}"
        );
        let entry_errors = compute(entry, &entry_uri, U16);
        assert!(
            !entry_errors
                .iter()
                .any(|error| error.message.contains("missingName")),
            "{extension} helper's error was falsely attributed to entry: {entry_errors:?}"
        );
    }
    Ok(())
}
