//! A sealed frame checked and read as typed values ([NATIVE-SHM-SAFETY], [NATIVE-SHM-VIEW],
//! [NATIVE-SHM-SCENE], [NATIVE-SHM-ASK]). Nothing from the application is trusted: every kind and
//! code must be known, every parent must be an earlier row, every text range must lie inside the
//! text run on UTF-8 boundaries, every float must be finite and every count must stay inside the
//! rows that follow it. A frame that fails any check is refused whole and the last good view stays.

use std::collections::HashMap;

use gpui_kit::accesskit::Role;

use crate::frame::{AskRow, Frame, Streamed, ViewRow};
use crate::scene::{access::Access, Outline, Paint, Shape, Step, Tint};

/// What a row draws, by its `kind` ([NATIVE-SHM-VIEW]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Column,
    Line,
    Label,
    Press,
    Entry,
    Gap,
    Scroll,
    Layer,
    Toggle,
    Picker,
    Choice,
    Rule,
    Image,
    Canvas,
    Editor,
    Menu,
    Sheet,
    /// [NATIVE-SHM-NODES] One told value, a record of members and a list of items.
    Field,
    Record,
    List,
}

impl Kind {
    /// The kind a row's code names, or none for a code this host does not know.
    fn of(code: i64) -> Option<Kind> {
        match code {
            0..=16 => KINDS.get(usize::try_from(code).ok()?).copied(),
            33 => Some(Kind::Field),
            34 => Some(Kind::Record),
            35 => Some(Kind::List),
            _ => None,
        }
    }

    /// Whether this row tells a component rather than draws a control.
    pub fn told(self) -> bool {
        matches!(self, Kind::Field | Kind::Record | Kind::List)
    }
}

/// The rows that draw, by their code from zero ([NATIVE-SHM-VIEW]).
const KINDS: [Kind; 17] = [
    Kind::Column,
    Kind::Line,
    Kind::Label,
    Kind::Press,
    Kind::Entry,
    Kind::Gap,
    Kind::Scroll,
    Kind::Layer,
    Kind::Toggle,
    Kind::Picker,
    Kind::Choice,
    Kind::Rule,
    Kind::Image,
    Kind::Canvas,
    Kind::Editor,
    Kind::Menu,
    Kind::Sheet,
];

/// The `traits` flags a row's fit adds up ([NATIVE-SHM-VIEW]).
pub mod flags {
    pub const DISABLED: u32 = 1;
    pub const CHOSEN: u32 = 2;
    pub const SECRET: u32 = 4;
    pub const FIXED_WIDTH: u32 = 8;
    pub const SLANTED: u32 = 16;
    pub const CENTRED: u32 = 32;
    pub const ON_COMMIT: u32 = 64;
    pub const ALL: u32 = 127;
}

/// How one control looks, every value decided in Osprey: `0xRRGGBBAA` ink and fill, sizes in points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Look {
    pub ink: u32,
    pub fill: u32,
    pub size: f32,
    pub padding: f32,
    pub spacing: f32,
    pub radius: f32,
    pub weight: f32,
}

/// How one control sits: fixed extents (0 = natural), its share of free space, flags and border.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Fit {
    pub width: f32,
    pub height: f32,
    pub grow: f32,
    pub flags: u32,
    pub border: u32,
}

/// One checked control and the rows drawn inside it.
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    pub kind: Kind,
    /// The control number a host event names, when the control delivers one.
    pub action: Option<i64>,
    pub words: String,
    /// A field's prompt, an option's or tab's key, or the icon a label or press is led by. A told
    /// row's name is its hint too.
    pub hint: String,
    pub look: Look,
    pub fit: Fit,
    /// A told field's whole number or flag; 0 for every row that draws.
    pub value: i64,
    pub children: Vec<usize>,
}

/// A canvas's scene: its extent, zoom, atlas, shapes and accessibility tree.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub zoom: f32,
    pub atlas: Vec<String>,
    pub shapes: Vec<Shape>,
    pub access: Vec<Access>,
}

/// One field of an ask: its argument position, number and text.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub field: i64,
    pub number: i64,
    pub text: String,
}

/// A capability request or a slice write: its code, its ticket (a slice write's key) and its fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub code: i64,
    pub ticket: String,
    pub fields: Vec<Field>,
}

