//! Adversarial API-export contracts beyond `api_docs.rs`: nested and `state`
//! modules, externs, type aliases, generic binders, static effects, ML-flavor
//! module docs, and malformed option handling. Implements [DOC-EXPORT].

use super::{finish, osprey, read_text, temp_dir};

use super::api_docs::export;

#[test]
fn extra_docs_reach_declarations_in_deeply_nested_modules() {
    let source = "module Outer {\nexport module Middle {\nexport module Inner {\n/// Deep.\nexport fn probe() = 1\n}\n}\n}\n";
    let (result, output) = export(source, "osp", "extra_docs_deep_modules");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let page = output.join("api/outer-middle-inner-probe.md");
    assert!(
        page.is_file(),
        "missing deep page: {:?}",
        output.join("api")
    );
    assert!(read_text(&page).contains("Deep."), "{page:?}");
}

#[test]
fn extra_docs_state_module_members_export_without_private_leak() {
    let source = "state module Counter {\nexport fn zero() = 0\nfn internal() = 0\n}\n";
    let (result, output) = export(source, "osp", "extra_docs_state_module");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(read_text(&output.join("api/counter.md")).contains("state module Counter"));
    assert!(
        read_text(&output.join("api/counter-zero.md")).contains("int"),
        "exported state fn documented"
    );
    assert!(
        !output.join("api/counter-internal.md").exists(),
        "private state fn must not be exported"
    );
}

#[test]
fn extra_docs_extern_declarations_carry_their_ffi_signature() {
    let source = "/// C sin.\nextern fn sin(value: float) -> float\n";
    let (result, output) = export(source, "osp", "extra_docs_extern");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let page = read_text(&output.join("api/sin.md"));
    assert!(page.contains("float"), "{page}");
    assert!(page.contains("C sin."), "{page}");
}

#[test]
fn extra_docs_type_aliases_export_with_their_target() {
    let source = "/// User id.\ntype UserId = int\n";
    let (result, output) = export(source, "osp", "extra_docs_alias");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let page = read_text(&output.join("api/userid.md"));
    assert!(page.contains("int"), "alias target shown: {page}");
    assert!(page.contains("User id."), "{page}");
}

#[test]
fn extra_docs_generic_binders_appear_in_exported_signatures() {
    let source = "/// Wraps.\nfn wrap<T>(value: T) -> T = value\n";
    let (result, output) = export(source, "osp", "extra_docs_generic_binder");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let page = read_text(&output.join("api/wrap.md"));
    assert!(
        page.contains("fn wrap<T>(value: T) -> T"),
        "binder in signature: {page}"
    );
}

#[test]
fn extra_docs_static_effect_declarations_export_their_stage() {
    let source = "/// Compile-time state.\nstatic effect Frozen {\npeek : fn() -> int\n}\n";
    let (result, output) = export(source, "osp", "extra_docs_static_effect");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let page = read_text(&output.join("api/frozen.md"));
    assert!(page.contains("Compile-time state."), "{page}");
    assert!(page.contains("static effect Frozen"), "{page}");
    assert!(output.join("api/frozen-peek.md").is_file());
}

#[test]
fn extra_docs_ml_flavor_module_documentation_exports() {
    let source = "module Numbers\n    (** Members. *)\n    export twice n = n * 2\n";
    let (result, output) = export(source, "ospml", "extra_docs_ml_module");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let page = read_text(&output.join("api/numbers-twice.md"));
    assert!(page.contains("Members."), "{page}");
}

#[test]
fn extra_docs_missing_docs_dir_flag_takes_the_usage_branch() {
    let root = temp_dir("extra_docs_no_dir");
    let source = root.join("s.osp");
    std::fs::write(&source, "fn ok() = 1\n").expect("source");
    let mut command = osprey();
    let _ = command.arg("--docs").arg("--source").arg(&source);
    let result = finish(command);
    assert_eq!(result.code, Some(2), "{}", result.stderr);
}

#[test]
fn extra_docs_reject_directory_passed_through_source_flag_for_pages() {
    let root = temp_dir("extra_docs_dir_source");
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).expect("proj dir");
    let output = root.join("out");
    let mut command = osprey();
    let _ = command
        .arg("--docs")
        .arg("--source")
        .arg(&dir)
        .arg("--docs-dir")
        .arg(&output);
    let result = finish(command);
    // A bare empty directory is a rejected project, not a silent success.
    assert_eq!(result.code, Some(1), "{}", result.stderr);
}

#[test]
fn extra_docs_export_top_level_bindings_as_public_api() {
    let source = "fn helper() = 1\nlet exposed = helper()\n";
    let (result, output) = export(source, "osp", "extra_docs_private_toplevel");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(
        output.join("api/helper.md").is_file(),
        "top-level fn is public API"
    );
    assert!(
        output.join("api/exposed.md").is_file(),
        "top-level let is public API"
    );
}

#[test]
fn extra_docs_exclude_public_members_below_a_private_module() {
    let source =
        "module Outer {\nmodule Middle {\nexport module Inner {\nexport fn probe() = 1\n}\n}\n}\n";
    let (result, output) = export(source, "osp", "extra_docs_private_ancestor");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(output.join("api/outer.md").is_file());
    assert!(!output.join("api/outer-middle.md").exists());
    assert!(!output.join("api/outer-middle-inner.md").exists());
    assert!(!output.join("api/outer-middle-inner-probe.md").exists());
}
