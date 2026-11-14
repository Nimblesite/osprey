//! What a checked frame shows, read by the words a person reads ([UI-MACHINE-SELF-TEST]): the
//! control a label names, the choice a list offers, each scene's control number, and the
//! accessibility tree an agent reads. A frame that told components is read from their trees; a
//! frame of plain view rows is read from its rows, so a windowless step reaches either.

use gpui_kit::accesskit::Role;

use crate::events::Touch;
use crate::rows::Scene;
use crate::rows::{flags, Kind, Shown};
use crate::scene::access::Access;
use crate::tree::{self, Frame, Tree};

/// One checked frame as a person and an agent see it.
pub(crate) struct Screen<'a> {
    shown: &'a Shown,
    /// The components the frame told, or none for a frame of plain view rows.
    told: Option<Frame>,
}

impl<'a> Screen<'a> {
    /// Reads one checked frame.
    pub(crate) fn new(shown: &'a Shown) -> Self {
        Self {
            shown,
            told: tree::frame(shown).ok().flatten(),
        }
    }

    /// Every region's tree the frame told, in key order.
    fn regions(&self) -> Vec<&Tree> {
        self.told.as_ref().map_or_else(Vec::new, |frame| {
            frame.regions.iter().map(|(_, node)| node).collect()
        })
    }

    /// Whether the frame shows `words` exactly, as a control's name or a scene's lettering.
    pub(crate) fn shows(&self, words: &str) -> bool {
        self.read(words, true)
    }

    /// Whether any string the frame shows contains `words`.
    pub(crate) fn contains(&self, words: &str) -> bool {
        self.read(words, false)
    }

    /// Whether `matches` admits any string the frame shows.
    fn read(&self, words: &str, exact: bool) -> bool {
        let matches = |text: &str| match exact {
            true => text == words,
            false => text.contains(words),
        };
        // A told frame's rows encode its components, its region keys and its theme's token
        // names: only the drawn trees are on screen, never the raw rows.
        let rows = || {
            self.shown
                .controls
                .iter()
                .any(|control| matches(&control.words) || matches(&control.hint))
        };
        let shown = match &self.told {
            Some(_) => self.regions().into_iter().any(|node| words_in(node, &matches)),
            None => rows(),
        };
        shown
            || self
                .shown
                .scenes
                .values()
                .any(|scene| scene_words(scene, &matches))
    }

    /// The bound control `label` names, with whether it reads as chosen or on.
    pub(crate) fn control(&self, label: &str) -> Option<(i64, bool)> {
        self.regions()
            .into_iter()
            .find_map(|node| told_control(node, label))
            .or_else(|| self.row_control(label))
    }

    /// The first scene A step can send to, and the control number its canvas issues.
    pub(crate) fn scene(&self) -> Option<(i64, &Scene)> {
        if let Some((row, number)) = self.scene_record() {
            return Some((number, self.shown.scenes.get(&row)?));
        }
        let row = *self.shown.scenes.keys().min()?;
        let number = self.shown.controls.get(row)?.action?;
        Some((number, self.shown.scenes.get(&row)?))
    }

    /// The scene component's row and control number.
    fn scene_record(&self) -> Option<(usize, i64)> {
        let found = self
            .regions()
            .into_iter()
            .find_map(|node| find(node, &|node| tree::string(node, "kind") == "scene"))?;
        let row = usize::try_from(found.get("sceneRow").and_then(Tree::as_u64)?).ok()?;
        Some((row, tree::ordinal(found)?))
    }

    /// The scene element a screen reader announces as `label`, reached over its canvas's number.
    pub(crate) fn element(&self, label: &str) -> Option<(i64, &Access)> {
        let (number, scene) = self.scene()?;
        Some((
            number,
            scene.access.iter().find(|item| item.label == label)?,
        ))
    }

    /// The choice a bound list or picker offers under `words`: the event that chooses it.
    pub(crate) fn chooser(&self, words: &str) -> Option<Touch> {
        for exact in [true, false] {
            for field in ["rows", "choices"] {
                for node in self.regions() {
                    if let Some(send) = chose(node, field, words, exact) {
                        return Some(send);
                    }
                }
            }
        }
        self.row_chooser(words)
    }

    /// The plain row frame's chooser: a picker's option, or a row's key over its press.
    fn row_chooser(&self, words: &str) -> Option<Touch> {
        self.shown
            .controls
            .iter()
            .find_map(|owner| match owner.kind {
                Kind::Picker => picker(owner, self.shown, words),
                _ => listed(owner, self.shown, words),
            })
    }

