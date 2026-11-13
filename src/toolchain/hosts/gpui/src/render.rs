//! The exact Osprey Native node-to-GPUI mapping ([GPUI-HOST-RENDER]).

use gpui_kit::component::{
    badge::Badge,
    button::{Button, ButtonVariants as _},
    label::Label,
    separator::Separator,
    ActiveTheme as _, StyledExt as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::host::HostView;
use crate::tree::{flag, list, ordinal, string, whole, Theme, Tree};

mod controls;
mod icons;
mod screens;
pub(crate) use icons::{icon_name, missing_symbol, symbol};

/// Mutable rendering context; it carries no application state or behaviour.
pub(super) struct Ctx<'a, 'b> {
    pub(super) view: &'a mut HostView,
    pub(super) window: &'a mut Window,
    pub(super) cx: &'a mut Context<'b, HostView>,
    pub(super) key: String,
    dark: bool,
    stack: Vec<String>,
    /// The theme a `themed` ancestor scopes to this subtree, else the application's.
    scoped: Option<Theme>,
}

impl Ctx<'_, '_> {
    /// The theme in force here: the nearest `themed` ancestor's, else the application's.
    pub(super) fn theme(&self) -> Option<&Theme> {
        self.scoped.as_ref().or_else(|| self.view.regions.theme())
    }
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
        stack: Vec::new(),
        scoped: None,
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
        return broken(ctx.view, format!("slot cycle at {key}"));
    }
    let Some(region) = ctx.view.regions.get(key).cloned() else {
        return broken(ctx.view, format!("missing region {key}"));
    };
    let previous = std::mem::replace(&mut ctx.key, key.to_owned());
    ctx.stack.push(key.to_owned());
    let element = node(ctx, &region.node);
    ctx.stack.pop();
    ctx.key = previous;
    element
}

/// Maps exactly one node kind; unknown kinds become visible diagnostics.
fn node(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    match string(node, "kind") {
        "stack" => stack(ctx, node),
        "text" => text(ctx, node),
        "label" => labelled(ctx, node),
        "pair" => pair(ctx, node),
        "icon" => symbol(ctx, string(node, "symbol")),
        "badge" => badge(ctx, node),
        "button" => controls::button(ctx, node),
        "field" => controls::field(ctx, node),
        "toggle" => controls::toggle(ctx, node),
        "slot" => embed(ctx, string(node, "key")),
        "scene" => crate::scene::node(ctx.view, ctx.window, ctx.cx, &ctx.key, node),
        kind => container(ctx, kind, node),
    }
}

/// Maps the remaining container and collection kinds.
fn container(ctx: &mut Ctx, kind: &str, node: &Tree) -> AnyElement {
    match kind {
        "section" => screens::section(ctx, node),
        "screen" => screens::screen(ctx, node),
        "divider" => Separator::horizontal().into_any_element(),
        "spacer" => div().flex_grow(1.).into_any_element(),
        "empty" => screens::empty(ctx, node),
        "progress" => screens::progress(ctx, node),
        "rows" => controls::rows(ctx, node),
        "picker" => controls::picker(ctx, node),
        "themed" => themed(ctx, node),
        "meter" => meter(ctx, node),
        kind => broken(ctx.view, format!("unknown node kind {kind}")),
    }
}

/// Draws `body` with `theme` scoped to it, on its `background` token in its `text` ink
/// ([UI-COMPONENTS], [UI-MACHINE-SKINS]); a malformed theme is refused visibly.
fn themed(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    let Some(theme) = Theme::told(node.get("theme")) else {
        return broken(ctx.view, "themed node without a valid theme".to_owned());
    };
    let outer = ctx.scoped.replace(theme);
    let fill = tint(ctx, "background", ctx.cx.theme().background);
    let ink = tint(ctx, "text", ctx.cx.theme().foreground);
    let body = node.get("body").cloned().unwrap_or(Tree::Null);
    let element = self::node(ctx, &body);
    ctx.scoped = outer;
    div()
        .flex()
        .flex_col()
        .flex_grow(1.)
        .bg(fill)
        .text_color(ink)
        .p(gap(ctx) * 3.)
        .child(element)
        .into_any_element()
}

/// Draws a labelled bar filled to `value` of `total` in the node's tone
/// ([UI-COMPONENTS-METER]); the framework already clamped both.
fn meter(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    let total = whole(node, "total").max(1) as f32;
    let value = whole(node, "value").max(0) as f32;
    let fill = tint(ctx, string(node, "tone"), ctx.cx.theme().accent);
    let track = tint(ctx, "secondary", ctx.cx.theme().muted).opacity(0.25);
    div()
        .id(SharedString::from(
            control_id(ctx, 0) + ":" + string(node, "label"),
        ))
        .flex_grow(1.)
        .min_w(px(120.))
        .h(px(6.))
        .rounded_full()
        .bg(track)
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(fill)
                .w(relative((value / total).min(1.))),
        )
        .into_any_element()
}

