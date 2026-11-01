//! One script beside each application, run with or without a window ([UI-MACHINE-SELF-TEST]).
//! Its steps name only what a screen reader announces, so the same file drives the WinUI host, this
//! host's real window and this host's C boundary alone. Every failure prints the accessibility
//! tree, and every `shot` keeps it beside the run, so an agent reads what it may act on.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::*;
use serde_json::{json, Value};

use crate::events;
use crate::host::{now, HostView};
use crate::regions::Regions;
use crate::smoke;

/// How long a windowed step waits for what it needs before it fails, and how often it looks.
const PATIENCE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(50);
/// How many display frames a windowless `wait` or `gone` runs before it fails.
const FRAMES: usize = 600;
const FRAME_SECONDS: f64 = 1. / 60.;

/// One event to send, bound to a region's current revision when it is sent.
#[derive(Debug, Clone, PartialEq)]
pub struct Send {
    region: String,
    ordinal: u64,
    value: Value,
}

/// What a step needs next.
#[derive(Debug, PartialEq)]
pub enum Attempt {
    /// Not yet true; the words say what is missing.
    Waiting(String),
    /// These events in order, then the step is done.
    Sending(Vec<Send>),
    /// These many display frames, then the step is done.
    Framing(usize),
    /// A record of the accessibility tree, then the step is done.
    Recording(String),
}

/// A run's script and where its records go, from `--self-test <script> <shots> [--headless]`.
pub struct Run {
    steps: Vec<(String, Vec<String>)>,
    shots: PathBuf,
}

/// Whether this launch is a self-test, and whether it has no window.
pub fn requested() -> Option<bool> {
    let arguments: Vec<String> = std::env::args().collect();
    arguments
        .iter()
        .any(|argument| argument == "--self-test")
        .then(|| arguments.iter().any(|argument| argument == "--headless"))
}

/// The application's own arguments, without this host's self-test flags.
pub fn app_arguments() -> Vec<String> {
    let mut given = std::env::args().skip(1);
    let mut kept = Vec::new();
    while let Some(argument) = given.next() {
        match argument.as_str() {
            "--self-test" => drop((given.next(), given.next())),
            "--headless" => {}
            _ => kept.push(argument),
        }
    }
    kept
}

/// Reads the script the command line names.
pub fn load() -> Result<Run, String> {
    let arguments: Vec<String> = std::env::args().collect();
    let index = arguments
        .iter()
        .position(|argument| argument == "--self-test")
        .ok_or("--self-test was not given")?;
    let (Some(script), Some(shots)) = (arguments.get(index + 1), arguments.get(index + 2)) else {
        return Err("--self-test needs a script and a folder for its records".to_owned());
    };
    let text = std::fs::read_to_string(script)
        .map_err(|problem| format!("could not read {script}: {problem}"))?;
    let shots = PathBuf::from(shots);
    std::fs::create_dir_all(&shots)
        .map_err(|problem| format!("could not make {}: {problem}", shots.display()))?;
    Ok(Run {
        steps: parse(&text)?,
        shots,
    })
}

/// Parses `{"steps": [{"verb": "word"}, {"verb": ["word", "word"]}]}`.
pub fn parse(text: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let script: Value =
        serde_json::from_str(text).map_err(|problem| format!("malformed script: {problem}"))?;
    let steps = script
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("a script holds a list of steps")?;
    steps
        .iter()
        .map(|step| {
            let (verb, words) = step
                .as_object()
                .filter(|members| members.len() == 1)
                .and_then(|members| members.iter().next())
                .ok_or_else(|| format!("a step is one verb: {step}"))?;
            let words = match words {
                Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
                other => other.as_str().into_iter().collect::<Vec<_>>(),
            };
            Ok((verb.clone(), words.into_iter().map(str::to_owned).collect()))
        })
        .collect()
}

