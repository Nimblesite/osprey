//! End-to-end regression coverage for a closure whose concrete instantiation
//! is pinned only by a LATER use of the let-generalized binding that holds it:
//! the Vigil shape, where `Sidebar::view` passes `\key => areaRow (key)` to a
//! generic `Lists::map` from another namespace and only `Ui::rows`'s action
//! argument settles `Row<A>`'s `A`. `--check` accepted the program while
//! codegen rejected the staged copy of `view` with "a closure value with a
//! still-generic type".
//!
//! Both flavors build the same three-file project — a `ui` namespace holding
//! the generic vocabulary, a `vigil` namespace holding the sidebar, and a main
//! that discharges a static signal so the staging pass specializes `view` —
//! and hold the run to one exact output.

#![expect(
    clippy::expect_used,
    reason = "fixture setup and subprocess failures must fail these tests immediately"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create project directory");
    }
    std::fs::write(path, text).expect("write project source");
}

fn write_project(root: &Path, ui: &str, vigil: &str, main: &str) {
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

fn run_project(name: &str, project: impl FnOnce(&Path)) {
    let root = std::env::temp_dir().join(format!(
        "osprey_let_alias_closure_{name}_{}",
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
        "ok 1 - sidebar rows keep their pinned instantiation\n1..1\n# tests=1 passed=1 failed=0 skipped=0\n",
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn a_staged_sidebar_closure_keeps_its_pinned_instantiation_across_namespaces() {
    run_project("ml", |root| {
        write_project(root, UI_BASE, VIGIL_NAV, MAIN);
    });
}
