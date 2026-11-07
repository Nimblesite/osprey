//! Adversarial API-export contracts beyond `api_docs.rs`: exported nested
//! modules with a private-ancestor negative control, `state` modules, externs,
//! type aliases, exact generic signatures, static effects, ML-flavor module
//! docs, and malformed option handling. Implements [DOC-EXPORT].

use super::api_docs::export_ok;
use super::{finish, osprey, read_text, temp_dir, Out};
use std::path::Path;

/// Fails unless `page` carries every fragment verbatim.
fn assert_page_has(page: &str, fragments: &[&str]) {
    for fragment in fragments {
        assert!(page.contains(fragment), "missing {fragment}: {page}");
    }
}

/// Exports `source` and asserts its `slug` page carries every fragment.
fn assert_exported_page(name: &str, extension: &str, source: &str, slug: &str, fragments: &[&str]) {
    let output = export_ok(source, extension, name);
    assert_page_has(
        &read_text(&output.join(format!("api/{slug}.md"))),
        fragments,
    );
}

/// Maps a `--docs-format` value to the page extension it writes.
fn page_extension(format: &str) -> &str {
    if format == "html" {
        "html"
    } else {
        "md"
    }
}

/// Runs `--docs` over `file` into `output`, optionally pinning the page format.
fn export_docs(file: &Path, output: &Path, format: Option<&str>) -> Out {
    let mut command = osprey();
    let _ = command
        .args(["--docs", "--source"])
        .arg(file)
        .arg("--docs-dir")
        .arg(output);
    if let Some(format) = format {
        let _ = command.args(["--docs-format", format]);
    }
    finish(&mut command)
}

/// [LSP-EFFECT-REQUIREMENTS] Published APIs retain transitive operation facts.
#[test]
fn extra_docs_publish_inferred_effect_requirements_in_both_flavors() {
    for (extension, source) in [
        ("osp", "effect Read { get: fn() -> int }\nfn leaf() = perform Read.get()\nfn relay() = leaf()\nfn safe() = {\n handle Read { get => 42 }\n relay()\n}\nfn invoke(f) = f()\n"),
        ("ospml", "effect Read\n    get : Unit => int\nleaf () = perform Read.get ()\nrelay () = leaf ()\nsafe () =\n    handle Read\n        get => 42\n    relay ()\ninvoke f = f ()\n"),
    ] {
        let output = export_ok(source, extension, &format!("effect_requirements_{extension}"));
        for name in ["leaf", "relay"] {
            assert_page_has(
                &read_text(&output.join(format!("api/{name}.md"))),
                &["Requires on full application: `Read.get`."],
            );
        }
        let safe = read_text(&output.join("api/safe.md"));
        assert!(!safe.contains("Requires on full application:"), "{safe}");
        assert_page_has(
            &read_text(&output.join("api/invoke.md")),
            &["Callback effects remain unresolved."],
        );
    }
}

/// Exported nested modules recurse: every level's page is written, down to the
/// leaf declaration.
#[test]
fn extra_docs_export_public_nested_module_chains_to_the_leaf() {
    let source = "module Outer {\nexport module Middle {\nexport module Inner {\nexport fn probe() = 1\n}\n}\n}\n";
    let output = export_ok(source, "osp", "extra_docs_deep_modules");
    for slug in [
        "outer-middle.md",
        "outer-middle-inner.md",
        "outer-middle-inner-probe.md",
    ] {
        assert!(output.join("api").join(slug).is_file(), "missing {slug}");
    }
    assert_page_has(
        &read_text(&output.join("api/outer-middle-inner-probe.md")),
        &["int"],
    );
}

/// Negative control: a private ancestor hides its whole subtree, even when the
/// child declaration itself is `export`.
#[test]
fn extra_docs_private_ancestor_excludes_its_entire_subtree() {
    let source = "module Outer {\nmodule Middle {\nexport fn secret() = 1\n}\n}\n";
    let output = export_ok(source, "osp", "extra_docs_private_ancestor");
    assert!(!output.join("api/outer-middle.md").exists());
    assert!(!output.join("api/outer-middle-secret.md").exists());
}

/// A `state` module exports its public members with genuine inferred types and
/// never its private ones. Explicit wrapping also pins `value` to `int`.
#[test]
fn extra_docs_state_module_exports_public_members_with_inferred_int() {
    let source =
        "state module Counter {\nexport fn hold(value) = wrapAdd(value, 1)\nfn internal() = 0\n}\n";
    let output = export_ok(source, "osp", "extra_docs_state_module");
    let page = read_text(&output.join("api/counter-hold.md"));
    assert_page_has(&page, &["::Counter::hold(value: int) -> int"]);
    assert!(!output.join("api/counter-internal.md").exists());
    // [MODULES-STATE-INVENTORY] lists ownership, never private implementation.
    let inventory = read_text(&output.join("api/project/state-boundaries.md"));
    assert_page_has(&inventory, &["Counter", "0 private cells"]);
    assert!(!inventory.contains("internal"), "{inventory}");
}