/// What one step needs of the regions as they stand.
pub fn attempt(regions: &Regions, verb: &str, words: &[String]) -> Result<Attempt, String> {
    let word = |index: usize| {
        words
            .get(index)
            .map(String::as_str)
            .ok_or_else(|| format!("{verb} needs {} words", index + 1))
    };
    let number = |index: usize| {
        word(index)?
            .parse::<f64>()
            .map_err(|_| format!("{verb} needs a number, not {}", words[index]))
    };
    Ok(match verb {
        "wait" if shows(regions, word(0)?) => Attempt::Sending(Vec::new()),
        "wait" => Attempt::Waiting(format!("nothing shows \"{}\"", word(0)?)),
        "gone" if !shows(regions, word(0)?) => Attempt::Sending(Vec::new()),
        "gone" => Attempt::Waiting(format!("\"{}\" still shows", word(0)?)),
        "invoke" => match (element(regions, word(0)?), control(regions, word(0)?)) {
            (Some(found), _) => found.clicked(),
            (None, Some(found)) => found.sent(Value::Null),
            (None, None) => Attempt::Waiting(format!("nothing offers \"{}\"", word(0)?)),
        },
        "set" => match element(regions, word(0)?) {
            Some(found) => found.valued(number(1)?)?,
            None => Attempt::Waiting(format!("no slider \"{}\"", word(0)?)),
        },
        "drag" => match element(regions, word(0)?) {
            Some(found) => found.dragged(number(1)?, number(2)?),
            None => Attempt::Waiting(format!("nothing to drag called \"{}\"", word(0)?)),
        },
        "toggle" => match control(regions, word(0)?) {
            Some(found) => {
                let on = found
                    .node
                    .get("on")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                found.sent(Value::Bool(!on))
            }
            None => Attempt::Waiting(format!("no switch \"{}\"", word(0)?)),
        },
        "type" => match control(regions, word(0)?) {
            Some(found) => found.sent(Value::String(word(1)?.to_owned())),
            None => Attempt::Waiting(format!("no field \"{}\"", word(0)?)),
        },
        "choose" => match chooser(regions, word(0)?) {
            Some(found) => Attempt::Sending(vec![found]),
            None => Attempt::Waiting(format!("no row \"{}\"", word(0)?)),
        },
        "key" => match scene(regions) {
            Some((region, ordinal, _)) => Attempt::Sending(
                [true, false]
                    .map(|down| Send {
                        region: region.clone(),
                        ordinal,
                        value: json!({"key": word(0).unwrap_or_default(), "down": down}),
                    })
                    .to_vec(),
            ),
            None => Attempt::Waiting("no scene is drawn to take a key".to_owned()),
        },
        "frames" => Attempt::Framing(number(0)? as usize),
        "sleep" => Attempt::Framing((number(0)? / FRAME_SECONDS) as usize),
        "shot" => Attempt::Recording(word(0)?.to_owned()),
        other => return Err(format!("there is no step called {other}")),
    })
}

/// A rendered node a step found by its words: its region, its binding and the node.
struct Found<'a> {
    region: String,
    ordinal: u64,
    node: &'a Value,
}

impl Found<'_> {
    fn sent(&self, value: Value) -> Attempt {
        Attempt::Sending(vec![Send {
            region: self.region.clone(),
            ordinal: self.ordinal,
            value,
        }])
    }

    fn number(&self, key: &str) -> f64 {
        self.node.get(key).and_then(Value::as_f64).unwrap_or(0.)
    }

    fn pointer(&self, x: f64, y: f64, down: bool) -> Send {
        Send {
            region: self.region.clone(),
            ordinal: self.ordinal,
            value: json!({"x": x, "y": y, "down": down, "shift": false}),
        }
    }

    fn middle(&self) -> (f64, f64) {
        (
            self.number("x") + self.number("w") / 2.,
            self.number("y") + self.number("h") / 2.,
        )
    }

    /// The press and release a click on an accessible element lands as.
    fn clicked(&self) -> Attempt {
        let (x, y) = self.middle();
        Attempt::Sending(vec![self.pointer(x, y, true), self.pointer(x, y, false)])
    }

    /// The press and release that set a slider to `value`, clamped to its range.
    fn valued(&self, value: f64) -> Result<Attempt, String> {
        let (low, high) = (self.number("min"), self.number("max"));
        if high <= low {
            return Err(format!("{} has no range to set", self.node));
        }
        let fraction = ((value - low) / (high - low)).clamp(0., 1.);
        let (x, y) = (
            self.number("x") + self.number("w") * fraction,
            self.middle().1,
        );
        Ok(Attempt::Sending(vec![
            self.pointer(x, y, true),
            self.pointer(x, y, false),
        ]))
    }

    /// The press, move and release that drag an element by `dx`, `dy` scene units.
    fn dragged(&self, dx: f64, dy: f64) -> Attempt {
        let (x, y) = self.middle();
        Attempt::Sending(vec![
            self.pointer(x, y, true),
            self.pointer(x + dx, y + dy, true),
            self.pointer(x + dx, y + dy, false),
        ])
    }
}

/// Every string in every region: a text, a label, a title or a scene's lettering.
fn shows(regions: &Regions, words: &str) -> bool {
    regions.keys().any(|key| {
        regions
            .get(key)
            .is_some_and(|region| holds(&region.node, words))
    })
}

fn holds(node: &Value, words: &str) -> bool {
    node.as_str() == Some(words) || smoke::children(node).any(|inside| holds(inside, words))
}

