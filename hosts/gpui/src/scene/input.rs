//! Focus, filtered keys and pointer events in scene units ([GPUI-HOST-SCENE]).

use gpui_kit::*;
use serde_json::{json, Value};

use super::{paint, Decoded};
use crate::events;
use crate::host::{now, HostView};
use crate::render::{self, Binding, Ctx};

/// Builds the focusable wrapper and canvas without adding application behaviour.
pub(super) fn interactive(ctx: &mut Ctx, ordinal: u64, decoded: Decoded) -> AnyElement {
    let handle = ctx.cx.entity();
    let binding = render::binding(ctx, ordinal);
    let focus = scene_focus(ctx);
    let key = ctx.key.clone();
    // A fitted scene's scale is the one its last frame was painted at.
    let scale = ctx
        .view
        .scene_transforms
        .get(&key)
        .map_or(decoded.zoom, |t| t.scale);
    let access = super::access::overlay(&handle, &binding, &decoded.access, scale);
    let surface = div()
        .id(render::control_id(ctx, ordinal))
        .relative()
        .w_full()
        .flex_grow(1.)
        .min_h(px(320.))
        .track_focus(&focus)
        .on_key_down(key_down(handle.clone(), binding.clone()))
        .on_key_up(key_up(handle.clone(), binding.clone()))
        .on_mouse_down(
            MouseButton::Left,
            mouse_down(handle.clone(), focus, binding.clone(), key.clone()),
        )
        .on_mouse_up(
            MouseButton::Left,
            mouse_up(handle.clone(), binding.clone(), key.clone()),
        )
        .on_mouse_move(mouse_move(handle.clone(), binding, key));
    surface
        .child(paint::canvas(ctx, handle, decoded))
        .children(access)
        .into_any_element()
}

/// Returns the persistent focus handle owned by this scene's region key.
fn scene_focus(ctx: &mut Ctx) -> FocusHandle {
    let cx = &mut *ctx.cx;
    ctx.view
        .scene_focus
        .entry(ctx.key.clone())
        .or_insert_with(|| cx.focus_handle())
        .clone()
}

/// Builds the filtered key-down callback.
fn key_down(
    handle: Entity<HostView>,
    binding: Binding,
) -> impl Fn(&KeyDownEvent, &mut Window, &mut App) {
    move |event, window, cx| send_key(&handle, &binding, &event.keystroke.key, true, window, cx)
}

/// Builds the filtered key-up callback.
fn key_up(
    handle: Entity<HostView>,
    binding: Binding,
) -> impl Fn(&KeyUpEvent, &mut Window, &mut App) {
    move |event, window, cx| send_key(&handle, &binding, &event.keystroke.key, false, window, cx)
}

/// Dispatches an admitted key without translating it into an application action.
fn send_key(
    handle: &Entity<HostView>,
    binding: &Binding,
    key: &str,
    down: bool,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(key) = key_name(key) {
        send(
            handle,
            binding,
            json!({"key": key, "down": down}),
            window,
            cx,
        );
    }
}

/// Admits only the host-neutral key vocabulary in the scene contract.
fn key_name(key: &str) -> Option<&str> {
    let named = matches!(
        key,
        "left" | "right" | "up" | "down" | "space" | "enter" | "escape" | "tab" | "backspace"
    );
    let character = key.len() == 1
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    (named || character).then_some(key)
}

/// Focuses a scene on press and forwards its scene-space pointer value.
fn mouse_down(
    handle: Entity<HostView>,
    focus: FocusHandle,
    binding: Binding,
    key: String,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) {
    move |event, window, cx| {
        focus.focus(window, cx);
        pointer(
            &handle,
            &binding,
            &key,
            (event.position, event.modifiers.shift),
            true,
            false,
            window,
            cx,
        );
    }
}

/// Forwards a pointer release.
fn mouse_up(
    handle: Entity<HostView>,
    binding: Binding,
    key: String,
) -> impl Fn(&MouseUpEvent, &mut Window, &mut App) {
    move |event, window, cx| {
        pointer(
            &handle,
            &binding,
            &key,
            (event.position, event.modifiers.shift),
            false,
            false,
            window,
            cx,
        )
    }
}

/// Forwards every drag movement and at most one hover movement per frame.
fn mouse_move(
    handle: Entity<HostView>,
    binding: Binding,
    key: String,
) -> impl Fn(&MouseMoveEvent, &mut Window, &mut App) {
    move |event, window, cx| {
        let down = event.pressed_button.is_some();
        pointer(
            &handle,
            &binding,
            &key,
            (event.position, event.modifiers.shift),
            down,
            !down,
            window,
            cx,
        );
    }
}

/// Converts and dispatches a pointer, with whether Shift is held, through the scene's existing binding.
#[allow(clippy::too_many_arguments)]
fn pointer(
    handle: &Entity<HostView>,
    binding: &Binding,
    key: &str,
    (position, shift): (Point<Pixels>, bool),
    down: bool,
    hover: bool,
    window: &mut Window,
    cx: &mut App,
) {
    handle.update(cx, |view, cx| {
        let Some(transform) = view.scene_transforms.get(key).copied() else {
            return;
        };
        if hover && !view.scene_hover.insert(key.to_owned()) {
            return;
        }
        let (x, y) = transform.scene(position);
        if hover {
            reset_hover(key, window, cx);
        }
        let event = events::control(
            now(),
            &binding.region,
            current(view, binding),
            binding.ordinal,
            json!({"x": x, "y": y, "down": down, "shift": shift}),
        );
        view.dispatch(event, window, cx);
    });
}

/// Reopens the hover gate on the next display frame.
fn reset_hover(key: &str, window: &mut Window, cx: &mut Context<HostView>) {
    let key = key.to_owned();
    cx.on_next_frame(window, move |view, _, _| {
        view.scene_hover.remove(&key);
    });
}

/// A scene re-renders every frame, so input targets the region's live revision; its binding ordinal is stable.
pub(super) fn current(view: &HostView, binding: &Binding) -> u64 {
    view.regions
        .get(&binding.region)
        .map_or(binding.revision, |region| region.revision)
}

/// Dispatches one already encoded scene value.
fn send(
    handle: &Entity<HostView>,
    binding: &Binding,
    value: Value,
    window: &mut Window,
    cx: &mut App,
) {
    handle.update(cx, |view, cx| {
        let event = events::control(
            now(),
            &binding.region,
            current(view, binding),
            binding.ordinal,
            value,
        );
        view.dispatch(event, window, cx);
    });
}
