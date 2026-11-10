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

/// A fresh, empty scratch directory named `prefix-name`, unique per process.
pub(crate) fn scratch_dir(prefix: &str, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("osprey-{prefix}-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn temp_project(name: &str) -> ProjectConfig {
    ProjectConfig::for_root(&scratch_dir("discipline", name))
}

fn write_file(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap_or(root)).expect("dir");
    std::fs::write(path, text).expect("write");
}

pub(crate) fn source(text: &str) -> SourceFile {
    let parsed = osprey_syntax::parse_program_with_flavor(text, Flavor::Ml);
    let errors = &parsed.errors;
    assert!(errors.is_empty(), "fixture parses: {errors:?}");
    SourceFile {
        path: PathBuf::from("src/Proof.ospml"),
        flavor: Flavor::Ml,
        source: text.to_string(),
        program: parsed.program,
        discipline: None,
    }
}

/// The `bounded` discipline rooted at `root`, its checker `root/checker.ospml`
/// and a `timeout`-second deadline.
pub(crate) fn discipline_at(root: &Path, timeout: u64) -> Discipline {
    Discipline {
        name: "bounded".into(),
        package: "@osprey/bounded".into(),
        root: root.to_path_buf(),
        checker: root.join("checker.ospml"),
        timeout: Duration::from_secs(timeout),
    }
}

fn bounded(root: &Path) -> Discipline {
    discipline_at(root, 1)
}

fn registry(root: &Path) -> Registry {
    Registry::from_disciplines(vec![bounded(root)]).expect("one discipline")
}

