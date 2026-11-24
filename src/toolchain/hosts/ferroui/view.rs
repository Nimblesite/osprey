//! The node kinds of Osprey Native as FerroUI elements (docs/osprey-prism/README.md):
//! one mapping per kind, read from the told component tree of shared memory (`crate::tree`). A
//! kind this host does not draw yet is drawn as a visible box that names it and is reported; it
//! is never drawn as something else. `DRAWN` and `REFUSED` together are every kind the framework
//! tells, each once, which the tests check. A control's binding is just the `i64` ordinal the
//! framework issued; the words each control sends are what the framework's `told` decoder reads.

use std::cell::RefCell;
use std::rc::Rc;

use ferroui_base::input::Key as KeyName;
use ferroui_base::layout::{HorizontalAlignment, Orientation, VerticalAlignment};
use ferroui_base::media::{Color, FontFamily, FontStyle, FontWeight, TextWrapping};
use ferroui_base::utilities::UriRef;
use ferroui_base::{CornerRadius, Thickness};
use ferroui_controls::primitives::{SelectingItemsControl, ToggleButton};
use ferroui_controls::{Expander, TextBox};
use ferroui_view::element::with_key;
use ferroui_view::prelude::*;

use crate::host::{Command, Host, Menu};
use crate::theme::{self, Look};
use crate::tree::{self, Tree};

/// The node kinds this host draws, each with its mapping in `node` or `container`.
pub const DRAWN: &[&str] = &[
    "stack",
    "text",
    "button",
    "field",
    "slot",
    "spacer",
    "rich-text",
    "pair",
    "badge",
    "label",
    "icon",
    "link",
    "divider",
    "segments",
    "empty",
    "progress",
    "meter",
    "swatch",
    "toggle",
    "picker",
    "disclosure",
    "alert",
    "section",
    "screen",
    "split",
    "layer",
    "framed",
    "themed",
    "dialog",
    "context-menu",
    "aside",
    "tabs",
    "rows",
    "editor",
    "scene",
];

/// The node kinds this host draws as a box that names them, until it maps them.
pub const REFUSED: &[&str] = &["math", "image", "video", "board"];

/// What a render reads: the host, the look in force and the regions being embedded.
pub struct Cx<'s> {
    host: &'s Rc<Host>,
    look: Look,
    /// The regions being embedded, to refuse a slot cycle.
    stack: Vec<String>,
}

/// The element of the root region; a frame that told no components is drawn as an empty border.
pub fn root(host: &Rc<Host>, look: Look) -> Element {
    region(host, look, "root")
}

pub fn region(host: &Rc<Host>, look: Look, key: &str) -> Element {
    let told = host.regions().told();
    if !told {
        return border().into();
    }
    let mut cx = Cx {
        host,
        look,
        stack: Vec::new(),
    };
    embed(&mut cx, key)
}

/// The element of a region, in place of the slot that names it.
fn embed(cx: &mut Cx, key: &str) -> Element {
    if cx.stack.iter().any(|item| item == key) {
        return refused(cx, format!("slot cycle at region {key}"));
    }
    // The tree is cloned out before recursing, so no borrow of the host's regions is held there.
    let node = {
        let regions = cx.host.regions();
        let Some(region) = regions.get(key) else {
            return refused(cx, format!("missing region {key}"));
        };
        region.node.clone()
    };
    cx.stack.push(key.to_owned());
    let element = self::node(cx, &node);
    cx.stack.pop();
    // Keyed by region, so a region keeps its controls wherever its slot moves among siblings.
    with_key(element, format!("region:{key}").into())
}

/// Maps one node by its kind.
fn node(cx: &mut Cx, node: &Tree) -> Element {
    match tree::string(node, "kind") {
        "stack" => stack(cx, node),
        "text" => text_node(cx, node),
        "rich-text" => rich_text(cx, node),
        "button" => button_node(cx, node),
        "field" => field(cx, node),
        "slot" => embed(cx, tree::string(node, "key")),
        "spacer" => spacer(),
        "pair" => pair(cx, node),
        "badge" => badge(cx, node),
        "label" => label_node(cx, node),
        "icon" => icon(cx, node),
        "link" => link(cx, node),
        "divider" => separator()
            .margin(Thickness::new(0.0, 2.0, 0.0, 2.0))
            .into(),
        "segments" => segments(cx, node),
        "empty" => empty(cx, node),
        "progress" => progress(cx, node),
        "meter" => meter(cx, node),
        "swatch" => swatch(cx, node),
        "toggle" => toggle(cx, node),
        "picker" => picker(cx, node),
        kind => container(cx, kind, node),
    }
}

/// Maps the containers and the collections.
fn container(cx: &mut Cx, kind: &str, node: &Tree) -> Element {
    match kind {
        "disclosure" => disclosure(cx, node),
        "alert" => alert(cx, node),
        "section" => section(cx, node),
        "screen" => screen(cx, node),
        "split" => split(cx, node),
        "layer" => layer(cx, node),
        "framed" => framed(cx, node),
        "themed" => themed(cx, node),
        "dialog" => dialog(cx, node),
        "context-menu" => context_menu(cx, node),
        "aside" => aside(cx, node),
        "tabs" => tabs(cx, node),
        "rows" => rows(cx, node),
        "editor" => editor(cx, node),
        "scene" => scene_node(cx, node),
        kind => refused(
            cx,
            format!("node kind {kind} is not drawn by this host yet"),
        ),
    }
}

