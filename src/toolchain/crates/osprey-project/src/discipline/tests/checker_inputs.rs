//! Checker packages and projects: build digests over every checker input,
//! inferred HM signatures and unreadable or manifest-less checkers
//! ([TYPES-DISCIPLINE-REGISTRY]).

use super::{
    apply_with, bounded, cache, inputs, one_error, requests_of, seam, source, source_at,
    temp_project, write_file, Discipline, Duration, Path, PathBuf, ProcessRunner, Request, MARKED,
    SHAPES_SIGNATURE, THM_ERASED, THM_MARKED, THM_SIGNATURE,
};

/// Fixture texts shared by the checker-package and checker-project builders
/// and the digest tests that rewrite them.
const PACKAGE_MANIFEST: &str =
    "[package]\nname = \"@osprey/bounded\"\n\n[discipline]\nname = \"bounded\"\nchecker = \"checker.ospml\"\n";
const PROJECT_MANIFEST: &str =
    "[package]\nname = \"@osprey/bounded\"\n\n[discipline]\nname = \"bounded\"\nchecker = \"checker\"\n";
const CHECKER_MANIFEST: &str =
    "[project]\nname = \"checker\"\nsource_roots = [\"src\"]\nentry = \"src/main.ospml\"\n";
const CHECKER_MANIFEST_STRICT: &str =
    "[project]\nname = \"checker\"\nsource_roots = [\"src\"]\nentry = \"src/main.ospml\"\n\n[modules]\nallow_wildcard_imports = false\n";
const RANGE_MAIN: &str = "namespace bounded\n\nimport bounded::Range\n\nmain () = Range::zero\n";
const RANGE_ZERO: &str = "namespace bounded\n\nmodule Range\n    export zero = 0\n";
const RANGE_ONE: &str = "namespace bounded\n\nmodule Range\n    export zero = 1\n";

/// Overwrite the checker input `relative` with `text` and return the build
/// digest the change produces.
fn digest_after_rewrite(discipline: &Discipline, relative: &str, text: &str) -> u64 {
    std::fs::write(discipline.root.join(relative), text).expect("rewrite input");
    inputs::build_digest(discipline).expect("digest")
}

/// A package on disk: manifest, entry module and one imported module whose
/// text a test rewrites.
fn checker_package(name: &str) -> PathBuf {
    let root = temp_project(name).directory;
    write_file(&root, "osprey.toml", PACKAGE_MANIFEST);
    write_file(&root, "checker.ospml", RANGE_MAIN);
    write_file(&root, "src/Range.ospml", RANGE_ZERO);
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
    let same = inputs::build_digest(&discipline).expect("digest");
    assert_eq!(same, digest, "unchanged inputs keep the digest");
    let compiler = root.join("fake-compiler");
    std::fs::write(&compiler, b"compiler one").expect("compiler");
    let build_dir = root.join("build");
    let compiler_digest = inputs::compiler_digest(&compiler).expect("compiler digest");
    let built = build_dir.join(format!("bounded-{digest:016x}-{compiler_digest:016x}"));
    std::fs::create_dir_all(&build_dir).expect("build dir");
    std::fs::write(&built, b"built").expect("prebuilt binary");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let found = ProcessRunner::new(compiler, build_dir).executable(&discipline, deadline);
    let why = "a built binary is found again under the name its inputs hash to";
    assert_eq!(found.expect("reused binary"), built, "{why}");
    let after = digest_after_rewrite(&discipline, "src/Range.ospml", RANGE_ONE);
    let why = "a change to an imported module is a change to an input";
    assert_ne!(after, digest, "{why}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_unreadable_checker_entry_is_an_error_and_not_a_cache_miss() {
    let config = temp_project("cache-unreadable");
    let dir = &config.directory;
    let discipline = bounded(dir);
    let runner = ProcessRunner::new(dir.join("no-compiler"), dir.join("build"));
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
    let (config, runner, registry) = seam("imports-omitted", THM_SIGNATURE, THM_ERASED);
    let sources = vec![
        source_at("src/Shapes.ospml", SHAPES_HM),
        source_at("src/Thm.ospml", USES_SHAPES),
    ];
    let _ = apply_with(&config, sources, &runner, &registry);
    let import = &requests_of(&runner)[0]["imports"][0];
    assert_eq!(import["signature"], SHAPES_SIGNATURE);
    let omitted = serde_json::json!(["type Shape", "value limit"]);
    let why = "a checker is told the signature is partial";
    assert_eq!(import["omitted"], omitted, "{why}");
    assert_eq!(requests_of(&runner)[0]["version"], 2);
}

#[test]
fn an_hm_import_that_does_not_type_check_is_an_error_and_never_a_signature() {
    let (config, runner, registry) = seam("imports-broken", THM_SIGNATURE, THM_ERASED);
    let sources = vec![
        source_at("src/Nat.ospml", BROKEN_HM),
        source_at("src/Thm.ospml", THM_MARKED),
    ];
    let errors = apply_with(&config, sources, &runner, &registry).expect_err("rejected");
    let why = format!("no checker runs after an import failed: {errors:?}");
    assert_eq!(runner.calls.get(), 0, "{why}");
    let message = one_error(&errors);
    let needle = "could not be inferred for an importing checker";
    assert!(message.contains(needle), "{message}");
}

/// A checker declared as a project directory: its own manifest names the
/// entry, and the entry imports a sibling module.
fn checker_project(name: &str) -> PathBuf {
    let root = temp_project(name).directory;
    write_file(&root, "osprey.toml", PROJECT_MANIFEST);
    write_file(&root, "checker/src/main.ospml", RANGE_MAIN);
    write_file(&root, "checker/src/range.ospml", RANGE_ZERO);
    write_file(&root, "checker/osprey.toml", CHECKER_MANIFEST);
    root
}

/// A discipline whose checker is the `checker/` project directory beside
/// the package manifest.
fn checker_discipline(root: &Path) -> Discipline {
    Discipline {
        checker: root.join("checker"),
        ..bounded(root)
    }
}

#[test]
fn a_checker_project_directory_is_digested_over_its_manifest_and_every_module() {
    // Implements [TYPES-DISCIPLINE-REGISTRY]: a checker is an Osprey program
    // built by the ordinary pipeline, so it may be a project of many modules;
    // every module and the project manifest are build inputs.
    let root = checker_project("checker-dir");
    let discipline = checker_discipline(&root);
    let digest = inputs::build_digest(&discipline).expect("digest");
    let module = digest_after_rewrite(&discipline, "checker/src/range.ospml", RANGE_ONE);
    let why = "a module of the checker project is an input";
    assert_ne!(module, digest, "{why}");
    let strict = CHECKER_MANIFEST_STRICT;
    let after = digest_after_rewrite(&discipline, "checker/osprey.toml", strict);
    let why = "the checker project's manifest is an input";
    assert_ne!(after, module, "{why}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_checker_directory_without_a_manifest_is_an_error_naming_it() {
    let root = checker_project("checker-dir-no-manifest");
    std::fs::remove_file(root.join("checker/osprey.toml")).expect("remove manifest");
    let discipline = checker_discipline(&root);
    let message = inputs::build_digest(&discipline).expect_err("no manifest");
    let mentions_manifest = message.contains("checker/osprey.toml")
        || message.contains("checker") && message.contains("osprey.toml");
    assert!(mentions_manifest, "{message}");
    let _ = std::fs::remove_dir_all(&root);
}