/// A manifest dependency named `name` on `path` (`None` = registry only).
fn dependency(name: &str, path: Option<&str>) -> Dependency {
    Dependency {
        name: name.into(),
        path: path.map(PathBuf::from),
    }
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

/// A fresh project plus a scripted checker that answers every request with an
/// `accepted` response for `signature` and `erased`, and its registry.
fn seam(name: &str, signature: &str, erased: &str) -> (ProjectConfig, Scripted, Registry) {
    let config = temp_project(name);
    let runner = scripted(Ok(&accepted(signature, erased)));
    let registry = registry(&config.directory);
    (config, runner, registry)
}

pub(crate) const MARKED: &str =
    "// osprey: types=bounded\nnamespace proof\n\nmodule Nat\n    export zero = 0\n";
const SIGNATURE: &str = "signature Nat\n    zero : int\n";
const ERASED: &str = "namespace proof\n\nmodule Nat\n    export zero = 0\n";

#[test]
fn selection_is_marker_then_project_then_hm() {
    // Implements [TYPES-DISCIPLINE-SELECT].
    let mut config = temp_project("select");
    assert_eq!(select(&config, "namespace x\n"), HM);
    let lean4 = "// osprey: types=lean4\nnamespace x\n";
    assert_eq!(select(&config, lean4), "lean4");
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
    let deps = &mut config.dependencies;
    deps.push(dependency("@missing/pkg", Some("nowhere")));
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
    let message = one_error(&errors);
    assert!(message.contains("`bounded`"));
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

const REGISTRY_MANIFEST: &str = "[package]\nname = \"@osprey/bounded\"\n[discipline]\nname = \"bounded\"\nchecker = \"checker.ospml\"\ntimeout_seconds = 7\n";

#[test]
fn registry_reads_discipline_tables_from_path_dependencies() {
    // Implements [TYPES-DISCIPLINE-REGISTRY].
    let mut config = temp_project("registry");
    let pkg = config.directory.join("bounded");
    write_file(&config.directory, "bounded/osprey.toml", REGISTRY_MANIFEST);
    let deps = [
        ("@osprey/bounded", Some("bounded")),
        ("@osprey/testing", None),
    ];
    for (name, path) in deps {
        config.dependencies.push(dependency(name, path));
    }
    let registry = Registry::discover(&config).expect("registry");
    let found = registry.get("bounded").expect("registered");
    assert_eq!(found.checker, pkg.join("checker.ospml"));
    assert_eq!(found.timeout, Duration::from_secs(7));
    assert!(registry.get("testing").is_none());
}

/// The one discipline error of `errors`, asserting it carries the error prefix.
fn one_error(errors: &[ProjectError]) -> &str {
    assert_eq!(errors.len(), 1);
    let message = &errors[0].message;
    assert!(message.starts_with(ERROR_PREFIX), "{message}");
    message
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
    let crash: Result<&str, &str> = Err("exited with an error with no response: boom");
    let timeout: Result<&str, &str> = Err("exceeded its time limit of 1 seconds");
    let malformed: Result<&str, &str> = Ok("{not json");
    let wrong_version = accepted(SIGNATURE, ERASED).replace("\"version\":2", "\"version\":3");
    let unparsable = accepted(SIGNATURE, "module Nat\n    export zero = = 0\n");
    let unsigned = accepted("", ERASED);
    let no_erased = r#"{"version":2,"result":"accepted","signature":"","diagnostics":[]}"#;
    let missing = "wrote no signature for module `Nat`";
    let cases: [(&str, Result<&str, &str>, &str); 7] = [
        ("crash", crash, "boom"),
        ("timeout", timeout, "time limit"),
        ("malformed", malformed, "malformed JSON"),
        ("version", Ok(&wrong_version), "protocol version 3"),
        ("unparsable", Ok(&unparsable), "cannot parse"),
        ("unsigned", Ok(&unsigned), missing),
        ("noerased", Ok(no_erased), "no erased module"),
    ];
    for (name, answer, expected) in cases {
        let errors = apply_one(name, answer).expect_err(name);
        let message = one_error(&errors);
        assert!(message.contains(expected), "{name}: {message}");
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
    let text = parsed["module"]["source"].as_str();
    assert!(text.is_some_and(|s| s.starts_with("// osprey: types=bounded")));
    assert_eq!(parsed["imports"].as_array().map(Vec::len), Some(0));
}

#[test]
fn a_cached_result_is_keyed_by_the_request_and_the_checker_binary() {
    // Implements [TYPES-DISCIPLINE-PROTOCOL]: "results are cached by the hash
    // of the request and the checker binary".
    let cases = [
        ("cache", false, 1, ""),
        ("cache-binary", true, 2, "a new binary is a cache miss"),
    ];
    for (name, rebuilt, calls, why) in cases {
        let (config, runner, registry) = seam(name, SIGNATURE, ERASED);
        let files = vec![source(MARKED)];
        for _ in 0..2 {
            let _erased = apply_with(&config, files.clone(), &runner, &registry).expect("accepted");
            runner.binary.set(runner.binary.get() + u64::from(rebuilt));
        }
        assert_eq!(runner.calls.get(), calls, "{why}");
        assert!(Response::parse("{\"version\":2,\"result\":\"maybe\"}").is_err());
        let _ = std::fs::remove_dir_all(&config.directory);
    }
}

pub(crate) fn source_at(path: &str, text: &str) -> SourceFile {
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
const NAT_SIGNATURE: &str =
    "signature Nat\n    label<t0> : (t0) -> string\n    same<t0> : (t0) -> t0\n";
const SHAPES_SIGNATURE: &str = "signature Shapes\n    size<t0> : (t0) -> int\n";
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
    let (config, runner, registry) = seam("imports-hm", THM_SIGNATURE, THM_ERASED);
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
    assert_eq!(signature, NAT_SIGNATURE);
    let parsed = osprey_syntax::parse_program_with_flavor(signature, Flavor::Ml);
    assert!(
        parsed.errors.is_empty(),
        "the rendered signature parses: {:?}",
        parsed.errors
    );
}

#[test]
fn a_checker_receives_the_signature_another_checker_wrote_for_an_imported_module() {
    let (config, runner, registry) = seam("imports-checked", THM_SIGNATURE, THM_ERASED);
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
    let (config, runner, registry) = seam("imports-lazy", SIGNATURE, ERASED);
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
    let imports = requests_of(&runner)[0]["imports"].as_array().map(Vec::len);
    assert_eq!(imports, Some(0));
}

mod checker_inputs;
