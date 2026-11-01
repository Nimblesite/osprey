//! One-pass scene JSON decoding and stable depth ordering ([GPUI-HOST-SCENE]).

use std::sync::Arc;

use serde_json::Value;

use super::{Decoded, Outline, Paint, Shape, Step, Tint};
use crate::ffi::Streamed;
use crate::render;

/// Decodes and stably depth-sorts every shape in one scene patch: the node's own shapes, then the
/// rects, discs and sprites the application streamed for it ([UI-SCENE-STREAM]).
pub(super) fn decode(
    node: &Value,
    revision: u64,
    streamed: &[Streamed],
) -> Result<Decoded, String> {
    let width = dimension(node.get("width"), "width")?;
    let height = dimension(node.get("height"), "height")?;
    if (width == 0.) != (height == 0.) {
        return Err("a window-pixel scene has zero width and zero height".to_owned());
    }
    let zoom = match node.get("zoom") {
        None => 1.,
        Some(value) => dimension(Some(value), "zoom").and_then(|z| match z > 0. {
            true => Ok(z),
            false => Err("scene zoom must be positive".to_owned()),
        })?,
    };
    let mut shapes = render::list(node, "shapes")
        .iter()
        .map(shape)
        .collect::<Result<Vec<_>, _>>()?;
    for item in streamed {
        shapes.push(streamed_shape(item)?);
    }
    shapes.sort_by(|left, right| left.depth().total_cmp(&right.depth()));
    let atlas = render::list(node, "atlas")
        .iter()
        .map(|path| path.as_str().map(str::to_owned).ok_or("an atlas entry is not a path"))
        .collect::<Result<Vec<_>, _>>()?;
    let has_rotation = shapes.iter().any(rotated);
    let has_sprites = shapes
        .iter()
        .any(|item| matches!(item, Shape::Sprite { .. }));
    Ok(Decoded {
        revision,
        width,
        zoom,
        shapes: Arc::new(shapes),
        atlas: Arc::new(atlas),
        has_rotation,
        has_sprites,
        access: Arc::new(super::access::decode(node)),
    })
}

/// One streamed primitive in the shape the painter takes; codes are the scene encoding's.
fn streamed_shape(item: &Streamed) -> Result<Shape, String> {
    let [a, b, c, d, e] = item.values.map(|value| value as f32);
    if !item.values.iter().all(|value| value.is_finite()) {
        return Err("a streamed shape number is not finite".to_owned());
    }
    let colour = u32::try_from(item.colour).map_err(|_| "streamed colour exceeds 32 bits")?;
    let depth = item.depth as f32;
    match item.code {
        0 => Ok(Shape::Rect {
            x: a,
            y: b,
            w: c,
            h: d,
            angle: e,
            colour,
            depth,
        }),
        1 => Ok(Shape::Disc {
            x: a,
            y: b,
            radius: c,
            colour,
            depth,
        }),
        3 => Ok(Shape::Sprite {
            atlas: usize::try_from(item.index).map_err(|_| "streamed sprite index is negative")?,
            x: a,
            y: b,
            w: c,
            h: d,
            angle: e,
            tint: colour,
            depth,
        }),
        code => Err(format!("unknown streamed shape code {code}")),
    }
}

/// Whether this primitive carries a nonzero, currently undrawn angle; sprites turn.
fn rotated(item: &Shape) -> bool {
    matches!(item, Shape::Rect { angle, .. } if *angle != 0.)
}

/// Decodes one tagged JSON primitive without interpreting its application meaning.
fn shape(value: &Value) -> Result<Shape, String> {
    let values = value.as_array().ok_or("a shape is not an array")?;
    match values.first().and_then(Value::as_u64) {
        Some(0) => rect(values),
        Some(1) => disc(values),
        Some(2) => strip(values),
        Some(3) => sprite(values),
        Some(4) => glyphs(values),
        Some(5) => path(values),
        Some(6) => lettering(values),
        code => Err(format!("unknown shape code {code:?}")),
    }
}

