//! Shared view rows drawn with GPUI ([GPUI-HOST-RENDER], [NATIVE-SHM-VIEW]). Every colour, size and
//! space comes from the row: this host applies the application's look and holds no style of its
//! own. A control that delivers an event sends its number and words, nothing else.

use std::sync::Arc;

use gpui_kit::component::{switch::Switch, Disableable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::events::Touch;
use crate::host::HostView;
use crate::render::symbol;
use crate::rows::{flags, Control, Kind, Shown};

mod inputs;

/// Mutable drawing context; it carries no application state or behaviour.
pub(crate) struct Ctx<'a, 'b> {
    pub(crate) view: &'a mut HostView,
    pub(crate) window: &'a mut Window,
    pub(crate) cx: &'a mut Context<'b, HostView>,
    pub(crate) shown: Arc<Shown>,
}

/// A packed `0xRRGGBBAA` colour.
pub(crate) fn colour(value: u32) -> Rgba {
    rgba(value)
}

/// Draws the frame's rows with no parent, top to bottom.
pub fn root(view: &mut HostView, window: &mut Window, cx: &mut Context<HostView>) -> AnyElement {
    let shown = view.shown.clone();
    let mut ctx = Ctx {
        view,
        window,
        cx,
        shown: shown.clone(),
    };
    let roots: Vec<AnyElement> = shown.roots.iter().map(|row| draw(&mut ctx, *row)).collect();
    div()
        .flex()
        .flex_col()
        .flex_grow(1.)
        .min_h_0()
        .children(roots)
        .into_any_element()
}

/// Sends one host event from a control callback.
pub(crate) fn send(handle: &Entity<HostView>, touch: Touch, window: &mut Window, cx: &mut App) {
    handle.update(cx, |view, cx| view.send(touch, window, cx));
}

/// The stable element id of the row at `index`.
pub(crate) fn id(index: usize) -> ElementId {
    ElementId::Name(format!("row:{index}").into())
}

fn children(ctx: &mut Ctx, control: &Control) -> Vec<AnyElement> {
    control.children.iter().map(|row| draw(ctx, *row)).collect()
}

/// A container in the row's look: fill, corner, padding, spacing and border.
fn boxed(control: &Control) -> Div {
    let look = control.look;
    div()
        .flex()
        .when(look.fill != 0, |this| this.bg(colour(look.fill)))
        .when(look.radius > 0., |this| this.rounded(px(look.radius)))
        .when(look.padding > 0., |this| this.p(px(look.padding)))
        .when(look.spacing > 0., |this| this.gap(px(look.spacing)))
        .when(control.fit.border != 0, |this| {
            this.border_1().border_color(colour(control.fit.border))
        })
}

/// The row's fixed extents and share of free space.
fn fitted<E: Styled + FluentBuilder>(element: E, control: &Control) -> E {
    let fit = control.fit;
    element
        .when(fit.width > 0., |this| this.w(px(fit.width)))
        .when(fit.height > 0., |this| this.h(px(fit.height)))
        .when(fit.grow > 0., |this| {
            this.flex_grow(fit.grow).flex_basis(px(0.))
        })
}

/// Text in the row's ink, size and weight, set fixed-width or slanted as its flags say.
fn worded(control: &Control, text: String) -> Div {
    let look = control.look;
    let set = control.fit.flags;
    div()
        .text_color(colour(look.ink))
        .when(look.size > 0., |this| this.text_size(px(look.size)))
        .font_weight(FontWeight(look.weight))
        .when(set & flags::FIXED_WIDTH != 0, |this| {
            this.font_family("Menlo")
        })
        .when(set & flags::SLANTED != 0, |this| this.italic())
        .when(set & flags::CENTRED != 0, |this| this.text_center())
        .child(text)
}

/// The icon a label or press is led by, when its hint names one.
fn led(ctx: &mut Ctx, control: &Control, body: Div) -> Div {
    match control.hint.is_empty() {
        true => body,
        false => div()
            .flex()
            .items_center()
            .gap_1()
            .child(symbol(ctx, &control.hint))
            .child(body),
    }
}

fn tapped(ctx: &mut Ctx, index: usize, control: &Control) -> AnyElement {
    let handle = ctx.cx.entity();
    let enabled = control.fit.flags & flags::DISABLED == 0;
    let ordinal = control.action.unwrap_or(-1);
    let words = worded(control, control.words.clone());
    let label = led(ctx, control, words);
    fitted(
        boxed(control).id(id(index)).items_center().justify_center(),
        control,
    )
    .when(!enabled, |this| this.opacity(0.5))
    .when(enabled, |this| {
        this.cursor_pointer()
            .active(|this| this.opacity(0.8))
            .on_click(move |_, window, cx| send(&handle, Touch::control(ordinal, ""), window, cx))
    })
    .child(label)
    .into_any_element()
}

fn switched(ctx: &mut Ctx, index: usize, control: &Control) -> AnyElement {
    let handle = ctx.cx.entity();
    let ordinal = control.action.unwrap_or(-1);
    Switch::new(id(index))
        .label(control.words.clone())
        .checked(control.fit.flags & flags::CHOSEN != 0)
        .disabled(control.fit.flags & flags::DISABLED != 0)
        .on_click(move |on, window, cx| {
            let words = if *on { "true" } else { "false" };
            send(&handle, Touch::control(ordinal, words), window, cx)
        })
        .into_any_element()
}

/// A picker: its chosen option's words, then every option as a button that delivers its key.
fn picked(ctx: &mut Ctx, index: usize, control: &Control) -> AnyElement {
    let ordinal = control.action.unwrap_or(-1);
    let options: Vec<AnyElement> = control
        .children
        .iter()
        .filter_map(|row| {
            ctx.shown
                .controls
                .get(*row)
                .map(|option| (*row, option.clone()))
        })
        .map(|(row, option)| {
            let handle = ctx.cx.entity();
            let key = option.hint.clone();
            let chosen = option.fit.flags & flags::CHOSEN != 0;
            fitted(boxed(&option).id(id(row)).cursor_pointer(), &option)
                .when(chosen, |this| {
                    this.border_1().border_color(colour(control.look.ink))
                })
                .on_click(move |_, window, cx| {
                    send(&handle, Touch::control(ordinal, &key), window, cx)
                })
                .child(worded(&option, option.words.clone()))
                .into_any_element()
        })
        .collect();
    fitted(
        boxed(control).id(id(index)).flex_wrap().items_center(),
        control,
    )
    .child(worded(control, control.words.clone()))
    .children(options)
    .into_any_element()
}

/// A sheet: its children over the view, with a close press that delivers its dismissal.
fn sheet(ctx: &mut Ctx, index: usize, control: &Control) -> AnyElement {
    let handle = ctx.cx.entity();
    let ordinal = control.action.unwrap_or(-1);
    let body = children(ctx, control);
    fitted(boxed(control).id(id(index)).flex_col(), control)
        .child(
            div()
                .id(ElementId::Name(format!("dismiss:{index}").into()))
                .cursor_pointer()
                .child(symbol(ctx, "dismiss"))
                .on_click(move |_, window, cx| {
                    send(&handle, Touch::control(ordinal, ""), window, cx)
                }),
        )
        .children(body)
        .into_any_element()
}

/// Draws the row at `index` and everything inside it.
fn draw(ctx: &mut Ctx, index: usize) -> AnyElement {
    let Some(control) = ctx.shown.controls.get(index).cloned() else {
        return div().into_any_element();
    };
    match control.kind {
        Kind::Column | Kind::Menu => {
            let inner = children(ctx, &control);
            fitted(boxed(&control).flex_col(), &control)
                .children(inner)
                .into_any_element()
        }
        Kind::Line => {
            let inner = children(ctx, &control);
            fitted(boxed(&control).flex_row().items_center(), &control)
                .children(inner)
                .into_any_element()
        }
        Kind::Scroll => {
            let inner = children(ctx, &control);
            fitted(
                boxed(&control).id(id(index)).flex_col().overflow_y_scroll(),
                &control,
            )
            .min_h_0()
            .children(inner)
            .into_any_element()
        }
        Kind::Layer => {
            let inner = children(ctx, &control);
            fitted(boxed(&control).relative(), &control)
                .children(inner.into_iter().enumerate().map(|(at, child)| match at {
                    0 => child,
                    _ => div().absolute().inset_0().child(child).into_any_element(),
                }))
                .into_any_element()
        }
        Kind::Label => {
            let words = worded(&control, control.words.clone());
            fitted(led(ctx, &control, words), &control).into_any_element()
        }
        Kind::Press => tapped(ctx, index, &control),
        Kind::Entry | Kind::Editor => inputs::field(ctx, index, &control),
        Kind::Gap => div().flex_grow(1.).into_any_element(),
        Kind::Toggle => switched(ctx, index, &control),
        Kind::Picker => picked(ctx, index, &control),
        Kind::Choice => div().into_any_element(),
        Kind::Rule => fitted(
            div()
                .h(px(control.look.size.max(1.)))
                .w_full()
                .bg(colour(control.look.ink)),
            &control,
        )
        .into_any_element(),
        Kind::Image => fitted(
            img(std::path::PathBuf::from(control.words.clone())),
            &control,
        )
        .into_any_element(),
        Kind::Canvas => match ctx.shown.scenes.get(&index).cloned() {
            Some(scene) => crate::scene::draw(
                ctx.view,
                ctx.window,
                ctx.cx,
                index,
                control.action.unwrap_or(-1),
                scene,
            ),
            None => fitted(div().flex_grow(1.), &control).into_any_element(),
        },
        Kind::Sheet => sheet(ctx, index, &control),
        // A told component is drawn by the component renderer, never as a plain row.
        Kind::Field | Kind::Record | Kind::List => {
            ctx.view
                .diagnose_once("a told component is drawn by the component renderer".to_owned());
            div().into_any_element()
        }
    }
}