// [DOC-STATE-BOUNDARIES]: HTML, Markdown, privacy and regeneration agree.
#[test]
fn extra_docs_state_inventory_survives_both_flavors_and_regeneration(
) -> Result<(), Box<dyn std::error::Error>> {
    for (extension, source) in [
        ("osp", "namespace z;\nmodule Outer { state module Hidden { fn secret() = 0 } }\nnamespace a;\nstate module Empty {}\n"),
        ("ospml", "namespace z\nmodule Outer\n    state Hidden\n        secret () = 0\nnamespace a\nstate Empty\n    privateSeed = 0\n"),
    ] {
        for format in ["html", "markdown"] { assert_state_inventory(extension, source, format)?; }
    }
    Ok(())
}

fn assert_state_inventory(
    extension: &str,
    source: &str,
    format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir(&format!("state_inventory_{extension}_{format}"));
    let file = root.join(format!("source.{extension}"));
    let output = root.join("docs");
    std::fs::write(&file, source)?;
    let result = export_docs(&file, &output, Some(format));
    result.expect_stderr(0, &["2 warnings (state-boundary)"], &[]);
    assert_inventory_pages(&output, format)?;
    regenerate_without_state(&file, &output, extension, format)?;
    Ok(())
}

fn assert_inventory_pages(output: &Path, format: &str) -> Result<(), Box<dyn std::error::Error>> {
    let extension = page_extension(format);
    let page =
        std::fs::read_to_string(output.join(format!("api/project/state-boundaries.{extension}")))?;
    assert!(
        page.contains("a::Empty") && page.contains("z::Outer::Hidden"),
        "{page}"
    );
    assert!(
        page.find("a::Empty") < page.find("z::Outer::Hidden"),
        "{page}"
    );
    assert!(
        !page.contains("secret") && !page.contains("privateSeed"),
        "{page}"
    );
    let index = std::fs::read_to_string(output.join(format!("api/index.{extension}")))?;
    assert!(
        index.contains(&format!("project/state-boundaries.{extension}")),
        "{index}"
    );
    if format == "html" {
        let search = std::fs::read_to_string(output.join("assets/search-index.js"))?;
        assert!(search.contains("api/project/state-boundaries"), "{search}");
    }
    Ok(())
}

fn regenerate_without_state(
    file: &Path,
    output: &Path,
    extension: &str,
    format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = if extension == "osp" {
        "fn identity(x) = x\n"
    } else {
        "identity x = x\n"
    };
    std::fs::write(file, source)?;
    let result = export_docs(file, output, Some(format));
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let extension = page_extension(format);
    assert!(!output
        .join(format!("api/project/state-boundaries.{extension}"))
        .exists());
    assert!(!read_text(&output.join(format!("api/index.{extension}"))).contains("state-boundaries"));
    Ok(())
}

/// Declared externs, aliases, generic binders and ML module docs reach their
/// page verbatim, comments included.
#[test]
fn extra_docs_export_signatures_aliases_and_module_docs_verbatim() {
    let cases: &[(&str, &str, &str, &str, &[&str])] = &[
        (
            "extra_docs_extern",
            "osp",
            "/// C sin.\nextern fn sin(value: float) -> float\n",
            "sin",
            &["float", "C sin."],
        ),
        (
            "extra_docs_alias",
            "osp",
            "/// User id.\ntype UserId = int\n",
            "userid",
            &["int", "User id."],
        ),
        (
            "extra_docs_generic_binder",
            "osp",
            "fn wrap<T>(value: T) -> T = value\n",
            "wrap",
            &["fn wrap<T>(value: T) -> T"],
        ),
        (
            "extra_docs_ml_module",
            "ospml",
            "module Numbers\n    (** Members. *)\n    export twice n = n * 2\n",
            "numbers-twice",
            &["Members."],
        ),
    ];
    for &(name, extension, source, slug, fragments) in cases {
        assert_exported_page(name, extension, source, slug, fragments);
    }
}

/// The exported effect page must state the `static` stage explicitly, not just
/// carry the summary prose.
#[test]
fn extra_docs_static_effect_page_states_the_static_stage() {
    let source = "/// Compile-time state.\nstatic effect Frozen {\npeek : fn() -> int\n}\n";
    let output = export_ok(source, "osp", "extra_docs_static_effect");
    assert_page_has(
        &read_text(&output.join("api/frozen.md")),
        &["static effect Frozen"],
    );
    assert!(output.join("api/frozen-peek.md").is_file());
}

/// Missing `--docs-dir` takes the usage branch; `--source` pointing at a bare
/// directory is a rejected project, not a silent success.
#[test]
fn extra_docs_reject_incomplete_or_invalid_export_invocations() {
    let root = temp_dir("extra_docs_rejected_invocations");
    let source = root.join("s.osp");
    std::fs::write(&source, "fn ok() = 1\n").expect("source");
    let mut command = osprey();
    let _ = command.arg("--docs").arg("--source").arg(&source);
    let result = finish(&mut command);
    assert_eq!(result.code, Some(2), "{}", result.stderr);
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).expect("proj dir");
    let result = export_docs(&dir, &root.join("out"), None);
    assert_eq!(result.code, Some(1), "{}", result.stderr);
}

