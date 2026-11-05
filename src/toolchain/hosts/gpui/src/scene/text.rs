//! Shared text shaping and passive labels. [GPUI-HOST-SCENE-TEXT-A11Y]

use gpui_kit::*;

use super::{access::Access, Shape, Transform};

/// The same system font and weight for painting and accessibility measurement.
pub(super) fn lettering_font(weight: f32) -> Font {
    Font {
        weight: FontWeight(weight),
        ..font(".SystemUIFont")
    }
}

/// Shapes a decoded single-line primitive; the decoder rejects newlines before
/// GPUI's single-line shaping boundary can encounter them. [GPUI-HOST-SCENE]
pub(super) fn line(
    text: &str,
    size: Pixels,
    colour: u32,
    font: Font,
    window: &Window,
) -> ShapedLine {
    let run = TextRun {
        len: text.len(),
        color: Hsla::from(rgba(colour)),
        font,
        ..Default::default()
    };
    window
        .text_system()
        .shape_line(text.to_owned().into(), size, &[run], None)
}

/// Visible words and numbers are static text, never extra player controls.
/// Bounds use the very font size the painter uses, including fitting and DPI.
pub(super) fn labels(shapes: &[Shape], scale: f32, window: &Window) -> Vec<Access> {
    if !scale.is_finite() || scale <= 0. {
        return Vec::new();
    }
    shapes
        .iter()
        .filter_map(|shape| label(shape, scale, window))
        .collect()
}

fn label(shape: &Shape, scale: f32, window: &Window) -> Option<Access> {
    match shape {
        Shape::Lettering {
            text,
            x,
            y,
            size,
            colour,
            anchor,
            weight,
            ..
        } => measured(
            text,
            (*x, *y, *size),
            (*colour, *anchor, lettering_font(*weight)),
            scale,
            window,
        ),
        Shape::Glyphs {
            text,
            x,
            y,
            size,
            colour,
            ..
        } => measured(
            text,
            (*x, *y, *size),
            (*colour, 0, TextRun::default().font),
            scale,
            window,
        ),
        _ => None,
    }
}

fn measured(
    text: &str,
    (x, y, size): (f32, f32, f32),
    (colour, anchor, font): (u32, u8, Font),
    scale: f32,
    window: &Window,
) -> Option<Access> {
    if colour & 255 == 0 || !text.chars().any(char::is_alphanumeric) {
        return None;
    }
    let width = f32::from(line(text, px(size * scale), colour, font, window).width) / scale;
    Some(Access {
        role: Role::Label,
        label: text.to_owned(),
        x: f64::from(x - width * f32::from(anchor) / 2.),
        y: f64::from(y),
        w: f64::from(width),
        h: f64::from(size),
        value: 0.,
        min: 0.,
        max: 0.,
        selected: false,
    })
}

/// Shapes one text run in the system font at `weight` and paints it with its left edge, centre or
/// right edge (`anchor` 0, 1 or 2) at x, the top of its line at y.
pub(super) fn paint_lettering(
    text: &str,
    (x, y, size): (f32, f32, f32),
    colour: u32,
    (anchor, weight): (u8, f32),
    transform: Transform,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    let scaled = px(size * transform.scale);
    let line = line(text, scaled, colour, lettering_font(weight), window);
    let shift = line.width * (f32::from(anchor) / 2.);
    let at = transform.point(x, y) - point(shift, px(0.));
    line.paint(at, scaled, TextAlign::Left, None, window, cx)
        .map_err(|problem| format!("scene lettering could not be painted: {problem}"))
}

/// Shapes and paints one coloured glyph run at its scaled position.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_glyphs(
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
    let line = line(text, scaled, colour, TextRun::default().font, window);
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
