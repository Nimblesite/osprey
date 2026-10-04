use super::*;

#[test]
fn recognizes_directory_and_manifest_project_inputs() {
    assert!(is_project_path("osprey.toml"));
    assert!(!is_project_path("main.osp"));
    assert!(is_project_path(
        std::env::temp_dir().to_string_lossy().as_ref()
    ));
}

#[test]
fn lexical_and_absolute_project_roots_have_one_identity() {
    let lexical = project_root(Path::new("."));
    let absolute = normalize_path(Path::new("."));
    assert_eq!(lexical, absolute);
    assert_eq!(
        ProjectConfig::for_root(&lexical).name,
        ProjectConfig::for_root(&absolute).name
    );
}

#[test]
fn only_module_aware_programs_request_single_source_assembly() {
    let script = osprey_syntax::parse_program("let answer = 42\n").program;
    let module =
        osprey_syntax::parse_program("module Answers {\n    export let answer = 42\n}\n").program;
    assert!(!needs_assembly(&script));
    assert!(needs_assembly(&module));
}

#[test]
fn assembled_client_preserves_string_results_for_length() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/projects/modules/client");
    let input = CompilationInput::load_project(root.to_string_lossy().as_ref())
        .expect("client project assembles");
    let errors = osprey_types::check_program(input.program());
    assert!(errors.is_empty(), "unexpected type errors: {errors:?}");
}

#[test]
fn source_output_defaults_match_the_existing_cli() {
    let program = osprey_syntax::parse_program("let answer = 42\n").program;
    let input = CompilationInput::script("nested/main.osp", String::new(), program);
    assert_eq!(input.output_path(None, "native"), PathBuf::from("main"));
    assert_eq!(input.output_path(None, "ios"), PathBuf::from("main.a"));
    assert_eq!(input.output_path(None, "ios-sim"), PathBuf::from("main.a"));
    assert_eq!(
        input.output_path(None, "wasm32"),
        PathBuf::from("main.wasm")
    );
    assert_eq!(
        input.output_path(Some("custom/out"), "native"),
        PathBuf::from("custom/out")
    );
}

#[test]
fn aggregate_sources_keeps_link_directives_from_every_file() {
    let sources = [
        ("first.osp", "// @link: sqlite3\nlet x = 1\n"),
        ("second.ospml", "// @linkdir: /opt/lib\ny = 2\n"),
    ];
    let sources = sources
        .iter()
        .enumerate()
        .map(|(index, (path, source))| osprey_project::SourceMetadata {
            index,
            path: PathBuf::from(path),
            flavor: Flavor::Default,
            source: (*source).to_string(),
            global_line_start: 1,
            global_line_end: 2,
        })
        .collect();
    let project = AssembledProject {
        warnings: Vec::new(),
        state_boundaries: Vec::new(),
        program: Program {
            statements: Vec::new(),
            doc: None,
        },
        entry_prologue: Vec::new(),
        entry_source: 0,
        sources,
        source_name_by_mangled: std::collections::BTreeMap::new(),
        public_api: std::collections::BTreeMap::new(),
        documentation_bindings: Vec::new(),
    };
    let aggregated = aggregate_sources(&project);
    assert!(aggregated.contains("// @link: sqlite3"));
    assert!(aggregated.contains("// @linkdir: /opt/lib"));

    let input = CompilationInput::assembled(
        project,
        aggregated,
        "demo".to_string(),
        OutputDefault::Project {
            root: PathBuf::from("out"),
            name: "demo".to_string(),
        },
    );
    assert_eq!(input.debug_path(), "first.osp");
    assert_eq!(
        input.diagnostic(Some(Position { line: 2, column: 3 }), "bad"),
        "first.osp:2:3: bad"
    );
    assert_eq!(
        input.output_path(None, "wasm32"),
        PathBuf::from("out/demo.wasm")
    );
}

#[test]
fn project_symbol_mapping_changes_exact_names_and_localizes_positions() {
    let program = osprey_syntax::parse_program("fn main() = 1\nfn remaining() = 2\n").program;
    let source = osprey_project::SourceMetadata {
        index: 0,
        path: PathBuf::from("src/main.osp"),
        flavor: Flavor::Default,
        source: String::new(),
        global_line_start: 1,
        global_line_end: 2,
    };
    let mut source_names = std::collections::BTreeMap::new();
    let _ = source_names.insert("main".to_string(), "app::main".to_string());
    let project = AssembledProject {
        warnings: Vec::new(),
        state_boundaries: Vec::new(),
        program,
        entry_prologue: Vec::new(),
        entry_source: 0,
        sources: vec![source],
        source_name_by_mangled: source_names,
        public_api: std::collections::BTreeMap::new(),
        documentation_bindings: Vec::new(),
    };
    let json = project_symbols_json(osprey_lsp::symbols_json(&project.program), &project);
    assert!(json.contains("\"name\":\"app::main\""));
    assert!(json.contains("\"name\":\"remaining\""));
    assert!(!json.contains("reapp::maining"));
    assert!(json.contains("\"path\":\"src/main.osp\""));
}

fn empty_project(
    source_name_by_mangled: std::collections::BTreeMap<String, String>,
) -> AssembledProject {
    AssembledProject {
        warnings: Vec::new(),
        state_boundaries: Vec::new(),
        program: Program {
            statements: Vec::new(),
            doc: None,
        },
        entry_prologue: Vec::new(),
        entry_source: 0,
        sources: Vec::new(),
        source_name_by_mangled,
        public_api: std::collections::BTreeMap::new(),
        documentation_bindings: Vec::new(),
    }
}

#[test]
fn an_entryless_project_falls_back_to_its_display_path() {
    let input = CompilationInput::assembled(
        empty_project(std::collections::BTreeMap::new()),
        String::new(),
        "whole/project".to_string(),
        OutputDefault::Project {
            root: PathBuf::from("out"),
            name: "demo".to_string(),
        },
    );
    // No entry source → debug_path and an unlocatable diagnostic both use display_path.
    assert_eq!(input.debug_path(), "whole/project");
    assert_eq!(
        input.diagnostic(Some(Position { line: 7, column: 2 }), "oops"),
        "whole/project:7:2: oops"
    );
    assert_eq!(input.diagnostic(None, "oops"), "whole/project: oops");
}

#[test]
fn project_symbol_projection_is_a_noop_on_unmappable_input() {
    let project = empty_project(std::collections::BTreeMap::new());
    // Non-JSON input is returned verbatim.
    assert_eq!(
        project_symbols_json("not json".to_string(), &project),
        "not json"
    );
    // Valid JSON that is not an array is also returned verbatim.
    assert_eq!(
        project_symbols_json("{\"a\":1}".to_string(), &project),
        "{\"a\":1}"
    );
    // An array entry that is not an object, has no mapped name, and no
    // position exercises every early-return guard in update_project_symbol.
    assert_eq!(project_symbols_json("[42]".to_string(), &project), "[42]");
}

#[test]
fn project_errors_include_every_available_location_component() {
    let cases = [
        (
            Some(PathBuf::from("src/a.osp")),
            Some(2),
            Some(3),
            "src/a.osp:2:3: bad",
        ),
        (None, Some(4), None, "fallback:4: bad"),
        (
            Some(PathBuf::from("src/b.osp")),
            None,
            Some(9),
            "src/b.osp: bad",
        ),
    ];
    for (path, line, column, expected) in cases {
        let error = ProjectError {
            message: "bad".to_string(),
            path,
            line,
            column,
        };
        assert_eq!(format_project_error(&error, "fallback"), expected);
    }
}