/// A column or a row of children, `gap` apart. A spacer among them takes the free space, so the
/// stack is then a grid whose spacers are star-sized.
fn stack(cx: &mut Cx, node: &Tree) -> Element {
    stacked(
        cx,
        tree::list(node, "children"),
        tree::string(node, "axis") == "horizontal",
    )
}

/// The children `items` as a stack (see `stack`).
fn stacked(cx: &mut Cx, items: &[Tree], horizontal: bool) -> Element {
    let gap = cx.look.gap();
    let children: Vec<Element> = items.iter().map(|child| self::node(cx, child)).collect();
    // A spacer takes the free space; so does a child that holds a scene, as the GPUI host passes
    // free space down to scenes.
    let grows: Vec<bool> = items
        .iter()
        .map(|child| grows(cx, child, horizontal))
        .collect();
    if !grows.contains(&true) {
        let orientation = if horizontal {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        };
        return stack_panel()
            .orientation(orientation)
            .spacing(gap)
            .children(children)
            .into();
    }
    let sizes: Vec<&str> = grows
        .iter()
        .map(|grows| if *grows { "*" } else { "Auto" })
        .collect();
    let placed: Vec<Element> = children
        .into_iter()
        .enumerate()
        .map(|(index, child)| {
            let cell = border().child(child);
            if horizontal {
                cell.grid_column(index as i32)
            } else {
                cell.grid_row(index as i32)
            }
            .into()
        })
        .collect();
    let grid = grid().children(placed);
    if horizontal {
        grid.column_definitions(&sizes.join(","))
            .column_spacing(gap)
            .into()
    } else {
        grid.row_definitions(&sizes.join(","))
            .row_spacing(gap)
            .into()
    }
}

/// Whether a stack gives this child its free space, as the GPUI host's flex layout does: a spacer,
/// a frame that grows, or a scene's holder unless a frame fixes its extent along the stack.
fn grows(cx: &Cx, child: &Tree, horizontal: bool) -> bool {
    let style = child.get("style").unwrap_or(&Tree::Null);
    let framed = tree::string(child, "kind") == "framed";
    let extent = tree::whole(style, if horizontal { "width" } else { "height" });
    match () {
        _ if tree::string(child, "kind") == "spacer" => true,
        _ if framed && tree::flag(style, "grow") => true,
        _ if framed && extent > 0 => false,
        _ => holds_scene(cx, child, 0),
    }
}

/// Whether a scene is below this node, through every member and slot, so its stacks give it the
/// free space.
fn holds_scene(cx: &Cx, node: &Tree, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    match tree::string(node, "kind") {
        "scene" => true,
        "slot" => {
            let region = {
                let regions = cx.host.regions();
                regions
                    .get(tree::string(node, "key"))
                    .map(|region| region.node.clone())
            };
            region.is_some_and(|node| holds_scene(cx, &node, depth + 1))
        }
        _ => tree::inside(node)
            .into_iter()
            .any(|child| holds_scene(cx, child, depth + 1)),
    }
}

/// Free space in a stack (see `stack`); alone it is an empty element.
fn spacer() -> Element {
    border().into()
}

/// Text in its role's size and weight and its tone's colour.
fn text_node(cx: &mut Cx, node: &Tree) -> Element {
    let role = tree::string(node, "role");
    let mut element = text_block()
        .text(tree::string(node, "text").to_owned())
        .text_wrapping(TextWrapping::Wrap);
    if let Some(size) = theme::role_size(role) {
        element = element.font_size(size);
    }
    if theme::role_bold(role) {
        element = element.font_weight(FontWeight::SemiBold);
    }
    if role == "mono" {
        element = element.font_family(FontFamily::from("Menlo, Consolas, monospace"));
    }
    if let Some(ink) = tone(cx, tree::string(node, "tone"), theme::role_tone(role)) {
        element = element.foreground(ink);
    }
    element.into()
}

/// The colour of a tone, else of the role's fallback token; `None` leaves FerroUI's colour.
fn tone(cx: &Cx, tone: &str, fallback: &str) -> Option<Color> {
    cx.look.colour(tone).or_else(|| cx.look.colour(fallback))
}

/// `panel` with the node's symbol added first, when the node names one.
fn with_symbol(cx: &mut Cx, panel: El<ferroui_controls::StackPanel>, node: &Tree) -> El<ferroui_controls::StackPanel> {
    match tree::string(node, "symbol") {
        "" => panel,
        name => panel.child(symbol(cx, name)),
    }
}

/// A symbol's glyph, or a box naming a symbol the framework does not define.
fn symbol(cx: &mut Cx, name: &str) -> Element {
    match crate::symbols::glyph(name) {
        Some(glyph) => text_block()
            .text(glyph)
            .vertical_alignment(VerticalAlignment::Center)
            .into(),
        None => refused(cx, format!("unknown symbol {name}")),
    }
}

/// A button: its label after its symbol, its role's look, enabled or not; a click sends a tap.
fn button_node(cx: &mut Cx, node: &Tree) -> Element {
    let Some(event) = tree::ordinal(node) else {
        return unbound(cx, "a button");
    };
    let label = tree::string(node, "label").to_owned();
    let name = tree::string(node, "symbol");
    let content: Element = if name.is_empty() {
        Element::Text(Rc::from(label.as_str()))
    } else {
        stack_panel()
            .orientation(Orientation::Horizontal)
            .spacing(6.0)
            .child(symbol(cx, name))
            .child(
                text_block()
                    .text(label)
                    .vertical_alignment(VerticalAlignment::Center),
            )
            .into()
    };
    let host = cx.host.clone();
    let mut element = button()
        .child(content)
        .is_enabled(tree::flag(node, "enabled"))
        .on_click(move |_| host.control(event, ""));
    match tree::string(node, "role") {
        "prominent" => {
            element = element.class("accent");
            if let Some(fill) = cx.look.colour("accent") {
                element = element.background(fill);
            }
        }
        "destructive" => {
            if let Some(ink) = cx.look.colour("danger") {
                element = element.foreground(ink);
            }
        }
        _ => {
            // A theme that names the colours of a plain button draws it with them.
            if let (Some(fill), Some(ink)) =
                (cx.look.colour("button"), cx.look.colour("buttonText"))
            {
                element = element.background(fill).foreground(ink);
            }
        }
    }
    element.into()
}