/// Top-level bindings are public API: both the fn and the runtime `let` export.
#[test]
fn extra_docs_top_level_bindings_are_public() {
    let source = "fn helper() = 1\nlet exposed = helper()\n";
    let output = export_ok(source, "osp", "extra_docs_public_toplevel");
    for slug in ["helper", "exposed"] {
        assert!(
            output.join(format!("api/{slug}.md")).is_file(),
            "missing {slug}"
        );
    }
}

/// An ML source whose declarations carry no comment at all. The page still has
/// to state what the declaration itself states, or a reader learns nothing.
///
/// The parameter names are the load-bearing assertion: ML lowers a clause head
/// into a chain of one-parameter lambdas, so a page built from the declaration
/// node alone names `id` and stops.
#[test]
fn extra_docs_undocumented_declarations_state_their_own_facts() {
    let source = "effect Store\n    read : int => string\n\n\
        fetch : int -> string -> string -> string ! Store\n\
        fetch id prefix suffix = \"${prefix}${perform Store.read id}${suffix}\"\n";
    let output = export_ok(source, "ospml", "extra_docs_derived_facts");
    let page = read_text(&output.join("api/fetch.md"));
    assert_page_has(
        &page,
        &[
            "fetch : int -> (string -> (string -> string)) ! Store\n```",
            "## Returns\n\n`string`",
            "## Effects",
            "- [Store]",
            "*Defined in `source.ospml`, line 5.*",
        ],
    );
    for parameter in ["- `id` — `int`", "- `prefix` — `string`", "- `suffix`"] {
        assert!(page.contains(parameter), "missing {parameter}: {page}");
    }
}

/// A Default signature line already names every parameter, so a derived list
/// under it would restate the line above with less in it. The declared row
/// appears once with source-local names.
#[test]
fn extra_docs_default_pages_state_effects_without_restating_parameters() {
    let source = "effect Store {\n  read: fn(int) -> string\n}\n\
        fn fetch(id) ![Store] = perform Store.read(id)\n";
    let output = export_ok(source, "osp", "extra_docs_default_facts");
    let page = read_text(&output.join("api/fetch.md"));
    assert_page_has(
        &page,
        &["fn fetch(id: int) -> string ![Store]\n```", "## Effects"],
    );
    assert!(
        !page.contains("## Parameters"),
        "a Default signature already names its parameters: {page}"
    );
}

#[test]
fn extra_docs_distinguish_an_explicit_empty_effect_row_from_an_omitted_row() {
    for (flavor, source) in [
        (
            "osp",
            "/// Declared pure.\nfn clean() -> int ![] = 42\nfn inferred() = 42\n",
        ),
        (
            "ospml",
            "(** Declared pure. *)\nclean : Unit -> int ![]\nclean () = 42\ninferred () = 42\n",
        ),
    ] {
        let output = export_ok(source, flavor, &format!("extra_docs_empty_row_{flavor}"));
        let clean = read_text(&output.join("api/clean.md"));
        let inferred = read_text(&output.join("api/inferred.md"));
        assert_page_has(&clean, &["![]", "Declared pure (`![]`)."]);
        assert!(!inferred.contains("![]"), "{flavor}: {inferred}");
    }
}

#[test]
fn extra_docs_show_an_open_effect_remainder() {
    for (flavor, source) in [
        (
            "osp",
            "effect Log { write: fn(string) -> Unit }\nfn invoke(callback: fn() -> Unit) -> Unit ![Log | e] = {\n    perform Log.write(\"fixed\")\n    callback()\n}\n",
        ),
        (
            "ospml",
            "effect Log\n    write : string => Unit\ninvoke : (Unit -> Unit) -> Unit ![Log | e]\ninvoke callback =\n    perform Log.write \"fixed\"\n    callback ()\n",
        ),
    ] {
        let output = export_ok(source, flavor, &format!("extra_docs_open_row_{flavor}"));
        let invoke = read_text(&output.join("api/invoke.md"));
        assert_page_has(
            &invoke,
            &["Log | e", "Additional effects come from the open remainder `e`."],
        );
    }
}

/// A member listing whose Description column is blank on every row tells a
/// reader nothing. An undocumented member is described by its own signature,
/// stripped of the qualification the Name column already carries.
#[test]
fn extra_docs_member_listings_fall_back_to_the_member_signature() {
    let source = "module Store {\nexport fn count() = 1\n}\n";
    let output = export_ok(source, "osp", "extra_docs_member_signature");
    let module = read_text(&output.join("api/store.md"));
    assert!(
        module.contains("| [count](store-count.md) | Function | `fn count() -> int` |"),
        "{module}"
    );
}
