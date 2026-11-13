//! What a smoke step expects of the host after it runs ([GPUI-HOST-VERIFICATION]): the frame's
//! requests, its root, its scene's primitives, the text it announces and its diagnostics count.

use super::{Expect, TextExpect};
use crate::rows::Shown;
use crate::screen::Screen;

/// Checks the ordered capability request list.
pub(super) fn check_asks(shown: &Shown, expect: &Expect) -> Result<(), String> {
    let Some(expected) = &expect.requests else {
        return Ok(());
    };
    let actual: Vec<&str> = shown.requests().map(|ask| ask.kind()).collect();
    let wanted: Vec<&str> = expected.iter().map(String::as_str).collect();
    match actual == wanted {
        true => Ok(()),
        false => Err(format!("requests were {actual:?}, expected {wanted:?}")),
    }
}

/// Checks the frame's root, its first scene's primitives, its text and its diagnostics.
pub(super) fn check_frame(shown: &Shown, expect: &Expect) -> Result<(), String> {
    if let Some(expected) = expect.root {
        match (expected, shown.roots.is_empty()) {
            (true, true) => return Err("the frame draws no root".to_owned()),
            (false, false) => {
                return Err(format!(
                    "the frame draws {} roots, expected none",
                    shown.roots.len()
                ))
            }
            _ => {}
        }
    }
    if let Some(expected) = expect.primitives {
        check_primitives(shown, expected)?;
    }
    check_text(shown, expect.text.as_ref())?;
    check_diagnostics(shown, expect.diagnostics)
}

/// Count what the frame's first scene draws, so a lost primitive cannot hide.
fn check_primitives(shown: &Shown, expected: usize) -> Result<(), String> {
    let scene = shown
        .scenes
        .keys()
        .min()
        .and_then(|row| shown.scenes.get(row))
        .ok_or("the frame draws no scene to count")?;
    if scene.shapes.len() != expected {
        return Err(format!(
            "the first scene has {} primitives, expected {expected}",
            scene.shapes.len()
        ));
    }
    Ok(())
}

/// Checks one substring against every string the frame announces.
fn check_text(shown: &Shown, expect: Option<&TextExpect>) -> Result<(), String> {
    let Some(expect) = expect else {
        return Ok(());
    };
    match Screen::new(shown).contains(&expect.contains) {
        true => Ok(()),
        false => Err(format!(
            "the frame has no string containing {:?}",
            expect.contains
        )),
    }
}

/// Checks the exact diagnostic count when a step supplies one.
fn check_diagnostics(shown: &Shown, expected: Option<usize>) -> Result<(), String> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual: Vec<&str> = shown.diagnostics().collect();
    match actual.len() == expected {
        true => Ok(()),
        false => Err(format!(
            "diagnostics were {actual:?}, expected {expected} entries"
        )),
    }
}