/// A labelled text field. `edit` sends every change; `submit` sends the text on Return.
fn field(cx: &mut Cx, node: &Tree) -> Element {
    let Some(event) = tree::ordinal(node) else {
        return unbound(cx, "a field");
    };
    let value = tree::string(node, "value").to_owned();
    let host = cx.host.clone();
    let mut input = text_box()
        .text(value.clone())
        .placeholder_text(tree::string(node, "prompt").to_owned())
        .accepts_return(tree::flag(node, "multiline"))
        .horizontal_alignment(HorizontalAlignment::Stretch);
    if tree::flag(node, "secure") {
        input = input.password_char('\u{2022}');
    }
    if tree::flag(node, "multiline") {
        input = input
            .text_wrapping(TextWrapping::Wrap)
            .min_height(64.0);
    }
    let typed = Rc::new(RefCell::new(value.clone()));
    let seen = typed.clone();
    let edit = tree::string(node, "commit") == "edit";
    let edit_host = host.clone();
    input = input.on_changed(TextBox::text_property(), move |text: &Option<String>| {
        let text = text.clone().unwrap_or_default();
        *seen.borrow_mut() = text.clone();
        if edit && text != value {
            edit_host.control(event, &text);
        }
    });
    if !edit {
        // A `submit` field sends its text on Return.
        input = input.on_key_down(move |press| {
            if press.key == KeyName::Return {
                host.control(event, &typed.borrow().clone());
            }
        });
    }
    let label = tree::string(node, "label");
    let mut column = stack_panel().spacing(4.0);
    if !label.is_empty() {
        column = column.child(text_block().text(label.to_owned()));
    }
    column.child(input).into()
}

/// Muted text: the theme's `secondary` colour, else FerroUI's text at lower opacity.
fn muted(cx: &Cx, text: &str) -> Element {
    let element = text_block()
        .text(text.to_owned())
        .font_size(12.0)
        .text_wrapping(TextWrapping::Wrap);
    match cx.look.colour("secondary") {
        Some(ink) => element.foreground(ink).into(),
        None => element.opacity(0.72).into(),
    }
}

/// The children of `member`, mapped in order.
fn children(cx: &mut Cx, node: &Tree, member: &str) -> Vec<Element> {
    tree::list(node, member)
        .iter()
        .map(|child| self::node(cx, child))
        .collect()
}

/// A column of mapped children `gap` apart.
fn column(cx: &mut Cx, node: &Tree, member: &str) -> Element {
    let gap = cx.look.gap();
    stack_panel()
        .spacing(gap)
        .children(children(cx, node, member))
        .into()
}

/// The theme colour of a semantic tone, else of the accent.
fn tone_or_accent(cx: &Cx, tone: &str) -> Option<Color> {
    cx.look.colour(tone).or_else(|| cx.look.colour("accent"))
}

/// A muted label and its value on one line (`pair`).
fn pair(cx: &mut Cx, node: &Tree) -> Element {
    stack_panel()
        .orientation(Orientation::Horizontal)
        .spacing(cx.look.gap())
        .child(muted(cx, tree::string(node, "label")))
        .child(text_block().text(tree::string(node, "value").to_owned()))
        .into()
}

/// A symbol and a label.
fn label_node(cx: &mut Cx, node: &Tree) -> Element {
    let gap = cx.look.gap();
    let row = with_symbol(cx, stack_panel().orientation(Orientation::Horizontal).spacing(gap), node);
    row.child(
        text_block()
            .text(tree::string(node, "label").to_owned())
            .vertical_alignment(VerticalAlignment::Center),
    )
    .into()
}

/// A large decorative symbol.
fn icon(cx: &mut Cx, node: &Tree) -> Element {
    match crate::symbols::glyph(tree::string(node, "symbol")) {
        Some(glyph) => text_block().text(glyph).font_size(28.0).into(),
        None => refused(
            cx,
            format!("unknown symbol {}", tree::string(node, "symbol")),
        ),
    }
}

/// A small rounded label in its tone's colour, its symbol before it.
fn badge(cx: &mut Cx, node: &Tree) -> Element {
    let content = with_symbol(cx, stack_panel().orientation(Orientation::Horizontal).spacing(4.0), node);
    let mut label = text_block()
        .text(tree::string(node, "label").to_owned())
        .font_size(12.0);
    let mut chip = border()
        .corner_radius(CornerRadius::uniform(9.0))
        .border_thickness(1.0)
        .padding(Thickness::new(8.0, 2.0, 8.0, 2.0))
        .horizontal_alignment(HorizontalAlignment::Left);
    match tone_or_accent(cx, tree::string(node, "tone")) {
        Some(tone) => {
            label = label.foreground(tone);
            chip = chip.border_brush(tone);
        }
        None => chip = chip.border_brush(Color::from_argb(0x80, 0x80, 0x80, 0x80)),
    }
    chip.child(content.child(label)).into()
}