/// The capability code of a slice write and of a refused event's diagnostic.
pub const SLICE_WRITE: i64 = 100;
pub const DIAGNOSTIC: i64 = 101;
/// The capability names, by code from 1, as the application's `Requests::kind` spells them.
const CAPABILITIES: [&str; 25] = [
    "run",
    "list-directories",
    "read-plists",
    "read-texts",
    "fingerprint",
    "store-read",
    "store-write",
    "store-list",
    "store-delete",
    "http",
    "host-call",
    "running-applications",
    "choose-file",
    "choose-folder",
    "save-file",
    "measure-folders",
    "reveal",
    "open-settings",
    "cancel",
    "recognise",
    "animate",
    "process-start",
    "process-write",
    "process-read",
    "process-stop",
];

impl Ask {
    /// The capability's name, or empty for a code this host does not know.
    pub fn kind(&self) -> &'static str {
        usize::try_from(self.code - 1)
            .ok()
            .and_then(|index| CAPABILITIES.get(index))
            .copied()
            .unwrap_or("")
    }

    /// Every text at argument `field`, in order: a list's items or a pair's halves.
    pub fn texts(&self, field: i64) -> Vec<String> {
        self.fields
            .iter()
            .filter(|f| f.field == field)
            .map(|f| f.text.clone())
            .collect()
    }

    /// The whole number at argument `field`, or 0.
    pub fn number(&self, field: i64) -> i64 {
        self.fields
            .iter()
            .find(|f| f.field == field)
            .map_or(0, |f| f.number)
    }

    /// The flag at argument `field`.
    pub fn flag(&self, field: i64) -> bool {
        self.number(field) != 0
    }
}

/// A checked frame: its controls (a row's index is its position), the rows with no parent,
/// each canvas's scene by its row, and its asks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shown {
    pub controls: Vec<Control>,
    pub roots: Vec<usize>,
    pub scenes: HashMap<usize, Scene>,
    pub asks: Vec<Ask>,
}

impl Shown {
    /// The capability requests to answer, in order.
    pub fn requests(&self) -> impl Iterator<Item = &Ask> {
        self.asks.iter().filter(|a| a.code >= 1 && a.code <= 25)
    }

    /// The slice writes, as key and text.
    pub fn slices(&self) -> impl Iterator<Item = (&str, &str)> {
        self.asks.iter().filter(|a| a.code == SLICE_WRITE).map(|a| {
            (
                a.ticket.as_str(),
                a.fields.first().map_or("", |f| f.text.as_str()),
            )
        })
    }

    /// Why the application refused the event this frame answers, if it did.
    pub fn diagnostics(&self) -> impl Iterator<Item = &str> {
        self.asks
            .iter()
            .filter(|a| a.code == DIAGNOSTIC)
            .map(|a| a.ticket.as_str())
    }
}

/// Text bytes `offset..offset + length` of the run, checked as UTF-8.
fn text(run: &[u8], offset: i64, length: i64) -> Result<String, String> {
    let start = usize::try_from(offset).map_err(|_| "a text offset below zero".to_owned())?;
    let count = usize::try_from(length).map_err(|_| "a text length below zero".to_owned())?;
    let end = start
        .checked_add(count)
        .filter(|end| *end <= run.len())
        .ok_or_else(|| "a text range outside the frame".to_owned())?;
    std::str::from_utf8(&run[start..end])
        .map(str::to_owned)
        .map_err(|_| "text that is not UTF-8".to_owned())
}

fn finite(values: &[f64]) -> Result<(), String> {
    match values.iter().all(|v| v.is_finite()) {
        true => Ok(()),
        false => Err("a number that is not finite".to_owned()),
    }
}

fn extent(value: f64) -> Result<f32, String> {
    match value.is_finite() && value >= 0. {
        true => Ok(value as f32),
        false => Err("an extent that is negative or not finite".to_owned()),
    }
}

fn colour(value: i64) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| "a colour outside 0xRRGGBBAA".to_owned())
}

