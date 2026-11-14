//! The deterministic check of the real boundary ([GPUI-HOST-VERIFICATION]): replay an
//! application's recorded typed steps, or start the probe and tap its Add button by the words it
//! shows, without a window.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::events::{self, Touch};
use crate::ffi::Library;
use crate::rows::{Ask, Shown};
use crate::screen::Screen;
use crate::store;

#[path = "smoke_checks.rs"]
mod checks;
use checks::{check_asks, check_frame};

/// The appearance every recorded run opens in, so a machine's own setting cannot change the frame.
const LIGHT: bool = false;
/// The display frame interval the smoke run builds a frame event with, in seconds.
const FRAME_SECONDS: f64 = 1. / 60.;

/// Drives the selected smoke and returns the process exit status.
pub fn run() -> i32 {
    let result = load_script().and_then(|script| match script {
        Some(script) => replay(&script).map(|steps| format!("smoke ok ({steps} steps)")),
        None => drive().map(|()| "smoke ok".to_owned()),
    });
    match result {
        Ok(message) => {
            println!("{message}");
            0
        }
        Err(problem) => {
            eprintln!("smoke failed: {problem}");
            1
        }
    }
}

/// Loads the optional application script, preserving the original probe fallback.
fn load_script() -> Result<Option<String>, String> {
    let Ok(app) = std::env::var("OSPREY_GPUI_APP") else {
        return Ok(None);
    };
    let path = Path::new(&app).join("smoke.jsonl");
    match std::fs::read_to_string(&path) {
        Ok(script) => Ok(Some(script)),
        Err(problem) if problem.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(problem) => Err(step_problem(
            1,
            format!("could not read {}: {problem}", path.display()),
        )),
    }
}

/// One recorded start observation or typed event and its assertions on the frame it draws.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    /// The typed event to send, or null to observe the first frame.
    send: Option<Send>,
    /// The assertions to make after this record's frame.
    expect: Expect,
}

