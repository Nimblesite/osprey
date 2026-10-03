use super::*;

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
            (54, 0, 54, 18),
        ),
        (
            "redundant type signature on `bank::handleRequest`: inference derives `(string, string, string, string) -> HttpResponse` without it",
            (113, 0, 113, 68),
        ),
    ];
    let mut main_warnings: Vec<_> = main_warnings
        .into_iter()
        .map(|(message, range)| ("redundant-annotation", message, range))
        .collect();
    main_warnings.extend([
        (
            "unused-pattern-binding",
            "unused pattern binding `message`",
            (27, 22, 27, 29),
        ),
        (
            "unused-variable",
            "unused variable `freed`",
            (28, 12, 28, 17),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `message`",
            (30, 14, 30, 21),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `message`",
            (58, 14, 58, 21),
        ),
        (
            "unused-pattern-binding",
            "unused pattern binding `value`",
            (59, 16, 59, 21),
        ),
        (
            "unused-variable",
            "unused variable `slept`",
            (60, 12, 60, 17),
        ),
        (
            "unused-variable",
            "unused variable `listening`",
            (80, 4, 80, 13),
        ),
        ("unused-variable", "unused variable `held`", (86, 4, 86, 8)),
        (
            "unused-variable",
            "unused variable `stopped`",
            (87, 4, 87, 11),
        ),
        (
            "unused-variable",
            "unused variable `closed`",
            (88, 4, 88, 10),
        ),
        ("unused-variable", "unused variable `made`", (96, 4, 96, 8)),
        (
            "unused-parameter",
            "unused parameter `headers`",
            (114, 29, 114, 36),
        ),
        (
            "unused-variable",
            "unused variable `seen`",
            (115, 4, 115, 8),
        ),
    ]);
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
    let annotations = [
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
    ];
    let mut expected: Vec<_> = annotations
        .into_iter()
        .map(|(message, range)| ("redundant-annotation", message, range))
        .collect();
    expected.extend([
        (
            "unused-handler-parameter",
            "unused handler parameter `id` of `test::Vault.balance`",
            (161, 16, 161, 18),
        ),
        (
            "unused-handler-parameter",
            "unused handler parameter `id` of `test::Vault.debit`",
            (162, 14, 162, 16),
        ),
        (
            "unused-handler-parameter",
            "unused handler parameter `id` of `test::Vault.balance`",
            (168, 16, 168, 18),
        ),
        (
            "unused-handler-parameter",
            "unused handler parameter `id` of `test::Vault.debit`",
            (169, 14, 169, 16),
        ),
        (
            "unused-handler-parameter",
            "unused handler parameter `cents` of `test::Vault.debit`",
            (169, 17, 169, 22),
        ),
        (
            "unused-handler-parameter",
            "unused handler parameter `id` of `test::Vault.balance`",
            (178, 16, 178, 18),
        ),
        (
            "unused-handler-parameter",
            "unused handler parameter `id` of `test::Vault.debit`",
            (179, 14, 179, 16),
        ),
        (
            "unused-handler-parameter",
            "unused handler parameter `cents` of `test::Vault.debit`",
            (179, 17, 179, 22),
        ),
    ]);
    assert_warnings(&diagnostics, &expected);
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