fn control(row: &ViewRow, index: usize, run: &[u8]) -> Result<Control, String> {
    let kind = Kind::of(row.kind)
        .ok_or_else(|| format!("row {index} has an unknown kind {}", row.kind))?;
    if kind.told() {
        return told(kind, row, run);
    }
    let flags = u32::try_from(row.flags)
        .ok()
        .filter(|f| f & !flags::ALL == 0)
        .ok_or_else(|| format!("row {index} has unknown flags {}", row.flags))?;
    let weight = match (100..=900).contains(&row.weight) {
        true => row.weight as f32,
        false => return Err(format!("row {index} has a font weight outside 100 to 900")),
    };
    Ok(Control {
        kind,
        action: (row.action >= 0).then_some(row.action),
        words: text(run, row.text_offset, row.text_length)?,
        hint: text(run, row.hint_offset, row.hint_length)?,
        look: Look {
            ink: colour(row.colour)?,
            fill: colour(row.background)?,
            size: extent(row.size)?,
            padding: extent(row.padding)?,
            spacing: extent(row.spacing)?,
            radius: extent(row.radius)?,
            weight,
        },
        fit: Fit {
            width: extent(row.width)?,
            height: extent(row.height)?,
            grow: extent(row.grow)?,
            flags,
            border: colour(row.border)?,
        },
        value: 0,
        children: Vec::new(),
    })
}

/// One told row ([NATIVE-SHM-NODES]): its name is its hint, a field's value is its words, its
/// whole number in its ink or its real in its points, and its weight says which (0 to 4). The
/// ordinary look checks do not apply: these colours and weights are values, not paint. Anything
/// else refuses the frame.
fn told(kind: Kind, row: &ViewRow, run: &[u8]) -> Result<Control, String> {
    let weight = match (0..=4).contains(&row.weight) {
        true => row.weight as f32,
        false => return Err(format!("a told value of an unknown type {}", row.weight)),
    };
    Ok(Control {
        kind,
        action: None,
        words: text(run, row.text_offset, row.text_length)?,
        hint: text(run, row.hint_offset, row.hint_length)?,
        look: Look {
            size: extent(row.size)?,
            weight,
            ..Look::default()
        },
        fit: Fit::default(),
        value: row.colour,
        children: Vec::new(),
    })
}

/// The controls, each child listed under its parent, and the rows with no parent.
fn controls(rows: &[ViewRow], run: &[u8]) -> Result<(Vec<Control>, Vec<usize>), String> {
    let mut all = Vec::with_capacity(rows.len());
    let mut roots = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        all.push(control(row, index, run)?);
        match row.parent {
            -1 => roots.push(index),
            parent => {
                let at = usize::try_from(parent)
                    .ok()
                    .filter(|p| *p < index)
                    .ok_or_else(|| {
                        format!("row {index} names a parent that is not an earlier row")
                    })?;
                all[at].children.push(index);
            }
        }
    }
    Ok((all, roots))
}

fn field(row: &AskRow, run: &[u8]) -> Result<Field, String> {
    Ok(Field {
        field: row.field,
        number: row.number,
        text: text(run, row.text_offset, row.text_length)?,
    })
}

/// The asks: a field-0 row starts one, and the rows after it are its fields.
fn asks(rows: &[AskRow], run: &[u8]) -> Result<Vec<Ask>, String> {
    let mut all: Vec<Ask> = Vec::new();
    for row in rows {
        let read = field(row, run)?;
        match (row.field, all.last_mut()) {
            (0, _) => all.push(Ask {
                code: row.number,
                ticket: read.text,
                fields: Vec::new(),
            }),
            (f, Some(ask)) if f > 0 => ask.fields.push(read),
            _ => return Err("an ask field before any ask, or below zero".to_owned()),
        }
    }
    match all
        .iter()
        .all(|a| (1..=25).contains(&a.code) || a.code == SLICE_WRITE || a.code == DIAGNOSTIC)
    {
        true => Ok(all),
        false => Err("an ask with an unknown capability code".to_owned()),
    }
}

/// Scene rows in order, with the next one taken on demand; every count is checked against what is left.
struct Rows<'a> {
    rows: &'a [Streamed],
    next: usize,
    run: &'a [u8],
}

const POINT: i64 = 7;

impl<'a> Rows<'a> {
    fn take(&mut self, code: i64) -> Result<&'a Streamed, String> {
        let row = self
            .rows
            .get(self.next)
            .filter(|r| r.code == code)
            .ok_or_else(|| format!("scene row {} should have code {code}", self.next))?;
        finite(&row.values)?;
        self.next += 1;
        Ok(row)
    }

    fn count(&self, value: i64) -> Result<usize, String> {
        usize::try_from(value)
            .ok()
            .filter(|n| *n <= self.rows.len() - self.next)
            .ok_or_else(|| "a scene count that runs past its rows".to_owned())
    }

    fn points(&mut self, count: i64) -> Result<Vec<(f32, f32)>, String> {
        (0..self.count(count)?)
            .map(|_| {
                self.take(POINT)
                    .map(|p| (p.values[0] as f32, p.values[1] as f32))
            })
            .collect()
    }

    fn text(&self, row: &Streamed) -> Result<String, String> {
        text(self.run, row.text_offset, row.text_length)
    }
}

