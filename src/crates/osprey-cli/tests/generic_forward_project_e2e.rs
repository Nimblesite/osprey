//! End-to-end regression coverage for generic instantiation across file
//! order: a generic helper whose source file sorts AFTER its caller's file
//! still instantiates freshly at every call site (#277). Both flavors build
//! the same three-file project and hold the run to one exact output.
//!
//! The bug: declaration schemes were bound monomorphically until the
//! declaration's own body was inferred, and bodies are inferred in file order,
//! so every call site in an earlier file shared one instantiation.

#![expect(
    clippy::expect_used,
    reason = "fixture setup and subprocess failures must fail these tests immediately"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

const LATER: &str = "later";
const EARLY: &str = "early";

const RECORD_DEFAULTS: &str = r"namespace demo
type Frame =
    width : int
    height : int
module Later
    export plain = Frame(width = 1, height = 2)
    export preset = plain(width = 2)
    export updated width = plain(width = width)
";
const RECORD_CALLER: &str = r#"namespace demo
module Early
    export result () =
        changed = Later::updated Later::preset.width
        "${Later::plain.width},${Later::plain.height} ${changed.width}"
"#;

#[test]
fn module_record_updates_keep_the_original_and_apply_the_new_fields() {
    run_project("record_update", |root| {
        write_project(
            root,
            RECORD_DEFAULTS,
            RECORD_CALLER,
            "namespace demo\nprint (Early::result ())\n",
            "src/main.ospml",
            "ospml",
        );
    });
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create project directory");
    }
    std::fs::write(path, text).expect("write project source");
}

/// The shared project skeleton: `early` calls a generic `apply` declared in
/// `later`, at `int -> string` and `string -> int`, so one shared
/// instantiation cannot type-check.
fn write_project(root: &Path, later: &str, early: &str, main: &str, entry: &str, ext: &str) {
    write(&root.join("osprey.toml"), &format!("[project]\nname = \"order\"\nsource_roots = [\"src\"]\ndefault_namespace = \"demo\"\nentry = \"{entry}\"\n"));
    write(&root.join("src").join(format!("{LATER}.{ext}")), later);
    write(&root.join("src").join(format!("{EARLY}.{ext}")), early);
    write(&root.join("src").join(format!("main.{ext}")), main);
}

fn run_project(name: &str, project: impl FnOnce(&Path)) {
    let root = std::env::temp_dir().join(format!(
        "osprey_generic_forward_{name}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    project(&root);
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo_root())
        .arg(&root)
        .arg("--run")
        .output()
        .expect("run osprey");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "1,2 2\n",
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn generic_helper_in_a_later_file_instantiates_per_call_site_ml() {
    run_project("ml", |root| {
        write_project(
            root,
            "namespace demo\n\nmodule Later\n    step<T, U> : (List<U>, List<T>, (T) -> U, int) -> List<U>\n    step (out, items, f, index) = match listGet items index\n        Success value => listAppend out (f value)\n        Error _ => out\n\n    export apply<T, U> : (List<T>, (T) -> U) -> List<U>\n    apply (items, f) = range (0, length items) |> fold [] (\\(out, index) => step (out, items, f, index))\n",
            "namespace demo\n\nmodule Early\n    export labels values = Later::apply (values, \\value => toString value)\n\n    export sizes texts = Later::apply (texts, \\text => length text)\n",
            "namespace demo\nprint \"${join (Early::labels [1, 2]) \",\"} ${length (Early::sizes [\"a\", \"bb\"])}\"\n",
            "src/main.ospml",
            "ospml",
        );
    });
}

#[test]
fn generic_helper_in_a_later_file_instantiates_per_call_site_default() {
    run_project("default", |root| {
        write_project(
            root,
            "module Later {\n    fn step<T, U>(out: List<U>, items: List<T>, f: fn(T) -> U, index: int) -> List<U> = match listGet(items, index) {\n        Success { value } => listAppend(out, f(value))\n        Error => out\n    }\n\n    export fn apply<T, U>(items: List<T>, f: fn(T) -> U) -> List<U> = range(0, length(items)) |> fold([], fn(out, index) => step(out, items, f, index))\n}\n",
            "module Early {\n    export fn labels(values) = Later::apply(values, fn(value) => toString(value))\n\n    export fn sizes(texts) = Later::apply(texts, fn(text) => length(text))\n}\n",
            "print(\"${join(Early::labels([1, 2]), \",\")} ${listLength(Early::sizes([\"a\", \"bb\"]))}\")\n",
            "src/main.osp",
            "osp",
        );
    });
}
