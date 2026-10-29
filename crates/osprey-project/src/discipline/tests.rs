//! The seam's own tests: selection, the `discipline-error` cases of
//! [TYPES-DISCIPLINE-VERIFICATION] and the erased-module round trip, all with
//! scripted checkers so no process is spawned.

#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: a missing element is a test failure, not a production panic"
)]

use super::protocol::{Request, Response, VERSION};
use super::*;
use crate::manifest::Dependency;
use osprey_ast::Stmt;
use osprey_syntax::Flavor;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A checker that answers every request with one fixed line and counts calls.
struct Scripted {
    answer: Result<String, String>,
    calls: Cell<usize>,
    requests: std::cell::RefCell<Vec<String>>,
    /// The binary identity this runner reports; a test changes it to pretend
    /// the checker was rebuilt.
    binary: Cell<u64>,
}

impl CheckerRunner for Scripted {
    fn run(&self, _: &Discipline, request: &str) -> Result<String, String> {
        self.calls.set(self.calls.get() + 1);
        self.requests.borrow_mut().push(request.to_string());
        self.answer.clone()
    }

    fn binary_digest(&self, _: &Discipline) -> Result<u64, String> {
        Ok(self.binary.get())
    }
}

fn scripted(answer: Result<&str, &str>) -> Scripted {
    Scripted {
        answer: answer.map(str::to_string).map_err(str::to_string),
        calls: Cell::new(0),
        requests: std::cell::RefCell::new(Vec::new()),
        binary: Cell::new(0),
    }
}

fn temp_project(name: &str) -> ProjectConfig {
    let dir = std::env::temp_dir().join(format!("osprey-discipline-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    ProjectConfig::for_root(&dir)
}

fn source(text: &str) -> SourceFile {
    let parsed = osprey_syntax::parse_program_with_flavor(text, Flavor::Ml);
    assert!(
        parsed.errors.is_empty(),
        "fixture parses: {:?}",
        parsed.errors
    );
    SourceFile {
        path: PathBuf::from("src/Proof.ospml"),
        flavor: Flavor::Ml,
        source: text.to_string(),
        program: parsed.program,
        discipline: None,
    }
}

fn bounded(root: &Path) -> Discipline {
    Discipline {
        name: "bounded".into(),
        package: "@osprey/bounded".into(),
        root: root.to_path_buf(),
        checker: root.join("checker.ospml"),
        timeout: Duration::from_secs(1),
    }
}

fn registry(root: &Path) -> Registry {
    Registry::from_disciplines(vec![bounded(root)]).expect("one discipline")
}

fn accepted(signature: &str, erased: &str) -> String {
    serde_json::json!({
        "version": VERSION,
        "result": "accepted",
        "signature": signature,
        "erased": { "flavor": "ml", "source": erased },
        "diagnostics": []
    })
    .to_string()
}

const MARKED: &str =
    "// osprey: types=bounded\nnamespace proof\n\nmodule Nat\n    export zero = 0\n";
const SIGNATURE: &str = "signature Nat\n    zero : int\n";
const ERASED: &str = "namespace proof\n\nmodule Nat\n    export zero = 0\n";

#[test]
fn selection_is_marker_then_project_then_hm() {
    // Implements [TYPES-DISCIPLINE-SELECT].
    let mut config = temp_project("select");
    assert_eq!(select(&config, "namespace x\n"), HM);
    assert_eq!(
        select(&config, "// osprey: types=lean4\nnamespace x\n"),
        "lean4"
    );
    assert_eq!(select(&config, "# osprey: types=lean4\n"), "lean4");
    config.types = Some("bounded".into());
    assert_eq!(select(&config, "namespace x\n"), "bounded");
    assert_eq!(select(&config, "// osprey: types=hm\n"), HM);
    assert_eq!(marker("//osprey: types= refined \n"), Some("refined"));
}

#[test]
fn markerless_project_never_touches_the_registry_or_a_runner() {
    // Implements [TYPES-DISCIPLINE-NO-HARM] rule 4.
    let mut config = temp_project("noharm");
    config.dependencies.push(Dependency {
        name: "@missing/pkg".into(),
        path: Some("nowhere".into()),
    });
    let runner = scripted(Err("must not run"));
    let sources = vec![source("namespace plain\n\nx = 1\n")];
    let out = apply(&config, sources, &runner).expect("untouched");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].discipline, None);
    assert_eq!(runner.calls.get(), 0);
}

