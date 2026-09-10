//! Public API export contracts. Implements [DOC-EXPORT].

use super::{finish, osprey, read_text, temp_dir};

pub(super) fn export(
    source: &str,
    extension: &str,
    name: &str,
) -> (super::Out, std::path::PathBuf) {
    let root = temp_dir(name);
    let path = root.join(format!("source.{extension}"));
    let written = std::fs::write(&path, source);
    assert!(written.is_ok(), "source fixture: {written:?}");
    let output = root.join("docs");
    let mut command = osprey();
    let _ = command
        .arg("--docs")
        .arg("--source")
        .arg(&path)
        .arg("--docs-dir")
        .arg(&output);
    (finish(command), output)
}

#[test]
fn api_docs_reject_invalid_sources_before_writing_pages() {
    for (source, tag) in [
        ("fn incomplete(", "syntax"),
        ("let n: int = \"bad\"", "types"),
    ] {
        let (result, output) = export(source, "osp", &format!("docs_invalid_{tag}"));
        assert_eq!(result.code, Some(1), "{}", result.stderr);
        assert!(
            !output.exists(),
            "invalid input must not publish partial documentation"
        );
    }
}

#[test]
fn api_docs_export_inferred_signatures_in_both_flavors() {
    for (source, extension, signature) in [
        (
            "/// Adds text.\nfn greet(name) = \"hello \" + name\n",
            "osp",
            "fn greet(name: string) -> string",
        ),
        (
            "(** Adds text. *)\ngreet name = \"hello \" + name\n",
            "ospml",
            "greet : string -> string",
        ),
    ] {
        let (result, output) = export(source, extension, &format!("docs_signature_{extension}"));
        assert_eq!(result.code, Some(0), "{}", result.stderr);
        let page = read_text(&output.join("api/greet.md"));
        assert!(page.contains(signature), "{page}");
        assert!(page.contains("Adds text."), "{page}");
    }
}

#[test]
fn api_docs_include_children_of_undocumented_modules_and_public_undocumented_apis() {
    let source = "module Math {\n/// Public identity.\nexport fn identity(value) = value\nexport fn label() = \"Math\"\nfn secret() = \"private\"\n}\n";
    let (result, output) = export(source, "osp", "docs_module_public");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(output.join("api/math.md").is_file());
    assert!(read_text(&output.join("api/math-identity.md")).contains("Public identity."));
    assert!(read_text(&output.join("api/math-label.md")).contains("string"));
    assert!(!output.join("api/math-secret.md").exists());
    let module = read_text(&output.join("api/math.md"));
    assert!(
        module.contains("[identity](math-identity.md)")
            && module.contains("[label](math-label.md)"),
        "{module}"
    );
    assert!(!module.contains("secret"), "{module}");
}

#[test]
fn api_docs_reject_missing_flag_values_and_unknown_flags() {
    for option in [
        "--source",
        "--docs-format",
        "--docs-page",
        "--docs-css",
        "--docs-theme",
        "--typo",
    ] {
        let root = temp_dir(&format!("docs_option_{}", option.trim_start_matches('-')));
        let mut command = osprey();
        let _ = command
            .args(["--docs", "--docs-dir"])
            .arg(&root)
            .arg(option);
        let result = finish(command);
        assert_eq!(result.code, Some(2), "{option}: {}", result.stderr);
    }
}

#[test]
fn api_docs_export_ascribed_public_members_without_leaking_abstract_representation() {
    let source = "signature Api { type Item\nfn create() -> Item\n}\nmodule Store : Api {\ntype Item = { secret: int }\nfn create() = Item { secret: 42 }\nfn hidden() = 7\n}\n";
    let (result, output) = export(source, "osp", "docs_ascription");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(output.join("api/store-create.md").is_file());
    assert!(output.join("api/store-item.md").is_file());
    assert!(!output.join("api/store-hidden.md").exists());
    assert!(!read_text(&output.join("api/store-item.md")).contains("secret"));
    assert!(read_text(&output.join("api/api.md")).contains("create"));
}

