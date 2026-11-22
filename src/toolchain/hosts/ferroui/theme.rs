//! The application's look: the colour roles and spacing scale of its theme, resolved in the
//! current appearance (docs/osprey-prism/README.md). A role the theme names is applied
//! to the control; a role it leaves out is not set, so FerroUI's theme draws that part.

use std::rc::Rc;

use ferroui_base::media::Color;

use crate::tree::Theme;

/// A `#RRGGBB` colour.
pub fn hex(text: &str) -> Option<Color> {
    let digits = text.strip_prefix('#').filter(|digits| digits.len() == 6)?;
    let value = u32::from_str_radix(digits, 16).ok()?;
    Some(Color::from_argb(
        0xFF,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ))
}

/// The theme in force at one place of a tree: the nearest `themed` node's, else the
/// application's, in the light or dark appearance.
#[derive(Clone, Default)]
pub struct Look {
    pub theme: Option<Rc<Theme>>,
    pub dark: bool,
}

impl Look {
    /// The colour of the role `name`, if the theme names it.
    pub fn colour(&self, name: &str) -> Option<Color> {
        let token = self
            .theme
            .as_ref()?
            .tokens
            .iter()
            .find(|token| token.name == name)?;
        hex(if self.dark { &token.dark } else { &token.light })
    }

    /// The spacing step `index` in points, if the theme's scale has it.
    pub fn step(&self, index: usize) -> Option<f64> {
        self.theme
            .as_ref()?
            .spacing
            .get(index)
            .map(|step| (*step).max(0) as f64)
    }

    /// The gap between the children of a container: the scale's second step, as the GPUI host
    /// uses it, else 8 points.
    pub fn gap(&self) -> f64 {
        self.step(1).unwrap_or(8.0)
    }
}

/// The font size of a text role, relative to FerroUI's body text of 14 points.
pub fn role_size(role: &str) -> Option<f64> {
    match role {
        "display" => Some(34.0),
        "large" => Some(28.0),
        "title" => Some(20.0),
        "heading" | "headline" => Some(17.0),
        "lead" => Some(16.0),
        "callout" => Some(15.0),
        "caption" | "eyebrow" => Some(12.0),
        _ => None,
    }
}

/// Whether a text role is drawn semibold.
pub fn role_bold(role: &str) -> bool {
    matches!(
        role,
        "display" | "large" | "title" | "heading" | "headline" | "eyebrow"
    )
}

/// The colour role a text role falls back to when its tone names none: captions are muted.
pub fn role_tone(role: &str) -> &'static str {
    match role {
        "caption" | "eyebrow" => "secondary",
        _ => "text",
    }
}
