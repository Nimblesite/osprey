//! Stateful and clickable Osprey Native control mappings ([GPUI-HOST-RENDER]).

use super::*;
use crate::events::Touch;
use gpui_kit::component::{
    input::{Input, InputEvent, InputState},
    switch::Switch,
    Disableable as _, Selectable as _,
};

/// Defines a control entry point that validates its framework ordinal first.
macro_rules! renderer {
    ($name:ident, $bound:ident) => {
        pub(super) fn $name(ctx: &mut Ctx, node: &Tree) -> AnyElement {
            with_ordinal(ctx, ordinal(node), |ctx, ordinal| {
                $bound(ctx, node, ordinal)
            })
        }
    };
}

renderer!(button, bound_button);

/// Draws a button after its framework binding has been validated.
fn bound_button(ctx: &mut Ctx, node: &Tree, ordinal: i64) -> AnyElement {
    let handle = ctx.cx.entity();
    let binding = binding(ordinal);
    if let Some(flat) = flat_button(ctx, node, ordinal) {
        let on_click = move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            send(&handle, &binding, Tree::Null, window, cx)
        };
        return flat.on_click(on_click).into_any_element();
    }
    let styled = button_style(ctx, node, ordinal);
    let button = decorate(
        ctx,
        styled.label(string(node, "label")),
        string(node, "symbol"),
    );
    button
        .disabled(!flag(node, "enabled"))
        .on_click(move |_, window, cx| send(&handle, &binding, Tree::Null, window, cx))
        .into_any_element()
}

/// A plain button drawn as a flat GPUI div when the application's theme names `button` and
/// `buttonText` colours, padded by its third and first spacing steps; otherwise the kit's button.
fn flat_button(ctx: &Ctx, node: &Tree, ordinal: i64) -> Option<Stateful<Div>> {
    let theme = ctx.theme();
    let background = colour(theme, "button", ctx.dark)?;
    let text = colour(theme, "buttonText", ctx.dark)?;
    let enabled = flag(node, "enabled");
    let plain = !matches!(string(node, "role"), "prominent" | "destructive");
    let step = |index: usize| {
        px(theme
            .and_then(|theme| theme.spacing.get(index))
            .copied()
            .unwrap_or(0)
            .max(0) as f32)
    };
    (plain && enabled).then(|| {
        div()
            .id(control_id(ctx, ordinal))
            .child(string(node, "label").to_owned())
            .bg(background)
            .text_color(text)
            .active(|this| this.opacity(0.8))
            .flex()
            .px(step(2))
            .py(step(0))
    })
}

/// Resolves the button's presentation role without changing its binding.
fn button_style(ctx: &Ctx, node: &Tree, ordinal: i64) -> Button {
    match string(node, "role") {
        "prominent" => Button::new(control_id(ctx, ordinal)).primary(),
        "destructive" => Button::new(control_id(ctx, ordinal)).danger(),
        _ => Button::new(control_id(ctx, ordinal)),
    }
}

/// Adds a real button icon or the visible unknown-symbol placeholder.
fn decorate(ctx: &mut Ctx, button: Button, name: &str) -> Button {
    if name.is_empty() {
        return button;
    }
    match icon_name(name) {
        Some(icon) => button.icon(icon),
        None => button.child(missing_symbol(ctx, name)),
    }
}

renderer!(field, bound_field);

/// Draws a field after its framework binding has been validated.
fn bound_field(ctx: &mut Ctx, node: &Tree, ordinal: i64) -> AnyElement {
    if flag(node, "multiline") {
        ctx.view
            .diagnose_once("multiline field uses a single-line Input".to_owned());
    }
    let input = input(ctx, node, ordinal);
    labelled_column(node)
        .child(Input::new(&input).id(control_id(ctx, ordinal)).w_full())
        .into_any_element()
}

/// Returns the editor kept for this control number, made again when the application's text moved
/// under an idle field, which is the store's truth.
fn input(ctx: &mut Ctx, node: &Tree, ordinal: i64) -> Entity<InputState> {
    let value = string(node, "value").to_owned();
    let kept = ctx.view.inputs.get(&ordinal).cloned().filter(|input| {
        let state = input.read(ctx.cx);
        state.value().as_ref() == value || state.focus_handle(ctx.cx).is_focused(ctx.window)
    });
    if let Some(input) = kept {
        return input;
    }
    let input = new_input(ctx, node);
    subscribe(
        ctx.cx,
        ctx.window,
        &input,
        binding(ordinal),
        string(node, "commit").to_owned(),
    );
    ctx.view.inputs.insert(ordinal, input.clone());
    input
}

/// Creates an editor with the initial framework value and presentation.
fn new_input(ctx: &mut Ctx, node: &Tree) -> Entity<InputState> {
    let value = string(node, "value").to_owned();
    let prompt = string(node, "prompt").to_owned();
    let secure = flag(node, "secure");
    let window = &mut *ctx.window;
    ctx.cx.new(|cx| {
        InputState::new(window, cx)
            .default_value(value)
            .placeholder(prompt)
            .masked(secure)
    })
}

/// Forwards only Change for edit fields and PressEnter for submit fields.
fn subscribe(
    cx: &mut Context<HostView>,
    window: &Window,
    input: &Entity<InputState>,
    binding: Binding,
    commit: String,
) {
    cx.subscribe_in(
        input,
        window,
        move |view, input, event: &InputEvent, window, cx| {
            let send = matches!(
                (commit.as_str(), event),
                ("edit", InputEvent::Change) | ("submit", InputEvent::PressEnter { .. })
            );
            if send {
                let value = Tree::Text(input.read(cx).value().to_string());
                forward(view, &binding, &value, window, cx);
            }
        },
    )
    .detach();
}