#[test]
fn api_docs_include_type_shapes_operation_signatures_and_authorship() {
    let source = "//! Data tools.\n//! @author Ada\n/// Coordinates.\ntype Point = { x: int, y: int }\n/// Outcomes.\ntype Outcome<T> = Found { value: T } | Missing\n/// Logging.\neffect Log {\n/// Writes a message.\nline : fn(string) -> Unit\n}\n";
    let (result, output) = export(source, "osp", "docs_shapes");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let point = read_text(&output.join("api/point.md"));
    assert!(
        point.contains("x: int") && point.contains("y: int"),
        "{point}"
    );
    let outcome = read_text(&output.join("api/outcome.md"));
    assert!(outcome.contains("type Outcome<T>"), "{outcome}");
    assert!(
        outcome.contains("Found") && outcome.contains("value: T") && outcome.contains("Missing"),
        "{outcome}"
    );
    let operation = read_text(&output.join("api/log-line.md"));
    assert!(
        operation.contains("fn(string) -> Unit") && operation.contains("Writes a message."),
        "{operation}"
    );
    assert!(read_text(&output.join("api/file-source-osp.md")).contains("Ada"));
}

#[test]
fn api_docs_merge_project_namespaces_and_keep_both_file_docs() {
    let root = temp_dir("docs_mixed_project");
    std::fs::write(
        root.join("osprey.toml"),
        "[project]\nname = \"docs\"\nentry = \"main.osp\"\n",
    )
    .expect("manifest");
    std::fs::write(
        root.join("main.osp"),
        "//! Application docs.\nnamespace common {\nmodule Numbers { export fn value() = 42 }\n}\n",
    )
    .expect("default source");
    std::fs::write(root.join("words.ospml"), "//! Library docs.\nnamespace common\n    module Words\n        export value () = \"word\"\n").expect("ML source");
    let output = root.join("docs");
    let mut command = osprey();
    let _ = command
        .arg("--docs")
        .arg(&root)
        .arg("--docs-dir")
        .arg(&output);
    let result = finish(command);
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(output.join("api/common.md").is_file());
    assert!(read_text(&output.join("api/common-numbers-value.md")).contains("int"));
    assert!(read_text(&output.join("api/common-words-value.md")).contains("Unit -> string"));
    assert!(read_text(&output.join("api/file-main-osp.md")).contains("Application docs."));
    assert!(read_text(&output.join("api/file-words-ospml.md")).contains("Library docs."));
}

#[test]
fn api_docs_export_library_projects_without_an_application_entry() {
    let root = temp_dir("docs_library_project");
    std::fs::write(root.join("osprey.toml"), "[project]\nname = \"library\"\n").expect("manifest");
    std::fs::write(
        root.join("numbers.osp"),
        "module Numbers { export fn value() = 42 }\n",
    )
    .expect("numbers");
    std::fs::write(
        root.join("words.osp"),
        "module Words { export fn value() = \"word\" }\n",
    )
    .expect("words");
    let output = root.join("docs");
    let mut command = osprey();
    let _ = command
        .arg("--docs")
        .arg(&root)
        .arg("--docs-dir")
        .arg(&output);
    let result = finish(command);
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(output.join("api/numbers-value.md").is_file());
    assert!(output.join("api/words-value.md").is_file());
}

#[test]
fn api_docs_keep_inlined_constants_and_runtime_binding_types() {
    let source = "namespace sample {\nmodule Units { export let each = 1\nexport let label = \"unit\" }\nfn identity(value) = value\nlet runtime = identity(\"runtime\")\n}\n";
    let (result, output) = export(source, "osp", "docs_constant_types");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(read_text(&output.join("api/sample-units-each.md")).contains(": int"));
    assert!(read_text(&output.join("api/sample-units-label.md")).contains(": string"));
    assert!(read_text(&output.join("api/sample-runtime.md")).contains(": string"));
}

#[test]
fn api_docs_resolve_visibility_by_namespace_identity_not_name_suffix() {
    let source = "module A { fn secret() = \"private\"\nexport fn exposed() = 1 }\nnamespace other { module A { export fn secret() = 42 } }\n";
    let (result, output) = export(source, "osp", "docs_visibility_identity");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(!output.join("api/a-secret.md").exists());
    assert!(output.join("api/a-exposed.md").is_file());
    assert!(read_text(&output.join("api/other-a-secret.md")).contains("int"));
}