/// The first scene and its binding and accessibility tree.
fn scene(regions: &Regions) -> Option<(String, u64, &Value)> {
    regions.keys().find_map(|key| {
        let node = find(&regions.get(key)?.node, &|node| {
            node.get("kind").and_then(Value::as_str) == Some("scene")
        })?;
        Some((key.to_owned(), node.get("event")?.as_u64()?, node))
    })
}

fn find<'a>(node: &'a Value, wanted: &dyn Fn(&Value) -> bool) -> Option<&'a Value> {
    if wanted(node) {
        return Some(node);
    }
    smoke::children(node).find_map(|inside| find(inside, wanted))
}

/// The scene element a screen reader announces as `label`.
fn element<'a>(regions: &'a Regions, label: &str) -> Option<Found<'a>> {
    let (region, ordinal, node) = scene(regions)?;
    let item = node
        .get("access")?
        .as_array()?
        .iter()
        .find(|item| item.get("label").and_then(Value::as_str) == Some(label))?;
    Some(Found {
        region,
        ordinal,
        node: item,
    })
}

/// The bound control a screen reader announces as `words`.
fn control<'a>(regions: &'a Regions, words: &str) -> Option<Found<'a>> {
    let named = |node: &Value| {
        node.get("event").is_some_and(Value::is_u64)
            && ["label", "text", "title"]
                .iter()
                .any(|key| node.get(*key).and_then(Value::as_str) == Some(words))
    };
    regions.keys().find_map(|key| {
        let node = find(&regions.get(key)?.node, &named)?;
        Some(Found {
            region: key.to_owned(),
            ordinal: node.get("event")?.as_u64()?,
            node,
        })
    })
}

/// The choice a bound list or picker offers under the words on its row.
fn chooser(regions: &Regions, words: &str) -> Option<Send> {
    regions.keys().find_map(|key| {
        let offers = |node: &Value, field: &str| {
            node.get(field)?.as_array()?.iter().find_map(|row| {
                let row_key = row.get("key")?.as_str()?;
                holds(row, words).then(|| row_key.to_owned())
            })
        };
        let node = find(&regions.get(key)?.node, &|node| {
            node.get("event").is_some()
                && (offers(node, "rows").is_some() || offers(node, "choices").is_some())
        })?;
        Some(Send {
            region: key.to_owned(),
            ordinal: node.get("event")?.as_u64()?,
            value: Value::String(offers(node, "rows").or_else(|| offers(node, "choices"))?),
        })
    })
}

/// The accessibility tree as an agent reads it: one line per thing it can act on, in hit order.
pub fn tree(regions: &Regions) -> String {
    let mut lines = Vec::new();
    for key in regions.keys() {
        if let Some(region) = regions.get(key) {
            described(&region.node, &mut lines);
        }
    }
    lines.join("\n")
}

fn described(node: &Value, lines: &mut Vec<String>) {
    if let Some(access) = node.get("access").and_then(Value::as_array) {
        lines.extend(access.iter().map(announced));
    }
    if node.get("event").is_some_and(Value::is_u64) {
        let kind = node
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("control");
        let name = ["label", "title", "text"]
            .iter()
            .find_map(|key| node.get(*key).and_then(Value::as_str))
            .unwrap_or("");
        if kind != "scene" {
            lines.push(format!("{kind} \"{name}\""));
        }
    }
    smoke::children(node).for_each(|inside| described(inside, lines));
}

fn announced(item: &Value) -> String {
    let word = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or("");
    let number = |key: &str| item.get(key).and_then(Value::as_f64).unwrap_or(0.);
    match word("role") {
        "slider" => format!(
            "slider \"{}\" value {} of {} to {}",
            word("label"),
            number("value"),
            number("min"),
            number("max")
        ),
        role if item.get("movable").and_then(Value::as_bool) == Some(true) => {
            format!("{role} \"{}\" movable", word("label"))
        }
        role => format!("{role} \"{}\"", word("label")),
    }
}

/// The event a `Send` is, at its region's current revision.
fn event(regions: &Regions, send: &Send) -> Result<String, String> {
    let region = regions
        .get(&send.region)
        .ok_or_else(|| format!("region {} is gone", send.region))?;
    Ok(events::control(
        now(),
        &send.region,
        region.revision,
        send.ordinal,
        send.value.clone(),
    ))
}

fn record(shots: &Path, name: &str, regions: &Regions) -> Result<(), String> {
    let path = shots.join(format!("{name}.access.txt"));
    std::fs::write(&path, tree(regions) + "\n")
        .map_err(|problem| format!("could not write {}: {problem}", path.display()))
}

/// A failure, with what the app offers, so an agent can see what it may act on instead.
fn failed(index: usize, verb: &str, words: &[String], why: &str, regions: &Regions) -> String {
    format!(
        "step {} {verb} {}: {why}\nthe accessibility tree offers:\n{}\ndiagnostics: {:?}",
        index + 1,
        words.join(" = "),
        tree(regions),
        regions.diagnostics()
    )
}

