//! [MATH-CODE-SOURCES], [MATH-CODE-UNTRACKED]: a `.lean` file under a source
//! root is compiled as the module generated from it, the module is stored
//! untracked and rewritten only when it changes, and a refusal fails the
//! project at the Lean source.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions: an unwritable fixture is a test failure, not a production panic"
)]

use osprey_project::{discover, load_and_assemble, AssembledProject, ProjectError};
use std::fs;
use std::path::{Path, PathBuf};

const KERNELS: &str = include_str!("../../osprey-math/tests/fixtures/Kernels.lean");
const GENERATED: &str = include_str!("../../osprey-math/tests/fixtures/Kernels.ospml");
const MAIN: &str = "namespace synth\n\nmain () =\n    print \"${Kernels::wrap 2.75} ${Kernels::blep (0.01, 0.02)} ${Kernels::sine 0.25} ${Kernels::inside (0.1, 0.5, true)} ${Kernels::pick 3.0}\"\n";

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("osprey-math-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let project = Self(root);
        let manifest = "[project]\nname = \"synth\"\nsource_roots = [\"src\"]\ndefault_namespace = \"synth\"\nentry = \"src/main.ospml\"\n";
        project.write("osprey.toml", manifest);
        project.write("src/main.ospml", MAIN);
        project
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(path, text).expect("fixture");
    }

    fn stored(&self, relative: &str) -> PathBuf {
        self.0.join(".osprey/math").join(relative)
    }

    fn assembled(&self) -> AssembledProject {
        let project = load_and_assemble(&self.0).unwrap_or_else(|errors| panic!("{errors:?}"));
        assert_eq!(osprey_types::check_program(&project.program), Vec::new());
        project
    }

    fn errors(&self) -> Vec<ProjectError> {
        load_and_assemble(&self.0).expect_err("the project must not build")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn read_only(path: &Path, value: bool) {
    let mut permissions = fs::metadata(path).expect("stored module").permissions();
    permissions.set_readonly(value);
    fs::set_permissions(path, permissions).expect("permissions");
}

fn drift(project: &AssembledProject) -> usize {
    let rule = "namespace-folder-drift";
    project
        .warnings
        .iter()
        .filter(|warning| warning.rule == rule)
        .count()
}

#[test]
fn a_lean_source_builds_as_the_module_generated_from_it() {
    let project = Project::new("builds");
    project.write("src/Kernels.lean", KERNELS);
    let (_, paths) = discover(&project.0).unwrap_or_else(|errors| panic!("{errors:?}"));
    let names: Vec<_> = paths
        .iter()
        .filter_map(|path| path.file_name()?.to_str())
        .collect();
    assert_eq!(names, ["Kernels.lean", "main.ospml"]);

    let assembled = project.assembled();
    let stored = project.stored("src/Kernels.lean.ospml");
    assert_eq!(
        fs::read_to_string(&stored).expect("stored module"),
        GENERATED
    );
    assert_eq!(
        fs::read_to_string(project.stored(".gitignore")).expect("ignore"),
        "*\n"
    );
    assert!(
        assembled.sources.iter().any(|source| source.path == stored),
        "{:?}",
        assembled.sources
    );
    assert!(
        assembled.public_api.contains_key("synth::Kernels::blep"),
        "{:?}",
        assembled.public_api
    );
    assert!(
        !assembled.public_api.contains_key("synth::Kernels::tau"),
        "a private def stays private"
    );
    assert_eq!(
        drift(&assembled),
        0,
        "a generated module lives beside its Lean source"
    );

    // Unchanged mathematics is never rewritten: a read-only module still builds.
    read_only(&stored, true);
    let _ = project.assembled();
    read_only(&stored, false);

    // Changed mathematics regenerates, and an edit to the module is overwritten.
    fs::write(&stored, "edited by hand\n").expect("edit");
    let _ = project.assembled();
    assert_eq!(
        fs::read_to_string(&stored).expect("stored module"),
        GENERATED
    );
    project.write(
        "src/Kernels.lean",
        &KERNELS.replace("6.283185307179586", "6.25"),
    );
    let _ = project.assembled();
    let regenerated = fs::read_to_string(&stored).expect("stored module");
    assert_eq!(regenerated, GENERATED.replace("6.283185307179586", "6.25"));
}

#[test]
fn a_generated_module_counts_as_living_in_its_lean_sources_folder() {
    let project = Project::new("folders");
    project.write("src/math/Kernels.lean", KERNELS);
    let assembled = project.assembled();
    assert!(project.stored("src/math/Kernels.lean.ospml").is_file());
    assert_eq!(
        drift(&assembled),
        2,
        "`synth` spans src and src/math: {:?}",
        assembled.warnings
    );
}

#[test]
fn a_refused_lean_source_fails_the_project_at_its_own_line() {
    let project = Project::new("refused");
    let source = "namespace synth.Kernels\n\ndef wrap (phase : Float) : Float :=\n  phase - Float.round phase\n\nend synth.Kernels\n";
    project.write("src/Kernels.lean", source);
    let errors = project.errors();
    let [error] = errors.as_slice() else {
        panic!("{errors:?}")
    };
    assert_eq!(
        error.path.as_deref(),
        Some(project.0.join("src/Kernels.lean").as_path())
    );
    assert_eq!((error.line, error.column), (Some(4), Some(10)));
    assert!(
        error.message.starts_with("UnsupportedFunction: "),
        "{}",
        error.message
    );
    assert!(error.message.contains("Float.round"), "{}", error.message);
    assert!(
        !project.stored("src/Kernels.lean.ospml").exists(),
        "a refused source stores nothing"
    );

    // A source the reader accepts but that defines nothing the program names.
    project.write(
        "src/Kernels.lean",
        "namespace synth.Other\ndef one : Float := 1\nend synth.Other\n",
    );
    let errors = project.errors();
    assert!(
        errors.iter().any(|error| error.message.contains("Kernels")),
        "{errors:?}"
    );
}