fn f(row: &Streamed, at: usize) -> f32 {
    row.values[at] as f32
}

fn tint(rows: &mut Rows) -> Result<Tint, String> {
    let row = rows.take(11)?;
    let values = [f(row, 0), f(row, 1), f(row, 2), f(row, 3)];
    match row.index {
        0 => Ok(Tint::Rgb(values)),
        1 => Ok(Tint::Hsl(values)),
        _ => Err("an unknown tint".to_owned()),
    }
}

fn step(rows: &mut Rows) -> Result<Step, String> {
    let row = rows.take(8)?;
    let at = |a: usize, b: usize| (f(row, a), f(row, b));
    match row.index {
        0 => Ok(Step::Move(at(0, 1))),
        1 => Ok(Step::Line(at(0, 1))),
        2 => Ok(Step::Curve(at(0, 1), at(2, 3))),
        3 => {
            let last = rows.take(POINT)?;
            Ok(Step::Cubic(at(0, 1), at(2, 3), (f(row, 4), f(last, 0))))
        }
        4 => Ok(Step::Arc {
            radii: at(0, 1),
            rotation: f(row, 2),
            large: row.colour & 1 != 0,
            sweep: row.colour & 2 != 0,
            to: at(3, 4),
        }),
        5 => Ok(Step::Polygon(rows.points(row.depth)?, row.colour & 1 != 0)),
        6 => Ok(Step::Close),
        _ => Err("an unknown path step".to_owned()),
    }
}

fn outline(rows: &mut Rows) -> Result<Outline, String> {
    let row = rows.take(9)?;
    match row.index {
        0 => Ok(Outline::Fill),
        1 => Ok(Outline::Stroke {
            width: f(row, 0),
            bevel: row.colour & 1 != 0,
            dashes: rows
                .points(row.depth)?
                .into_iter()
                .map(|(x, _)| x)
                .collect(),
        }),
        _ => Err("an unknown outline".to_owned()),
    }
}

fn paint(rows: &mut Rows) -> Result<Paint, String> {
    let row = rows.take(10)?;
    match row.index {
        0 => Ok(Paint::Solid(tint(rows)?)),
        1 => Ok(Paint::Linear {
            angle: f(row, 0),
            from: (tint(rows)?, f(row, 1)),
            to: (tint(rows)?, f(row, 2)),
            oklab: row.colour & 1 != 0,
        }),
        _ => Err("an unknown paint".to_owned()),
    }
}

fn path(rows: &mut Rows, head: &Streamed) -> Result<Shape, String> {
    let steps = (0..rows.count(head.index)?)
        .map(|_| step(rows))
        .collect::<Result<Vec<_>, _>>()?;
    let (moved, scale) = ((f(head, 0), f(head, 1)), f(head, 2));
    Ok(Shape::Path {
        steps: std::sync::Arc::new(steps),
        outline: outline(rows)?,
        paint: paint(rows)?,
        transform: (moved != (0., 0.) || scale != 1.).then_some((moved.0, moved.1, scale)),
        depth: head.depth as f32,
    })
}

fn tinted(row: &Streamed) -> Result<u32, String> {
    colour(row.colour)
}

fn shape(rows: &mut Rows, head: &Streamed) -> Result<Shape, String> {
    let depth = head.depth as f32;
    match head.code {
        0 => Ok(Shape::Rect {
            x: f(head, 0),
            y: f(head, 1),
            w: f(head, 2),
            h: f(head, 3),
            angle: f(head, 4),
            colour: tinted(head)?,
            depth,
        }),
        1 => Ok(Shape::Disc {
            x: f(head, 0),
            y: f(head, 1),
            radius: f(head, 2),
            colour: tinted(head)?,
            depth,
        }),
        2 => Ok(Shape::Strip {
            points: rows.points(head.index)?,
            colour: tinted(head)?,
            stroke: f(head, 2),
            depth,
        }),
        3 => Ok(Shape::Sprite {
            atlas: usize::try_from(head.index)
                .map_err(|_| "a sprite index below zero".to_owned())?,
            x: f(head, 0),
            y: f(head, 1),
            w: f(head, 2),
            h: f(head, 3),
            angle: f(head, 4),
            tint: tinted(head)?,
            depth,
        }),
        4 => Ok(Shape::Glyphs {
            text: rows.text(head)?,
            x: f(head, 0),
            y: f(head, 1),
            size: f(head, 2),
            colour: tinted(head)?,
            depth,
        }),
        5 => path(rows, head),
        6 => Ok(Shape::Lettering {
            text: rows.text(head)?,
            x: f(head, 0),
            y: f(head, 1),
            size: f(head, 2),
            colour: tinted(head)?,
            depth,
            anchor: u8::try_from(head.index)
                .ok()
                .filter(|a| *a <= 2)
                .ok_or_else(|| "an unknown lettering anchor".to_owned())?,
            weight: f(head, 3),
        }),
        other => Err(format!("an unknown scene code {other}")),
    }
}