/// A labelled `https` page the system opens; any other address is refused.
fn link(cx: &mut Cx, node: &Tree) -> Element {
    let url = tree::string(node, "url");
    let parsed = url
        .starts_with("https://")
        .then(|| UriRef::parse(url).ok())
        .flatten();
    match parsed {
        Some(uri) => hyperlink_button()
            .navigate_uri(Some(uri))
            .child(tree::string(node, "label").to_owned())
            .into(),
        None => refused(
            cx,
            format!("a link to {url}, which is not an https address"),
        ),
    }
}

/// A strip of small blocks in their tones' colours, and its label.
fn segments(cx: &mut Cx, node: &Tree) -> Element {
    let blocks: Vec<Element> = tree::list(node, "tones")
        .iter()
        .map(|tone| {
            let fill = tone_or_accent(cx, tone.as_str().unwrap_or(""))
                .unwrap_or(Color::from_argb(0x80, 0x80, 0x80, 0x80));
            border()
                .width(16.0)
                .height(8.0)
                .corner_radius(CornerRadius::uniform(2.0))
                .background(fill)
                .into()
        })
        .collect();
    stack_panel()
        .orientation(Orientation::Horizontal)
        .spacing(cx.look.gap())
        .child(
            stack_panel()
                .orientation(Orientation::Horizontal)
                .spacing(2.0)
                .children(blocks),
        )
        .child(muted(cx, tree::string(node, "label")))
        .into()
}

/// A centred empty state: its symbol, title and detail.
fn empty(cx: &mut Cx, node: &Tree) -> Element {
    let mut panel = stack_panel()
        .spacing(cx.look.gap())
        .horizontal_alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Center);
    if !tree::string(node, "symbol").is_empty() {
        panel = panel.child(icon(cx, node));
    }
    panel
        .child(
            text_block()
                .text(tree::string(node, "title").to_owned())
                .font_weight(FontWeight::SemiBold),
        )
        .child(muted(cx, tree::string(node, "detail")))
        .into()
}

/// An indeterminate progress bar under its label and detail.
fn progress(cx: &mut Cx, node: &Tree) -> Element {
    stack_panel()
        .spacing(4.0)
        .child(text_block().text(tree::string(node, "label").to_owned()))
        .child(muted(cx, tree::string(node, "detail")))
        .child(progress_bar().is_indeterminate(true))
        .into()
}

/// A bar filled to `value` of `total` in its tone; the framework clamped both.
fn meter(cx: &mut Cx, node: &Tree) -> Element {
    let total = tree::whole(node, "total").max(1) as f64;
    let value = tree::whole(node, "value").max(0) as f64;
    let mut bar = progress_bar()
        .minimum(0.0)
        .maximum(total)
        .value(value.min(total))
        .min_width(120.0);
    if let Some(tone) = tone_or_accent(cx, tree::string(node, "tone")) {
        bar = bar.foreground(tone);
    }
    bar.into()
}

/// A colour in the current appearance beside its name.
fn swatch(cx: &mut Cx, node: &Tree) -> Element {
    let shown = if cx.look.dark {
        tree::string(node, "dark")
    } else {
        tree::string(node, "light")
    };
    let Some(fill) = theme::hex(shown) else {
        return refused(cx, format!("a swatch has the colour {shown}"));
    };
    stack_panel()
        .orientation(Orientation::Horizontal)
        .spacing(cx.look.gap())
        .child(
            border()
                .width(16.0)
                .height(16.0)
                .corner_radius(CornerRadius::uniform(3.0))
                .background(fill)
                .border_thickness(1.0)
                .border_brush(Color::from_argb(0x40, 0x80, 0x80, 0x80)),
        )
        .child(
            text_block()
                .text(tree::string(node, "name").to_owned())
                .vertical_alignment(VerticalAlignment::Center),
        )
        .into()
}

/// A switch with its label; a change sends the new state.
fn toggle(cx: &mut Cx, node: &Tree) -> Element {
    let Some(event) = tree::ordinal(node) else {
        return unbound(cx, "a toggle");
    };
    let on = tree::flag(node, "on");
    let host = cx.host.clone();
    toggle_switch()
        .child(tree::string(node, "label").to_owned())
        .is_checked(Some(on))
        .on_changed(
            ToggleButton::is_checked_property(),
            move |checked: &Option<bool>| {
                let checked = checked.unwrap_or(false);
                if checked != on {
                    host.control(event, if checked { "true" } else { "false" });
                }
            },
        )
        .into()
}

/// Each item's key, in order, as a choice control holds them.
fn keys_of<'a>(items: impl Iterator<Item = &'a Tree>) -> Vec<Option<String>> {
    items.map(|item| Some(tree::string(item, "key").to_owned())).collect()
}

/// The index of the key `selected` among `keys`, or -1.
fn index_of(keys: &[Option<String>], selected: &str) -> i32 {
    keys.iter()
        .position(|key| key.as_deref() == Some(selected))
        .map_or(-1, |index| index as i32)
}

/// The node's title at `size` in a semibold weight.
fn title(node: &Tree, size: f64) -> El<ferroui_controls::TextBlock> {
    text_block()
        .text(tree::string(node, "title").to_owned())
        .font_size(size)
        .font_weight(FontWeight::SemiBold)
}

/// A trailing button that sends one tap of `event`, dismissing the surface `label` names.
fn dismissing(label: &str, host: Rc<Host>, event: i64) -> El<ferroui_controls::Button> {
    button()
        .child(label.to_owned())
        .horizontal_alignment(HorizontalAlignment::Right)
        .on_click(move |_| host.control(event, ""))
}

