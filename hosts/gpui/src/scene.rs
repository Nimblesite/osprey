//! Scene decoding, painting and host-neutral input ([GPUI-HOST-SCENE]).

use std::sync::Arc;

use gpui_kit::*;
use serde_json::Value;

use crate::host::HostView;
use crate::render::{self, Ctx};

mod decode;
mod input;
mod paint;

/// One decoded scene, replaced only when its region revision changes.
#[derive(Clone)]
pub struct Decoded {
    pub revision: u64,
    width: f32,
    height: f32,
    shapes: Arc<Vec<Shape>>,
    has_rotation: bool,
    has_sprites: bool,
}

/// A primitive in scene units; the painter applies the shared letterbox transform.
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
}

impl Shape {
    /// The back-to-front ordering key carried by every primitive.
    fn depth(&self) -> f32 {
        match *self {
            Self::Rect { depth, .. }
            | Self::Disc { depth, .. }
            | Self::Strip { depth, .. }
            | Self::Sprite { depth, .. }
            | Self::Glyphs { depth, .. } => depth,
        }
    }
}

/// The uniform scale and centred origin shared by painting and pointer input.
#[derive(Clone, Copy)]
pub struct Transform {
    origin: Point<Pixels>,
    scale: f32,
}

impl Transform {
    /// Fits the complete scene inside `bounds`, leaving any excess as letterbox.
    fn new(bounds: Bounds<Pixels>, width: f32, height: f32) -> Self {
        let scale =
            (f32::from(bounds.size.width) / width).min(f32::from(bounds.size.height) / height);
        let used = size(px(width * scale), px(height * scale));
        let origin = bounds.origin
            + point(
                (bounds.size.width - used.width) / 2.,
                (bounds.size.height - used.height) / 2.,
            );
        Self { origin, scale }
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
    let decoded = decode::decode(node, ctx.revision)?;
    ctx.view.scenes.insert(ctx.key.clone(), decoded.clone());
    Ok(decoded)
}

/// Records each painter limitation once rather than on every display refresh.
fn diagnostics(view: &mut HostView, decoded: &Decoded) {
    if decoded.has_rotation {
        view.diagnose_once("scene rotation is not drawn yet".to_owned());
    }
    if decoded.has_sprites {
        view.diagnose_once("sprites draw as quads; the atlas is not loaded".to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, Shape};
    use serde_json::Value;

    /// Every primitive decodes and equal depths retain their input order.
    #[test]
    fn decodes_every_shape_and_depth_order() {
        let decoded = decode::decode(&fixture(), 9).unwrap();
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