renderer!(toggle, bound_toggle);

/// Draws a toggle after its framework binding has been validated.
fn bound_toggle(ctx: &mut Ctx, node: &Tree, ordinal: i64) -> AnyElement {
    let handle = ctx.cx.entity();
    let binding = binding(ordinal);
    Switch::new(control_id(ctx, ordinal))
        .label(string(node, "label"))
        .checked(flag(node, "on"))
        .on_click(move |on, window, cx| send(&handle, &binding, Tree::Bool(*on), window, cx))
        .into_any_element()
}

renderer!(rows, bound_rows);

/// Draws selectable rows without owning or changing the selection.
fn bound_rows(ctx: &mut Ctx, node: &Tree, ordinal: i64) -> AnyElement {
    let rows = items(ctx, node, "rows", ordinal, row);
    div().flex().flex_col().children(rows).into_any_element()
}

/// Draws one row and returns its framework key when clicked.
fn row(ctx: &mut Ctx, row: &Tree, selected: &str, ordinal: i64, index: usize) -> AnyElement {
    let handle = ctx.cx.entity();
    let binding = binding(ordinal);
    let key = string(row, "key").to_owned();
    let child = row.get("child").map_or(Tree::Null, Clone::clone);
    let child = node(ctx, &child);
    div()
        .id(format!("{}:{index}", control_id(ctx, ordinal)))
        .flex()
        .items_center()
        .gap_2()
        .p_2()
        .cursor_pointer()
        .when(key == selected, |this| this.bg(ctx.cx.theme().accent))
        .child(symbol(ctx, string(row, "symbol")))
        .child(child)
        .on_click(choose(handle, binding, key))
        .into_any_element()
}

renderer!(picker, bound_picker);

/// Uses simple choice buttons rather than introducing another stateful adapter.
fn bound_picker(ctx: &mut Ctx, node: &Tree, ordinal: i64) -> AnyElement {
    let choices = items(ctx, node, "choices", ordinal, choice);
    picker_row(ctx, node, ordinal, choices)
}

/// Draws the picker label and its validated choices, wrapping them in a narrow container.
fn picker_row(ctx: &Ctx, node: &Tree, ordinal: i64, choices: Vec<AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_wrap()
        .min_w_0()
        .items_center()
        .gap(gap(ctx))
        .child(picker_label(ctx, ordinal, string(node, "label")))
        .children(choices)
        .into_any_element()
}

/// Exposes the picker's real visible label to native accessibility clients ([UI-MACHINE-A11Y]).
fn picker_label(ctx: &Ctx, ordinal: i64, label: &str) -> AnyElement {
    div()
        .id(SharedString::from(control_id(ctx, ordinal) + ":label"))
        .role(Role::Label)
        .aria_label(label.to_owned())
        .child(Label::new(label))
        .into_any_element()
}

/// Draws one picker choice, announcing its selection and returning its framework key when clicked.
fn choice(ctx: &mut Ctx, choice: &Tree, selected: &str, ordinal: i64, index: usize) -> AnyElement {
    let handle = ctx.cx.entity();
    let binding = binding(ordinal);
    let key = string(choice, "key").to_owned();
    Button::new(format!("{}:{index}", control_id(ctx, ordinal)))
        .compact()
        .selected(key == selected)
        .aria_selected(key == selected)
        .min_w_0()
        .max_w_full()
        .label(string(choice, "label"))
        .on_click(choose(handle, binding, key))
        .into_any_element()
}

/// Applies one validated ordinal to a control renderer or rejects the node.
fn with_ordinal(
    ctx: &mut Ctx,
    ordinal: Option<i64>,
    render: impl FnOnce(&mut Ctx, i64) -> AnyElement,
) -> AnyElement {
    match ordinal {
        Some(ordinal) => render(ctx, ordinal),
        None => unbound(ctx.view, &ctx.key),
    }
}

/// Renders a keyed collection with one shared selection and binding ordinal.
fn items(
    ctx: &mut Ctx,
    node: &Tree,
    name: &str,
    ordinal: i64,
    render: impl Fn(&mut Ctx, &Tree, &str, i64, usize) -> AnyElement,
) -> Vec<AnyElement> {
    let selected = string(node, "selected").to_owned();
    list(node, name)
        .iter()
        .enumerate()
        .map(|(index, item)| render(ctx, item, &selected, ordinal, index))
        .collect()
}

/// Builds the shared row/picker callback that returns the selected framework key.
fn choose(
    handle: Entity<HostView>,
    binding: Binding,
    key: String,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| send(&handle, &binding, Tree::Text(key.clone()), window, cx)
}

/// Sends a captured value through the host's single dispatch function.
fn send(
    handle: &Entity<HostView>,
    binding: &Binding,
    value: Tree,
    window: &mut Window,
    cx: &mut App,
) {
    handle.update(cx, |view, cx| forward(view, binding, &value, window, cx));
}

/// Builds and dispatches one control event: its ordinal and the words it carries.
fn forward(
    view: &mut HostView,
    binding: &Binding,
    value: &Tree,
    window: &mut Window,
    cx: &mut Context<HostView>,
) {
    view.send(Touch::control(binding.ordinal, &value.words()), window, cx);
}
