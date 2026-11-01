//! Low-level GPUI painting for cached scene primitives ([GPUI-HOST-SCENE]).

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::*;

use super::{Decoded, Outline, Paint, Shape, Step, Tint, Transform};
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
        move |bounds, window, cx| {
            let transform = Transform::new(bounds, decoded.width, decoded.zoom);
            let images = prepaint_handle.update(cx, |view, _| {
                view.scene_transforms.insert(key, transform);
                let images = sprites(view, &decoded);
                for old in view.sprites.released() {
                    // A turned copy no longer drawn leaves GPUI's atlas; failing to is harmless.
                    let _released = window.drop_image(old);
                }
                images
            });
            (decoded, transform, images)
        },
        move |bounds, (decoded, transform, images), window, cx| {
            let started = Instant::now();
            // A sized scene is clipped to its region; a window-pixel scene paints anywhere in the
            // window, as a GPUI canvas does.
            let mask = (decoded.width > 0.).then_some(ContentMask { bounds });
            let painted = window.with_content_mask(mask, |window| {
                paint(&decoded.shapes, &images, transform, window, cx)
            });
            if let Err(problem) = painted {
                handle.update(cx, |view, _| view.diagnose_once(problem));
            }
            let elapsed = started.elapsed().as_secs_f64() * 1000.;
            handle.update(cx, |view, _| {
                view.paint_ms = elapsed;
                if let Some(arrived) = view.frame_started.take() {
                    view.frame_ms = arrived.elapsed().as_secs_f64() * 1000.;
                }
            });
        },
    )
    .size_full()
}

/// Paints decoded primitives in their stable depth order. A long run of quads (rects, discs and
/// sprites) sharing one depth paints inside one GPUI layer, so the run takes a single draw order instead of
/// one bounds-tree insertion per quad; within a run GPUI keeps insertion order. Paths, strips and
/// glyphs keep their own draw orders, which GPUI's path batching depends on.
fn paint(
    shapes: &[Shape],
    images: &[Option<Arc<RenderImage>>],
    transform: Transform,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    for (run, layered) in runs(shapes) {
        let start = run.start;
        let each = |window: &mut Window, cx: &mut App| {
            shapes[run.clone()].iter().enumerate().try_for_each(|(offset, item)| {
                let image = images.get(start + offset).cloned().flatten();
                paint_shape(item, image, transform, window, cx)
            })
        };
        if layered {
            let bounds = window.content_mask().bounds;
            window.paint_layer(bounds, |window| each(window, cx))?;
        } else {
            each(window, cx)?;
        }
    }
    Ok(())
}

/// Each sprite's turned image from the scene's atlas, aligned with the shapes; a sprite whose
/// image is missing draws its placeholder quad and reports the problem once.
fn sprites(view: &mut HostView, decoded: &Decoded) -> Vec<Option<Arc<RenderImage>>> {
    let mut problems = Vec::new();
    let images = decoded
        .shapes
        .iter()
        .map(|shape| match shape {
            Shape::Sprite { atlas, angle, .. } => {
                let path = decoded.atlas.get(*atlas)?;
                view.sprites
                    .image(path, *angle)
                    .map_err(|problem| problems.push(problem))
                    .ok()
            }
            _ => None,
        })
        .collect();
    for problem in problems {
        view.diagnose_once(problem);
    }
    images
}

/// Splits depth-sorted shapes into consecutive runs that together cover every shape once: a run of
/// quads sharing a depth, or a single other shape, each marked for a layer when it is long enough.
fn runs(shapes: &[Shape]) -> Vec<(std::ops::Range<usize>, bool)> {
    let mut found = Vec::new();
    let mut start = 0;
    while let Some(shape) = shapes.get(start) {
        let length = shapes[start..]
            .iter()
            .take_while(|next| quad_like(next) && quad_like(shape) && next.depth() == shape.depth())
            .count()
            .max(1);
        found.push((start..start + length, length >= LAYERED_RUN));
        start += length;
    }
    found
}

/// The shortest run of same-depth quads worth one GPUI layer. A layer covers the scene, so it
/// changes the draw orders GPUI gives the paths after it, and with them how overlapping
/// translucent paths batch; a few quads paint one by one, as a native GPUI program paints them,
/// which keeps the painting port pixel-identical to GPUI's own example.
const LAYERED_RUN: usize = 64;

