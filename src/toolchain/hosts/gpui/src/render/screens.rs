//! Screens and sections ([GPUI-HOST-RENDER]): titled sections, a screen with its heading,
//! the empty state and progress.

use super::{caption, control_id, gap, labelled_column, many, string, symbol, Ctx};
use crate::tree::{flag, whole, Tree};
use gpui_kit::component::{progress::Progress, ActiveTheme as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Draws a titled group box and optional footer caption.
pub(super) fn section(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(gap(ctx))
        .border_1()
        .border_color(ctx.cx.theme().border)
        .rounded_md()
        .p_3()
        .child(
            div()
                .font_semibold()
                .child(string(node, "title").to_owned()),
        )
        .children(many(ctx, node, "children"))
        .child(caption(ctx, string(node, "footer")))
        .into_any_element()
}

/// Draws a screen heading, trailing toolbar and body.
pub(super) fn screen(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    let heading = screen_heading(ctx, node);
    let toolbar = div()
        .flex()
        .gap(gap(ctx))
        .children(many(ctx, node, "toolbar"));
    div()
        .flex()
        .flex_col()
        .gap(gap(ctx))
        .child(div().flex().justify_between().child(heading).child(toolbar))
        .children(many(ctx, node, "children"))
        .into_any_element()
}

/// Draws the title and subtitle half of a screen heading.
fn screen_heading(ctx: &mut Ctx, node: &Tree) -> Div {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_xl()
                .font_semibold()
                .child(string(node, "title").to_owned()),
        )
        .child(caption(ctx, string(node, "subtitle")))
}

/// Draws a centred empty state.
pub(super) fn empty(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(gap(ctx))
        .child(symbol(ctx, string(node, "symbol")))
        .child(
            div()
                .font_semibold()
                .child(string(node, "title").to_owned()),
        )
        .child(caption(ctx, string(node, "detail")))
        .into_any_element()
}

/// Draws an indeterminate progress indicator with its supplied text.
pub(super) fn progress(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    labelled_column(node)
        .child(caption(ctx, string(node, "detail")))
        .child(Progress::new(control_id(ctx, 0)).loading(true))
        .into_any_element()
}

/// Implements [PRISM-ROUTING-PRESENTATION]: content-sized native frames retain the app's metrics.
pub(super) fn framed(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    let style = node.get("style").unwrap_or(&Tree::Null);
    let body = super::node(ctx, node.get("body").unwrap_or(&Tree::Null));
    frame_box(style).child(body).into_any_element()
}

/// Apply explicit sizes and insets; unsupported viewport-relative sizes remain rejected by Tree.
fn frame_box(style: &Tree) -> Div {
    div()
        .flex()
        .flex_col()
        .when(whole(style, "width") > 0, |d| {
            d.w(px(whole(style, "width") as f32))
        })
        .when(whole(style, "height") > 0, |d| {
            d.h(px(whole(style, "height") as f32)).flex_shrink(0.)
        })
        .when(flag(style, "grow"), |d| d.flex_grow(1.).min_h_0())
        .when(whole(style, "textSize") > 0, |d| {
            d.text_size(px(whole(style, "textSize") as f32))
        })
        .pt(px(whole(style, "top") as f32))
        .pr(px(whole(style, "right") as f32))
        .pb(px(whole(style, "bottom") as f32))
        .pl(px(whole(style, "left") as f32))
        .gap(px(whole(style, "gap") as f32))
}

/// One flexible canvas and an independently scrollable, fixed-width inspector.
pub(super) fn aside(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    let pane = super::node(ctx, node.get("pane").unwrap_or(&Tree::Null));
    let inspector = super::node(ctx, node.get("aside").unwrap_or(&Tree::Null));
    let width = whole(node, "width").clamp(160, 640) as f32;
    let column = div()
        .id(control_id(ctx, crate::tree::ordinal(node).unwrap_or(0)))
        .w(px(width))
        .h_full()
        .flex_shrink(0.)
        .overflow_y_scroll()
        .border_l_1()
        .border_color(ctx.cx.theme().border)
        .child(inspector);
    let main = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .child(pane);
    let row = div().flex().flex_row().flex_1().min_h_0().w_full();
    match string(node, "side") {
        "leading" => row.child(column).child(main).into_any_element(),
        _ => row.child(main).child(column).into_any_element(),
    }
}

/// App-supplied image assets, including the Prism logo, keep their declared dimensions.
pub(super) fn image(node: &Tree) -> AnyElement {
    img(std::path::PathBuf::from(string(node, "source")))
        .w(px(whole(node, "width").max(1) as f32))
        .h(px(whole(node, "height").max(1) as f32))
        .into_any_element()
}
