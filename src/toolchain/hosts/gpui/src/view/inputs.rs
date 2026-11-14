//! Text fields over shared view rows ([GPUI-HOST-RENDER], [NATIVE-SHM-VIEW]). An editor's state is
//! kept by its control number while the application keeps drawing it; when the application's text
//! moves away from what the person typed and the field is not focused, the editor starts again from
//! the application's text, which is the store's truth.

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{fitted, id, Ctx};
use crate::events::Touch;
use crate::rows::{flags, Control, Kind};

/// The editor kept for `control`, made again when the application's text moved under an idle field.
fn editor(ctx: &mut Ctx, control: &Control) -> Entity<InputState> {
    let ordinal = control.action.unwrap_or(-1);
    let kept = ctx.view.inputs.get(&ordinal).cloned().filter(|input| {
        let state = input.read(ctx.cx);
        state.value().as_ref() == control.words || state.focus_handle(ctx.cx).is_focused(ctx.window)
    });
    if let Some(input) = kept {
        return input;
    }
    let (words, prompt) = (control.words.clone(), control.hint.clone());
    let secret = control.fit.flags & flags::SECRET != 0;
    let window = &mut *ctx.window;
    let input = ctx.cx.new(|cx| {
        InputState::new(window, cx)
            .default_value(words)
            .placeholder(prompt)
            .masked(secret)
    });
    let commit = control.fit.flags & flags::ON_COMMIT != 0;
    ctx.cx
        .subscribe_in(
            &input,
            ctx.window,
            move |view, input, event: &InputEvent, window, cx| {
                let wanted = match event {
                    InputEvent::Change => !commit,
                    InputEvent::PressEnter { .. } => commit,
                    _ => false,
                };
                if wanted {
                    let text = input.read(cx).value().to_string();
                    view.send(Touch::control(ordinal, &text), window, cx);
                }
            },
        )
        .detach();
    ctx.view.inputs.insert(ordinal, input.clone());
    input
}

/// A one-line field or, for an editor row, a field of many lines, drawn in the row's look.
pub(super) fn field(ctx: &mut Ctx, index: usize, control: &Control) -> AnyElement {
    if control.kind == Kind::Editor {
        ctx.view
            .diagnose_once("an editor of many lines is drawn as a one-line field".to_owned());
    }
    let input = editor(ctx, control);
    fitted(
        div()
            .flex_grow(1.)
            .text_color(super::colour(control.look.ink))
            .when(control.look.size > 0., |this| {
                this.text_size(px(control.look.size))
            })
            .child(Input::new(&input).id(id(index)).w_full()),
        control,
    )
    .into_any_element()
}