/// Whether GPUI draws this primitive as a single quad.
fn quad_like(shape: &Shape) -> bool {
    matches!(
        shape,
        Shape::Rect { .. } | Shape::Disc { .. } | Shape::Sprite { .. }
    )
}

/// Paints exactly one primitive with GPUI's low-level operations.
fn paint_shape(
    shape: &Shape,
    image: Option<Arc<RenderImage>>,
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
        } => match image {
            Some(image) => {
                let bounds = centre(*x, *y, *w, *h, transform);
                window
                    .paint_image(bounds, bounds, Corners::default(), image, 0, false)
                    .map_err(|problem| format!("a sprite could not be painted: {problem}"))?
            }
            None => window.paint_quad(fill(
                centre(*x, *y, *w, *h, transform),
                sprite_colour(*atlas, *tint),
            )),
        },
        Shape::Glyphs {
            text,
            x,
            y,
            size,
            colour,
            ..
        } => paint_glyphs(text, *x, *y, *size, *colour, transform, window, cx)?,
        Shape::Lettering {
            text,
            x,
            y,
            size,
            colour,
            anchor,
            weight,
            ..
        } => paint_lettering(text, (*x, *y, *size), *colour, (*anchor, *weight), transform, window, cx)?,
        Shape::Path {
            steps,
            outline,
            paint,
            transform: moved,
            ..
        } => paint_path(steps, outline, paint, *moved, transform, window)?,
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