/// Sends the key at a newly selected index of a choice control, when it is not the selected one.
fn on_choice(
    host: Rc<Host>,
    event: i64,
    keys: Vec<Option<String>>,
    selected: String,
) -> impl Fn(&i32) + 'static {
    move |index: &i32| {
        let Some(Some(key)) = usize::try_from(*index)
            .ok()
            .and_then(|index| keys.get(index))
        else {
            return;
        };
        if *key != selected {
            host.control(event, key);
        }
    }
}

/// The choice keys of `items`, and the key the node's `selected` member names.
fn choices_of<'a>(
    items: impl Iterator<Item = &'a Tree>,
    node: &Tree,
) -> (Vec<Option<String>>, String) {
    (keys_of(items), tree::string(node, "selected").to_owned())
}

/// A choice control's selection: the index of its selected key, and its change handler.
fn choosing(
    host: &Rc<Host>,
    event: i64,
    keys: Vec<Option<String>>,
    selected: String,
) -> (i32, impl Fn(&i32) + 'static) {
    let index = index_of(&keys, &selected);
    (index, on_choice(host.clone(), event, keys, selected))
}

/// A labelled drop-down of choices; choosing one sends its key.
fn picker(cx: &mut Cx, node: &Tree) -> Element {
    let Some(event) = tree::ordinal(node) else {
        return unbound(cx, "a picker");
    };
    let choices = tree::list(node, "choices");
    let (keys, selected) = choices_of(choices.iter(), node);
    let labels: Vec<Element> = choices
        .iter()
        .map(|choice| Element::Text(Rc::from(tree::string(choice, "label"))))
        .collect();
    let (index, changed) = choosing(cx.host, event, keys, selected);
    let mut row = stack_panel()
        .orientation(Orientation::Horizontal)
        .spacing(cx.look.gap());
    if !tree::string(node, "label").is_empty() {
        row = row.child(
            text_block()
                .text(tree::string(node, "label").to_owned())
                .vertical_alignment(VerticalAlignment::Center),
        );
    }
    row.child(
        combo_box()
            .children(labels)
            .selected_index(index)
            .min_width(120.0)
            .on_changed(SelectingItemsControl::selected_index_property(), changed),
    )
    .into()
}

/// A titled section that expands to its children; expanding or collapsing sends the new state.
fn disclosure(cx: &mut Cx, node: &Tree) -> Element {
    let Some(event) = tree::ordinal(node) else {
        return unbound(cx, "a disclosure");
    };
    let expanded = tree::flag(node, "expanded");
    let host = cx.host.clone();
    let body = column(cx, node, "children");
    expander()
        .header(text_block().text(tree::string(node, "title").to_owned()))
        .is_expanded(expanded)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .child(body)
        .on_changed(Expander::is_expanded_property(), move |now: &bool| {
            if *now != expanded {
                host.control(event, if *now { "true" } else { "false" });
            }
        })
        .into()
}

/// A card with an outline, padding and rounded corners: sections, alerts and dialogs.
fn card(cx: &Cx, line: &str) -> El<ferroui_controls::Border> {
    let edge = cx
        .look
        .colour(line)
        .unwrap_or(Color::from_argb(0x40, 0x80, 0x80, 0x80));
    border()
        .border_thickness(1.0)
        .border_brush(edge)
        .corner_radius(CornerRadius::uniform(6.0))
        .padding(Thickness::uniform(12.0))
}

/// A stack at the theme's gap, led by the node's title in a semibold weight.
fn titled(cx: &Cx, node: &Tree) -> El<ferroui_controls::StackPanel> {
    stack_panel()
        .spacing(cx.look.gap())
        .child(
            text_block()
                .text(tree::string(node, "title").to_owned())
                .font_weight(FontWeight::SemiBold),
        )
}

/// While visible, the alert's title and message and a button that dismisses it (one tap).
fn alert(cx: &mut Cx, node: &Tree) -> Element {
    let Some(event) = tree::ordinal(node) else {
        return unbound(cx, "an alert");
    };
    if !tree::flag(node, "visible") {
        return Element::Empty;
    }
    let host = cx.host.clone();
    card(cx, "caution")
        .child(
            titled(cx, node)
                .child(
                    text_block()
                        .text(tree::string(node, "message").to_owned())
                        .text_wrapping(TextWrapping::Wrap),
                )
                .child(dismissing("OK", host, event)),
        )
        .into()
}

/// A titled group: its title, children and footer caption.
fn section(cx: &mut Cx, node: &Tree) -> Element {
    let mut panel = titled(cx, node).children(children(cx, node, "children"));
    if !tree::string(node, "footer").is_empty() {
        panel = panel.child(muted(cx, tree::string(node, "footer")));
    }
    card(cx, "separator").child(panel).into()
}

/// A screen: its title and subtitle with the toolbar at the trailing edge, above its body.
fn screen(cx: &mut Cx, node: &Tree) -> Element {
    let gap = cx.look.gap();
    let mut heading = stack_panel().grid_column(0).child(title(node, 20.0));
    if !tree::string(node, "subtitle").is_empty() {
        heading = heading.child(muted(cx, tree::string(node, "subtitle")));
    }
    let toolbar = stack_panel()
        .grid_column(1)
        .orientation(Orientation::Horizontal)
        .spacing(gap)
        .vertical_alignment(VerticalAlignment::Center)
        .children(children(cx, node, "toolbar"));
    let body = stacked(cx, tree::list(node, "children"), false);
    grid()
        .row_definitions("Auto,*")
        .row_spacing(gap)
        .child(
            grid()
                .grid_row(0)
                .column_definitions("*,Auto")
                .child(heading)
                .child(toolbar),
        )
        .child(border().grid_row(1).child(body))
        .into()
}