fn element(rows: &mut Rows, head: &Streamed) -> Result<Option<Access>, String> {
    let range = rows.take(POINT)?;
    let role = match head.index {
        0 | 2 => Role::Button,
        1 => Role::Slider,
        3 => Role::MenuItem,
        4 => return Ok(None),
        _ => return Err("an unknown accessible role".to_owned()),
    };
    let label = rows.text(head)?;
    if label.is_empty() {
        // An element with no name cannot be announced.
        return Ok(None);
    }
    Ok(Some(Access {
        role,
        label,
        x: head.values[0],
        y: head.values[1],
        w: head.values[2],
        h: head.values[3],
        value: head.values[4],
        min: range.values[0],
        max: range.values[1],
        // Only a menu item is announced as the current choice ([UI-MACHINE-A11Y]).
        selected: role == Role::MenuItem && head.colour & 1 != 0,
    }))
}

/// One scene row's meaning once read.
enum Item {
    Atlas(String),
    Element(Option<Access>),
    Shape(Shape),
}

/// Every scene, by the canvas row its marker names.
fn scenes(
    rows: &[Streamed],
    run: &[u8],
    canvases: &[usize],
) -> Result<HashMap<usize, Scene>, String> {
    let mut all: HashMap<usize, Scene> = HashMap::new();
    let mut at = Rows { rows, next: 0, run };
    let mut current: Option<usize> = None;
    while at.next < rows.len() {
        let head = &rows[at.next];
        finite(&head.values)?;
        at.next += 1;
        if head.code == 20 {
            let row = usize::try_from(head.index)
                .ok()
                .filter(|r| canvases.contains(r) && !all.contains_key(r))
                .ok_or_else(|| "a scene that names no canvas, or one twice".to_owned())?;
            let (width, height) = (extent(head.values[0])?, extent(head.values[1])?);
            if (width == 0.) != (height == 0.) {
                return Err("a window-pixel scene has zero width and zero height".to_owned());
            }
            all.insert(
                row,
                Scene {
                    width,
                    height,
                    zoom: extent(head.values[2])?,
                    ..Scene::default()
                },
            );
            current = Some(row);
            continue;
        }
        let row = current.ok_or_else(|| "a scene row before any scene".to_owned())?;
        let item = match head.code {
            21 => Item::Atlas(at.text(head)?),
            22 => Item::Element(element(&mut at, head)?),
            _ => Item::Shape(shape(&mut at, head)?),
        };
        if let Some(scene) = all.get_mut(&row) {
            match item {
                Item::Atlas(name) => scene.atlas.push(name),
                Item::Element(found) => scene.access.extend(found),
                Item::Shape(drawn) => scene.shapes.push(drawn),
            }
        }
    }
    Ok(all)
}

/// Checks and reads one sealed frame.
pub fn read(frame: &Frame) -> Result<Shown, String> {
    checked(frame.views(), frame.shapes(), frame.text(), frame.asks())
}

/// Checks and reads a frame's runs.
pub(crate) fn checked(
    views: &[ViewRow],
    shapes: &[Streamed],
    run: &[u8],
    asked: &[AskRow],
) -> Result<Shown, String> {
    let (controls, roots) = controls(views, run)?;
    let canvases: Vec<usize> = (0..controls.len())
        .filter(|i| matches!(controls[*i].kind, Kind::Canvas | Kind::Record))
        .collect();
    Ok(Shown {
        scenes: scenes(shapes, run, &canvases)?,
        asks: asks(asked, run)?,
        controls,
        roots,
    })
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
