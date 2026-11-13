//! Finding what a self-test step names ([UI-MACHINE-SELF-TEST]): a control by the words a screen
//! reader announces for it, a scene's announced element, the first scene, and the typed events a
//! found thing sends.

use crate::events::Touch;
use crate::rows::{Control, Kind, Scene, Shown};
use crate::scene::access::Access;
use crate::scene::Shape;

use super::Attempt;

/// Every word a screen reader announces: a control's words or hint, a scene element's label, and
/// the lettering a scene draws.
pub(super) fn shows(shown: &Shown, words: &str) -> bool {
    shown
        .controls
        .iter()
        .any(|control| control.words == words || control.hint == words)
        || shown.scenes.values().any(|scene| {
            scene.access.iter().any(|item| item.label == words)
                || scene.shapes.iter().any(|shape| match shape {
                    Shape::Glyphs { text, .. } | Shape::Lettering { text, .. } => text == words,
                    _ => false,
                })
        })
}

/// The first scene, by its canvas row, and the control number its canvas delivers.
pub(super) fn scene(shown: &Shown) -> Option<(i64, &Scene)> {
    let row = *shown.scenes.keys().min()?;
    let number = shown.controls.get(row)?.action?;
    Some((number, shown.scenes.get(&row)?))
}

/// The scene element a screen reader announces as `label`, reached over its canvas's number.
pub(super) fn element<'a>(shown: &'a Shown, label: &str) -> Option<(i64, &'a Access)> {
    let (number, scene) = scene(shown)?;
    let item = scene.access.iter().find(|item| item.label == label)?;
    Some((number, item))
}

/// The bound control a screen reader announces as `words`: the same lookup the recorded smoke
/// replay uses, so a step and a record can never disagree on what a label names.
pub(super) use crate::smoke::control;

/// The choice a bound list or picker offers under the words on its row: the key it delivers, sent
/// to the control that owns the choice.
pub(super) fn chooser(shown: &Shown, words: &str) -> Option<Touch> {
    shown.controls.iter().find_map(|owner| match owner.kind {
        Kind::Picker => picked(shown, owner, words),
        _ => listed(shown, owner, words),
    })
}

/// A picker's option under `words`: its key goes to the picker's own number.
fn picked(shown: &Shown, owner: &Control, words: &str) -> Option<Touch> {
    let number = owner.action?;
    owner.children.iter().find_map(|row| {
        let option = shown.controls.get(*row)?;
        (option.kind == Kind::Choice && option.words == words)
            .then(|| Touch::control(number, &option.hint))
    })
}

/// A row of a list under `words`: the row's key sits on its container, and the press inside it
/// delivers the key.
fn listed(shown: &Shown, owner: &Control, words: &str) -> Option<Touch> {
    let key = &owner.hint;
    if key.is_empty() || !holds(shown, &owner.children, words) {
        return None;
    }
    let (number, _) = press(shown, &owner.children)?;
    Some(Touch::control(number, key))
}

/// Whether any control inside `rows` announces `words`.
fn holds(shown: &Shown, rows: &[usize], words: &str) -> bool {
    rows.iter().any(|row| {
        shown
            .controls
            .get(*row)
            .is_some_and(|control| control.words == words || holds(shown, &control.children, words))
    })
}

/// The first press inside `rows` and the number it delivers.
fn press<'a>(shown: &'a Shown, rows: &[usize]) -> Option<(i64, &'a Control)> {
    rows.iter().find_map(|row| {
        let control = shown.controls.get(*row)?;
        match control.kind {
            Kind::Press => control.action.map(|number| (number, control)),
            _ => press(shown, &control.children),
        }
    })
}

/// The press and release a click on an accessible element lands as.
pub(super) fn clicked(access: &Access, number: i64) -> Attempt {
    let (x, y) = middle(access);
    Attempt::Sending(vec![
        Touch::pointer(number, x, y, true, false),
        Touch::pointer(number, x, y, false, false),
    ])
}

/// The press and release that set a slider to `value`, clamped to its range.
pub(super) fn valued(access: &Access, number: i64, value: f64) -> Result<Attempt, String> {
    if access.max <= access.min {
        return Err(format!("\"{}\" has no range to set", access.label));
    }
    let fraction = ((value - access.min) / (access.max - access.min)).clamp(0., 1.);
    let (x, y) = (access.x + access.w * fraction, middle(access).1);
    Ok(Attempt::Sending(vec![
        Touch::pointer(number, x, y, true, false),
        Touch::pointer(number, x, y, false, false),
    ]))
}

/// The press, move and release that drag an element by `dx`, `dy` scene units.
pub(super) fn dragged(access: &Access, number: i64, dx: f64, dy: f64) -> Attempt {
    let (x, y) = middle(access);
    Attempt::Sending(vec![
        Touch::pointer(number, x, y, true, false),
        Touch::pointer(number, x + dx, y + dy, true, false),
        Touch::pointer(number, x + dx, y + dy, false, false),
    ])
}

/// The middle of an element, where a hand would press it.
fn middle(access: &Access) -> (f64, f64) {
    (access.x + access.w / 2., access.y + access.h / 2.)
}
