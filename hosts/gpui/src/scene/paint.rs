//! Low-level GPUI painting for cached scene primitives ([GPUI-HOST-SCENE]).

use std::time::Instant;

use gpui_kit::*;

use super::{Decoded, Shape, Transform};
use crate::host::HostView;
use crate::render::Ctx;

/// Builds the one-shot prepaint/paint pair with decoded data moved, not reparsed.
pub(super) fn canvas(
    ctx: &mut Ctx,
    handle: Entity<HostView>,
    decoded: Decoded,
) -> impl IntoElement {
    let key = ctx.key.clone();
    let prepaint_handle = handle.clone();
    gpui_kit::canvas(
        move |bounds, _, cx| {
            let transform = Transform::new(bounds, decoded.width, decoded.height);
            prepaint_handle.update(cx, |view, _| {
                view.scene_transforms.insert(key, transform);
            });
            (decoded, transform)
        },
        move |_, (decoded, transform), window, cx| {
            let started = Instant::now();
            // Shapes outside the scene (a ring around a dog at the edge) are clipped to the letterboxed field.
            let field = Bounds {
                origin: transform.origin,
                size: size(
                    px(decoded.width * transform.scale),
                    px(decoded.height * transform.scale),
                ),
            };
            let painted = window.with_content_mask(Some(ContentMask { bounds: field }), |window| {
                paint(&decoded.shapes, transform, window, cx)
            });
            if let Err(problem) = painted {
                handle.update(cx, |view, _| view.diagnose_once(problem));
            }
            let elapsed = started.elapsed().as_secs_f64() * 1000.;
            handle.update(cx, |view, _| view.paint_ms = elapsed);
        },
    )
    .size_full()
}

/// Paints decoded primitives in their stable depth order.
fn paint(
    shapes: &[Shape],
    transform: Transform,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    for shape in shapes {
        paint_shape(shape, transform, window, cx)?;
    }
    Ok(())
}

/// Paints exactly one primitive with GPUI's low-level operations.
fn paint_shape(
    shape: &Shape,
    transform: Transform,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    match shape {
        Shape::Rect {
            x, y, w, h, colour, ..
        } => window.paint_quad(fill(centre(*x, *y, *w, *h, transform), rgba(*colour))),
        Shape::Disc {
            x,
            y,
            radius,
            colour,
            ..
        } => {
            let scaled = radius * transform.scale;
            window.paint_quad(
                fill(
                    centre(*x, *y, radius * 2., radius * 2., transform),
                    rgba(*colour),
                )
                .corner_radii(px(scaled)),
            );
        }
        Shape::Strip {
            points,
            colour,
            stroke,
            ..
        } => paint_strip(points, *colour, *stroke, transform, window)?,
        Shape::Sprite {
            atlas,
            x,
            y,
            w,
            h,
            tint,
            ..
        } => window.paint_quad(fill(
            centre(*x, *y, *w, *h, transform),
            sprite_colour(*atlas, *tint),
        )),
        Shape::Glyphs {
            text,
            x,
            y,
            size,
            colour,
            ..
        } => paint_glyphs(text, *x, *y, *size, *colour, transform, window, cx)?,
    }
    Ok(())
}

/// Produces centre-based pixel bounds for rects, discs and sprite placeholders.
fn centre(x: f32, y: f32, w: f32, h: f32, transform: Transform) -> Bounds<Pixels> {
    Bounds {
        origin: transform.point(x - w / 2., y - h / 2.),
        size: size(px(w * transform.scale), px(h * transform.scale)),
    }
}

/// Makes untextured opaque-white sprites distinguishable by their atlas index.
fn sprite_colour(atlas: usize, tint: u32) -> Rgba {
    if tint != 0xffff_ffff {
        return rgba(tint);
    }
    match atlas {
        0 => rgb(0x87ceeb),
        1 => rgb(0xffa500),
        2 => rgb(0x55aa55),
        _ => rgb(0x888888),
    }
}

/// Strokes a decoded polyline at the uniformly scaled width.
fn paint_strip(
    points: &[(f32, f32)],
    colour: u32,
    stroke: f32,
    transform: Transform,
    window: &mut Window,
) -> Result<(), String> {
    if points.len() < 2 {
        return Ok(());
    }
    let mut builder = PathBuilder::stroke(px(stroke * transform.scale));
    builder.move_to(transform.point(points[0].0, points[0].1));
    for (x, y) in &points[1..] {
        builder.line_to(transform.point(*x, *y));
    }
    let path = builder
        .build()
        .map_err(|problem| format!("scene strip could not be built: {problem}"))?;
    window.paint_path(path, rgba(colour));
    Ok(())
}

/// Shapes and paints one coloured glyph run at its scaled position.
#[allow(clippy::too_many_arguments)]
fn paint_glyphs(
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    colour: u32,
    transform: Transform,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    let scaled = px(size * transform.scale);
    let run = TextRun {
        len: text.len(),
        color: Hsla::from(rgba(colour)),
        ..Default::default()
    };
    let line = window
        .text_system()
        .shape_line(text.to_owned().into(), scaled, &[run], None);
    line.paint(
        transform.point(x, y),
        scaled,
        TextAlign::Left,
        None,
        window,
        cx,
    )
    .map_err(|problem| format!("scene glyphs could not be painted: {problem}"))
}
