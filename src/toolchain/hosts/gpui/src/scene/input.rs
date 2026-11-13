//! Focus, filtered keys and pointer events in scene units ([GPUI-HOST-SCENE]).

use gpui_kit::*;

use super::{paint, Decoded};
use crate::events::Touch;
use crate::host::HostView;
use crate::render::Binding;
use crate::view;

/// Builds the focusable wrapper and canvas without adding application behaviour.
pub(super) fn interactive<'b>(
    view: &mut HostView,
    window: &mut Window,
    cx: &mut Context<'b, HostView>,
    row: usize,
    ordinal: i64,
    decoded: Decoded,
) -> AnyElement {
    let handle = cx.entity();
    let binding = Binding { ordinal };
    let key = format!("canvas:{row}");
    let focus = scene_focus(view, cx, &key);
    // A fitted scene's scale is the one its last frame was painted at.
    let scale = view
        .scene_transforms
        .get(&key)
        .map_or(decoded.zoom, |t| t.scale);
    let mut elements = decoded.access.to_vec();
    elements.extend(super::text::labels(&decoded.shapes, scale, window));
    let access = super::access::overlay(&handle, &binding, &elements, scale);
    let binding_for_paint = binding;
    let surface = div()
        .id(view::id(row))
        .relative()
        .w_full()
        .flex_grow(1.)
        .min_h(px(320.))
        .track_focus(&focus)
        .on_key_down(key_down(handle.clone(), binding))
        .on_key_up(key_up(handle.clone(), binding))
        .on_mouse_down(
            MouseButton::Left,
            mouse_down(handle.clone(), focus, binding, key.clone()),
        )
        .on_mouse_up(
            MouseButton::Left,
            mouse_up(handle.clone(), binding, key.clone()),
        )
        .on_mouse_move(mouse_move(handle.clone(), binding, key.clone()));
    surface
        .child(paint::canvas(
            key.clone(),
            handle,
            binding_for_paint,
            decoded,
        ))
        .children(access)
        .into_any_element()
}

/// Returns the persistent focus handle owned by this scene's canvas.
fn scene_focus(view: &mut HostView, cx: &mut Context<HostView>, key: &str) -> FocusHandle {
    view.scene_focus
        .entry(key.to_owned())
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
        view::send(handle, Touch::key(binding.ordinal, key, down), window, cx);
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
        view.send(
            Touch::pointer(binding.ordinal, x, y, down, shift),
            window,
            cx,
        );
    });
}

/// Reopens the hover gate on the next display frame.
fn reset_hover(key: &str, window: &mut Window, cx: &mut Context<HostView>) {
    let key = key.to_owned();
    cx.on_next_frame(window, move |view, _, _| {
        view.scene_hover.remove(&key);
    });
}

/// Tells the application its scene's region is now `width` by `height` scene units, on the next
/// frame rather than from inside a paint.
pub(super) fn viewed(
    binding: &Binding,
    (width, height): (f32, f32),
    window: &mut Window,
    cx: &mut Context<HostView>,
) {
    let binding = *binding;
    cx.on_next_frame(window, move |view, window, cx| {
        let touch = Touch::viewed(binding.ordinal, f64::from(width), f64::from(height));
        view.send(touch, window, cx);
    });
}
