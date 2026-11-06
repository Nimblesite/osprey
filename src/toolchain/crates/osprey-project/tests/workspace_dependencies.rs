//! A package names its dependencies by canonical key; in this repository their
//! source sits beside it. Nothing read `[dependencies]` for source, so
//! `osprey --check` and the language server reported every symbol from a
//! dependency as unknown, on every package that had one, while the test
//! manifests that list sibling source roots by hand stayed green.
#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::format_push_string,
    reason = "test assertions: an unwritable fixture is a test failure, not a production panic"
)]

use osprey_project::{discover, load_and_assemble};
use std::fs;
use std::path::{Path, PathBuf};

struct Workspace(PathBuf);

impl Workspace {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "osprey-workspace-deps-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        Self(root)
    }

    fn package(&self, directory: &str, key: &str, extra: &str) -> PathBuf {
        let root = self.0.join(directory);
        fs::create_dir_all(root.join("src")).expect("package source directory");
        let manifest = format!(
            "[project]\nname = \"{directory}\"\nsource_roots = [\"src\"]\ndefault_namespace = \"{directory}\"\n{extra}\n[package]\nname = \"{key}\"\n"
        );
        fs::write(root.join("osprey.toml"), manifest).expect("package manifest");
        fs::write(
            root.join("src/lib.ospml"),
            format!("namespace {directory}\nwidth x = x\n"),
        )
        .expect("package source");
        root
    }

    fn package_depending(&self, directory: &str, key: &str, dependencies: &[&str]) -> PathBuf {
        let root = self.package(directory, key, "");
        let manifest = root.join("osprey.toml");
        let mut text = fs::read_to_string(&manifest).expect("manifest");
        text.push_str("\n[dependencies]\n");
        for dependency in dependencies {
            text.push_str(&format!(
                "\"{dependency}\" = {{ epoch = 1, min_release = 1 }}\n"
            ));
        }
        fs::write(manifest, text).expect("manifest");
        root
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn found(root: &Path) -> Vec<String> {
    let (_, paths) = discover(root).unwrap_or_else(|errors| panic!("discovery: {errors:?}"));
    let mut names: Vec<String> = paths
        .iter()
        .map(|path| {
            let directory = path
                .canonicalize()
                .expect("a discovered source exists")
                .components()
                .rev()
                .nth(2)
                .expect("package directory")
                .as_os_str()
                .to_string_lossy()
                .into_owned();
            let file = path
                .file_name()
                .expect("file")
                .to_string_lossy()
                .into_owned();
            format!("{directory}/{file}")
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_package_reaches_the_sibling_packages_it_depends_on_transitively() {
    let workspace = Workspace::new("transitive");
    let _json = workspace.package("json", "@o/json", "");
    let _graphics = workspace.package_depending("graphics", "@o/graphics", &["@o/json"]);
    let _unrelated = workspace.package("unrelated", "@o/unrelated", "");
    let native = workspace.package_depending("native", "@o/native", &["@o/graphics"]);
    assert_eq!(
        found(&native),
        ["graphics/lib.ospml", "json/lib.ospml", "native/lib.ospml"]
    );
}

#[test]
fn a_dependency_cycle_ends_and_keeps_each_package_once() {
    let workspace = Workspace::new("cycle");
    let first = workspace.package_depending("first", "@o/first", &["@o/second"]);
    let _second = workspace.package_depending("second", "@o/second", &["@o/first"]);
    assert_eq!(found(&first), ["first/lib.ospml", "second/lib.ospml"]);
}

#[test]
fn a_sibling_directory_answers_only_to_the_key_its_own_manifest_declares() {
    let workspace = Workspace::new("identity");
    let _impostor = workspace.package("json", "@other/json", "");
    let native = workspace.package_depending("native", "@o/native", &["@o/json"]);
    assert_eq!(found(&native), ["native/lib.ospml"]);
}

#[test]
fn a_sibling_without_a_package_table_is_no_dependency() {
    let workspace = Workspace::new("plain");
    let plain = workspace.0.join("json");
    fs::create_dir_all(plain.join("src")).expect("source directory");
    fs::write(
        plain.join("osprey.toml"),
        "[project]\nname = \"@o/json\"\nsource_roots = [\"src\"]\n",
    )
    .expect("manifest");
    fs::write(plain.join("src/lib.ospml"), "namespace json\nwidth x = x\n").expect("source");
    let native = workspace.package_depending("native", "@o/native", &["@o/json"]);
    assert_eq!(found(&native), ["native/lib.ospml"]);
}

#[test]
fn a_dependency_with_its_own_python_sources_brings_them() {
    let workspace = Workspace::new("python");
    let numpy = workspace.package("numpy", "@o/numpy", "python_sources = [\"src/array.py\"]");
    fs::write(
        numpy.join("src/array.py"),
        "# osprey: flavor=python\nx = 1\n",
    )
    .expect("python");
    fs::write(numpy.join("src/ignored.py"), "x = 1\n").expect("python");
    let torch = workspace.package_depending("torch", "@o/torch", &["@o/numpy"]);
    assert_eq!(
        found(&torch),
        ["numpy/array.py", "numpy/lib.ospml", "torch/lib.ospml"]
    );
}

#[test]
fn an_installed_release_and_a_declared_path_stay_the_solvers_business() {
    let workspace = Workspace::new("installed");
    let _json = workspace.package("json", "@o/json", "");
    let installed = workspace.package_depending("installed", "@o/installed", &["@o/json"]);
    fs::create_dir_all(installed.join(".osprey/packages/@o/json/digest")).expect("install");
    assert_eq!(found(&installed), ["installed/lib.ospml"]);

    let pathed = workspace.package("pathed", "@o/pathed", "");
    let manifest = pathed.join("osprey.toml");
    let mut text = fs::read_to_string(&manifest).expect("manifest");
    text.push_str("\n[dependencies]\n\"@o/json\" = { path = \"../json\" }\n");
    fs::write(manifest, text).expect("manifest");
    assert_eq!(found(&pathed), ["pathed/lib.ospml"]);
}

#[test]
fn a_root_the_manifest_already_lists_is_not_read_twice() {
    let workspace = Workspace::new("listed");
    let _json = workspace.package("json", "@o/json", "");
    let native = workspace.package_depending("native", "@o/native", &["@o/json"]);
    let manifest = native.join("osprey.toml");
    let text = fs::read_to_string(&manifest)
        .expect("manifest")
        .replace("[\"src\"]", "[\"src\", \"../json/src\"]");
    fs::write(manifest, text).expect("manifest");
    assert_eq!(found(&native), ["json/lib.ospml", "native/lib.ospml"]);
}

#[test]
fn a_namespace_shared_with_a_dependency_is_not_folder_drift() {
    let workspace = Workspace::new("drift");
    let json = workspace.package("json", "@o/json", "");
    fs::write(json.join("src/lib.ospml"), "namespace ui\nwidth x = x\n").expect("source");
    let native = workspace.package_depending("native", "@o/native", &["@o/json"]);
    fs::write(native.join("src/lib.ospml"), "namespace ui\nheight x = x\n").expect("source");
    let project = load_and_assemble(&native).unwrap_or_else(|errors| panic!("{errors:?}"));
    assert_eq!(project.sources.len(), 2);
    assert!(
        project
            .warnings
            .iter()
            .all(|warning| warning.rule != "namespace-folder-drift"),
        "{:?}",
        project.warnings
    );

    fs::create_dir_all(native.join("src/deep")).expect("own folder");
    fs::write(
        native.join("src/deep/more.ospml"),
        "namespace ui\ndepth x = x\n",
    )
    .expect("source");
    let project = load_and_assemble(&native).unwrap_or_else(|errors| panic!("{errors:?}"));
    assert!(
        project
            .warnings
            .iter()
            .any(|warning| warning.rule == "namespace-folder-drift"),
        "the project's own two folders still drift: {:?}",
        project.warnings
    );
}
