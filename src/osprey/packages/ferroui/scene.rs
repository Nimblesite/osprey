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
pub type Access = Accessible<Role>;

#[path = "../../../toolchain/hosts/gpui/src/scene/shapes.rs"]
mod shapes;
pub use shapes::*;