/// Decodes a centre-based rectangle.
fn rect(v: &[Value]) -> Result<Shape, String> {
    arity(v, 8)?;
    Ok(Shape::Rect {
        x: number(v, 1)?,
        y: number(v, 2)?,
        w: number(v, 3)?,
        h: number(v, 4)?,
        angle: number(v, 5)?,
        colour: colour(v, 6)?,
        depth: number(v, 7)?,
    })
}

/// Decodes a disc.
fn disc(v: &[Value]) -> Result<Shape, String> {
    arity(v, 6)?;
    Ok(Shape::Disc {
        x: number(v, 1)?,
        y: number(v, 2)?,
        radius: number(v, 3)?,
        colour: colour(v, 4)?,
        depth: number(v, 5)?,
    })
}

/// Decodes a line strip and its explicit point count.
fn strip(v: &[Value]) -> Result<Shape, String> {
    let count = usize::try_from(integer(v, 1)?).map_err(|_| "strip point count is too large")?;
    arity(
        v,
        count
            .checked_mul(2)
            .and_then(|n| n.checked_add(5))
            .ok_or("strip is too large")?,
    )?;
    let points = (0..count)
        .map(|index| Ok((number(v, 2 + index * 2)?, number(v, 3 + index * 2)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let tail = 2 + count * 2;
    Ok(Shape::Strip {
        points,
        colour: colour(v, tail)?,
        stroke: number(v, tail + 1)?,
        depth: number(v, tail + 2)?,
    })
}

/// Decodes a sprite; atlas pixels remain a later host capability.
fn sprite(v: &[Value]) -> Result<Shape, String> {
    arity(v, 9)?;
    let atlas = usize::try_from(integer(v, 1)?).map_err(|_| "sprite atlas index is too large")?;
    Ok(Shape::Sprite {
        atlas,
        x: number(v, 2)?,
        y: number(v, 3)?,
        w: number(v, 4)?,
        h: number(v, 5)?,
        angle: number(v, 6)?,
        tint: colour(v, 7)?,
        depth: number(v, 8)?,
    })
}

/// Decodes one single-line glyph run.
fn glyphs(v: &[Value]) -> Result<Shape, String> {
    arity(v, 7)?;
    let text = v[1]
        .as_str()
        .filter(|text| !text.contains('\n'))
        .ok_or("glyph text must be one line")?
        .to_owned();
    Ok(Shape::Glyphs {
        text,
        x: number(v, 2)?,
        y: number(v, 3)?,
        size: number(v, 4)?,
        colour: colour(v, 5)?,
        depth: number(v, 6)?,
    })
}

/// Decodes an anchored, weighted text run: text, x y size colour depth anchor weight.
fn lettering(v: &[Value]) -> Result<Shape, String> {
    arity(v, 9)?;
    let Shape::Glyphs { text, x, y, size, colour, depth } = glyphs(&v[..7])? else {
        return Err("lettering is not a glyph run".to_owned());
    };
    let anchor = u8::try_from(integer(v, 7)?).ok().filter(|anchor| *anchor <= 2);
    Ok(Shape::Lettering {
        text,
        x,
        y,
        size,
        colour,
        depth,
        anchor: anchor.ok_or("lettering anchor is not 0, 1 or 2")?,
        weight: number(v, 8)?,
    })
}

/// Decodes a path: builder steps, outline, paint, optional transform and depth.
fn path(v: &[Value]) -> Result<Shape, String> {
    arity(v, 6)?;
    let steps = array(&v[1])?
        .iter()
        .map(|item| step(array(item)?))
        .collect::<Result<Vec<_>, _>>()?;
    let transform = match array(&v[4])? {
        [] => None,
        moved => Some((number(moved, 0)?, number(moved, 1)?, number(moved, 2)?)),
    };
    Ok(Shape::Path {
        steps: Arc::new(steps),
        outline: outline(array(&v[2])?)?,
        paint: paint(array(&v[3])?)?,
        transform,
        depth: number(v, 5)?,
    })
}

/// Decodes one builder command; curves carry their end point first, as GPUI's builder takes them.
fn step(v: &[Value]) -> Result<Step, String> {
    let point = |index| Ok::<_, String>((number(v, index)?, number(v, index + 1)?));
    match v.first().and_then(Value::as_u64) {
        Some(0) => Ok(Step::Move(point(1)?)),
        Some(1) => Ok(Step::Line(point(1)?)),
        Some(2) => Ok(Step::Curve(point(1)?, point(3)?)),
        Some(3) => Ok(Step::Cubic(point(1)?, point(3)?, point(5)?)),
        Some(4) => Ok(Step::Arc {
            radii: point(1)?,
            rotation: number(v, 3)?,
            large: integer(v, 4)? != 0,
            sweep: integer(v, 5)? != 0,
            to: point(6)?,
        }),
        Some(5) => {
            let corners = (2..v.len())
                .step_by(2)
                .map(point)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Step::Polygon(corners, integer(v, 1)? != 0))
        }
        Some(6) => Ok(Step::Close),
        code => Err(format!("unknown path step {code:?}")),
    }
}

/// Decodes a fill, or a stroke with its width, join and dash pattern.
fn outline(v: &[Value]) -> Result<Outline, String> {
    match v.first().and_then(Value::as_u64) {
        Some(0) => Ok(Outline::Fill),
        Some(1) => Ok(Outline::Stroke {
            width: number(v, 1)?,
            bevel: integer(v, 2)? != 0,
            dashes: (3..v.len())
                .map(|index| number(v, index))
                .collect::<Result<_, _>>()?,
        }),
        code => Err(format!("unknown path outline {code:?}")),
    }
}

/// Decodes a solid tint or a two-stop linear gradient.
fn paint(v: &[Value]) -> Result<Paint, String> {
    match v.first().and_then(Value::as_u64) {
        Some(0) => Ok(Paint::Solid(tint(v.get(1))?)),
        Some(1) => Ok(Paint::Linear {
            angle: number(v, 1)?,
            from: (tint(v.get(2))?, number(v, 3)?),
            to: (tint(v.get(4))?, number(v, 5)?),
            oklab: integer(v, 6)? != 0,
        }),
        code => Err(format!("unknown path paint {code:?}")),
    }
}

/// Decodes a float sRGB or HSL colour, kept in its own space so no conversion rounds it.
fn tint(value: Option<&Value>) -> Result<Tint, String> {
    let v = array(value.ok_or("a path tint is missing")?)?;
    arity(v, 5)?;
    let channels = [number(v, 1)?, number(v, 2)?, number(v, 3)?, number(v, 4)?];
    match v[0].as_u64() {
        Some(0) => Ok(Tint::Rgb(channels)),
        Some(1) => Ok(Tint::Hsl(channels)),
        code => Err(format!("unknown tint space {code:?}")),
    }
}

/// Reads a nested JSON array.
fn array(value: &Value) -> Result<&[Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| "a path field is not an array".to_owned())
}

/// Checks the exact tagged-array length before indexing its fields.
fn arity(values: &[Value], expected: usize) -> Result<(), String> {
    if values.len() == expected {
        Ok(())
    } else {
        Err(format!(
            "shape has {} fields, expected {expected}",
            values.len()
        ))
    }
}

/// Reads a finite scene number.
fn number(values: &[Value], index: usize) -> Result<f32, String> {
    let value = values
        .get(index)
        .and_then(Value::as_f64)
        .ok_or("shape number is missing")?;
    if value.is_finite() {
        Ok(value as f32)
    } else {
        Err("shape number is not finite".to_owned())
    }
}

/// Reads a scene dimension: positive for a unit space, zero for window pixels.
fn dimension(value: Option<&Value>, name: &str) -> Result<f32, String> {
    let value = value
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && *n >= 0.)
        .ok_or(format!("scene {name} must be zero or positive"))?;
    Ok(value as f32)
}

/// Reads one packed colour or integer index.
fn integer(values: &[Value], index: usize) -> Result<u64, String> {
    values
        .get(index)
        .and_then(Value::as_u64)
        .ok_or_else(|| "shape integer is missing".to_owned())
}

/// Reads a packed colour within the 32-bit protocol range.
fn colour(values: &[Value], index: usize) -> Result<u32, String> {
    u32::try_from(integer(values, index)?).map_err(|_| "shape colour exceeds 32 bits".to_owned())
}