    /// The bound row control `label` names, with its chosen flag.
    fn row_control(&self, label: &str) -> Option<(i64, bool)> {
        let chosen = |control: &crate::rows::Control| control.fit.flags & flags::CHOSEN != 0;
        let direct = self.shown.controls.iter().find_map(|control| {
            let named = control.words == label
                || (matches!(control.kind, Kind::Entry | Kind::Editor | Kind::Picker)
                    && control.hint == label);
            named
                .then_some(control)
                .and_then(|control| control.action.map(|number| (number, chosen(control))))
        });
        direct.or_else(|| {
            let heading = self
                .shown
                .controls
                .iter()
                .position(|control| control.kind == Kind::Label && control.words == label)?;
            self.shown
                .controls
                .iter()
                .skip(heading.saturating_add(1))
                .find_map(|control| {
                    matches!(control.kind, Kind::Entry | Kind::Editor)
                        .then(|| control.action.map(|number| (number, chosen(control))))
                        .flatten()
                })
        })
    }

    /// The accessibility tree as an agent reads it: one line per thing it can act on.
    pub(crate) fn outline(&self) -> String {
        let mut lines = Vec::new();
        match self.told() {
            true => {
                for node in self.regions() {
                    self.told_outline(node, &mut lines);
                }
            }
            false => self.row_outline(&mut lines),
        }
        lines.join("\n")
    }

    /// Whether the frame told components rather than only plain view rows.
    fn told(&self) -> bool {
        self.told.is_some()
    }

    /// One told component and everything inside it, as an agent reads it.
    fn told_outline(&self, node: &Tree, lines: &mut Vec<String>) {
        scene_access(node, self.shown, lines);
        if tree::ordinal(node).is_some() {
            let name = match tree::string(node, "label") {
                "" => tree::string(node, "title"),
                label => label,
            };
            let disabled = match node.get("enabled").and_then(Tree::as_bool) {
                Some(false) => " disabled",
                _ => "",
            };
            lines.push(format!(
                "{} \"{name}\"{disabled}",
                tree::string(node, "kind")
            ));
        } else if !tree::string(node, "text").is_empty() {
            lines.push(format!("text \"{}\"", tree::string(node, "text")));
        }
        for child in tree::inside(node) {
            self.told_outline(child, lines);
        }
    }

    /// One plain view row and its scene, as an agent reads it.
    fn row_outline(&self, lines: &mut Vec<String>) {
        for (row, control) in self.shown.controls.iter().enumerate() {
            if let Some(scene) = self.shown.scenes.get(&row) {
                lines.extend(scene.access.iter().map(announced));
            }
            if control.kind == Kind::Canvas || control.action.is_none() || control.kind.told() {
                continue;
            }
            lines.push(format!(
                "{} \"{}\"",
                role(control.kind),
                name(self.shown, row, control)
            ));
        }
    }
}

/// Whether any string inside a told value satisfies `matches`.
fn words_in(node: &Tree, matches: &dyn Fn(&str) -> bool) -> bool {
    match node {
        Tree::Text(text) => matches(text),
        Tree::Record(members) => members.values().any(|value| words_in(value, matches)),
        Tree::Items(items) => items.iter().any(|item| words_in(item, matches)),
        _ => false,
    }
}

/// Whether a scene announces or draws a string `matches` admits.
fn scene_words(scene: &Scene, matches: &dyn Fn(&str) -> bool) -> bool {
    use crate::scene::Shape;
    scene.access.iter().any(|item| matches(&item.label))
        || scene.shapes.iter().any(|shape| match shape {
            Shape::Glyphs { text, .. } | Shape::Lettering { text, .. } => matches(text.as_str()),
            _ => false,
        })
}

/// The bound told control named `label`, found depth first.
fn told_control(node: &Tree, label: &str) -> Option<(i64, bool)> {
    if let Some(number) = tree::ordinal(node) {
        let named = ["label", "title"]
            .iter()
            .any(|key| tree::string(node, key) == label);
        if named {
            return Some((number, tree::flag(node, "on")));
        }
    }
    tree::inside(node)
        .into_iter()
        .find_map(|child| told_control(child, label))
}

/// The first told node `wanted` admits, found depth first.
fn find<'a>(node: &'a Tree, wanted: &dyn Fn(&Tree) -> bool) -> Option<&'a Tree> {
    match wanted(node) {
        true => Some(node),
        false => tree::inside(node)
            .into_iter()
            .find_map(|child| find(child, wanted)),
    }
}