/// Panes side by side, equal at first, with splitters between them.
fn split(cx: &mut Cx, node: &Tree) -> Element {
    let panes = children(cx, node, "children");
    let count = panes.len();
    let mut columns = Vec::new();
    let mut cells: Vec<Element> = Vec::new();
    for (index, pane) in panes.into_iter().enumerate() {
        if index > 0 {
            columns.push("Auto");
            cells.push(
                grid_splitter()
                    .grid_column((columns.len() - 1) as i32)
                    .width(4.0)
                    .into(),
            );
        }
        columns.push("*");
        cells.push(
            border()
                .grid_column((columns.len() - 1) as i32)
                .child(pane)
                .into(),
        );
    }
    if count == 0 {
        return Element::Empty;
    }
    grid()
        .column_definitions(&columns.join(","))
        .children(cells)
        .into()
}

/// Children drawn over each other, the first at the back.
fn layer(cx: &mut Cx, node: &Tree) -> Element {
    grid().children(children(cx, node, "children")).into()
}

/// The body in the frame's geometry and type, as the GPUI host's `framed` draws it
/// ([PRISM-ROUTING-PRESENTATION]): its width, height, grow, insets and text size. A frame sized
/// to the window or made to scroll was refused whole by `tree::frame`; the control metrics a
/// frame carries are not applied: FerroUI's controls keep their theme's metrics.
fn framed(cx: &mut Cx, node: &Tree) -> Element {
    let style = node.get("style").unwrap_or(&Tree::Null);
    let body = match node.get("body") {
        Some(body) => self::node(cx, body),
        None => Element::Empty,
    };
    let edge = |value: i64| value.max(0) as f64;
    let mut frame = border().padding(Thickness::new(
        edge(tree::whole(style, "left")),
        edge(tree::whole(style, "top")),
        edge(tree::whole(style, "right")),
        edge(tree::whole(style, "bottom")),
    ));
    if tree::whole(style, "width") > 0 {
        frame = frame.width(tree::whole(style, "width") as f64);
    }
    if tree::whole(style, "height") > 0 {
        frame = frame.height(tree::whole(style, "height") as f64);
    }
    if tree::flag(style, "grow") {
        frame = frame
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .vertical_alignment(VerticalAlignment::Stretch);
    }
    // The frame's type reaches the text inside it through a content control, whose font its
    // descendants inherit.
    let mut typed = content_control()
        .horizontal_content_alignment(HorizontalAlignment::Stretch)
        .vertical_content_alignment(VerticalAlignment::Stretch)
        .child(body);
    if tree::whole(style, "textSize") > 0 {
        typed = typed.font_size(tree::whole(style, "textSize") as f64);
    }
    frame.child(typed).into()
}

/// The body with its own theme: its colours and spacing apply below it, on its background.
fn themed(cx: &mut Cx, node: &Tree) -> Element {
    let Some(theme) = crate::tree::Theme::told(node.get("theme")) else {
        return refused(cx, "a themed node without a valid theme".to_owned());
    };
    let dark = cx.look.dark;
    let outer = std::mem::replace(
        &mut cx.look,
        Look {
            theme: Some(Rc::new(theme)),
            dark,
        },
    );
    let gap = cx.look.gap();
    let fill = cx.look.colour("background");
    let body = match node.get("body") {
        Some(body) => self::node(cx, body),
        None => Element::Empty,
    };
    let mut surface = border().padding(Thickness::uniform(gap * 3.0)).child(body);
    if let Some(fill) = fill {
        surface = surface.background(fill);
    }
    cx.look = outer;
    surface.into()
}

/// A modal surface drawn in place: its title, its body and a button that dismisses it (a tap).
fn dialog(cx: &mut Cx, node: &Tree) -> Element {
    let Some(event) = tree::ordinal(node) else {
        return unbound(cx, "a dialog");
    };
    let host = cx.host.clone();
    let body = match node.get("body") {
        Some(body) => self::node(cx, body),
        None => Element::Empty,
    };
    let mut surface = card(cx, "separator");
    if let Some(fill) = cx.look.colour("background") {
        surface = surface.background(fill);
    }
    surface
        .child(
            stack_panel()
                .spacing(cx.look.gap())
                .child(title(node, 17.0))
                .child(body)
                .child(dismissing("Close", host, event)),
        )
        .into()
}

/// A section among a context menu's actions as one menu of the window's menu bar: its title,
/// and its buttons as commands, each button's symbol naming its shortcut and a prominent button
/// being the current choice.
fn window_menu(section: &Tree) -> Menu {
    let commands = tree::list(section, "children")
        .iter()
        .filter(|item| tree::string(item, "kind") == "button")
        .filter_map(|item| {
            Some(Command {
                label: tree::string(item, "label").to_owned(),
                keys: tree::string(item, "symbol").to_owned(),
                enabled: tree::flag(item, "enabled"),
                checked: tree::string(item, "role") == "prominent",
                event: tree::ordinal(item)?,
            })
        })
        .collect();
    Menu {
        title: tree::string(section, "title").to_owned(),
        commands,
    }
}

/// The window menus a context menu node names: its sections, in order.
pub fn window_menus(node: &Tree) -> Vec<Menu> {
    tree::list(node, "items")
        .iter()
        .filter(|item| tree::string(item, "kind") == "section")
        .map(window_menu)
        .collect()
}