#[test]
fn unknown_discipline_is_one_error_on_the_marker_line() {
    let config = temp_project("unknown");
    let runner = scripted(Err("must not run"));
    let errors = apply(&config, vec![source(MARKED)], &runner).expect_err("unknown name");
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].message.starts_with(ERROR_PREFIX),
        "{}",
        errors[0].message
    );
    assert!(errors[0].message.contains("`bounded`"));
    assert_eq!(errors[0].line, Some(1));
    assert_eq!(runner.calls.get(), 0);
}

#[test]
fn duplicate_registration_names_both_packages() {
    let root = temp_project("dup").directory;
    let mut second = bounded(&root);
    second.package = "@other/bounded".into();
    let error = Registry::from_disciplines(vec![bounded(&root), second]).expect_err("duplicate");
    assert!(error.message.contains("@osprey/bounded") && error.message.contains("@other/bounded"));
}

#[test]
fn registry_reads_discipline_tables_from_path_dependencies() {
    // Implements [TYPES-DISCIPLINE-REGISTRY].
    let mut config = temp_project("registry");
    let pkg = config.directory.join("bounded");
    std::fs::create_dir_all(&pkg).expect("pkg dir");
    std::fs::write(
        pkg.join("osprey.toml"),
        "[package]\nname = \"@osprey/bounded\"\n[discipline]\nname = \"bounded\"\nchecker = \"checker.ospml\"\ntimeout_seconds = 7\n",
    )
    .expect("manifest");
    config.dependencies.push(Dependency {
        name: "@osprey/bounded".into(),
        path: Some("bounded".into()),
    });
    config.dependencies.push(Dependency {
        name: "@osprey/testing".into(),
        path: None,
    });
    let registry = Registry::discover(&config).expect("registry");
    let found = registry.get("bounded").expect("registered");
    assert_eq!(found.checker, pkg.join("checker.ospml"));
    assert_eq!(found.timeout, Duration::from_secs(7));
    assert!(registry.get("testing").is_none());
}

fn apply_one(name: &str, answer: Result<&str, &str>) -> Result<Vec<SourceFile>, Vec<ProjectError>> {
    let config = temp_project(name);
    let runner = scripted(answer);
    let registry = registry(&config.directory);
    let sources = vec![source(MARKED), source("namespace other\n\ny = 2\n")];
    apply_with(&config, sources, &runner, &registry)
}

#[test]
fn accepted_module_is_replaced_by_its_erased_form_under_its_signature() {
    // Implements [TYPES-DISCIPLINE-SEAM] and [TYPES-DISCIPLINE-BOUNDARY].
    let out = apply_one("accept", Ok(&accepted(SIGNATURE, ERASED))).expect("accepted");
    let erased = &out[0];
    assert_eq!(erased.discipline.as_deref(), Some("bounded"));
    assert_eq!(erased.source, ERASED);
    assert!(
        matches!(erased.program.statements.first(), Some(Stmt::Signature { name, .. }) if name == "Nat")
    );
    let ascribed = erased.program.statements.iter().any(|s| match s {
        Stmt::Namespace { body, .. } => body.iter().any(
            |m| matches!(m, Stmt::Module { signature: Some(a), .. } if a.path.segments == ["Nat"]),
        ),
        _ => false,
    });
    assert!(ascribed, "module Nat is ascribed to signature Nat");
    assert_eq!(out[1].discipline, None);
}

#[test]
fn rejection_carries_the_checker_diagnostics_prefixed_by_its_name() {
    let answer = serde_json::json!({
        "version": VERSION, "result": "rejected", "signature": "", "erased": null,
        "diagnostics": [ { "code": "range", "severity": "error", "line": 4, "column": 11, "message": "0..100 exceeded" } ]
    })
    .to_string();
    let errors = apply_one("reject", Ok(&answer)).expect_err("rejected");
    assert_eq!(errors[0].message, "bounded/range: 0..100 exceeded");
    assert_eq!((errors[0].line, errors[0].column), (Some(4), Some(11)));
}

