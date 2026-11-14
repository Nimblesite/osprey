//! [NATIVE-SHM-NODES] on the host: the component tree the application told, read from the checked
//! rows of the shared frame. A `field` row is one typed value, a `record` row a component or one of
//! its members and a `list` row the items under one name. Nothing here is parsed from text: every
//! value is a row the frame check already accepted. The renderer reads these values with the same
//! accessors main's JSON renderer used, so its code is unchanged.

use std::collections::BTreeMap;

use crate::rows::{Control, Kind, Shown};

/// One told value: nothing, a flag, a whole number, a real, text, a list of items or a record of
/// named members.
#[derive(Debug, Clone, PartialEq)]
pub enum Tree {
    Null,
    Bool(bool),
    Whole(i64),
    Real(f64),
    Text(String),
    Items(Vec<Tree>),
    Record(BTreeMap<String, Tree>),
}

impl Tree {
    /// The member `name` of a record.
    pub(crate) fn get(&self, name: &str) -> Option<&Tree> {
        self.object()?.get(name)
    }

    /// This value read as text, when it is text.
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// This value read as a flag, when it is a flag.
    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(on) => Some(*on),
            _ => None,
        }
    }

    /// This value read as a whole number, when it is one.
    pub(crate) fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Whole(whole) => Some(*whole),
            _ => None,
        }
    }

    /// This value read as a whole number above zero, when it is one.
    pub(crate) fn as_u64(&self) -> Option<u64> {
        u64::try_from(self.as_i64()?).ok()
    }

    /// Every item of a list, when this value is a list.
    pub(crate) fn as_array(&self) -> Option<&Vec<Tree>> {
        match self {
            Self::Items(items) => Some(items),
            _ => None,
        }
    }

    /// Every member of a record, when this value is a record.
    pub(crate) fn object(&self) -> Option<&BTreeMap<String, Tree>> {
        match self {
            Self::Record(members) => Some(members),
            _ => None,
        }
    }

    /// The words a control event carries for this value: nothing, text, a flag or a whole number.
    pub(crate) fn words(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Bool(on) => (*on).to_string(),
            Self::Whole(whole) => whole.to_string(),
            Self::Real(real) if real.fract() == 0. && real.abs() < 9e15 => {
                (*real as i64).to_string()
            }
            Self::Real(real) => real.to_string(),
            Self::Null | Self::Items(_) | Self::Record(_) => String::new(),
        }
    }
}

/// Reads a string member of a well-formed record.
pub(crate) fn string<'a>(node: &'a Tree, name: &str) -> &'a str {
    node.get(name).and_then(Tree::as_str).unwrap_or("")
}

/// Reads a boolean member of a well-formed record.
pub(crate) fn flag(node: &Tree, name: &str) -> bool {
    node.get(name).and_then(Tree::as_bool).unwrap_or(false)
}

/// Reads a whole-number member of a well-formed record, or 0.
pub(crate) fn whole(node: &Tree, name: &str) -> i64 {
    node.get(name).and_then(Tree::as_i64).unwrap_or(0)
}

/// Reads a control's issued event ordinal.
pub(crate) fn ordinal(node: &Tree) -> Option<i64> {
    node.get("event").and_then(Tree::as_i64)
}

/// Reads a list member of a well-formed record without allocating a default list.
pub(crate) fn list<'a>(node: &'a Tree, name: &str) -> &'a [Tree] {
    node.get(name)
        .and_then(Tree::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Every value directly inside `node`: a record's members or a list's items.
pub(crate) fn inside(node: &Tree) -> Vec<&Tree> {
    match node {
        Tree::Record(members) => members.values().collect(),
        Tree::Items(items) => items.iter().collect(),
        _ => Vec::new(),
    }
}

/// One colour role, `#RRGGBB` in each appearance ([UI-THEME]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub name: String,
    pub light: String,
    pub dark: String,
}

/// The application's theme: colour tokens, a spacing scale and motion durations ([UI-THEME]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(dead_code)] // The motion durations are kept for the adapters that animate.
pub struct Theme {
    pub tokens: Vec<Token>,
    pub spacing: Vec<i64>,
    pub feedback: i64,
    pub settle: i64,
}

impl Theme {
    /// The theme a told record names; nil when it is no record.
    pub(crate) fn told(node: Option<&Tree>) -> Option<Self> {
        let told = node?;
        told.object()?;
        Some(Self {
            tokens: list(told, "tokens")
                .iter()
                .map(|token| Token {
                    name: string(token, "name").to_owned(),
                    light: string(token, "light").to_owned(),
                    dark: string(token, "dark").to_owned(),
                })
                .collect(),
            spacing: list(told, "spacing")
                .iter()
                .filter_map(Tree::as_i64)
                .collect(),
            feedback: whole(told, "feedback"),
            settle: whole(told, "settle"),
        })
    }
}

/// The frame a host draws: the theme and every region's component tree, by key.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Frame {
    pub theme: Option<Theme>,
    pub regions: Vec<(String, Tree)>,
}