#[test]
fn api_docs_never_overwrite_an_index_named_declaration() {
    let (result, output) = export("/// Indexing.\nfn index() = 42\n", "osp", "docs_index_name");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(read_text(&output.join("api/index-2.md")).contains("Indexing."));
    assert!(read_text(&output.join("api/index.md")).contains("index-2.md"));
}

#[test]
fn api_docs_reject_duplicate_guide_paths_before_any_output() {
    let root = temp_dir("docs_duplicate_guides");
    let guide = root.join("intro.md");
    std::fs::write(&guide, "# Introduction\nA guide.\n").expect("guide");
    let output = root.join("docs");
    let mut command = osprey();
    let _ = command
        .args(["--docs", "--docs-dir"])
        .arg(&output)
        .arg("--docs-page")
        .arg(&guide)
        .arg("--docs-page")
        .arg(&guide);
    let result = finish(command);
    assert_eq!(result.code, Some(1), "{}", result.stderr);
    assert!(!output.exists());
}

#[test]
fn api_docs_reject_an_explicit_non_markdown_page() {
    let root = temp_dir("docs_non_markdown_page");
    let guide = root.join("intro.txt");
    std::fs::write(&guide, "# Introduction\n").expect("guide");
    let output = root.join("docs");
    let mut command = osprey();
    let _ = command
        .args(["--docs", "--docs-dir"])
        .arg(&output)
        .arg("--docs-page")
        .arg(&guide);
    let result = finish(command);
    assert_eq!(result.code, Some(1), "{}", result.stderr);
    assert!(result.stderr.contains("Markdown"), "{}", result.stderr);
    assert!(!output.exists());
}

#[test]
fn api_docs_html_integrates_authored_pages_themes_and_ordered_css() {
    for theme in ["osprey", "midnight", "paper"] {
        let root = temp_dir(&format!("docs_site_{theme}"));
        let guides = root.join("guides/deep");
        std::fs::create_dir_all(&guides).expect("guides");
        std::fs::write(
            guides.join("My Guide.md"),
            "# Learning Osprey\n\nA **guide**.\n",
        )
        .expect("guide");
        std::fs::write(guides.join("untitled.md"), "No heading here.\n").expect("untitled guide");
        std::fs::write(guides.join("ignored.txt"), "not a Markdown page").expect("other input");
        let first = root.join("brand.css");
        let second = root.join("override.css");
        std::fs::write(&first, ":root { --accent: red; }").expect("brand CSS");
        std::fs::write(&second, ":root { --accent: blue; }").expect("override CSS");
        let output = root.join("site");
        let mut command = osprey();
        let _ = command
            .args([
                "--docs",
                "--docs-format",
                "html",
                "--docs-theme",
                theme,
                "--docs-dir",
            ])
            .arg(&output)
            .arg("--docs-page")
            .arg(root.join("guides"))
            .arg("--docs-css")
            .arg(&first)
            .arg("--docs-css")
            .arg(&second);
        let result = finish(command);
        assert_eq!(result.code, Some(0), "{}", result.stderr);
        let page = read_text(&output.join("guides/deep/my-guide.html"));
        assert!(
            page.contains("Learning Osprey") && page.contains("<strong>guide</strong>"),
            "{page}"
        );
        assert!(
            page.find("theme.css").expect("theme")
                < page.find("custom-0-brand.css").expect("brand")
        );
        assert!(
            page.find("custom-0-brand.css").expect("brand")
                < page.find("custom-1-override.css").expect("override")
        );
        assert_eq!(
            read_text(&output.join("assets/custom-1-override.css")),
            ":root { --accent: blue; }"
        );
        assert!(output.join("guides/deep/untitled.html").is_file());
        assert!(!output.join("guides/deep/ignored.html").exists());
        assert!(output.join("index.html").is_file());
    }
}

#[test]
fn api_docs_reject_incompatible_and_unknown_option_values() {
    for arguments in [
        vec!["--docs-format", "pdf"],
        vec!["--docs-theme", "missing"],
        vec!["--flavor", "unknown"],
        vec!["--docs-css", "brand.css"],
        vec!["--docs-theme", "midnight"],
        vec!["--source", "a.osp", "--source", "b.osp"],
    ] {
        let output = temp_dir("docs_invalid_values").join("unwritten");
        let mut command = osprey();
        let _ = command
            .args(["--docs", "--docs-dir"])
            .arg(&output)
            .args(&arguments);
        let result = finish(command);
        assert_eq!(result.code, Some(2), "{arguments:?}: {}", result.stderr);
        assert!(!output.exists(), "{arguments:?}");
    }
}