#[test]
fn every_checker_fault_is_a_discipline_error() {
    // Implements [TYPES-DISCIPLINE-VERIFICATION]: crash, timeout, malformed
    // response, unsupported version, unparsable erased module, missing signature.
    let wrong_version = accepted(SIGNATURE, ERASED).replace("\"version\":2", "\"version\":3");
    let unparsable = accepted(SIGNATURE, "module Nat\n    export zero = = 0\n");
    let unsigned = accepted("", ERASED);
    let cases: Vec<(&str, Result<&str, &str>, &str)> = vec![
        (
            "crash",
            Err("exited with an error with no response: boom"),
            "boom",
        ),
        (
            "timeout",
            Err("exceeded its time limit of 1 seconds"),
            "time limit",
        ),
        ("malformed", Ok("{not json"), "malformed JSON"),
        ("version", Ok(&wrong_version), "protocol version 3"),
        ("unparsable", Ok(&unparsable), "cannot parse"),
        (
            "unsigned",
            Ok(&unsigned),
            "wrote no signature for module `Nat`",
        ),
        (
            "noerased",
            Ok(r#"{"version":2,"result":"accepted","signature":"","diagnostics":[]}"#),
            "no erased module",
        ),
    ];
    for (name, answer, expected) in cases {
        let errors = apply_one(name, answer).expect_err(name);
        assert!(
            errors[0].message.starts_with(ERROR_PREFIX),
            "{name}: {}",
            errors[0].message
        );
        assert!(
            errors[0].message.contains(expected),
            "{name}: {}",
            errors[0].message
        );
    }
}

#[test]
fn request_carries_module_flavor_and_marker_and_protocol_version() {
    let config = temp_project("request");
    let request = Request::new(&source(MARKED), &config, Vec::new());
    let line = request.to_line().expect("json");
    let parsed: serde_json::Value = serde_json::from_str(&line).expect("round trip");
    assert_eq!(parsed["version"], VERSION);
    assert_eq!(parsed["module"]["flavor"], "ml");
    assert!(parsed["module"]["source"]
        .as_str()
        .is_some_and(|s| s.starts_with("// osprey: types=bounded")));
    assert_eq!(parsed["imports"].as_array().map(Vec::len), Some(0));
}

#[test]
fn unchanged_request_is_answered_from_the_cache() {
    let config = temp_project("cache");
    let runner = scripted(Ok(&accepted(SIGNATURE, ERASED)));
    let registry = registry(&config.directory);
    for _ in 0..2 {
        let _erased =
            apply_with(&config, vec![source(MARKED)], &runner, &registry).expect("accepted");
    }
    assert_eq!(runner.calls.get(), 1);
    assert!(Response::parse("{\"version\":2,\"result\":\"maybe\"}").is_err());
}

fn source_at(path: &str, text: &str) -> SourceFile {
    let mut file = source(text);
    file.path = PathBuf::from(path);
    file
}

/// An `hm` module with a polymorphic and a monomorphic export, imported by
/// a disciplined module; a disciplined module imported by another.
const NAT_HM: &str =
    "namespace proof\n\nmodule Nat\n    export label s = \"n${s}\"\n    export same x = x\n    hidden () = 0\n";
const THM_MARKED: &str = "// osprey: types=bounded\nnamespace proof\n\nimport proof::Nat\n\nmodule Thm\n    export zero = Nat::label \"0\"\n";
const THM_SIGNATURE: &str = "signature Thm\n    zero : string\n";
const THM_ERASED: &str =
    "namespace proof\n\nimport proof::Nat\n\nmodule Thm\n    export zero = Nat::label \"0\"\n";
const USER_MARKED: &str = "// osprey: types=bounded\nnamespace proof\n\nimport proof::Thm\n\nmodule User\n    export one = Thm::zero\n";

fn requests_of(runner: &Scripted) -> Vec<serde_json::Value> {
    runner
        .requests
        .borrow()
        .iter()
        .map(|line| serde_json::from_str(line).expect("request json"))
        .collect()
}

#[test]
fn a_checker_receives_the_inferred_signature_of_each_hm_module_its_source_imports() {
    let config = temp_project("imports-hm");
    let runner = scripted(Ok(&accepted(THM_SIGNATURE, THM_ERASED)));
    let registry = registry(&config.directory);
    // The disciplined file is listed first: dependency order, not file order, decides.
    let sources = vec![
        source_at("src/Thm.ospml", THM_MARKED),
        source_at("src/Nat.ospml", NAT_HM),
    ];
    let checked = apply_with(&config, sources, &runner, &registry).expect("accepted");
    assert_eq!(checked.len(), 2);
    let requests = requests_of(&runner);
    let imports = requests[0]["imports"].as_array().expect("imports");
    assert_eq!(imports.len(), 1, "{imports:?}");
    assert_eq!(imports[0]["name"], "proof::Nat");
    assert_eq!(imports[0]["discipline"], HM);
    let signature = imports[0]["signature"].as_str().expect("signature");
    assert_eq!(
        signature,
        "signature Nat\n    label<t0> : (t0) -> string\n    same<t0> : (t0) -> t0\n"
    );
    let parsed = osprey_syntax::parse_program_with_flavor(signature, Flavor::Ml);
    assert!(
        parsed.errors.is_empty(),
        "the rendered signature parses: {:?}",
        parsed.errors
    );
}

#[test]
fn a_checker_receives_the_signature_another_checker_wrote_for_an_imported_module() {
    let config = temp_project("imports-checked");
    let runner = scripted(Ok(&accepted(THM_SIGNATURE, THM_ERASED)));
    let registry = registry(&config.directory);
    let sources = vec![
        source_at("src/User.ospml", USER_MARKED),
        source_at("src/Nat.ospml", NAT_HM),
        source_at("src/Thm.ospml", THM_MARKED),
    ];
    let _checked = apply_with(&config, sources, &runner, &registry).expect("accepted");
    let requests = requests_of(&runner);
    assert_eq!(requests.len(), 2);
    let user = requests
        .iter()
        .find(|request| request["module"]["path"].as_str() == Some("src/User.ospml"))
        .expect("the User request");
    let imports = user["imports"].as_array().expect("imports");
    assert_eq!(imports.len(), 1, "{imports:?}");
    assert_eq!(imports[0]["name"], "proof::Thm");
    assert_eq!(imports[0]["discipline"], "bounded");
    assert_eq!(imports[0]["signature"], THM_SIGNATURE);
}

#[test]
fn an_hm_module_nobody_disciplined_imports_is_never_inferred_on_its_own() {
    let config = temp_project("imports-lazy");
    let runner = scripted(Ok(&accepted(SIGNATURE, ERASED)));
    let registry = registry(&config.directory);
    // A broken `hm` file the disciplined module does not import: assembly of
    // the whole project reports it later, the seam does not.
    let sources = vec![
        source_at("src/Proof.ospml", MARKED),
        source_at(
            "src/Other.ospml",
            "namespace other\n\nmodule Other\n    export broken () = Missing::thing ()\n",
        ),
    ];
    let checked = apply_with(&config, sources, &runner, &registry).expect("accepted");
    assert_eq!(checked.len(), 2);
    assert_eq!(
        requests_of(&runner)[0]["imports"].as_array().map(Vec::len),
        Some(0)
    );
}

/// A package on disk: manifest, entry module and one imported module whose
/// text a test rewrites.
fn checker_package(name: &str) -> PathBuf {
    let root = temp_project(name).directory;
    let write = |relative: &str, text: &str| {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap_or(&root)).expect("dir");
        std::fs::write(path, text).expect("write");
    };
    write(
        "osprey.toml",
        "[package]\nname = \"@osprey/bounded\"\n\n[discipline]\nname = \"bounded\"\nchecker = \"checker.ospml\"\n",
    );
    write(
        "checker.ospml",
        "namespace bounded\n\nimport bounded::Range\n\nmain () = Range::zero\n",
    );
    write(
        "src/Range.ospml",
        "namespace bounded\n\nmodule Range\n    export zero = 0\n",
    );
    root
}