/// Lays out a vertical or horizontal child stack with framework spacing.
fn stack(ctx: &mut Ctx, node: &Tree) -> AnyElement {
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
fn holds_scene(ctx: &Ctx, node: &Tree, depth: usize) -> bool {
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
fn text(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    let role = string(node, "role");
    // Body text takes the application's `text` token when its theme defines one.
    let fallback = if role == "caption" {
        tint(ctx, "secondary", ctx.cx.theme().muted_foreground)
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
fn labelled(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    icon_label(ctx, node, gap(ctx)).into_any_element()
}

/// Draws a label and its value on one line, the label muted.
fn pair(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    div()
        .flex()
        .gap(gap(ctx))
        .child(caption(ctx, string(node, "label")))
        .child(string(node, "value").to_owned())
        .into_any_element()
}

/// Draws the shared icon-and-label content used by labels and badges.
fn icon_label(ctx: &mut Ctx, node: &Tree, spacing: Pixels) -> Div {
    div()
        .flex()
        .items_center()
        .gap(spacing)
        .child(symbol(ctx, string(node, "symbol")))
        .child(Label::new(string(node, "label")))
}

/// Draws a badge with its optional symbol and semantic tone.
fn badge(ctx: &mut Ctx, node: &Tree) -> AnyElement {
    let colour = tint(ctx, string(node, "tone"), ctx.cx.theme().accent);
    Badge::new()
        .color(colour)
        .child(icon_label(ctx, node, px(4.)))
        .into_any_element()
}

/// Starts the shared labelled vertical layout used by fields and progress.
pub(super) fn labelled_column(node: &Tree) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(Label::new(string(node, "label")))
}

/// Renders every child in a named JSON array.
fn many(ctx: &mut Ctx, node: &Tree, name: &str) -> Vec<AnyElement> {
    list(node, name)
        .iter()
        .map(|child| self::node(ctx, child))
        .collect()
}

/// A control binding copied from the frame, never application state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Binding {
    pub(super) ordinal: i64,
}

/// Captures the binding identity for a control callback.
pub(super) fn binding(ordinal: i64) -> Binding {
    Binding { ordinal }
}

/// Every control ID is anchored to its region key and framework ordinal.
pub(super) fn control_id(ctx: &Ctx, ordinal: i64) -> String {
    format!("{}:{ordinal}", ctx.key)
}

/// Resolves a semantic token, falling back to the kit theme.
pub(super) fn tint(ctx: &Ctx, name: &str, fallback: Hsla) -> Hsla {
    colour(ctx.theme(), name, ctx.dark).map_or(fallback, Hsla::from)
}

/// Reads the second framework spacing token.
pub(super) fn gap(ctx: &Ctx) -> Pixels {
    let value = ctx.theme().and_then(|theme| theme.spacing.get(1)).copied();
    px(value.unwrap_or(8).max(0) as f32)
}

/// Draws a muted caption, including an intentionally empty caption.
fn caption(ctx: &Ctx, text: &str) -> Div {
    div()
        .text_sm()
        .text_color(tint(ctx, "secondary", ctx.cx.theme().muted_foreground))
        .child(text.to_owned())
}

/// Records a rejected mapping once (render runs every frame) and draws a red box naming it.
pub(crate) fn broken(view: &mut HostView, message: String) -> AnyElement {
    view.diagnose_once(message.clone());
    div()
        .border_1()
        .border_color(red())
        .text_color(red())
        .p_2()
        .child(message)
        .into_any_element()
}

/// Rejects a control that lacks the framework binding it must return.
pub(super) fn unbound(view: &mut HostView, key: &str) -> AnyElement {
    broken(view, format!("control in {key} has no event ordinal"))
}

#[cfg(test)]
mod tests {
    use super::icon_name;

    /// Every `Symbols::name` in src/osprey/packages/native/src/symbol.ospml ([UI-COMPONENTS-SYMBOLS]).
    const SYMBOLS: [&str; 33] = [
        "add", "delete", "dismiss", "check", "warning", "info", "question", "search", "settings",
        "star", "play", "pause", "stop", "refresh", "folder", "document", "package", "inbox",
        "globe", "external", "lock", "shield", "branch", "tag", "clock", "person", "cursor",
        "chevron", "send", "terminal", "edit", "panel", "sparkle",
    ];

    #[test]
    fn every_framework_symbol_has_a_kit_icon() {
        let missing: Vec<_> = SYMBOLS
            .iter()
            .filter(|name| icon_name(name).is_none())
            .collect();
        assert!(missing.is_empty(), "no kit icon for {missing:?}");
    }

    #[test]
    fn platform_icon_spellings_are_not_symbols() {
        for name in ["xmark", "trash", "gear", "shippingbox", "chevron.right", ""] {
            assert!(icon_name(name).is_none(), "{name} should not map");
        }
    }
}