/// The body with a context menu of its actions: each button among them is a menu item that
/// sends its tap, each section is a menu of the window's menu bar, and an action of another
/// kind is refused.
fn context_menu(cx: &mut Cx, node: &Tree) -> Element {
    let body = match node.get("body") {
        Some(body) => self::node(cx, body),
        None => Element::Empty,
    };
    window_menus(node)
        .into_iter()
        .for_each(|menu| cx.host.gather(menu));
    let mut items: Vec<Element> = Vec::new();
    for item in tree::list(node, "items") {
        if tree::string(item, "kind") == "section" {
            continue;
        }
        if tree::string(item, "kind") != "button" {
            refused(
                cx,
                format!(
                    "a context menu action of kind {}",
                    tree::string(item, "kind")
                ),
            );
            continue;
        }
        let Some(event) = tree::ordinal(item) else {
            continue;
        };
        let host = cx.host.clone();
        items.push(
            menu_item()
                .header(text_block().text(tree::string(item, "label").to_owned()))
                .is_enabled(tree::flag(item, "enabled"))
                .on_click(move |_| host.control(event, ""))
                .into(),
        );
    }
    if items.is_empty() {
        return body;
    }
    border()
        .context_menu(ferroui_view::prelude::context_menu().children(items))
        .child(body)
        .into()
}

/// Two columns with a splitter: the pane, and the aside at its leading or trailing edge, its
/// width within `least` and `most`; dragging the splitter sends the aside's new width.
fn aside(cx: &mut Cx, node: &Tree) -> Element {
    let Some(event) = tree::ordinal(node) else {
        return unbound(cx, "an aside");
    };
    let width = tree::whole(node, "width").max(0) as f64;
    let pane = match node.get("pane") {
        Some(pane) => self::node(cx, pane),
        None => Element::Empty,
    };
    let beside = match node.get("aside") {
        Some(beside) => self::node(cx, beside),
        None => Element::Empty,
    };
    let leading = tree::string(node, "side") == "leading";
    let (aside_column, pane_column) = if leading { (0, 2) } else { (2, 0) };
    let host = cx.host.clone();
    let sized = move |e: &ferroui_controls::SizeChangedEventArgs| {
        let now = e.new_size().width.round();
        if (now - width).abs() >= 1.0 {
            host.control(event, &(now as i64).to_string());
        }
    };
    let least = tree::whole(node, "least").max(0) as f64;
    let most = if tree::whole(node, "most") > 0 {
        tree::whole(node, "most") as f64
    } else {
        f64::INFINITY
    };
    let columns = if leading {
        format!("{width},Auto,*")
    } else {
        format!("*,Auto,{width}")
    };
    grid()
        .column_definitions(&columns)
        .child(border().grid_column(pane_column).child(pane))
        .child(grid_splitter().grid_column(1).width(4.0))
        .child(
            border()
                .grid_column(aside_column)
                .min_width(least)
                .max_width(most)
                .on_size_changed(sized)
                .child(scroll_viewer().child(beside)),
        )
        .into()
}

/// Tabs: one tab item per entry, its label as the header and its content below; choosing a tab
/// sends its key.
fn tabs(cx: &mut Cx, node: &Tree) -> Element {
    let Some(event) = tree::ordinal(node) else {
        return refused(cx, "tabs have no event ordinal".to_owned());
    };
    let entries = tree::list(node, "tabs");
    let (keys, selected) = choices_of(entries.iter(), node);
    let items: Vec<Element> = entries
        .iter()
        .map(|entry| {
            let content = match entry.get("child") {
                Some(child) => self::node(cx, child),
                None => Element::Empty,
            };
            tab_item()
                .key(format!("tab:{}", tree::string(entry, "key")))
                .header(text_block().text(tree::string(entry, "label").to_owned()))
                .child(content)
                .into()
        })
        .collect();
    let (index, changed) = choosing(cx.host, event, keys, selected);
    tab_control()
        .children(items)
        .selected_index(index)
        .on_changed(SelectingItemsControl::selected_index_property(), changed)
        .into()
}

/// Selectable rows: one list item per row (its symbol, then its content), with a header where
/// the group changes; choosing a row sends its key.
fn rows(cx: &mut Cx, node: &Tree) -> Element {
    let Some(event) = tree::ordinal(node) else {
        return refused(cx, "rows have no event ordinal".to_owned());
    };
    let selected = tree::string(node, "selected").to_owned();
    let mut keys: Vec<Option<String>> = Vec::new();
    let mut items: Vec<Element> = Vec::new();
    let mut group = String::new();
    for row in tree::list(node, "rows") {
        let this_group = tree::string(row, "group");
        if !this_group.is_empty() && this_group != group {
            group = this_group.to_owned();
            keys.push(None);
            items.push(
                list_box_item()
                    .key(format!("group:{group}"))
                    .is_enabled(false)
                    .child(muted(cx, &group))
                    .into(),
            );
        }
        let content = match row.get("child") {
            Some(child) => self::node(cx, child),
            None => Element::Empty,
        };
        let gap = cx.look.gap();
        let line = with_symbol(cx, stack_panel().orientation(Orientation::Horizontal).spacing(gap), row);
        keys.push(Some(tree::string(row, "key").to_owned()));
        items.push(
            list_box_item()
                .key(format!("row:{}", tree::string(row, "key")))
                .child(line.child(content))
                .into(),
        );
    }
    let (index, changed) = choosing(cx.host, event, keys, selected);
    list_box()
        .children(items)
        .selected_index(index)
        .on_changed(SelectingItemsControl::selected_index_property(), changed)
        .into()
}