#[test]
fn api_docs_export_every_module_corpus_source_in_both_flavors() {
    let root = temp_dir("docs_module_corpus");
    let mut count = 0;
    for entry in std::fs::read_dir(super::repo_root().join("tests/modules")).expect("module corpus")
    {
        let path = entry.expect("module source").path();
        if !path
            .extension()
            .is_some_and(|extension| extension == "osp" || extension == "ospml")
        {
            continue;
        }
        let output = root.join(path.file_name().expect("filename"));
        let mut command = osprey();
        let _ = command
            .arg("--docs")
            .arg(&path)
            .arg("--docs-dir")
            .arg(&output);
        let result = finish(command);
        assert_eq!(
            result.code,
            Some(0),
            "{}: {}",
            path.display(),
            result.stderr
        );
        assert!(output.join("api/index.md").is_file(), "{}", path.display());
        count += 1;
    }
    assert!(
        count >= 24,
        "expected both flavors of the complete module corpus, found {count}"
    );
}

#[test]
fn api_docs_honor_file_flavor_overrides_and_reject_project_overrides() {
    let root = temp_dir("docs_flavor_override");
    let source = root.join("source.osp");
    std::fs::write(&source, "label () = \"ML\"\n").expect("ML in a default extension");
    let output = root.join("docs");
    let mut command = osprey();
    let _ = command
        .args(["--docs", "--flavor", "ml"])
        .arg(&source)
        .arg("--docs-dir")
        .arg(&output);
    let result = finish(command);
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert!(read_text(&output.join("api/label.md")).contains("label : Unit -> string"));
    std::fs::write(
        root.join("osprey.toml"),
        "[project]\nname = \"flavor\"\nflavor = \"ml\"\n",
    )
    .expect("manifest");
    let mut command = osprey();
    let _ = command
        .args(["--docs", "--flavor", "ml"])
        .arg(&root)
        .arg("--docs-dir")
        .arg(root.join("unwritten"));
    let result = finish(command);
    assert_eq!(result.code, Some(1), "{}", result.stderr);
    assert!(
        result.stderr.contains("--flavor applies to single files"),
        "{}",
        result.stderr
    );
    assert!(!root.join("unwritten").exists());
}

#[test]
fn api_docs_cleanup_preserves_unrelated_markdown() {
    let root = temp_dir("docs_cleanup_owned");
    let source = root.join("library.osp");
    let output = root.join("docs");
    let run = || {
        let mut command = osprey();
        let _ = command
            .arg("--docs")
            .arg(&source)
            .arg("--docs-dir")
            .arg(&output);
        finish(command)
    };
    std::fs::write(&source, "fn oldApi() = 1\n").expect("initial source");
    assert_eq!(run().code, Some(0));
    std::fs::write(output.join("api/handwritten.md"), "keep this").expect("manual page");
    std::fs::write(&source, "fn newApi() = 2\n").expect("updated source");
    assert_eq!(run().code, Some(0));
    assert!(!output.join("api/oldapi.md").exists());
    assert!(output.join("api/newapi.md").is_file());
    assert_eq!(read_text(&output.join("api/handwritten.md")), "keep this");
}

#[cfg(unix)]
#[test]
fn api_docs_reject_output_symlinks_without_touching_their_targets() {
    let root = temp_dir("docs_symlink");
    let outside = root.join("outside");
    let output = root.join("docs");
    std::fs::create_dir_all(&outside).expect("outside");
    std::fs::create_dir_all(&output).expect("output");
    std::fs::write(outside.join("index.md"), "original").expect("outside page");
    std::os::unix::fs::symlink(&outside, output.join("functions")).expect("symlink");
    let mut command = osprey();
    let _ = command.args(["--docs", "--docs-dir"]).arg(&output);
    let result = finish(command);
    assert_eq!(result.code, Some(1), "{}", result.stderr);
    assert_eq!(read_text(&outside.join("index.md")), "original");
    assert_eq!(
        std::fs::read_dir(outside).expect("outside listing").count(),
        1
    );
}