#[test]
fn changing_an_imported_checker_module_changes_the_build_digest_and_binary_name() {
    // Implements [TYPES-DISCIPLINE-REGISTRY]: the checker is "built once ...
    // cached beside the package by content hash" of its whole build, not of
    // its entry module alone.
    let root = checker_package("build-digest");
    let discipline = bounded(&root);
    let digest = inputs::build_digest(&discipline).expect("digest");
    assert_eq!(
        inputs::build_digest(&discipline).expect("digest"),
        digest,
        "unchanged inputs keep the digest"
    );
    let compiler = root.join("fake-compiler");
    std::fs::write(&compiler, b"compiler one").expect("compiler");
    let build_dir = root.join("build");
    let built = build_dir.join(format!(
        "bounded-{digest:016x}-{:016x}",
        inputs::compiler_digest(&compiler).expect("compiler digest")
    ));
    std::fs::create_dir_all(&build_dir).expect("build dir");
    std::fs::write(&built, b"built").expect("prebuilt binary");
    let runner = ProcessRunner::new(compiler, build_dir);
    assert_eq!(
        runner
            .executable(
                &discipline,
                std::time::Instant::now() + Duration::from_secs(5)
            )
            .expect("reused binary"),
        built,
        "a built binary is found again under the name its inputs hash to"
    );
    std::fs::write(
        root.join("src/Range.ospml"),
        "namespace bounded\n\nmodule Range\n    export zero = 1\n",
    )
    .expect("rewrite imported module");
    assert_ne!(
        inputs::build_digest(&discipline).expect("digest"),
        digest,
        "a change to an imported module is a change to an input"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_rebuilt_checker_binary_misses_the_result_cache() {
    // Implements [TYPES-DISCIPLINE-PROTOCOL]: "results are cached by the hash
    // of the request and the checker binary".
    let config = temp_project("cache-binary");
    let runner = scripted(Ok(&accepted(SIGNATURE, ERASED)));
    let registry = registry(&config.directory);
    for _ in 0..2 {
        let _erased =
            apply_with(&config, vec![source(MARKED)], &runner, &registry).expect("accepted");
        runner.binary.set(runner.binary.get() + 1);
    }
    assert_eq!(runner.calls.get(), 2, "a new binary is a cache miss");
    let _ = std::fs::remove_dir_all(&config.directory);
}

#[test]
fn an_unreadable_checker_entry_is_an_error_and_not_a_cache_miss() {
    let config = temp_project("cache-unreadable");
    let discipline = bounded(&config.directory);
    let runner = ProcessRunner::new(
        config.directory.join("no-compiler"),
        config.directory.join("build"),
    );
    let request = Request::new(&source(MARKED), &config, Vec::new());
    let error = cache::run_cached(&config, &discipline, &runner, &request)
        .expect_err("an unreadable checker is a discipline error");
    assert!(error.contains("has no readable checker at"), "{error}");
    assert!(error.contains("checker.ospml"), "{error}");
    let _ = std::fs::remove_dir_all(&config.directory);
}

/// An `hm` module exporting a type and a value beside a function.
const SHAPES_HM: &str = "namespace proof\n\nmodule Shapes\n    export type Shape = Dot | Line int\n    export limit = 3\n    export size s = 1\n";
const USES_SHAPES: &str = "// osprey: types=bounded\nnamespace proof\n\nimport proof::Shapes\n\nmodule Thm\n    export zero = Shapes::size Dot\n";
/// An `hm` module that does not type-check.
const BROKEN_HM: &str = "namespace proof\n\nmodule Nat\n    export label s = listLength 1\n";

#[test]
fn an_inferred_signature_names_every_exported_item_it_leaves_out() {
    let config = temp_project("imports-omitted");
    let runner = scripted(Ok(&accepted(THM_SIGNATURE, THM_ERASED)));
    let registry = registry(&config.directory);
    let sources = vec![
        source_at("src/Shapes.ospml", SHAPES_HM),
        source_at("src/Thm.ospml", USES_SHAPES),
    ];
    let _ = apply_with(&config, sources, &runner, &registry);
    let requests = requests_of(&runner);
    let import = &requests[0]["imports"][0];
    assert_eq!(
        import["signature"],
        "signature Shapes\n    size<t0> : (t0) -> int\n"
    );
    assert_eq!(
        import["omitted"],
        serde_json::json!(["type Shape", "value limit"]),
        "a checker is told the signature is partial"
    );
    assert_eq!(requests[0]["version"], 2);
}

#[test]
fn an_hm_import_that_does_not_type_check_is_an_error_and_never_a_signature() {
    let config = temp_project("imports-broken");
    let runner = scripted(Ok(&accepted(THM_SIGNATURE, THM_ERASED)));
    let registry = registry(&config.directory);
    let sources = vec![
        source_at("src/Nat.ospml", BROKEN_HM),
        source_at("src/Thm.ospml", THM_MARKED),
    ];
    let errors = apply_with(&config, sources, &runner, &registry).expect_err("rejected");
    assert_eq!(
        runner.calls.get(),
        0,
        "no checker runs after an import failed: {errors:?}"
    );
    assert!(
        errors[0]
            .message
            .contains("could not be inferred for an importing checker"),
        "{}",
        errors[0].message
    );
}

/// A checker declared as a project directory: its own manifest names the
/// entry, and the entry imports a sibling module.
fn checker_project(name: &str) -> PathBuf {
    let root = temp_project(name).directory;
    let write = |relative: &str, text: &str| {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap_or(&root)).expect("dir");
        std::fs::write(path, text).expect("write");
    };
    write(
        "osprey.toml",
        "[package]\nname = \"@osprey/bounded\"\n\n[discipline]\nname = \"bounded\"\nchecker = \"checker\"\n",
    );
    write(
        "checker/osprey.toml",
        "[project]\nname = \"checker\"\nsource_roots = [\"src\"]\nentry = \"src/main.ospml\"\n",
    );
    write(
        "checker/src/main.ospml",
        "namespace bounded\n\nimport bounded::Range\n\nmain () = Range::zero\n",
    );
    write(
        "checker/src/range.ospml",
        "namespace bounded\n\nmodule Range\n    export zero = 0\n",
    );
    root
}

#[test]
fn a_checker_project_directory_is_digested_over_its_manifest_and_every_module() {
    // Implements [TYPES-DISCIPLINE-REGISTRY]: a checker is an Osprey program
    // built by the ordinary pipeline, so it may be a project of many modules;
    // every module and the project manifest are build inputs.
    let root = checker_project("checker-dir");
    let discipline = Discipline {
        checker: root.join("checker"),
        ..bounded(&root)
    };
    let digest = inputs::build_digest(&discipline).expect("digest");
    std::fs::write(
        root.join("checker/src/range.ospml"),
        "namespace bounded\n\nmodule Range\n    export zero = 1\n",
    )
    .expect("rewrite module");
    let changed = inputs::build_digest(&discipline).expect("digest");
    assert_ne!(
        changed, digest,
        "a module of the checker project is an input"
    );
    std::fs::write(
        root.join("checker/osprey.toml"),
        "[project]\nname = \"checker\"\nsource_roots = [\"src\"]\nentry = \"src/main.ospml\"\n\n[modules]\nallow_wildcard_imports = false\n",
    )
    .expect("rewrite manifest");
    assert_ne!(
        inputs::build_digest(&discipline).expect("digest"),
        changed,
        "the checker project's manifest is an input"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_checker_directory_without_a_manifest_is_an_error_naming_it() {
    let root = checker_project("checker-dir-no-manifest");
    std::fs::remove_file(root.join("checker/osprey.toml")).expect("remove manifest");
    let discipline = Discipline {
        checker: root.join("checker"),
        ..bounded(&root)
    };
    let message = inputs::build_digest(&discipline).expect_err("no manifest");
    assert!(
        message.contains("checker/osprey.toml")
            || message.contains("checker") && message.contains("osprey.toml"),
        "{message}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
