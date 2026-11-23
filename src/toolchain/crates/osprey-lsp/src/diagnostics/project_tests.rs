use super::*;

#[test]
fn module_files_use_the_assembled_project_graph() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let path = root.join("src/osprey/examples/talon/backend/main.ospml");
    let source = std::fs::read_to_string(&path).expect("read module example");
    let uri = format!("file://{}", path.display());
    let diagnostics = compute(&source, &uri, U16);
    assert_warnings(&diagnostics, &bank_warnings());
}

fn bank_warnings() -> Vec<(&'static str, &'static str, crate::model::Span)> {
    vec![
        (
            "namespace-folder-drift",
            "namespace `bank` spans 5 folders; source paths do not change its identity",
            (3, 0, 3, 14),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `message`",
            (19, 22, 19, 29),
        ),
        (
            "unused-variable",
            "unused variable `freed`",
            (20, 12, 20, 17),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `message`",
            (22, 14, 22, 21),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `message`",
            (45, 14, 45, 21),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `value`",
            (46, 16, 46, 21),
        ),
        (
            "unused-variable",
            "unused variable `slept`",
            (47, 12, 47, 17),
        ),
        (
            "unused-variable",
            "unused variable `listening`",
            (67, 4, 67, 13),
        ),
        ("unused-variable", "unused variable `held`", (73, 4, 73, 8)),
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
        ("unused-variable", "unused variable `made`", (82, 4, 82, 8)),
        (
            "unused-parameter",
            "unused parameter `headers`",
            (90, 29, 90, 36),
        ),
        ("unused-variable", "unused variable `seen`", (91, 4, 91, 8)),
        ("repeated-literal", "the literal `\"/api/accounts\"` is written 4 times in this file; name it once as a constant", (25, 4, 25, 82)),
        ("repeated-literal", "the literal `\"/api/deposit\"` is written 3 times in this file; name it once as a constant", (28, 4, 28, 131)),
        ("consecutive-print", "4 `print` calls in a row: join them into one interpolated `print`", (34, 4, 34, 141)),
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let path = root.join("src/osprey/examples/talon/backend/test/accounts.test.ospml");
    let source = std::fs::read_to_string(&path).expect("read module test suite");
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

#[cfg(unix)]
#[test]
fn project_diagnostics_map_resolution_and_type_errors_to_the_open_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let main_path = root.join("src/osprey/examples/talon/backend/main.ospml");
    let main = std::fs::read_to_string(&main_path).expect("read ML module example");
    let unresolved = main.replace(
        "import bank::Metrics\n",
        "import bank::Metrics\nimport \"missing/web\" as web\n",
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
