use super::*;

#[test]
fn module_files_use_the_assembled_project_graph() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let main_warnings = bank_warnings();
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
        assert_warnings(&diagnostics, expected);
    }
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
            (20, 22, 20, 29),
        ),
        (
            "unused-variable",
            "unused variable `freed`",
            (21, 12, 21, 17),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `message`",
            (23, 14, 23, 21),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `message`",
            (49, 14, 49, 21),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `value`",
            (50, 16, 50, 21),
        ),
        (
            "unused-variable",
            "unused variable `slept`",
            (51, 12, 51, 17),
        ),
        (
            "unused-variable",
            "unused variable `listening`",
            (71, 4, 71, 13),
        ),
        ("unused-variable", "unused variable `held`", (77, 4, 77, 8)),
        (
            "unused-variable",
            "unused variable `stopped`",
            (78, 4, 78, 11),
        ),
        (
            "unused-variable",
            "unused variable `closed`",
            (79, 4, 79, 10),
        ),
        ("unused-variable", "unused variable `made`", (86, 4, 86, 8)),
        (
            "unused-parameter",
            "unused parameter `headers`",
            (95, 29, 95, 36),
        ),
        ("unused-variable", "unused variable `seen`", (96, 4, 96, 8)),
    ]
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
    // The inferred implementations carry no redundant annotations; contract
    // aliases must not reintroduce phantom warnings, and the `_` handler
    // parameters leave nothing unused.
    assert_warnings(&diagnostics, &[]);
    // This edited buffer retains one intentional redundant signature, so
    // warning identity and source mapping stay exact.
    let source = "namespace test\n\nmodule Money\n    export positive : int -> bool\n    positive cents = cents > 0\n\ntest \"positive\" (\\() => expect (Money::positive 1) true)\n";
    assert_redundant_annotations(
        &compute(source, &uri, U16),
        &[(
            "redundant type signature on `test::Money::positive`: inference derives `(int) -> bool` without it",
            (3, 11, 3, 33),
        )],
    );
    let inferred = source
        .replace("    export positive : int -> bool\n", "")
        .replace("    positive cents =", "    export positive cents =");
    assert_warnings(&compute(&inferred, &uri, U16), &[]);
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
        warnings: Vec::new(),
        state_boundaries: Vec::new(),
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