/// A multi-line editor: its text goes to its `event` ordinal, its caret to `placeEvent`. The
/// marks of an editor (underlines, badges, notes by line) are not drawn yet.
fn editor(cx: &mut Cx, node: &Tree) -> Element {
    let (Some(edit), Some(place)) = (
        tree::ordinal(node),
        node.get("placeEvent").and_then(Tree::as_i64),
    ) else {
        return refused(cx, "an editor has no event ordinals".to_owned());
    };
    let value = tree::string(node, "value").to_owned();
    let cursor = tree::whole(node, "cursor");
    let (edit_host, place_host) = (cx.host.clone(), cx.host.clone());
    let inline = tree::flag(node, "inline");
    let mut input = text_box()
        .text(value.clone())
        .caret_index(cursor.clamp(0, i32::MAX as i64) as i32)
        .accepts_return(!inline)
        .accepts_tab(!inline)
        .font_family(FontFamily::from("Menlo, Consolas, monospace"))
        .on_changed(TextBox::text_property(), move |text: &Option<String>| {
            let text = text.clone().unwrap_or_default();
            if text != value {
                edit_host.control(edit, &text);
            }
        })
        .on_changed(TextBox::caret_index_property(), move |at: &i32| {
            if i64::from(*at) != cursor {
                place_host.control(place, &at.to_string());
            }
        });
    if !inline {
        input = input.min_height(120.0);
    }
    let mut panel = stack_panel().spacing(4.0);
    if !tree::string(node, "label").is_empty() {
        panel = panel.child(text_block().text(tree::string(node, "label").to_owned()));
    }
    panel.child(input).into()
}

/// A paragraph of runs: each run's ink, italics and emphasis (`strong`, `code`).
fn rich_text(cx: &mut Cx, node: &Tree) -> Element {
    let role = tree::string(node, "role");
    let size = theme::role_size(role);
    let runs: Vec<Element> = tree::list(node, "runs")
        .iter()
        .map(|run| {
            let mut piece = text_block().text(tree::string(run, "text").to_owned());
            if let Some(size) = size {
                piece = piece.font_size(size);
            }
            if let Some(ink) = cx
                .look
                .colour(tree::string(run, "ink"))
                .or_else(|| cx.look.colour(theme::role_tone(role)))
            {
                piece = piece.foreground(ink);
            }
            if tree::flag(run, "italic") {
                piece = piece.font_style(FontStyle::Italic);
            }
            match tree::string(run, "emphasis") {
                "strong" => piece = piece.font_weight(FontWeight::SemiBold),
                "code" => piece = piece.font_family(FontFamily::from("Menlo, Consolas, monospace")),
                _ => {}
            }
            piece.into()
        })
        .collect();
    wrap_panel().children(runs).into()
}

/// A scene: the surface control that draws the scene row the frame gave this node
/// (`sceneRow`), its input going back through its `event` ordinal; a missing row is refused.
fn scene_node(cx: &mut Cx, node: &Tree) -> Element {
    let Some(ordinal) = tree::ordinal(node) else {
        return unbound(cx, "a scene");
    };
    let Some(row) = node
        .get("sceneRow")
        .and_then(Tree::as_u64)
        .and_then(|row| usize::try_from(row).ok())
    else {
        return refused(cx, "a scene node without its scene row".to_owned());
    };
    let shown = cx.host.shown();
    match shown.scenes.get(&row) {
        Some(scene) => crate::surface::scene_element(cx.host, row, ordinal, scene),
        None => refused(cx, format!("the scene at row {row} streamed no scene")),
    }
}

/// Rejects a control that lacks the event ordinal it must return.
fn unbound(cx: &mut Cx, control: &str) -> Element {
    refused(cx, format!("{control} has no event ordinal"))
}

/// A part this host cannot draw: a box that names it, and a diagnostic.
fn refused(cx: &mut Cx, message: String) -> Element {
    cx.host.refuse(message.clone());
    let ink = Color::from_argb(0xFF, 0xC4, 0x2B, 0x1C);
    border()
        .border_brush(ink)
        .border_thickness(1.0)
        .padding(Thickness::uniform(6.0))
        .child(text_block().text(message).foreground(ink).font_size(12.0))
        .into()
}

#[cfg(test)]
mod tests {
    use super::{DRAWN, REFUSED};

    const SOURCE: &str = include_str!("view.rs");

    /// Every kind the framework tells, in the wire names of its `Tell::walk`
    /// (src/osprey/packages/native/src/told.ospml).
    const TOLD: &[&str] = &[
        "rich-text", "text", "pair", "badge", "label", "icon", "link", "spacer", "divider",
        "segments", "empty", "progress", "meter", "swatch", "slot", "button", "field", "toggle",
        "picker", "disclosure", "alert", "stack", "section", "screen", "split", "layer",
        "framed", "themed", "dialog", "context-menu", "aside", "tabs", "rows", "math", "image",
        "video", "editor", "board", "scene",
    ];

    #[test]
    fn every_told_kind_is_drawn_or_refused_exactly_once() {
        let mut told: Vec<&str> = TOLD.to_vec();
        told.sort_unstable();
        let mut declared: Vec<&str> = DRAWN.iter().chain(REFUSED).copied().collect();
        declared.sort_unstable();
        assert_eq!(
            declared, told,
            "DRAWN and REFUSED must name every kind the framework tells once"
        );
    }

    #[test]
    fn every_drawn_kind_has_its_mapping_and_no_refused_kind_has_one() {
        for kind in DRAWN {
            assert!(
                SOURCE.contains(&format!("\"{kind}\" =>")),
                "view.rs maps no {kind}"
            );
        }
        for kind in REFUSED {
            assert!(
                !SOURCE.contains(&format!("\"{kind}\" =>")),
                "view.rs maps {kind}: move it to DRAWN"
            );
        }
    }
}