/// One typed host event, named by what it does rather than encoded as an envelope.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
enum Send {
    /// A control delivering `words`, found by the words or hint it announces.
    Control(ControlSend),
    /// The pointer over the first scene, in scene units.
    Pointer(PointerSend),
    /// One key going down or up over the first scene.
    Key(KeySend),
    /// The first scene measured, in scene units.
    Viewed(ViewedSend),
    /// One display frame, `elapsed` seconds after the last.
    Frame(f64),
    /// A lifecycle phase, such as `active` or `open:<path>`.
    Phase(String),
    /// The system appearance changed.
    Appearance(bool),
    /// A capability finished under a ticket.
    Completed(CompletedSend),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlSend {
    /// The words a control announces, or a field's or picker's name in its hint.
    label: String,
    /// What the control delivers: empty for a press, text for a field or option key.
    #[serde(default)]
    words: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PointerSend {
    x: f64,
    y: f64,
    down: bool,
    #[serde(default)]
    shift: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeySend {
    key: String,
    down: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewedSend {
    width: f64,
    height: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompletedSend {
    ticket: String,
    /// The result text of a success.
    #[serde(default)]
    text: String,
    /// The reason a failure carries instead.
    reason: Option<String>,
}

/// The optional assertions a recorded step makes about one frame.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    /// Whether the frame must draw a root.
    root: Option<bool>,
    /// The number of shapes in the frame's first scene.
    primitives: Option<usize>,
    /// Capability names this frame requests, in order.
    requests: Option<Vec<String>>,
    /// One substring that must occur in what the frame shows.
    text: Option<TextExpect>,
    /// The exact number of diagnostics this frame carries.
    diagnostics: Option<usize>,
}

/// A substring assertion over every control's words and every scene's text.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextExpect {
    /// The substring at least one announced string must contain.
    contains: String,
}

/// Parses nonblank, noncomment JSON lines and names malformed source lines.
fn parse_script(script: &str) -> Result<Vec<Step>, String> {
    script
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with("//")
        })
        .map(|(index, line)| {
            serde_json::from_str(line.trim())
                .map_err(|problem| step_problem(index + 1, format!("malformed line: {problem}")))
        })
        .collect()
}

/// Replays a script through start, typed events and generic capability completion.
fn replay(script: &str) -> Result<usize, String> {
    let steps = parse_script(script)?;
    let now = clock().map_err(|problem| step_problem(1, problem))?;
    let mut shown = Shown::default();
    let started = start(now).map_err(|problem| step_problem(1, problem))?;
    let observes_start = steps.first().is_some_and(|step| step.send.is_none());
    let empty = Expect::default();
    let first = if observes_start {
        &steps[0].expect
    } else {
        &empty
    };
    settle(&mut shown, now, started, first).map_err(|problem| step_problem(1, problem))?;
    replay_events(&mut shown, now, &steps, observes_start)?;
    Ok(steps.len())
}

/// A session started at the C boundary with its requests answered, for a windowless self-test.
pub(crate) fn started() -> Result<Shown, String> {
    let now = clock()?;
    let mut shown = Shown::default();
    settle(&mut shown, now, start(now)?, &Expect::default())?;
    Ok(shown)
}

/// One typed event sent through the C boundary, its frame applied and its requests answered.
pub(crate) fn dispatched(shown: &mut Shown, touch: &Touch) -> Result<(), String> {
    let now = clock()?;
    let next = Library::event(now, touch)?;
    settle(shown, now, next, &Expect::default())
}

/// Performs the same pre-window C boundary calls as the probe smoke.
fn start(now: i64) -> Result<Shown, String> {
    wire()?;
    Library::init()?;
    Library::arguments(&crate::launch::arguments()?)?;
    Library::open(now, 0, std::env::consts::OS, &crate::home()?, LIGHT)
}

/// Dispatches each recorded event after the first frame has settled.
fn replay_events(
    shown: &mut Shown,
    now: i64,
    steps: &[Step],
    observes_start: bool,
) -> Result<(), String> {
    for (index, step) in steps.iter().enumerate().skip(usize::from(observes_start)) {
        let send = step.send.as_ref().ok_or_else(|| {
            step_problem(
                index + 1,
                "only the first record may observe the start".to_owned(),
            )
        })?;
        let touch = recorded(shown, send).map_err(|problem| step_problem(index + 1, problem))?;
        let next =
            Library::event(now, &touch).map_err(|problem| step_problem(index + 1, problem))?;
        settle(shown, now, next, &step.expect)
            .map_err(|problem| step_problem(index + 1, problem))?;
    }
    Ok(())
}

/// Turns a recorded step into the typed event it sends against the frame it arrives at.
fn recorded(shown: &Shown, send: &Send) -> Result<Touch, String> {
    match send {
        Send::Control(control) => {
            let (number, _) = named(shown, &control.label)?;
            Ok(Touch::control(number, &control.words))
        }
        Send::Pointer(pointer) => Ok(Touch::pointer(
            canvas(shown)?,
            pointer.x,
            pointer.y,
            pointer.down,
            pointer.shift,
        )),
        Send::Key(key) => Ok(Touch::key(canvas(shown)?, &key.key, key.down)),
        Send::Viewed(viewed) => Ok(Touch::viewed(canvas(shown)?, viewed.width, viewed.height)),
        Send::Frame(elapsed) => Ok(Touch::frame(*elapsed)),
        Send::Phase(phase) => Ok(Touch::phase(phase)),
        Send::Appearance(dark) => Ok(Touch::appearance(*dark)),
        Send::Completed(completed) => Ok(Touch::completed(
            &completed.ticket,
            match &completed.reason {
                Some(reason) => Err(reason.clone()),
                None => Ok(completed.text.clone()),
            },
        )),
    }
}

/// The bound control a recorded label names.
fn named(shown: &Shown, label: &str) -> Result<(i64, bool), String> {
    Screen::new(shown)
        .control(label)
        .ok_or_else(|| format!("no control offers {label:?}"))
}

/// The control number of the first scene's canvas.
fn canvas(shown: &Shown) -> Result<i64, String> {
    Screen::new(shown)
        .scene()
        .map(|(number, _)| number)
        .ok_or_else(|| "the frame draws no scene to send to".to_owned())
}

/// Applies and checks one frame, then answers every request it produced.
fn settle(shown: &mut Shown, now: i64, next: Shown, expect: &Expect) -> Result<(), String> {
    check_asks(&next, expect)?;
    adopt(shown, next)?;
    check_frame(shown, expect)?;
    let requests: Vec<Ask> = shown.requests().cloned().collect();
    answer_all(shown, now, &requests)
}

/// Keeps the frame's slice writes, then makes it the current frame.
fn adopt(shown: &mut Shown, next: Shown) -> Result<(), String> {
    for (key, text) in next.slices() {
        crate::ffi::put_slice(key, text)?;
    }
    *shown = next;
    Ok(())
}

/// Answers capability requests in protocol order, including follow-up requests.
fn answer_all(shown: &mut Shown, now: i64, requests: &[Ask]) -> Result<(), String> {
    for ask in requests {
        answer_one(shown, now, ask)?;
    }
    Ok(())
}

/// Dispatches one generic completion and adopts the frame it draws.
fn answer_one(shown: &mut Shown, now: i64, ask: &Ask) -> Result<(), String> {
    let completion = completion(ask)?;
    let next = Library::event(now, &completion)?;
    adopt(shown, next)?;
    let requests: Vec<Ask> = shown.requests().cloned().collect();
    answer_all(shown, now, &requests)
}

/// Builds the generic smoke answer for one supported capability.
fn completion(ask: &Ask) -> Result<Touch, String> {
    let outcome = match ask.kind() {
        "animate" | "store-write" | "run" => Ok(String::new()),
        "store-read" => Err(store::missing_read(ask)?),
        kind => {
            return Err(format!(
                "unsupported capability {kind} under {}",
                ask.ticket
            ))
        }
    };
    Ok(Touch::completed(&ask.ticket, outcome))
}

/// Drives the original probe and checks its Add button and resulting count.
fn drive() -> Result<(), String> {
    let now = clock()?;
    let shown = start(now)?;
    settled(&shown)?;
    if shown.roots.is_empty() {
        return Err("the first frame draws no root".to_owned());
    }
    let (number, _) = named(&shown, "Add")
        .map_err(|_| "the first frame has no button labelled Add".to_owned())?;
    let tapped = Library::event(now, &Touch::control(number, ""))?;
    settled(&tapped)?;
    match Screen::new(&tapped).shows("Count 1") {
        true => Ok(()),
        false => Err("the frame does not read \"Count 1\" after the tap".to_owned()),
    }
}

/// Refuses capabilities and diagnostics the probe smoke run cannot answer.
fn settled(shown: &Shown) -> Result<(), String> {
    if let Some(ask) = shown.requests().next() {
        return Err(format!(
            "the application asked for {} under {}, and this smoke run answers no capability",
            ask.kind(),
            ask.ticket
        ));
    }
    match shown.diagnostics().next() {
        None => Ok(()),
        Some(_) => Err(format!(
            "the application reported {}",
            shown.diagnostics().collect::<Vec<_>>().join("; ")
        )),
    }
}

/// Builds every event kind once, so a builder the smoke run never dispatches still cannot drift
/// from the kinds `ui::Hosting` declares.
fn wire() -> Result<(), String> {
    let built = [
        (events::kind::CONTROL, Touch::control(7, "true")),
        (events::kind::KEY, Touch::key(7, "left", true)),
        (
            events::kind::POINTER,
            Touch::pointer(7, 1., 2., true, false),
        ),
        (events::kind::VIEWED, Touch::viewed(7, 640., 480.)),
        (
            events::kind::COMPLETED,
            Touch::completed("smoke:1", Ok(String::new())),
        ),
        (
            events::kind::COMPLETED,
            Touch::completed("smoke:1", Err("no adapter in the smoke run".to_owned())),
        ),
        (events::kind::PHASE, Touch::phase("visible")),
        (events::kind::FRAME, Touch::frame(FRAME_SECONDS)),
        (events::kind::APPEARANCE, Touch::appearance(true)),
    ];
    for (kind, touch) in built {
        if touch.kind != kind {
            return Err(format!("The host built a malformed event: {touch:?}"));
        }
    }
    let (finished, failed) = (
        Touch::completed("smoke:1", Ok("done".to_owned())),
        Touch::completed("smoke:1", Err("refused".to_owned())),
    );
    match finished.down && !failed.down && finished.detail == "done" && failed.detail == "refused" {
        true => Ok(()),
        false => Err("The host did not carry a completion's outcome".to_owned()),
    }
}

/// Unix seconds, the clock every event carries.
fn clock() -> Result<i64, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|problem| format!("The system clock is before the Unix epoch: {problem}"))?
        .as_secs();
    i64::try_from(seconds).map_err(|_| "The system clock is beyond i64 seconds".to_owned())
}

/// Prefixes a replay failure with its one-based script line number.
fn step_problem(step: usize, problem: String) -> String {
    format!("step {step}: {problem}")
}

/// Unit tests for recorded script syntax and source-line diagnostics.
#[cfg(test)]
#[path = "smoke_tests.rs"]
mod tests;