/// Shapes one text run in the system font at `weight` and paints it with its left edge, centre or
/// right edge (`anchor` 0, 1 or 2) at x, the top of its line at y.
fn paint_lettering(
    text: &str,
    (x, y, size): (f32, f32, f32),
    colour: u32,
    (anchor, weight): (u8, f32),
    transform: Transform,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    let scaled = px(size * transform.scale);
    let run = TextRun {
        len: text.len(),
        color: Hsla::from(rgba(colour)),
        font: Font {
            weight: FontWeight(weight),
            ..font(".SystemUIFont")
        },
        ..Default::default()
    };
    let line = window
        .text_system()
        .shape_line(text.to_owned().into(), scaled, &[run], None);
    let shift = line.width * (f32::from(anchor) / 2.);
    let at = transform.point(x, y) - point(shift, px(0.));
    line.paint(at, scaled, TextAlign::Left, None, window, cx)
        .map_err(|problem| format!("scene lettering could not be painted: {problem}"))
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

/// Replays a path's steps on GPUI's builder in the original call order; the application's own
/// transform comes first and the scene's last, and a window-pixel scene adds no call at all.
fn paint_path(
    steps: &[Step],
    outline: &Outline,
    paint: &Paint,
    moved: Option<(f32, f32, f32)>,
    transform: Transform,
    window: &mut Window,
) -> Result<(), String> {
    let mut builder = builder(outline, transform.scale);
    for step in steps {
        add(&mut builder, step);
    }
    if let Some((x, y, scale)) = moved {
        builder.translate(point(px(x), px(y)));
        builder.scale(scale);
    }
    if !transform.identity() {
        builder.scale(transform.scale);
        builder.translate(transform.origin);
    }
    let path = builder
        .build()
        .map_err(|problem| format!("scene path could not be built: {problem}"))?;
    window.paint_path(path, background(paint));
    Ok(())
}

/// Starts a fill or stroke builder; stroke widths and dashes scale with the scene, not the path.
fn builder(outline: &Outline, scale: f32) -> PathBuilder {
    match outline {
        Outline::Fill => PathBuilder::fill(),
        Outline::Stroke {
            width,
            bevel,
            dashes,
        } => {
            let width = px(width * scale);
            let mut builder = match bevel {
                true => PathBuilder::stroke(width).with_style(PathStyle::Stroke(
                    StrokeOptions::default()
                        .with_line_width(f32::from(width))
                        .with_line_join(lyon::tessellation::LineJoin::Bevel),
                )),
                false => PathBuilder::stroke(width),
            };
            if !dashes.is_empty() {
                let dashes: Vec<Pixels> = dashes.iter().map(|dash| px(dash * scale)).collect();
                builder = builder.dash_array(&dashes);
            }
            builder
        }
    }
}

/// Adds one step in scene units.
fn add(builder: &mut PathBuilder, step: &Step) {
    let at = |(x, y): (f32, f32)| point(px(x), px(y));
    match step {
        Step::Move(to) => builder.move_to(at(*to)),
        Step::Line(to) => builder.line_to(at(*to)),
        Step::Curve(to, ctrl) => builder.curve_to(at(*to), at(*ctrl)),
        Step::Cubic(to, a, b) => builder.cubic_bezier_to(at(*to), at(*a), at(*b)),
        Step::Arc {
            radii,
            rotation,
            large,
            sweep,
            to,
        } => builder.arc_to(at(*radii), px(*rotation), *large, *sweep, at(*to)),
        Step::Polygon(corners, closed) => {
            let corners: Vec<Point<Pixels>> = corners.iter().map(|corner| at(*corner)).collect();
            builder.add_polygon(&corners, *closed);
        }
        Step::Close => builder.close(),
    }
}

/// Converts a paint to GPUI's background without passing a colour through another space.
fn background(paint: &Paint) -> Background {
    match paint {
        Paint::Solid(tint) => solid(*tint),
        Paint::Linear {
            angle,
            from,
            to,
            oklab,
        } => {
            let gradient = linear_gradient(
                *angle,
                linear_color_stop(hsla(from.0), from.1),
                linear_color_stop(hsla(to.0), to.1),
            );
            match oklab {
                true => gradient.color_space(ColorSpace::Oklab),
                false => gradient,
            }
        }
    }
}

/// A solid background from the tint's own type, as GPUI converts an `Rgba` or `Hsla` itself.
fn solid(tint: Tint) -> Background {
    match tint {
        Tint::Rgb([r, g, b, a]) => Rgba { r, g, b, a }.into(),
        Tint::Hsl([h, s, l, a]) => Hsla { h, s, l, a }.into(),
    }
}

/// The tint as `Hsla`, the type a gradient stop takes.
fn hsla(tint: Tint) -> Hsla {
    match tint {
        Tint::Rgb([r, g, b, a]) => Rgba { r, g, b, a }.into(),
        Tint::Hsl([h, s, l, a]) => Hsla { h, s, l, a },
    }
}

#[cfg(test)]
mod tests {
    use super::{hsla, runs, LAYERED_RUN};
    use crate::scene::{Shape, Tint};

    #[test]
    fn tint_keeps_fractional_hue_channels_and_alpha() {
        let green = gpui_kit::Rgba::from(hsla(Tint::Hsl([1. / 3., 1., 0.5, 0.4])));
        assert_channels(green, [0., 1., 0., 0.4]);
        assert_eq!(green.a, 0.4);
        let roundtrip = gpui_kit::Rgba::from(hsla(Tint::Rgb([0.2, 0.4, 0.6, 0.3])));
        assert_channels(roundtrip, [0.2, 0.4, 0.6, 0.3]);
    }

    fn assert_channels(colour: gpui_kit::Rgba, expected: [f32; 4]) {
        let actual = [colour.r, colour.g, colour.b, colour.a];
        for (got, expected) in actual.into_iter().zip(expected) {
            assert!((got - expected).abs() < 0.00001);
        }
    }

    fn disc(depth: f32) -> Shape {
        Shape::Disc {
            x: 0.,
            y: 0.,
            radius: 1.,
            colour: 0,
            depth,
        }
    }

    fn glyphs(depth: f32) -> Shape {
        Shape::Glyphs {
            text: "x".to_owned(),
            x: 0.,
            y: 0.,
            size: 1.,
            colour: 0,
            depth,
        }
    }

    /// Every shape is painted exactly once: short quad runs one by one, a long run in one layer,
    /// and a depth change or a non-quad ends a run.
    #[test]
    fn runs_cover_every_shape_once_and_layer_only_long_quad_runs() {
        let mut shapes: Vec<Shape> = (0..5).map(|_| disc(0.)).collect();
        shapes.push(glyphs(0.));
        shapes.extend((0..LAYERED_RUN).map(|_| disc(1.)));
        shapes.push(disc(2.));
        let found = runs(&shapes);
        assert_eq!(
            found,
            vec![
                (0..5, false),
                (5..6, false),
                (6..6 + LAYERED_RUN, true),
                (6 + LAYERED_RUN..7 + LAYERED_RUN, false)
            ]
        );
        let covered: usize = found.iter().map(|(run, _)| run.len()).sum();
        assert_eq!(covered, shapes.len());
    }
}
