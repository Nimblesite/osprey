//! Scene primitives as the shared frame's scene rows carry them ([NATIVE-SHM-SCENE]), checked by
//! `rows.rs` and drawn by `surface.rs` with FerroUI's drawing context. Ported from Osprey's GPUI
//! host (MIT); the primitives themselves are GPUI's `scene/shapes.rs`.

/// The role of one accessible scene element ([UI-MACHINE-A11Y]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Button,
    Slider,
    MenuItem,
    Label,
}

/// One element of the session's accessibility tree, bounds in scene units.
// BEGIN GENERATED [UI-MACHINE-A11Y]
#[derive(Debug, Clone, PartialEq)]
pub struct Access {
    pub role: Role,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub selected: bool,
}
// END GENERATED [UI-MACHINE-A11Y]

#[path = "../gpui/src/scene/shapes.rs"]
mod shapes;
pub use shapes::*;
