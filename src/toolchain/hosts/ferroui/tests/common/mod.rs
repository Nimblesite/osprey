//! Test-support helpers the FerroUI host's application tests share: the
//! whole-frame shape count, the depth-first scene-ordinal lookup, a
//! repo-root-relative path and the pointer event that reports its
//! application's own refusal. Each binary links exactly one application, so
//! the helpers stay app-agnostic and the app name travels with the call.
//! Each binary imports only its subset; the rest is inert in that binary.
#![allow(dead_code)]

use std::sync::atomic::Ordering;

use ferroui_osprey_native::events::Touch;
use ferroui_osprey_native::ffi::{self, Library};
use ferroui_osprey_native::rows::Shown;
use ferroui_osprey_native::tree::Tree;

/// Every shape the linked application drew in its last frame.
pub fn shapes(shown: &Shown) -> usize {
    shown.scenes.values().map(|scene| scene.shapes.len()).sum()
}

/// The control number of the first scene at or below `node`, depth first.
pub fn scene(node: &Tree) -> Option<i64> {
    match node {
        Tree::Record(members) => match members.get("kind") {
            Some(Tree::Text(kind)) if kind == "scene" => match members.get("event") {
                Some(Tree::Whole(ordinal)) => Some(*ordinal),
                _ => None,
            },
            _ => members.values().find_map(scene),
        },
        Tree::Items(items) => items.iter().find_map(scene),
        _ => None,
    }
}

/// A repository-root-relative path as an absolute string.
pub fn repo(path: &str) -> String {
    format!("{}/../../../{path}", env!("CARGO_MANIFEST_DIR"))
}

/// The centre of the access element `label` names, in scene units.
pub fn center(shown: &Shown, label: &str) -> (f64, f64) {
    let area = shown
        .scenes
        .values()
        .flat_map(|scene| &scene.access)
        .find(|area| area.label == label)
        .unwrap_or_else(|| panic!("missing {label}"));
    (area.x + area.w / 2.0, area.y + area.h / 2.0)
}

/// One pointer event; returns its frame and the milliseconds the application took.
pub fn point(app: &str, canvas: i64, x: f64, y: f64, down: bool) -> (Shown, f64) {
    match Library::event(0, &Touch::pointer(canvas, x, y, down, false)) {
        Ok(shown) => (shown, ffi::APP_MICROS.load(Ordering::Relaxed) as f64 / 1000.0),
        Err(problem) => panic!("{app} refused a pointer event at {x},{y}: {problem}"),
    }
}