/// Reads the told rows of a checked frame into trees, refusing one this host cannot draw.
pub(crate) fn frame(shown: &Shown) -> Result<Option<Frame>, String> {
    let told: Vec<usize> = shown
        .roots
        .iter()
        .copied()
        .filter(|index| shown.controls[*index].kind == Kind::Record)
        .collect();
    if told.is_empty() {
        return Ok(None);
    }
    let mut theme = None;
    let mut regions = Vec::new();
    for root in told {
        let value = self::value(shown, root);
        match shown.controls[root].hint.as_str() {
            "theme" => theme = Theme::told(Some(&value)),
            "region" => {
                let key = string(&value, "key").to_owned();
                viewports(&value, &key)?;
                regions.push((key, value.get("node").cloned().unwrap_or(Tree::Null)));
            }
            _ => {}
        }
    }
    Ok(Some(Frame { theme, regions }))
}

/// The value of one told row and the rows inside it. A record that a scene names carries its row
/// as `sceneRow`, so the canvas finds the scene the frame gave it.
fn value(shown: &Shown, index: usize) -> Tree {
    let row = &shown.controls[index];
    match row.kind {
        Kind::Field => typed(row),
        Kind::List => Tree::Items(
            row.children
                .iter()
                .map(|child| value(shown, *child))
                .collect(),
        ),
        Kind::Record => record(shown, index, row),
        _ => Tree::Null,
    }
}

/// One told value, typed by the weight its row carries: text, a whole number, a real, a flag or a
/// negative real ([NATIVE-SHM-NODES]).
fn typed(row: &Control) -> Tree {
    match row.look.weight as i64 {
        1 => Tree::Whole(row.value),
        2 => Tree::Real(f64::from(row.look.size)),
        3 => Tree::Bool(row.value != 0),
        4 => Tree::Real(-f64::from(row.look.size)),
        _ => Tree::Text(row.words.clone()),
    }
}

/// A record's members, named by the hints of the rows under it; a member with no name is ignored.
fn record(shown: &Shown, index: usize, row: &Control) -> Tree {
    let mut members = BTreeMap::new();
    for child in &row.children {
        let member = &shown.controls[*child];
        if !member.hint.is_empty() {
            members.insert(member.hint.clone(), value(shown, *child));
        }
    }
    if shown.scenes.contains_key(&index) {
        members.insert("sceneRow".to_owned(), Tree::Whole(index as i64));
    }
    Tree::Record(members)
}

/// The only inset this host draws: a frame sized to its content.
const CONTENT_SIZED: i64 = -1;

/// Refuses a frame whose frames size themselves to the window or scroll, wherever they nest.
fn viewports(node: &Tree, key: &str) -> Result<(), String> {
    if string(node, "kind") == "framed" {
        match node.get("style") {
            None => {}
            Some(style @ Tree::Record(_)) => {
                inset(key, style.get("viewportHeightInset"))?;
                scrolled(key, style.get("scrollY"))?;
            }
            Some(found) => return Err(needs(key, "an object style", found)),
        }
    }
    inside(node)
        .into_iter()
        .try_for_each(|child| viewports(child, key))
}

/// Refuses any inset but the content-sized default; only absence reads as that default.
fn inset(key: &str, value: Option<&Tree>) -> Result<(), String> {
    match value {
        None => Ok(()),
        Some(Tree::Whole(CONTENT_SIZED)) => Ok(()),
        Some(Tree::Whole(whole)) => Err(not_content_sized(key, *whole)),
        Some(found) => Err(needs(key, "an integer viewportHeightInset", found)),
    }
}

/// Refuses vertical scrolling this host cannot perform; only absence reads as unscrolled.
fn scrolled(key: &str, value: Option<&Tree>) -> Result<(), String> {
    match value {
        None | Some(Tree::Bool(false)) => Ok(()),
        Some(Tree::Bool(true)) => Err(format!(
            "Frame scrollY in region {key} asks the frame to scroll, which this host does not draw; \
             the default is false"
        )),
        Some(found) => Err(needs(key, "a boolean scrollY", found)),
    }
}

/// The refusal for an inset other than the content-sized default.
fn not_content_sized(key: &str, whole: i64) -> String {
    format!(
        "Frame viewportHeightInset {whole} in region {key} is not the content-sized default \
         {CONTENT_SIZED}, which this host draws by sizing the frame to its content"
    )
}

/// The refusal for a present field of the wrong kind, named so the application's author can fix it.
fn needs(key: &str, wanted: &str, found: &Tree) -> String {
    format!(
        "Frame in region {key} has {} where it needs {wanted}",
        shape(found)
    )
}

/// The shape of a told value, as the frame's own diagnostics name it.
fn shape(value: &Tree) -> &'static str {
    match value {
        Tree::Null => "null",
        Tree::Bool(_) => "a boolean",
        Tree::Whole(_) | Tree::Real(_) => "a number",
        Tree::Text(_) => "a string",
        Tree::Items(_) => "an array",
        Tree::Record(_) => "an object",
    }
}

#[cfg(test)]
mod tests;