/// Runs the script against the C boundary alone and returns the exit status.
pub fn windowless() -> i32 {
    match load().and_then(|run| perform_windowless(&run)) {
        Ok(steps) => {
            println!("self-test ok ({steps} steps, no window)");
            0
        }
        Err(problem) => {
            eprintln!("self-test failed: {problem}");
            1
        }
    }
}

fn perform_windowless(run: &Run) -> Result<usize, String> {
    let mut regions = smoke::started()?;
    for (index, (verb, words)) in run.steps.iter().enumerate() {
        let mut frames = 0;
        loop {
            let fail = |why: &str, regions: &Regions| failed(index, verb, words, why, regions);
            match attempt(&regions, verb, words).map_err(|why| fail(&why, &regions))? {
                Attempt::Waiting(why) if frames >= FRAMES => return Err(fail(&why, &regions)),
                Attempt::Waiting(_) => {
                    frame(&mut regions)?;
                    frames += 1;
                }
                Attempt::Sending(sends) => {
                    for send in &sends {
                        let text = event(&regions, send)?;
                        smoke::dispatched(&mut regions, &text)?;
                    }
                    break;
                }
                Attempt::Framing(count) => {
                    for _ in 0..count {
                        frame(&mut regions)?;
                    }
                    break;
                }
                Attempt::Recording(name) => {
                    record(&run.shots, &name, &regions)?;
                    break;
                }
            }
        }
    }
    clean(&regions, Vec::new())?;
    Ok(run.steps.len())
}

fn frame(regions: &mut Regions) -> Result<(), String> {
    smoke::dispatched(regions, &events::frame(now(), FRAME_SECONDS))
}

fn clean(regions: &Regions, messages: Vec<String>) -> Result<(), String> {
    let mut problems = regions.diagnostics().to_vec();
    problems.extend(messages);
    match problems.is_empty() {
        true => Ok(()),
        false => Err(format!("the app reported: {}", problems.join(" | "))),
    }
}

/// Drives the real window from the script, then ends the process with the verdict.
pub fn in_window(run: Run, window: &mut Window, cx: &mut Context<HostView>) {
    cx.spawn_in(window, async move |view, cx| {
        let verdict = drive(&run, &view, cx).await;
        if let Err(problem) = &verdict {
            eprintln!("self-test failed: {problem}");
        }
        if verdict.is_ok() {
            println!("self-test ok ({} steps, in a window)", run.steps.len());
        }
        std::process::exit(i32::from(verdict.is_err()));
    })
    .detach();
}

fn gone(problem: impl std::fmt::Display) -> String {
    format!("the window closed: {problem}")
}

async fn drive(
    run: &Run,
    view: &WeakEntity<HostView>,
    cx: &mut AsyncWindowContext,
) -> Result<(), String> {
    for (index, (verb, words)) in run.steps.iter().enumerate() {
        let began = Instant::now();
        loop {
            let done = view
                .update_in(cx, |view, window, cx| {
                    step(run, view, (index, verb, words), began, window, cx)
                })
                .map_err(gone)??;
            match done {
                Some(wait) if wait.is_zero() => break,
                Some(wait) => {
                    cx.background_executor().timer(wait).await;
                    break;
                }
                None => cx.background_executor().timer(POLL).await,
            }
        }
    }
    view.update_in(cx, |view, _, _| clean(&view.regions, view.messages.clone()))
        .map_err(gone)?
}

/// One look at a step: `None` to look again, or how long to let the display run once it is done.
fn step(
    run: &Run,
    view: &mut HostView,
    (index, verb, words): (usize, &String, &Vec<String>),
    began: Instant,
    window: &mut Window,
    cx: &mut Context<HostView>,
) -> Result<Option<Duration>, String> {
    let fail = |why: &str, view: &HostView| failed(index, verb, words, why, &view.regions);
    match attempt(&view.regions, verb, words).map_err(|why| fail(&why, view))? {
        Attempt::Waiting(why) if began.elapsed() > PATIENCE => Err(fail(&why, view)),
        Attempt::Waiting(_) => Ok(None),
        Attempt::Sending(sends) => {
            for send in &sends {
                let text = event(&view.regions, send)?;
                view.dispatch(text, window, cx);
            }
            Ok(Some(Duration::ZERO))
        }
        Attempt::Framing(count) => Ok(Some(Duration::from_secs_f64(count as f64 * FRAME_SECONDS))),
        Attempt::Recording(name) => {
            record(&run.shots, &name, &view.regions)?;
            Ok(Some(Duration::ZERO))
        }
    }
}

#[cfg(test)]
#[path = "self_test_tests.rs"]
mod tests;
