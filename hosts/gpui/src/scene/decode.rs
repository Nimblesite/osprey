//! One-pass scene JSON decoding and stable depth ordering ([GPUI-HOST-SCENE]).

use std::sync::Arc;

use serde_json::Value;

use super::{Decoded, Shape};
use crate::render;

/// Decodes and stably depth-sorts every shape in one scene patch.
pub(super) fn decode(node: &Value, revision: u64) -> Result<Decoded, String> {
    let width = positive(node.get("width"), "width")?;
    let height = positive(node.get("height"), "height")?;
    let mut shapes = render::list(node, "shapes")
        .iter()
        .map(shape)
        .collect::<Result<Vec<_>, _>>()?;
    shapes.sort_by(|left, right| left.depth().total_cmp(&right.depth()));
    let has_rotation = shapes.iter().any(rotated);
    let has_sprites = shapes
        .iter()
        .any(|item| matches!(item, Shape::Sprite { .. }));
    Ok(Decoded {
        revision,
        width,
        height,
        shapes: Arc::new(shapes),
        has_rotation,
        has_sprites,
    })
}

/// Whether this primitive carries a nonzero, currently undrawn angle.
fn rotated(item: &Shape) -> bool {
    matches!(item, Shape::Rect { angle, .. } | Shape::Sprite { angle, .. } if *angle != 0.)
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

/// Reads a positive scene dimension.
fn positive(value: Option<&Value>, name: &str) -> Result<f32, String> {
    let value = value
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && *n > 0.)
        .ok_or(format!("scene {name} must be positive"))?;
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
