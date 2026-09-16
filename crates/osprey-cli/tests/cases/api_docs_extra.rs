//! Adversarial API-export contracts beyond `api_docs.rs`: exported nested
//! modules with a private-ancestor negative control, `state` modules, externs,
//! type aliases, exact generic signatures, static effects, ML-flavor module
//! docs, and malformed option handling. Implements [DOC-EXPORT].

use super::api_docs::export;
use super::{finish, osprey, read_text, temp_dir};

/// Exported nested modules recurse: every level's page is written, down to the
/// leaf declaration.
#[test]
fn extra_docs_export_public_nested_module_chains_to_the_leaf() {
    let source = "module Outer {\nexport module Middle {\nexport module Inner {\nexport fn probe() = 1\n}\n}\n}\n";
    let (result, output) = export(source, "osp", "extra_docs_deep_modules");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    for slug in [
        "outer-middle.md",
        "outer-middle-inner.md",
        "outer-middle-inner-probe.md",
    ] {
        assert!(output.join("api").join(slug).is_file(), "missing {slug}");
    }
    assert!(read_text(&output.join("api/outer-middle-inner-probe.md")).contains("int"));
}

/// Negative control: a private ancestor hides its whole subtree, even when the
/// child declaration itself is `export`.
#[test]
fn extra_docs_private_ancestor_excludes_its_entire_subtree() {
    let source = "module Outer {\nmodule Middle {\nexport fn secret() = 1\n}\n}\n";
    let (result, output) = export(source, "osp", "extra_docs_private_ancestor");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(!output.join("api/outer-middle.md").exists());
    assert!(!output.join("api/outer-middle-secret.md").exists());
}

/// A `state` module exports its public members with genuine inferred types and
/// never its private ones. The arithmetic effect needs `?:`, which also pins
/// `value` to `int`.
#[test]
fn extra_docs_state_module_exports_public_members_with_inferred_int() {
    let source =
        "state module Counter {\nexport fn hold(value) = (value + 1) ?: 0\nfn internal() = 0\n}\n";
    let (result, output) = export(source, "osp", "extra_docs_state_module");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let page = read_text(&output.join("api/counter-hold.md"));
    assert!(
        page.contains("::Counter::hold(value: int) -> int"),
        "{page}"
    );
    assert!(!output.join("api/counter-internal.md").exists());
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
    assert!(page.contains("int"), "{page}");
    assert!(page.contains("User id."), "{page}");
}

/// The generic binder and its parameter/return types must all survive export —
/// not merely any `T` substring.
#[test]
fn extra_docs_generic_signature_is_exact() {
    let source = "fn wrap<T>(value: T) -> T = value\n";
    let (result, output) = export(source, "osp", "extra_docs_generic_binder");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let page = read_text(&output.join("api/wrap.md"));
    assert!(page.contains("fn wrap<T>(value: T) -> T"), "{page}");
}

/// The exported effect page must state the `static` stage explicitly, not just
/// carry the summary prose.
#[test]
fn extra_docs_static_effect_page_states_the_static_stage() {
    let source = "/// Compile-time state.\nstatic effect Frozen {\npeek : fn() -> int\n}\n";
    let (result, output) = export(source, "osp", "extra_docs_static_effect");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let page = read_text(&output.join("api/frozen.md"));
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
    // `--source` pointing at a directory exports the project tree; a bare empty
    // directory is a rejected project, not a silent success.
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
    assert_eq!(result.code, Some(1), "{}", result.stderr);
}

/// Top-level bindings are public API: both the fn and the runtime `let` export.
#[test]
fn extra_docs_top_level_bindings_are_public() {
    let source = "fn helper() = 1\nlet exposed = helper()\n";
    let (result, output) = export(source, "osp", "extra_docs_public_toplevel");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(output.join("api/helper.md").is_file());
    assert!(output.join("api/exposed.md").is_file());
}

/// An ML source whose declarations carry no comment at all. The page still has
/// to state what the declaration itself states, or a reader learns nothing from
/// opening it.
///
/// The parameter names are the load-bearing assertion: ML lowers a clause head
/// into a chain of one-parameter lambdas, so a page built from the declaration
/// node alone names `id` and stops — telling a reader a three-argument function
/// takes one argument.
#[test]
fn extra_docs_undocumented_declarations_state_their_own_facts() {
    let source = "effect Store\n    read : int => string\n\n\
        fetch : int -> string -> string -> string ! Store\n\
        fetch id prefix suffix = \"${prefix}${perform Store.read id}${suffix}\"\n";
    let (result, output) = export(source, "ospml", "extra_docs_derived_facts");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let page = read_text(&output.join("api/fetch.md"));
    assert!(
        page.contains("fetch : int -> (string -> (string -> string)) ! Store"),
        "the effect row belongs on the signature: {page}"
    );
    for parameter in ["- `id` — `int`", "- `prefix` — `string`", "- `suffix`"] {
        assert!(page.contains(parameter), "missing {parameter}: {page}");
    }
    assert!(page.contains("## Returns\n\n`string`"), "{page}");
    assert!(
        page.contains("## Effects") && page.contains("- [Store]"),
        "{page}"
    );
    assert!(
        page.contains("*Defined in `source.ospml`, line 5.*"),
        "{page}"
    );
}

/// A Default signature line already names every parameter, so a derived list
/// under it would restate the line above with less in it. The effect row is
/// still the fact the type model leaves out, and it still appears.
#[test]
fn extra_docs_default_pages_state_effects_without_restating_parameters() {
    let source = "effect Store {\n  read: fn(int) -> string\n}\n\
        fn fetch(id) ![Store] = perform Store.read(id)\n";
    let (result, output) = export(source, "osp", "extra_docs_default_facts");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let page = read_text(&output.join("api/fetch.md"));
    assert!(
        page.contains("![Store]"),
        "the effect row is missing: {page}"
    );
    assert!(page.contains("## Effects"), "{page}");
    assert!(
        !page.contains("## Parameters"),
        "a Default signature already names its parameters: {page}"
    );
}

/// A member listing whose Description column is blank on every row tells a
/// reader nothing. An undocumented member is described by its own signature,
/// stripped of the qualification the Name column already carries.
#[test]
fn extra_docs_member_listings_fall_back_to_the_member_signature() {
    let source = "module Store {\nexport fn count() = 1\n}\n";
    let (result, output) = export(source, "osp", "extra_docs_member_signature");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let module = read_text(&output.join("api/store.md"));
    assert!(
        module.contains("| [count](store-count.md) | Function | `fn count() -> int` |"),
        "{module}"
    );
}
