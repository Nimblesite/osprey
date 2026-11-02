//! Scene decoding, painting and host-neutral input ([GPUI-HOST-SCENE]).

use std::sync::Arc;

use gpui_kit::*;
use serde_json::Value;

use crate::host::HostView;
use crate::render::{self, Ctx};

pub mod access;
pub mod atlas;
mod decode;
mod input;
mod paint;

/// One decoded scene, replaced only when its region revision changes.
#[derive(Clone)]
pub struct Decoded {
    pub revision: u64,
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
    fn depth(&self) -> f32 {
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
    /// region whole. A zero-sized scene is in window pixels, so its origin is the window's.
    fn new(bounds: Bounds<Pixels>, decoded: &Decoded) -> Self {
        let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        let scale = match decoded.zoom == 0. {
            true => (width / decoded.width).min(height / decoded.height),
            false => decoded.zoom,
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

/// Maps a scene node to an interactive, full-area GPUI canvas.
pub fn node(ctx: &mut Ctx, node: &Value) -> AnyElement {
    let Some(ordinal) = render::ordinal(node) else {
        return render::unbound(ctx);
    };
    let decoded = match cached(ctx, node) {
        Ok(decoded) => decoded,
        Err(problem) => return render::broken(ctx, format!("invalid scene: {problem}")),
    };
    diagnostics(ctx.view, &decoded);
    input::interactive(ctx, ordinal, decoded)
}

/// Returns the revision's decoded shapes, decoding a new patch exactly once.
fn cached(ctx: &mut Ctx, node: &Value) -> Result<Decoded, String> {
    if let Some(found) = ctx
        .view
        .scenes
        .get(&ctx.key)
        .filter(|item| item.revision == ctx.revision)
    {
        return Ok(found.clone());
    }
    // [UI-SCENE-STREAM] A streamed scene's rects, discs and sprites come from the region, not the node.
    let streamed = node
        .get("streamSlot")
        .and_then(Value::as_u64)
        .and_then(|slot| {
            let region = ctx.view.regions.get(&ctx.key)?;
            region.streams.get(usize::try_from(slot).ok()?).cloned()
        })
        .unwrap_or_default();
    let decoded = decode::decode(node, ctx.revision, &streamed)?;
    ctx.view.scenes.insert(ctx.key.clone(), decoded.clone());
    Ok(decoded)
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
    use super::{decode, Shape, Transform};
    use crate::ffi::Streamed;
    use gpui_kit::{bounds, point, px, size};
    use serde_json::Value;

    /// Zoom 0 grows and shrinks the scene to fit its region whole; a fixed zoom ignores the region
    /// and a window-pixel scene stays in window pixels, and pointer input maps back through each.
    #[test]
    fn zoom_zero_fits_the_region_and_a_fixed_zoom_does_not() {
        let region = |w: f32, h: f32| bounds(point(px(10.), px(20.)), size(px(w), px(h)));
        let scene =
            |json: &str| decode::decode(&serde_json::from_str(json).unwrap(), 1, &[]).unwrap();
        let fit = scene(r#"{"width":200,"height":100,"zoom":0,"shapes":[]}"#);
        let fixed = scene(r#"{"width":200,"height":100,"zoom":0.75,"shapes":[]}"#);
        let pixels = scene(r#"{"width":0,"height":0,"shapes":[]}"#);
        let cases = [
            (&fit, 400., 400., 2.),
            (&fit, 400., 100., 1.),
            (&fit, 100., 900., 0.5),
            (&fixed, 400., 400., 0.75),
            (&fixed, 100., 50., 0.75),
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
        }
        assert!(decode::decode(
            &serde_json::from_str(r#"{"width":2,"height":2,"zoom":-1,"shapes":[]}"#).unwrap(),
            1,
            &[]
        )
        .is_err());
    }

    /// Streamed shapes follow the node's own at equal depth and sort by depth with them ([UI-SCENE-STREAM]).
    #[test]
    fn streamed_shapes_join_the_depth_order_after_the_node_shapes() {
        let streamed = [
            Streamed {
                code: 1,
                index: 0,
                values: [1., 2., 3., 0., 0.],
                colour: 255,
                depth: 4,
            },
            Streamed {
                code: 0,
                index: 0,
                values: [4., 5., 6., 7., 0.],
                colour: 511,
                depth: 0,
            },
        ];
        let node: Value = serde_json::from_str(
            r#"{"width":10,"height":10,"shapes":[[1,9,9,1,65535,4]],"streamed":2}"#,
        )
        .unwrap();
        let decoded = decode::decode(&node, 1, &streamed).unwrap();
        let depths: Vec<_> = decoded.shapes.iter().map(Shape::depth).collect();
        assert_eq!(depths, vec![0., 4., 4.]);
        assert!(matches!(decoded.shapes[1], Shape::Disc { x, colour: 65535, .. } if x == 9.));
        assert!(matches!(decoded.shapes[2], Shape::Disc { x, colour: 255, .. } if x == 1.));
        assert!(matches!(decoded.shapes[0], Shape::Rect { w, .. } if w == 6.));
    }

    /// A streamed code outside rect, disc and sprite is refused, not drawn as a guess.
    #[test]
    fn unknown_streamed_codes_are_refused() {
        let streamed = [Streamed {
            code: 2,
            index: 0,
            values: [0.; 5],
            colour: 0,
            depth: 0,
        }];
        let node: Value =
            serde_json::from_str(r#"{"width":10,"height":10,"shapes":[],"streamed":1}"#).unwrap();
        assert!(decode::decode(&node, 1, &streamed).is_err());
    }

    /// Every primitive decodes and equal depths retain their input order.
    #[test]
    fn decodes_every_shape_and_depth_order() {
        let decoded = decode::decode(&fixture(), 9, &[]).unwrap();
        let expected = vec![
            Shape::Disc {
                x: 5.,
                y: 6.,
                radius: 7.,
                colour: 0x00ff00ff,
                depth: 1.,
            },
            Shape::Rect {
                x: 10.,
                y: 20.,
                w: 30.,
                h: 40.,
                angle: 0.5,
                colour: 0xff0000ff,
                depth: 2.,
            },
            Shape::Sprite {
                atlas: 1,
                x: 8.,
                y: 9.,
                w: 10.,
                h: 11.,
                angle: -0.5,
                tint: 0xffffffff,
                depth: 2.,
            },
            Shape::Strip {
                points: vec![(0., 1.), (2., 3.)],
                colour: 0x000000ff,
                stroke: 2.5,
                depth: 4.,
            },
            Shape::Glyphs {
                text: "hello".to_owned(),
                x: 1.,
                y: 2.,
                size: 12.,
                colour: 0x11223344,
                depth: 8.,
            },
        ];
        assert_eq!(decoded.shapes.as_slice(), expected.as_slice());
    }

    /// Uses parsed JSON to stay below the test macro's recursion limit.
    fn fixture() -> Value {
        serde_json::from_str(r#"{"width":100,"height":50,"shapes":[[4,"hello",1,2,12,287454020,8],[0,10,20,30,40,0.5,4278190335,2],[1,5,6,7,16711935,1],[2,2,0,1,2,3,255,2.5,4],[3,1,8,9,10,11,-0.5,4294967295,2]]}"#).unwrap()
    }
}
