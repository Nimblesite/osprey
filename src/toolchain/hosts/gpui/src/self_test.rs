//! One script beside each application, run with or without a window ([UI-MACHINE-SELF-TEST]).
//! Its steps name only what a screen reader announces, so the same file drives the WinUI host, this
//! host's real window and this host's C boundary alone. Every failure prints the accessibility
//! tree, and every `shot` keeps it beside the run, so an agent reads what it may act on.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::*;
use serde_json::Value;

use crate::events::Touch;
use crate::host::HostView;
use crate::rows::Shown;
use crate::screen::Screen;
use crate::smoke;

#[path = "self_test_find.rs"]
mod find;
use find::{clicked, dragged, valued};

/// How long a windowed step waits for what it needs before it fails, and how often it looks.
const PATIENCE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(50);
/// How many display frames a windowless `wait` or `gone` runs before it fails.
const FRAMES: usize = 600;
const FRAME_SECONDS: f64 = 1. / 60.;

/// What a step needs next.
#[derive(Debug, PartialEq)]
pub enum Attempt {
    /// Not yet true; the words say what is missing.
    Waiting(String),
    /// These events in order, then the step is done.
    Sending(Vec<Touch>),
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

/// What one step needs of the frame as it stands.
pub fn attempt(shown: &Shown, verb: &str, words: &[String]) -> Result<Attempt, String> {
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
    let screen = Screen::new(shown);
    Ok(match verb {
        "wait" if screen.shows(word(0)?) => Attempt::Sending(Vec::new()),
        "wait" => Attempt::Waiting(format!("nothing shows \"{}\"", word(0)?)),
        "gone" if !screen.shows(word(0)?) => Attempt::Sending(Vec::new()),
        "gone" => Attempt::Waiting(format!("\"{}\" still shows", word(0)?)),
        "invoke" => match (screen.element(word(0)?), screen.control(word(0)?)) {
            (Some((action, access)), _) => clicked(access, action),
            (None, Some((action, _))) => Attempt::Sending(vec![Touch::control(action, "")]),
            (None, None) => Attempt::Waiting(format!("nothing offers \"{}\"", word(0)?)),
        },
        "set" => match screen.element(word(0)?) {
            Some((action, access)) => valued(access, action, number(1)?)?,
            None => Attempt::Waiting(format!("no slider \"{}\"", word(0)?)),
        },
        "drag" => match screen.element(word(0)?) {
            Some((action, access)) => dragged(access, action, number(1)?, number(2)?),
            None => Attempt::Waiting(format!("nothing to drag called \"{}\"", word(0)?)),
        },
        "toggle" => match screen.control(word(0)?) {
            Some((action, chosen)) => {
                let words = match chosen {
                    true => "false",
                    false => "true",
                };
                Attempt::Sending(vec![Touch::control(action, words)])
            }
            None => Attempt::Waiting(format!("no switch \"{}\"", word(0)?)),
        },
        "type" => match screen.control(word(0)?) {
            Some((action, _)) => Attempt::Sending(vec![Touch::control(action, word(1)?)]),
            None => Attempt::Waiting(format!("no field \"{}\"", word(0)?)),
        },
        "choose" => match screen.chooser(word(0)?) {
            Some(found) => Attempt::Sending(vec![found]),
            None => Attempt::Waiting(format!("no row \"{}\"", word(0)?)),
        },
        "key" => match screen.scene() {
            Some((action, _)) => Attempt::Sending(vec![
                Touch::key(action, word(0).unwrap_or_default(), true),
                Touch::key(action, word(0).unwrap_or_default(), false),
            ]),
            None => Attempt::Waiting("no scene is drawn to take a key".to_owned()),
        },
        "frames" => Attempt::Framing(number(0)? as usize),
        "sleep" => Attempt::Framing((number(0)? / FRAME_SECONDS) as usize),
        "shot" => Attempt::Recording(word(0)?.to_owned()),
        other => return Err(format!("there is no step called {other}")),
    })
}

/// The accessibility tree as an agent reads it: one line per thing it can act on, in drawing order.
pub fn tree(shown: &Shown) -> String {
    Screen::new(shown).outline()
}

fn record(shots: &Path, name: &str, shown: &Shown) -> Result<(), String> {
    let path = shots.join(format!("{name}.access.txt"));
    std::fs::write(&path, tree(shown) + "\n")
        .map_err(|problem| format!("could not write {}: {problem}", path.display()))
}

/// Saves what the window shows beside the run, and refuses a blank picture: the shot must
/// have size and at least two different pixels, or the step fails saying why.
fn capture(shots: &Path, name: &str, window: &mut Window) -> Result<(), String> {
    let path = shots.join(format!("{name}.png"));
    window
        .render_to_image()
        .map_err(|problem| format!("could not render {}: {problem:?}", path.display()))?
        .save(&path)
        .map_err(|problem| format!("could not save {}: {problem}", path.display()))?;
    let shot = image::open(&path)
        .map_err(|problem| format!("could not read back {}: {problem}", path.display()))?
        .to_rgba8();
    let (width, height) = shot.dimensions();
    if width == 0 || height == 0 {
        return Err(format!(
            "the shot {} is empty: {width} by {height}",
            path.display()
        ));
    }
    let first = shot.get_pixel(0, 0);
    if shot.pixels().all(|pixel| pixel == first) {
        return Err(format!(
            "the shot {} is blank: every one of its {} pixels is the same",
            path.display(),
            width * height
        ));
    }
    Ok(())
}

/// A failure, with what the app offers, so an agent can see what it may act on instead.
fn failed(index: usize, verb: &str, words: &[String], why: &str, shown: &Shown) -> String {
    let reported: Vec<&str> = shown.diagnostics().collect();
    format!(
        "step {} {verb} {}: {why}\nthe accessibility tree offers:\n{}\ndiagnostics: {reported:?}",
        index + 1,
        words.join(" = "),
        tree(shown)
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
    let mut shown = smoke::started()?;
    let mut reported: Vec<String> = shown.diagnostics().map(str::to_owned).collect();
    for (index, (verb, words)) in run.steps.iter().enumerate() {
        let mut frames = 0;
        loop {
            let fail = |why: &str, shown: &Shown| failed(index, verb, words, why, shown);
            match attempt(&shown, verb, words).map_err(|why| fail(&why, &shown))? {
                Attempt::Waiting(why) if frames >= FRAMES => return Err(fail(&why, &shown)),
                Attempt::Waiting(_) => {
                    frame(&mut shown)?;
                    reported.extend(shown.diagnostics().map(str::to_owned));
                    frames += 1;
                }
                Attempt::Sending(sends) => {
                    for send in &sends {
                        smoke::dispatched(&mut shown, send)?;
                        reported.extend(shown.diagnostics().map(str::to_owned));
                    }
                    break;
                }
                Attempt::Framing(count) => {
                    for _ in 0..count {
                        frame(&mut shown)?;
                        reported.extend(shown.diagnostics().map(str::to_owned));
                    }
                    break;
                }
                Attempt::Recording(name) => {
                    record(&run.shots, &name, &shown)?;
                    break;
                }
            }
        }
    }
    clean(reported)?;
    Ok(run.steps.len())
}

fn frame(shown: &mut Shown) -> Result<(), String> {
    smoke::dispatched(shown, &Touch::frame(FRAME_SECONDS))
}

fn clean(mut reported: Vec<String>) -> Result<(), String> {
    reported.dedup();
    match reported.is_empty() {
        true => Ok(()),
        false => Err(format!("the app reported: {}", reported.join(" | "))),
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
        if verb == "shot" {
            rendered(cx).await?;
        }
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
    view.update_in(cx, |view, _, _| clean(view.messages.clone()))
        .map_err(gone)?
}

/// A state assertion may pass before its redraw. Capture only after a real frame,
/// bounded by the same deadline as every other windowed observation.
async fn rendered(cx: &mut AsyncWindowContext) -> Result<(), String> {
    let ready = Arc::new(AtomicBool::new(false));
    let completed = Arc::clone(&ready);
    cx.update(move |window, _| {
        window.refresh();
        window.on_next_frame(move |_, _| completed.store(true, Ordering::Release));
    })
    .map_err(gone)?;
    let began = Instant::now();
    while !ready.load(Ordering::Acquire) {
        if began.elapsed() > PATIENCE {
            return Err("the window did not render the requested screenshot frame".to_owned());
        }
        cx.background_executor().timer(POLL).await;
    }
    Ok(())
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
    let fail = |why: &str, view: &HostView| failed(index, verb, words, why, view.shown.as_ref());
    match attempt(view.shown.as_ref(), verb, words).map_err(|why| fail(&why, view))? {
        Attempt::Waiting(why) if began.elapsed() > PATIENCE => Err(fail(&why, view)),
        Attempt::Waiting(_) => Ok(None),
        Attempt::Sending(sends) => {
            for send in &sends {
                view.send(send.clone(), window, cx);
            }
            Ok(Some(Duration::ZERO))
        }
        Attempt::Framing(count) => Ok(Some(Duration::from_secs_f64(count as f64 * FRAME_SECONDS))),
        Attempt::Recording(name) => {
            record(&run.shots, &name, view.shown.as_ref())?;
            capture(&run.shots, &name, window)?;
            Ok(Some(Duration::ZERO))
        }
    }
}

#[cfg(test)]
#[path = "self_test_tests.rs"]
mod tests;
