//! The seam's own tests: selection, the `discipline-error` cases of
//! [TYPES-DISCIPLINE-VERIFICATION] and the erased-module round trip, all with
//! scripted checkers so no process is spawned.

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
}

impl CheckerRunner for Scripted {
    fn run(&self, _: &Discipline, _: &str) -> Result<String, String> {
        self.calls.set(self.calls.get() + 1);
        self.answer.clone()
    }
}

fn scripted(answer: Result<&str, &str>) -> Scripted {
    Scripted {
        answer: answer.map(str::to_string).map_err(str::to_string),
        calls: Cell::new(0),
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
    assert!(parsed.errors.is_empty(), "fixture parses: {:?}", parsed.errors);
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

const MARKED: &str = "// osprey: types=bounded\nnamespace proof\n\nmodule Nat\n    export zero = 0\n";
const SIGNATURE: &str = "signature Nat\n    zero : int\n";
const ERASED: &str = "namespace proof\n\nmodule Nat\n    export zero = 0\n";

#[test]
fn selection_is_marker_then_project_then_hm() {
    // Implements [TYPES-DISCIPLINE-SELECT].
    let mut config = temp_project("select");
    assert_eq!(select(&config, "namespace x\n"), HM);
    assert_eq!(select(&config, "// osprey: types=lean4\nnamespace x\n"), "lean4");
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
    config.dependencies.push(Dependency { name: "@missing/pkg".into(), path: Some("nowhere".into()) });
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
    assert!(errors[0].message.starts_with(ERROR_PREFIX), "{}", errors[0].message);
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
    config.dependencies.push(Dependency { name: "@osprey/bounded".into(), path: Some("bounded".into()) });
    config.dependencies.push(Dependency { name: "@osprey/testing".into(), path: None });
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
    let (ok, errors): (Vec<_>, Vec<_>) = sources
        .into_iter()
        .map(|s| check_module(&config, &registry, &runner, s))
        .partition(Result::is_ok);
    if errors.is_empty() {
        Ok(ok.into_iter().flatten().collect())
    } else {
        Err(errors.into_iter().filter_map(Result::err).collect())
    }
}

#[test]
fn accepted_module_is_replaced_by_its_erased_form_under_its_signature() {
    // Implements [TYPES-DISCIPLINE-SEAM] and [TYPES-DISCIPLINE-BOUNDARY].
    let out = apply_one("accept", Ok(&accepted(SIGNATURE, ERASED))).expect("accepted");
    let erased = &out[0];
    assert_eq!(erased.discipline.as_deref(), Some("bounded"));
    assert_eq!(erased.source, ERASED);
    assert!(matches!(erased.program.statements.first(), Some(Stmt::Signature { name, .. }) if name == "Nat"));
    let ascribed = erased.program.statements.iter().any(|s| match s {
        Stmt::Namespace { body, .. } => body.iter().any(|m| matches!(m, Stmt::Module { signature: Some(a), .. } if a.path.segments == ["Nat"])),
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
    let wrong_version = accepted(SIGNATURE, ERASED).replace("\"version\":1", "\"version\":2");
    let unparsable = accepted(SIGNATURE, "module Nat\n    export zero = = 0\n");
    let unsigned = accepted("", ERASED);
    let cases: Vec<(&str, Result<&str, &str>, &str)> = vec![
        ("crash", Err("exited with an error with no response: boom"), "boom"),
        ("timeout", Err("exceeded its time limit of 1 seconds"), "time limit"),
        ("malformed", Ok("{not json"), "malformed JSON"),
        ("version", Ok(&wrong_version), "protocol version 2"),
        ("unparsable", Ok(&unparsable), "cannot parse"),
        ("unsigned", Ok(&unsigned), "wrote no signature for module `Nat`"),
        ("noerased", Ok(r#"{"version":1,"result":"accepted","signature":"","diagnostics":[]}"#), "no erased module"),
    ];
    for (name, answer, expected) in cases {
        let errors = apply_one(name, answer).expect_err(name);
        assert!(errors[0].message.starts_with(ERROR_PREFIX), "{name}: {}", errors[0].message);
        assert!(errors[0].message.contains(expected), "{name}: {}", errors[0].message);
    }
}

#[test]
fn request_carries_module_flavor_and_marker_and_protocol_version() {
    let config = temp_project("request");
    let request = Request::new(&source(MARKED), &config);
    let line = request.to_line().expect("json");
    let parsed: serde_json::Value = serde_json::from_str(&line).expect("round trip");
    assert_eq!(parsed["version"], VERSION);
    assert_eq!(parsed["module"]["flavor"], "ml");
    assert!(parsed["module"]["source"].as_str().is_some_and(|s| s.starts_with("// osprey: types=bounded")));
    assert_eq!(parsed["imports"].as_array().map(Vec::len), Some(0));
}

#[test]
fn unchanged_request_is_answered_from_the_cache() {
    let config = temp_project("cache");
    let runner = scripted(Ok(&accepted(SIGNATURE, ERASED)));
    let registry = registry(&config.directory);
    for _ in 0..2 {
        let _erased = check_module(&config, &registry, &runner, source(MARKED)).expect("accepted");
    }
    assert_eq!(runner.calls.get(), 1);
    assert!(Response::parse("{\"version\":1,\"result\":\"maybe\"}").is_err());
}
