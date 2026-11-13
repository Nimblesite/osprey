//! The host-only diagnostic strip ([GPUI-HOST-RENDER]): why a frame or a capability was refused,
//! newest last, and the frame clock's measurements while it animates.

use gpui_kit::component::{
    button::{Button, ButtonVariants as _},
    ActiveTheme as _,
};
use gpui_kit::*;

use crate::host::HostView;

/// Renders the collapsible host diagnostic strip, newest messages last.
pub fn strip(view: &mut HostView, cx: &mut Context<HostView>) -> AnyElement {
    let messages = diagnostic_messages(view);
    let metrics = view.animating && !crate::host::bare();
    if messages.is_empty() && !metrics {
        return div().into_any_element();
    }
    let expanded = !view.collapsed;
    let mut strip = div()
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(diagnostic_header(cx, messages.len(), expanded));
    if expanded {
        // Only the latest few lines show, so a burst of diagnostics cannot push the application off screen.
        let recent = messages[messages.len().saturating_sub(6)..].to_vec();
        strip = strip.children(diagnostic_lines(recent, cx));
    }
    if metrics {
        strip = strip.child(div().text_sm().child(format!(
            "fps {} dispatch {:.3} paint {:.3} frame {:.3}",
            view.fps, view.dispatch_ms, view.paint_ms, view.frame_ms
        )));
    }
    strip.into_any_element()
}

/// The application's and the renderer's diagnostics, in encounter order.
fn diagnostic_messages(view: &HostView) -> Vec<String> {
    view.messages.clone()
}

/// Draws the host-only disclosure control.
fn diagnostic_header(cx: &mut Context<HostView>, count: usize, expanded: bool) -> Button {
    let handle = cx.entity();
    Button::new("host:diagnostics")
        .ghost()
        .label(format!(
            "Diagnostics ({count}) {}",
            if expanded { "▾" } else { "▸" }
        ))
        .on_click(move |_, _, cx| {
            handle.update(cx, |view, cx| {
                view.collapsed = !view.collapsed;
                cx.notify();
            })
        })
}

/// Draws diagnostic messages in encounter order.
fn diagnostic_lines(messages: Vec<String>, cx: &App) -> Vec<AnyElement> {
    messages
        .into_iter()
        .map(|message| {
            div()
                .text_sm()
                .text_color(cx.theme().danger)
                .child(message)
                .into_any_element()
        })
        .collect()
}
