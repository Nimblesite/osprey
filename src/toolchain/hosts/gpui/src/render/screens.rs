//! Screens and sections ([GPUI-HOST-RENDER]): titled sections, a screen with its heading,
//! the empty state and progress.

use super::{caption, control_id, gap, labelled_column, many, string, symbol, Ctx};
use crate::tree::Tree;
use gpui_kit::component::{progress::Progress, ActiveTheme as _, StyledExt as _};
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
