//! Touch and key input for the activity's window: raw GPUI touch events for each pointer a
//! motion event describes, and key presses resolved through the device's character map.

use android_activity::input::{Axis, KeyAction, KeyEvent, MotionAction, MotionEvent};
use android_activity::{AndroidApp, InputStatus};
use gpui::{point, px, PlatformInput, TouchEvent, TouchId, TouchPhase};

use super::{locked, AndroidWindow};
use crate::keyboard;

impl AndroidWindow {
    /// Turns one motion event into raw GPUI touch events. GPUI itself recognizes
    /// taps, pans, pinches and long presses from these.
    pub(super) fn motion(&self, event: &MotionEvent) -> InputStatus {
        let Some(phase) = touch_phase(event.action()) else {
            return InputStatus::Handled;
        };
        for pointer in touched_pointers(event) {
            let (position, force) = {
                let mut state = locked(&self.state);
                let position = point(
                    px(pointer.x / state.scale_factor),
                    px(pointer.y / state.scale_factor),
                );
                state.mouse = position;
                (
                    position,
                    (pointer.pressure > 0.).then_some(pointer.pressure),
                )
            };
            let touch = TouchEvent {
                id: TouchId(pointer.id as u64),
                phase,
                position,
                predicted_position: None,
                force,
            };
            self.callbacks
                .input
                .with(|input| input(PlatformInput::Touch(touch)));
        }
        InputStatus::Handled
    }

    /// Turns one key event into a GPUI key press, resolving the printable
    /// character through the device's key character map.
    pub(super) fn key(&self, app: &AndroidApp, event: &KeyEvent) -> InputStatus {
        let unicode = unicode_for(app, event);
        let input = match event.action() {
            KeyAction::Down => keyboard::key_down(event, unicode).map(PlatformInput::KeyDown),
            KeyAction::Up => keyboard::key_up(event, unicode).map(PlatformInput::KeyUp),
            _ => None,
        };
        match input {
            Some(pressed) => {
                self.callbacks.input.with(|input| input(pressed));
                InputStatus::Handled
            }
            None => InputStatus::Unhandled,
        }
    }
}

/// The GPUI touch phase an Android motion action describes; `None` for hover and
/// scroll-wheel actions GPUI has no raw equivalent for.
fn touch_phase(action: MotionAction) -> Option<TouchPhase> {
    match action {
        MotionAction::Down | MotionAction::PointerDown => Some(TouchPhase::Started),
        MotionAction::Move => Some(TouchPhase::Moved),
        MotionAction::Up | MotionAction::PointerUp => Some(TouchPhase::Ended),
        MotionAction::Cancel => Some(TouchPhase::Cancelled),
        _ => None,
    }
}

/// One touch pointer's delivered state.
struct Touch {
    id: i32,
    x: f32,
    y: f32,
    pressure: f32,
}

/// The pointers one motion event describes: only the pointer that changed for
/// press and release actions, every pointer for moves and cancels.
fn touched_pointers(event: &MotionEvent) -> Vec<Touch> {
    fn read(pointer: android_activity::input::Pointer<'_>) -> Touch {
        Touch {
            id: pointer.pointer_id(),
            x: pointer.x(),
            y: pointer.y(),
            pressure: pointer.axis_value(Axis::Pressure),
        }
    }
    match event.action() {
        MotionAction::Down
        | MotionAction::Up
        | MotionAction::PointerDown
        | MotionAction::PointerUp => vec![read(event.pointer_at_index(event.pointer_index()))],
        _ => {
            let mut pointers = Vec::new();
            for pointer in event.pointers() {
                pointers.push(read(pointer));
            }
            pointers
        }
    }
}

/// The printable character for a key event, from the device's character map.
fn unicode_for(app: &AndroidApp, event: &KeyEvent) -> Option<char> {
    let map = app.device_key_character_map(event.device_id()).ok()?;
    match map.get(event.key_code(), event.meta_state()) {
        Ok(android_activity::input::KeyMapChar::Unicode(character)) => Some(character),
        _ => None,
    }
}
