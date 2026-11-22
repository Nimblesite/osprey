//! End-to-end regression coverage for generic instantiation across file
//! order: a generic helper whose source file sorts AFTER its caller's file
//! still instantiates freshly at every call site (#277). Both flavors build
//! the same three-file project and hold the run to one exact output.
//!
//! The bug: declaration schemes were bound monomorphically until the
//! declaration's own body was inferred, and bodies are inferred in file order,
//! so every call site in an earlier file shared one instantiation.
//!
//! The same harness covers a closure whose concrete instantiation
//! is pinned only by a LATER use of the let-generalized binding that holds it:
//! the Vigil shape, where `Sidebar::view` passes `\key => areaRow (key)` to a
//! generic `Lists::map` from another namespace and only `Ui::rows`'s action
//! argument settles `Row<A>`'s `A`. `--check` accepted the program while
//! codegen rejected the staged copy of `view` with "a closure value with a
//! still-generic type".
//!
//! That case builds a three-file project of its own — a `ui` namespace holding
//! the generic vocabulary, a `vigil` namespace holding the sidebar, and a main
//! that discharges a static signal so the staging pass specializes `view` —
//! and hold the run to one exact output.

#![expect(
    clippy::expect_used,
    reason = "fixture setup and subprocess failures must fail these tests immediately"
)]

#[path = "common/repo_root.rs"]
mod repo_root;
use repo_root::repo_root;
#[path = "common/write_file.rs"]
mod write_file;
use write_file::write;

use std::path::Path;
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
    run_project("record_update", ORDER_OUTPUT, |root| {
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

/// The shared project skeleton: `early` calls a generic `apply` declared in
/// `later`, at `int -> string` and `string -> int`, so one shared
/// instantiation cannot type-check.
fn write_project(root: &Path, later: &str, early: &str, main: &str, entry: &str, ext: &str) {
    write(&root.join("osprey.toml"), &format!("[project]\nname = \"order\"\nsource_roots = [\"src\"]\ndefault_namespace = \"demo\"\nentry = \"{entry}\"\n"));
    write(&root.join("src").join(format!("{LATER}.{ext}")), later);
    write(&root.join("src").join(format!("{EARLY}.{ext}")), early);
    write(&root.join("src").join(format!("main.{ext}")), main);
}

const ORDER_OUTPUT: &str = "1,2 2\n";

fn run_project(name: &str, expected: &str, project: impl FnOnce(&Path)) {
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
        expected,
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn generic_helper_in_a_later_file_instantiates_per_call_site_ml() {
    run_project("ml", ORDER_OUTPUT, |root| {
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
    run_project("default", ORDER_OUTPUT, |root| {
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

fn write_sidebar_project(root: &Path, ui: &str, vigil: &str, main: &str) {
    write(
        &root.join("osprey.toml"),
        "[project]\nname = \"sidebar\"\nsource_roots = [\"ui\", \"app\", \"src\"]\ndefault_namespace = \"app\"\nentry = \"src/main.ospml\"\n",
    );
    write(&root.join("ui").join("base.ospml"), ui);
    write(&root.join("app").join("nav.ospml"), vigil);
    write(&root.join("src").join("main.ospml"), main);
}

const UI_BASE: &str = r#"namespace ui

type Node A =
    Text
        content : string
    Bound
        label : string
        fire : A

type Row A =
    rowKey : string
    cell : Node<A>

module Ui
    export label<A> : string -> Node<A>
    label text = Text(content = text)

    export entry<A> : (string, Node<A>) -> Row<A>
    entry (key, cell) = Row(rowKey = key, cell = cell)

    export rows<A> : (List<Row<A>>, A, int) -> Node<A>
    rows (entries, picked, extra) = Bound(label = "${length entries}:${extra}", fire = picked)

module Lists
    step (out, items, f, index) = match listGet items index
        Success item => listAppend out (f item)
        Error _ => out

    export mapOver<T, U> : (List<T>, (T) -> U) -> List<U>
    mapOver (items, f) = range (0, length items) |> fold [] (\(out, index) => step (out, items, f, index))
"#;

const VIGIL_NAV: &str = r#"namespace vigil

import ui::Lists
import ui::Ui

type Action = Navigate

type Found =
    warnings : int

module Watch
    export found () = perform Signal<Found>.read ()

module Sidebar
    areaRow (key) = Ui::entry (key, Ui::label key)

    export view : Unit -> ui::Node<Action>
    view () =
        found = Watch::found ()
        entries = Lists::mapOver (["startup", "files"], \key => areaRow key)
        Ui::rows (entries, Navigate, found.warnings)
"#;

const MAIN: &str = r#"namespace app

import vigil::Sidebar

show () =
    handle static Signal<vigil::Found>
        read => vigil::Found(warnings = 3)
    match Sidebar::view ()
        ui::Bound shown => shown
        ui::Text fallback => fallback

test "sidebar rows keep their pinned instantiation" (\() => expect (show (), "2:3"))
"#;

#[test]
fn a_staged_sidebar_closure_keeps_its_pinned_instantiation_across_namespaces() {
    run_project(
        "sidebar",
        "ok 1 - sidebar rows keep their pinned instantiation\n1..1\n# tests=1 passed=1 failed=0 skipped=0\n",
        |root| write_sidebar_project(root, UI_BASE, VIGIL_NAV, MAIN),
    );
}