/// The event that chooses the item under `field` showing `words`.
fn chose(node: &Tree, field: &str, words: &str, exact: bool) -> Option<Touch> {
    if let Some(number) = tree::ordinal(node) {
        let suffix = format!(" · {words}");
        let wanted = |row: &&Tree| {
            let label = tree::string(row, "label");
            words_in(row, &|text| text.contains(words))
                && (!exact
                    || label == words
                    || label.ends_with(&suffix)
                    || tree::string(row, "key") == words)
        };
        if let Some(row) = tree::list(node, field).iter().find(wanted) {
            return Some(Touch::control(number, tree::string(row, "key")));
        }
    }
    tree::inside(node)
        .into_iter()
        .find_map(|child| chose(child, field, words, exact))
}

/// A picker's option under `words`: its key goes to the picker's own number.
fn picker(owner: &crate::rows::Control, shown: &Shown, words: &str) -> Option<Touch> {
    let number = owner.action?;
    owner.children.iter().find_map(|row| {
        let option = shown.controls.get(*row)?;
        (option.kind == Kind::Choice && option.words == words)
            .then(|| Touch::control(number, &option.hint))
    })
}

/// A row of a list under `words`: the row's key sits on its container, and the press inside it
/// delivers the key.
fn listed(owner: &crate::rows::Control, shown: &Shown, words: &str) -> Option<Touch> {
    let key = &owner.hint;
    if key.is_empty() || !holds(shown, &owner.children, words) {
        return None;
    }
    let (number, _) = press(shown, &owner.children)?;
    Some(Touch::control(number, key))
}

/// Whether any control inside `rows` announces `words`.
fn holds(shown: &Shown, rows: &[usize], words: &str) -> bool {
    rows.iter().any(|row| {
        shown
            .controls
            .get(*row)
            .is_some_and(|control| control.words == words || holds(shown, &control.children, words))
    })
}

/// The first press inside `rows` and the number it delivers.
fn press<'a>(shown: &'a Shown, rows: &[usize]) -> Option<(i64, &'a crate::rows::Control)> {
    rows.iter().find_map(|row| {
        let control = shown.controls.get(*row)?;
        match control.kind {
            Kind::Press => control.action.map(|number| (number, control)),
            _ => press(shown, &control.children),
        }
    })
}

/// One scene's accessibility tree, for the told record that draws it.
fn scene_access(node: &Tree, shown: &Shown, lines: &mut Vec<String>) {
    if tree::string(node, "kind") != "scene" {
        return;
    }
    let Some(row) = node
        .get("sceneRow")
        .and_then(Tree::as_u64)
        .and_then(|row| usize::try_from(row).ok())
    else {
        return;
    };
    if let Some(scene) = shown.scenes.get(&row) {
        lines.extend(scene.access.iter().map(announced));
    }
}

/// The name a screen reader announces for a plain row's control.
fn name<'a>(shown: &'a Shown, index: usize, control: &'a crate::rows::Control) -> &'a str {
    match control.kind {
        Kind::Entry | Kind::Editor => match heading(shown, index) {
            Some(words) => words,
            None if !control.hint.is_empty() => &control.hint,
            None => &control.words,
        },
        Kind::Picker if !control.hint.is_empty() => &control.hint,
        _ if !control.words.is_empty() => &control.words,
        _ => &control.hint,
    }
}

/// The heading lowering draws directly above a field, when the frame carries one.
fn heading(shown: &Shown, index: usize) -> Option<&str> {
    let above = shown.controls.get(index.checked_sub(1)?)?;
    (above.kind == Kind::Label && !above.words.is_empty()).then_some(above.words.as_str())
}

/// The role a plain row announces.
fn role(kind: Kind) -> &'static str {
    match kind {
        Kind::Column => "stack",
        Kind::Line => "line",
        Kind::Label => "label",
        Kind::Press => "button",
        Kind::Entry => "field",
        Kind::Gap => "gap",
        Kind::Scroll => "scroll",
        Kind::Layer => "layer",
        Kind::Toggle => "toggle",
        Kind::Picker => "picker",
        Kind::Choice => "option",
        Kind::Rule => "rule",
        Kind::Image => "image",
        Kind::Canvas => "scene",
        Kind::Editor => "editor",
        Kind::Menu => "menu",
        Kind::Sheet => "sheet",
        Kind::Field => "field",
        Kind::Record => "record",
        Kind::List => "list",
    }
}

/// One announced element, as an agent reads it.
fn announced(item: &Access) -> String {
    match item.role {
        Role::Slider => format!(
            "slider \"{}\" value {} of {} to {}",
            item.label, item.value, item.min, item.max
        ),
        Role::MenuItem => format!("menu item \"{}\"", item.label),
        Role::Button => format!("button \"{}\"", item.label),
        Role::Label => format!("label \"{}\"", item.label),
        _ => format!("element \"{}\"", item.label),
    }
}
