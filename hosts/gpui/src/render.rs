//! The exact Osprey Native node-to-GPUI mapping ([GPUI-HOST-RENDER]).

use gpui_kit::component::{
    badge::Badge,
    button::{Button, ButtonVariants as _},
    label::Label,
    progress::Progress,
    separator::Separator,
    ActiveTheme as _, Icon, IconName, StyledExt as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::Value;

use crate::envelope::Theme;
use crate::host::HostView;

mod controls;

/// Mutable rendering context; it carries no application state or behaviour.
pub(super) struct Ctx<'a, 'b> {
    pub(super) view: &'a mut HostView,
    pub(super) window: &'a mut Window,
    pub(super) cx: &'a mut Context<'b, HostView>,
    pub(super) key: String,
    pub(super) revision: u64,
    dark: bool,
    stack: Vec<String>,
}

/// Renders one framework region by key.
pub fn region<'a, 'b>(
    view: &'a mut HostView,
    key: &str,
    window: &'a mut Window,
    cx: &'a mut Context<'b, HostView>,
) -> AnyElement {
    let mut ctx = Ctx {
        view,
        dark: is_dark(window),
        window,
        cx,
        key: key.to_owned(),
        revision: 0,
        stack: Vec::new(),
    };
    embed(&mut ctx, key)
}

/// Whether the native window currently uses a dark appearance.
pub(crate) fn is_dark(window: &Window) -> bool {
    matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// Resolves a colour token in the current system appearance.
pub(crate) fn colour(theme: Option<&Theme>, name: &str, dark: bool) -> Option<Rgba> {
    let token = theme?.tokens.iter().find(|token| token.name == name)?;
    let text = if dark { &token.dark } else { &token.light };
    let digits = text.strip_prefix('#').filter(|digits| digits.len() == 6)?;
    u32::from_str_radix(digits, 16).ok().map(rgb)
}

/// Embeds a region and rejects recursive slot cycles instead of overflowing.
fn embed(ctx: &mut Ctx, key: &str) -> AnyElement {
    if ctx.stack.iter().any(|item| item == key) {
        return broken(ctx, format!("slot cycle at {key}"));
    }
    let Some(region) = ctx.view.regions.get(key).cloned() else {
        return broken(ctx, format!("missing region {key}"));
    };
    let previous = std::mem::replace(&mut ctx.key, key.to_owned());
    let revision = std::mem::replace(&mut ctx.revision, region.revision);
    ctx.stack.push(key.to_owned());
    let element = node(ctx, &region.node);
    ctx.stack.pop();
    ctx.key = previous;
    ctx.revision = revision;
    element
}

/// Maps exactly one node kind; unknown kinds become visible diagnostics.
fn node(ctx: &mut Ctx, node: &Value) -> AnyElement {
    match string(node, "kind") {
        "stack" => stack(ctx, node),
        "text" => text(ctx, node),
        "label" => labelled(ctx, node),
        "icon" => symbol(ctx, string(node, "symbol")),
        "badge" => badge(ctx, node),
        "button" => controls::button(ctx, node),
        "field" => controls::field(ctx, node),
        "toggle" => controls::toggle(ctx, node),
        "slot" => embed(ctx, string(node, "key")),
        "scene" => crate::scene::node(ctx, node),
        kind => container(ctx, kind, node),
    }
}

/// Maps the remaining container and collection kinds.
fn container(ctx: &mut Ctx, kind: &str, node: &Value) -> AnyElement {
    match kind {
        "section" => section(ctx, node),
        "screen" => screen(ctx, node),
        "divider" => Separator::horizontal().into_any_element(),
        "spacer" => div().flex_grow(1.).into_any_element(),
        "empty" => empty(ctx, node),
        "progress" => progress(ctx, node),
        "rows" => controls::rows(ctx, node),
        "picker" => controls::picker(ctx, node),
        kind => broken(ctx, format!("unknown node kind {kind}")),
    }
}

/// Lays out a vertical or horizontal child stack with framework spacing.
fn stack(ctx: &mut Ctx, node: &Value) -> AnyElement {
    let vertical = string(node, "axis") != "horizontal";
    let grows = holds_scene(ctx, node, 0);
    div()
        .flex()
        .when(grows, |this| this.flex_grow(1.).min_h_0())
        .when(vertical, |this| this.flex_col())
        .when(!vertical, |this| this.flex_row().items_center())
        .gap(gap(ctx))
        .children(many(ctx, node, "children"))
        .into_any_element()
}

/// Whether a scene sits under this node, so its stacks pass free space down to it.
fn holds_scene(ctx: &Ctx, node: &Value, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    match string(node, "kind") {
        "scene" => true,
        "slot" => ctx
            .view
            .regions
            .get(string(node, "key"))
            .is_some_and(|region| holds_scene(ctx, &region.node, depth + 1)),
        _ => list(node, "children")
            .iter()
            .any(|child| holds_scene(ctx, child, depth + 1)),
    }
}

/// Draws text using its role and semantic theme tone.
fn text(ctx: &mut Ctx, node: &Value) -> AnyElement {
    let role = string(node, "role");
    // Body text takes the application's `text` token when its theme defines one.
    let fallback = if role == "caption" {
        ctx.cx.theme().muted_foreground
    } else {
        tint(ctx, "text", ctx.cx.theme().foreground)
    };
    div()
        .text_color(tint(ctx, string(node, "tone"), fallback))
        .when(role == "title", |this| this.text_xl().font_semibold())
        .when(role == "caption", |this| this.text_sm())
        .child(string(node, "text").to_owned())
        .into_any_element()
}

/// Draws an icon and label without interpreting the label.
fn labelled(ctx: &mut Ctx, node: &Value) -> AnyElement {
    icon_label(ctx, node, gap(ctx)).into_any_element()
}

/// Draws the shared icon-and-label content used by labels and badges.
fn icon_label(ctx: &mut Ctx, node: &Value, spacing: Pixels) -> Div {
    div()
        .flex()
        .items_center()
        .gap(spacing)
        .child(symbol(ctx, string(node, "symbol")))
        .child(Label::new(string(node, "label")))
}

/// Draws a badge with its optional symbol and semantic tone.
fn badge(ctx: &mut Ctx, node: &Value) -> AnyElement {
    let colour = tint(ctx, string(node, "tone"), ctx.cx.theme().accent);
    Badge::new()
        .color(colour)
        .child(icon_label(ctx, node, px(4.)))
        .into_any_element()
}

/// Draws a titled group box and optional footer caption.
fn section(ctx: &mut Ctx, node: &Value) -> AnyElement {
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
fn screen(ctx: &mut Ctx, node: &Value) -> AnyElement {
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
fn screen_heading(ctx: &mut Ctx, node: &Value) -> Div {
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
fn empty(ctx: &mut Ctx, node: &Value) -> AnyElement {
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
fn progress(ctx: &mut Ctx, node: &Value) -> AnyElement {
    labelled_column(node)
        .child(caption(ctx, string(node, "detail")))
        .child(Progress::new(control_id(ctx, 0)).loading(true))
        .into_any_element()
}

/// Starts the shared labelled vertical layout used by fields and progress.
pub(super) fn labelled_column(node: &Value) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(Label::new(string(node, "label")))
}

/// Renders every child in a named JSON array.
fn many(ctx: &mut Ctx, node: &Value, name: &str) -> Vec<AnyElement> {
    list(node, name)
        .iter()
        .map(|child| self::node(ctx, child))
        .collect()
}

/// A control binding copied from the current region, never application state.
#[derive(Clone)]
pub(super) struct Binding {
    pub(super) region: String,
    pub(super) revision: u64,
    pub(super) ordinal: u64,
}

/// Captures the binding identity for a control callback.
pub(super) fn binding(ctx: &Ctx, ordinal: u64) -> Binding {
    Binding {
        region: ctx.key.clone(),
        revision: ctx.revision,
        ordinal,
    }
}

/// Every control ID is anchored to its region key and framework ordinal.
pub(super) fn control_id(ctx: &Ctx, ordinal: u64) -> String {
    format!("{}:{ordinal}", ctx.key)
}

/// Returns a known kit icon, with the SF Symbol spellings mapped explicitly.
pub(super) fn icon_name(name: &str) -> Option<IconName> {
    match name {
        "plus" => Some(IconName::Plus),
        "tray" => Some(IconName::Inbox),
        "play" => Some(IconName::Play),
        "gear" => Some(IconName::Settings),
        "search" => Some(IconName::Search),
        "info" => Some(IconName::Info),
        "star" => Some(IconName::Star),
        "check" => Some(IconName::Check),
        "warning" => Some(IconName::TriangleAlert),
        "xmark" => Some(IconName::Close),
        "trash" => Some(IconName::Delete),
        "folder" => Some(IconName::Folder),
        "globe" => Some(IconName::Globe),
        "link" => Some(IconName::ExternalLink),
        "chevron.right" => Some(IconName::ChevronRight),
        _ => None,
    }
}

/// Draws an icon or a visible empty square for an unknown symbol.
pub(super) fn symbol(ctx: &mut Ctx, name: &str) -> AnyElement {
    match icon_name(name) {
        Some(icon) => Icon::new(icon).into_any_element(),
        None if name.is_empty() => div().into_any_element(),
        None => missing_symbol(ctx, name),
    }
}

/// Records an unknown symbol once and never silently omits its placeholder.
pub(super) fn missing_symbol(ctx: &mut Ctx, name: &str) -> AnyElement {
    ctx.view.diagnose_once(format!("no icon for symbol {name}"));
    div()
        .size_4()
        .border_1()
        .border_color(ctx.cx.theme().foreground)
        .into_any_element()
}

/// Resolves a semantic token, falling back to the kit theme.
pub(super) fn tint(ctx: &Ctx, name: &str, fallback: Hsla) -> Hsla {
    colour(ctx.view.regions.theme(), name, ctx.dark).map_or(fallback, Hsla::from)
}

/// Reads the second framework spacing token.
pub(super) fn gap(ctx: &Ctx) -> Pixels {
    let value = ctx
        .view
        .regions
        .theme()
        .and_then(|theme| theme.spacing.get(1))
        .copied();
    px(value.unwrap_or(8).max(0) as f32)
}

/// Draws a muted caption, including an intentionally empty caption.
fn caption(ctx: &Ctx, text: &str) -> Div {
    div()
        .text_sm()
        .text_color(ctx.cx.theme().muted_foreground)
        .child(text.to_owned())
}

/// Records a rejected mapping once (render runs every frame) and draws a red box naming it.
pub(crate) fn broken(ctx: &mut Ctx, message: String) -> AnyElement {
    ctx.view.diagnose_once(message.clone());
    div()
        .border_1()
        .border_color(red())
        .text_color(red())
        .p_2()
        .child(message)
        .into_any_element()
}

/// Rejects a control that lacks the framework binding it must return.
pub(super) fn unbound(ctx: &mut Ctx) -> AnyElement {
    broken(ctx, format!("control in {} has no event ordinal", ctx.key))
}

/// Renders the collapsible host diagnostic strip, newest messages last.
pub fn diagnostics(view: &mut HostView, cx: &mut Context<HostView>) -> AnyElement {
    let messages = diagnostic_messages(view);
    if messages.is_empty() && !view.animating {
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
    if view.animating {
        strip = strip.child(div().text_sm().child(format!(
            "fps {} dispatch {:.3} paint {:.3} envelope {}",
            view.fps, view.dispatch_ms, view.paint_ms, view.envelope_bytes
        )));
    }
    strip.into_any_element()
}

/// Combines framework and renderer diagnostics without reordering either source.
fn diagnostic_messages(view: &HostView) -> Vec<String> {
    view.regions
        .diagnostics()
        .iter()
        .chain(view.messages.iter())
        .cloned()
        .collect()
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

/// Reads a string field from a well-formed framework node.
pub(super) fn string<'a>(node: &'a Value, name: &str) -> &'a str {
    node.get(name).and_then(Value::as_str).unwrap_or("")
}

/// Reads a boolean field from a well-formed framework node.
pub(super) fn flag(node: &Value, name: &str) -> bool {
    node.get(name).and_then(Value::as_bool).unwrap_or(false)
}

/// Reads a control's issued event ordinal.
pub(super) fn ordinal(node: &Value) -> Option<u64> {
    node.get("event").and_then(Value::as_u64)
}

/// Reads an array field without allocating a default array.
pub(super) fn list<'a>(node: &'a Value, name: &str) -> &'a [Value] {
    node.get(name)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}
