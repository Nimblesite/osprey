//! Finding what a self-test step names ([UI-MACHINE-SELF-TEST]): the press, drag and value a
//! scene's announced element takes, sent as the pointer events a hand would send. What the frame
//! shows by words, and the controls and choices a step reaches, is `screen.rs`.

use crate::events::Touch;
use crate::scene::access::Access;

use super::Attempt;

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
