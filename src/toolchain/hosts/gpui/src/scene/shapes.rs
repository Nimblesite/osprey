//! The scene primitives a frame's scene rows carry ([NATIVE-SHM-SCENE]), as `rows.rs` checks them:
//! toolkit-free, so GPUI paints them and FerroUI draws them from the same model.

use std::sync::Arc;

/// A primitive in scene units; the painter applies the shared transform.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        angle: f32,
        colour: u32,
        depth: f32,
    },
    Disc {
        x: f32,
        y: f32,
        radius: f32,
        colour: u32,
        depth: f32,
    },
    Strip {
        points: Vec<(f32, f32)>,
        colour: u32,
        stroke: f32,
        depth: f32,
    },
    Sprite {
        atlas: usize,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        angle: f32,
        tint: u32,
        depth: f32,
    },
    Glyphs {
        text: String,
        x: f32,
        y: f32,
        size: f32,
        colour: u32,
        depth: f32,
    },
    Path {
        steps: Arc<Vec<Step>>,
        outline: Outline,
        paint: Paint,
        transform: Option<(f32, f32, f32)>,
        depth: f32,
    },
    /// A text run in the system font at a weight, anchored at its left (0), centre (1) or right (2).
    Lettering {
        text: String,
        x: f32,
        y: f32,
        size: f32,
        colour: u32,
        depth: f32,
        anchor: u8,
        weight: f32,
    },
}

/// One path-builder command in scene units; curves name their end point first.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Move((f32, f32)),
    Line((f32, f32)),
    Curve((f32, f32), (f32, f32)),
    Cubic((f32, f32), (f32, f32), (f32, f32)),
    Arc {
        radii: (f32, f32),
        rotation: f32,
        large: bool,
        sweep: bool,
        to: (f32, f32),
    },
    Polygon(Vec<(f32, f32)>, bool),
    Close,
}

/// Fill, or stroke with a width, bevel or miter joins and an optional dash pattern.
#[derive(Debug, Clone, PartialEq)]
pub enum Outline {
    Fill,
    Stroke {
        width: f32,
        bevel: bool,
        dashes: Vec<f32>,
    },
}

/// A float colour in the space the application gave it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tint {
    Rgb([f32; 4]),
    Hsl([f32; 4]),
}

/// A solid tint or a two-stop linear gradient, angle in degrees.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Solid(Tint),
    Linear {
        angle: f32,
        from: (Tint, f32),
        to: (Tint, f32),
        oklab: bool,
    },
}

impl Shape {
    /// The back-to-front ordering key carried by every primitive.
    pub fn depth(&self) -> f32 {
        match *self {
            Self::Rect { depth, .. }
            | Self::Disc { depth, .. }
            | Self::Strip { depth, .. }
            | Self::Sprite { depth, .. }
            | Self::Glyphs { depth, .. }
            | Self::Lettering { depth, .. }
            | Self::Path { depth, .. } => depth,
        }
    }
}
