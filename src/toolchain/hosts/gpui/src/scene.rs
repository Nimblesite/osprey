//! Scene decoding, painting and host-neutral input ([GPUI-HOST-SCENE]).

use std::sync::Arc;

use gpui_kit::*;

use crate::host::HostView;
use crate::rows::Scene;
use crate::tree::{self, Tree};

pub mod access;
pub mod atlas;
mod input;
mod paint;
mod text;

/// One checked scene, ready to paint.
#[derive(Clone)]
pub struct Decoded {
    width: f32,
    height: f32,
    /// Window pixels per scene unit, whatever the window's size; 0 fits the scene to its region.
    zoom: f32,
    shapes: Arc<Vec<Shape>>,
    /// The sprite images the scene names, by atlas index ([UI-SCENE]).
    atlas: Arc<Vec<String>>,
    has_rotation: bool,
    has_sprites: bool,
    /// The accessibility tree a host announces over the scene ([UI-MACHINE-A11Y]).
    access: Arc<Vec<access::Access>>,
}

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
    pub(crate) fn depth(&self) -> f32 {
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

/// The scene's zoom and top-left origin, shared by painting and pointer input.
#[derive(Clone, Copy)]
pub struct Transform {
    origin: Point<Pixels>,
    scale: f32,
    /// The region's width and height in scene units, which the application is told when they change.
    viewed: (f32, f32),
}

impl Transform {
    /// Places the scene's origin at the region's top left at its fixed `zoom`, so a larger window
    /// shows more of the canvas and never resizes what is on it, or with zoom 0 as large as fits the
    /// region whole. A fixed zoom never magnifies the scene past what its region fits whole either,
    /// so a narrow window or phone sees the whole artboard instead of clipping it. A zero-sized
    /// scene is in window pixels, so its origin is the window's.
    fn new(bounds: Bounds<Pixels>, decoded: &Decoded) -> Self {
        let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        let whole = (width / decoded.width).min(height / decoded.height);
        let scale = match decoded.zoom {
            0. => whole,
            zoom => zoom.min(whole),
        };
        match decoded.width == 0. {
            true => Self {
                origin: Point::default(),
                scale: 1.,
                viewed: (width, height),
            },
            false => Self {
                origin: bounds.origin,
                scale,
                viewed: (width / scale, height / scale),
            },
        }
    }

    /// Whether scene units are window pixels, so paths need no extra transform.
    fn identity(self) -> bool {
        self.scale == 1. && self.origin == Point::default()
    }

    /// Converts one scene-space point to window pixels.
    fn point(self, x: f32, y: f32) -> Point<Pixels> {
        self.origin + point(px(x * self.scale), px(y * self.scale))
    }

    /// Converts one window-space pointer position to scene units.
    fn scene(self, position: Point<Pixels>) -> (f64, f64) {
        let relative = position - self.origin;
        (
            f64::from(relative.x) / f64::from(self.scale),
            f64::from(relative.y) / f64::from(self.scale),
        )
    }
}

/// Maps a scene component to an interactive, full-area GPUI canvas ([UI-SCENE]): the scene rows the
/// record's `sceneRow` names, delivering input to the control its `event` member issues.
pub fn node<'b>(
    view: &mut HostView,
    window: &mut Window,
    cx: &mut Context<'b, HostView>,
    key: &str,
    node: &Tree,
) -> AnyElement {
    let Some(ordinal) = tree::ordinal(node) else {
        return crate::render::unbound(view, key);
    };
    let Some(row) = node.get("sceneRow").and_then(Tree::as_u64) else {
        return crate::render::broken(view, "scene node without its scene row".to_owned());
    };
    let row = usize::try_from(row).unwrap_or(usize::MAX);
    match view.shown.scenes.get(&row).cloned() {
        Some(scene) => draw(view, window, cx, row, ordinal, scene),
        None => crate::render::broken(view, format!("scene at row {row} streamed no scene")),
    }
}

/// Draws the canvas at view row `row` with its scene, delivering input to control `ordinal`.
pub fn draw<'b>(
    view: &mut HostView,
    window: &mut Window,
    cx: &mut Context<'b, HostView>,
    row: usize,
    ordinal: i64,
    scene: Scene,
) -> AnyElement {
    let decoded = decoded(scene);
    diagnostics(view, &decoded);
    input::interactive(view, window, cx, row, ordinal, decoded)
}

