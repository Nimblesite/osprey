//! NDK key codes to GPUI keystrokes.
//!
//! NativeActivity delivers hardware-keyboard keys as `AKEYCODE_*` values; the
//! printable character comes from the device's `KeyCharacterMap` when the caller
//! can obtain one (see `window::unicode_for`). Keys Android itself interprets
//! (Back, Home, volume) map to `None` so the event stays `Unhandled` and the
//! system keeps its behaviour.

use android_activity::input::{KeyEvent, Keycode, MetaState};
use gpui::{KeyDownEvent, KeyUpEvent, Keystroke, Modifiers};

/// The key-down event for `event`, or `None` when the key is not GPUI's to handle.
pub(crate) fn key_down(event: &KeyEvent, unicode: Option<char>) -> Option<KeyDownEvent> {
    keystroke(event, unicode).map(|keystroke| KeyDownEvent {
        keystroke,
        is_held: event.repeat_count() > 0,
        prefer_character_input: false,
    })
}

/// The key-up event for `event`, or `None` for the same reason as [`key_down`].
pub(crate) fn key_up(event: &KeyEvent, unicode: Option<char>) -> Option<KeyUpEvent> {
    keystroke(event, unicode).map(|keystroke| KeyUpEvent { keystroke })
}

/// The GPUI keystroke for one Android key event.
fn keystroke(event: &KeyEvent, unicode: Option<char>) -> Option<Keystroke> {
    let key = key_name(event.key_code(), unicode)?;
    Some(Keystroke {
        modifiers: modifiers(event.meta_state()),
        key: key.to_owned(),
        key_char: unicode.map(|character| character.to_string()),
    })
}

/// The modifier state Android reports, in GPUI's shape.
fn modifiers(meta: MetaState) -> Modifiers {
    Modifiers {
        control: meta.ctrl_on(),
        alt: meta.alt_on(),
        shift: meta.shift_on(),
        platform: meta.meta_on(),
        ..Default::default()
    }
}

/// The GPUI key name for an NDK key code, taking its printable character when
/// one exists (so shift+2 is still `2` to bindings, with `key_char` carrying `@`).
fn key_name(code: Keycode, unicode: Option<char>) -> Option<String> {
    if let Some(character) = unicode {
        return Some(character.to_lowercase().to_string());
    }
    let name = match code {
        Keycode::DpadUp => "up",
        Keycode::DpadDown => "down",
        Keycode::DpadLeft => "left",
        Keycode::DpadRight => "right",
        Keycode::DpadCenter | Keycode::Enter => "enter",
        Keycode::Del => "backspace",
        Keycode::ForwardDel => "delete",
        Keycode::Escape => "escape",
        Keycode::Tab => "tab",
        Keycode::Space => "space",
        Keycode::Minus => "-",
        Keycode::Equals => "=",
        Keycode::LeftBracket => "[",
        Keycode::RightBracket => "]",
        Keycode::Backslash => "\\",
        Keycode::Semicolon => ";",
        Keycode::Apostrophe => "'",
        Keycode::Comma => ",",
        Keycode::Period => ".",
        Keycode::Slash => "/",
        Keycode::Grave => "`",
        Keycode::At => "@",
        Keycode::Star => "*",
        Keycode::Pound => "#",
        Keycode::Plus => "+",
        Keycode::A => "a",
        Keycode::B => "b",
        Keycode::C => "c",
        Keycode::D => "d",
        Keycode::E => "e",
        Keycode::F => "f",
        Keycode::G => "g",
        Keycode::H => "h",
        Keycode::I => "i",
        Keycode::J => "j",
        Keycode::K => "k",
        Keycode::L => "l",
        Keycode::M => "m",
        Keycode::N => "n",
        Keycode::O => "o",
        Keycode::P => "p",
        Keycode::Q => "q",
        Keycode::R => "r",
        Keycode::S => "s",
        Keycode::T => "t",
        Keycode::U => "u",
        Keycode::V => "v",
        Keycode::W => "w",
        Keycode::X => "x",
        Keycode::Y => "y",
        Keycode::Z => "z",
        Keycode::Keycode0 => "0",
        Keycode::Keycode1 => "1",
        Keycode::Keycode2 => "2",
        Keycode::Keycode3 => "3",
        Keycode::Keycode4 => "4",
        Keycode::Keycode5 => "5",
        Keycode::Keycode6 => "6",
        Keycode::Keycode7 => "7",
        Keycode::Keycode8 => "8",
        Keycode::Keycode9 => "9",
        _ => return None,
    };
    Some(name.to_owned())
}

/// The Android keyboard layout GPUI reports to keymap-aware code.
pub(crate) struct AndroidKeyboardLayout;

impl gpui::PlatformKeyboardLayout for AndroidKeyboardLayout {
    fn id(&self) -> &str {
        "android-default"
    }

    fn name(&self) -> &str {
        "Android Default"
    }
}