/// A checked scene stably depth-sorted for the painter.
fn decoded(scene: Scene) -> Decoded {
    let mut shapes = scene.shapes;
    shapes.sort_by(|left, right| left.depth().total_cmp(&right.depth()));
    Decoded {
        width: scene.width,
        height: scene.height,
        zoom: scene.zoom,
        has_rotation: shapes
            .iter()
            .any(|item| matches!(item, Shape::Rect { angle, .. } if *angle != 0.)),
        has_sprites: shapes
            .iter()
            .any(|item| matches!(item, Shape::Sprite { .. })),
        shapes: Arc::new(shapes),
        atlas: Arc::new(scene.atlas),
        access: Arc::new(scene.access),
    }
}

/// Records each painter limitation once rather than on every display refresh.
fn diagnostics(view: &mut HostView, decoded: &Decoded) {
    if decoded.has_rotation {
        view.diagnose_once("scene rotation is not drawn yet".to_owned());
    }
    if decoded.has_sprites && decoded.atlas.is_empty() {
        view.diagnose_once("sprites draw as quads; the scene names no atlas".to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::{decoded, Shape, Transform};
    use crate::rows::Scene;
    use gpui_kit::{bounds, point, px, size};

    fn scene(width: f32, height: f32, zoom: f32) -> super::Decoded {
        decoded(Scene {
            width,
            height,
            zoom,
            ..Scene::default()
        })
    }

    /// Zoom 0 grows and shrinks the scene to fit its region whole; a fixed zoom keeps its size
    /// while the scene fits and never magnifies it past what the region fits whole, so a narrow
    /// window or phone sees the whole artboard instead of clipping it; a window-pixel scene stays
    /// in window pixels. Pointer input maps back through each, and the scene never leaves its region.
    #[test]
    fn the_scene_fits_its_region_whole_whatever_the_zoom() {
        let region = |w: f32, h: f32| bounds(point(px(10.), px(20.)), size(px(w), px(h)));
        let (fit, fixed, pixels) = (
            scene(200., 100., 0.),
            scene(200., 100., 0.75),
            scene(0., 0., 1.),
        );
        let cases = [
            (&fit, 400., 400., 2.),
            (&fit, 400., 100., 1.),
            (&fit, 100., 900., 0.5),
            (&fixed, 400., 400., 0.75),
            (&fixed, 160., 400., 0.75),
            (&fixed, 100., 900., 0.5),
            (&fixed, 100., 50., 0.5),
            (&fixed, 50., 25., 0.25),
            (&pixels, 400., 400., 1.),
        ];
        for (decoded, w, h, scale) in cases {
            let transform = Transform::new(region(w, h), decoded);
            assert_eq!(transform.scale, scale);
            let corner = transform.point(200., 100.);
            let origin = if decoded.width == 0. {
                (0., 0.)
            } else {
                (10., 20.)
            };
            assert_eq!(
                corner,
                point(px(origin.0 + 200. * scale), px(origin.1 + 100. * scale))
            );
            assert_eq!(transform.scene(corner), (200., 100.));
            assert_eq!(transform.viewed, (w / transform.scale, h / transform.scale));
            let (right, bottom) = (
                origin.0 + decoded.width * scale,
                origin.1 + decoded.height * scale,
            );
            assert!(right <= 10. + w + 1e-4 && bottom <= 20. + h + 1e-4);
        }
    }

    /// Shapes sort back to front by depth and equal depths keep their drawing order.
    #[test]
    fn shapes_sort_by_depth_and_keep_their_order_within_one() {
        let disc = |x: f32, depth: f32| Shape::Disc {
            x,
            y: 0.,
            radius: 1.,
            colour: 255,
            depth,
        };
        let ordered = decoded(Scene {
            width: 10.,
            height: 10.,
            zoom: 1.,
            shapes: vec![disc(1., 4.), disc(2., 0.), disc(3., 4.)],
            ..Scene::default()
        });
        let xs: Vec<f32> = ordered
            .shapes
            .iter()
            .map(|s| match s {
                Shape::Disc { x, .. } => *x,
                _ => -1.,
            })
            .collect();
        assert_eq!(xs, vec![2., 1., 3.]);
    }
}
