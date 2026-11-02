//! The deterministic check of the real boundary ([GPUI-HOST-VERIFICATION]):
//! replay an application's recorded events, or start the probe and tap its Add
//! button by the ordinal the node carries, without a window.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::Value;

use crate::envelope::{self, Envelope, Request};
use crate::events;
use crate::ffi::Library;
use crate::regions::Regions;

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

/// One recorded start observation or event and its envelope assertions.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    /// A verbatim event, a control lookup, or null to observe the start envelope.
    send: Value,
    /// The assertions to make after this record's envelope.
    expect: Expect,
}

/// The optional assertions a recorded step makes about one envelope.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    /// Region keys that must exist after applying the envelope.
    regions: Option<Vec<String>>,
    /// Region keys that must occur among this envelope's patches.
    patched: Option<Vec<String>>,
    /// One substring that must occur in a region's string values.
    text: Option<TextExpect>,
    /// Capability kinds this envelope must request, in order.
    requests: Option<Vec<String>>,
    /// The exact number of diagnostics accumulated by the host.
    diagnostics: Option<usize>,
}

/// A substring assertion over every string value in a region tree.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextExpect {
    /// The region whose current tree is searched.
    region: String,
    /// The substring at least one string value must contain.
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

/// Replays a script through start, dispatch and generic capability completion.
fn replay(script: &str) -> Result<usize, String> {
    let steps = parse_script(script)?;
    let now = clock().map_err(|problem| step_problem(1, problem))?;
    let started = start(now).map_err(|problem| step_problem(1, problem))?;
    let observes_start = steps.first().is_some_and(|step| step.send.is_null());
    let empty = Expect::default();
    let first = if observes_start {
        &steps[0].expect
    } else {
        &empty
    };
    let mut regions = Regions::new();
    settle(&mut regions, now, started, first).map_err(|problem| step_problem(1, problem))?;
    replay_events(&mut regions, now, &steps, observes_start)?;
    Ok(steps.len())
}

/// A session started at the C boundary with its requests answered, for a windowless self-test.
pub(crate) fn started() -> Result<Regions, String> {
    let now = clock()?;
    let mut regions = Regions::new();
    settle(&mut regions, now, start(now)?, &Expect::default())?;
    Ok(regions)
}

/// One event sent through the C boundary, its envelope applied and its requests answered.
pub(crate) fn dispatched(regions: &mut Regions, event: &str) -> Result<(), String> {
    let next = envelope::parse(&Library::dispatch(regions.state(), event)?)?;
    settle(regions, clock()?, next, &Expect::default())
}

/// Performs the same pre-window C boundary calls as the probe smoke.
fn start(now: i64) -> Result<Envelope, String> {
    Library::init()?;
    wire(now)?;
    envelope::parse(&Library::start(&launch_context(now)?)?)
}

/// Dispatches each recorded event after the initial envelope has settled.
fn replay_events(
    regions: &mut Regions,
    now: i64,
    steps: &[Step],
    observes_start: bool,
) -> Result<(), String> {
    for (index, step) in steps.iter().enumerate().skip(usize::from(observes_start)) {
        let event = recorded_event(regions, now, &step.send)
            .map_err(|problem| step_problem(index + 1, problem))?;
        let next = Library::dispatch(regions.state(), &event)
            .and_then(|text| envelope::parse(&text))
            .map_err(|problem| step_problem(index + 1, problem))?;
        settle(regions, now, next, &step.expect)
            .map_err(|problem| step_problem(index + 1, problem))?;
    }
    Ok(())
}

/// Applies and checks one envelope, then answers every request it produced.
fn settle(
    regions: &mut Regions,
    now: i64,
    envelope: Envelope,
    expect: &Expect,
) -> Result<(), String> {
    check_envelope(&envelope, expect)?;
    let requests = regions.apply(envelope)?;
    check_regions(regions, expect)?;
    answer_all(regions, now, requests)
}

/// Answers capability requests in protocol order, including follow-up requests.
fn answer_all(regions: &mut Regions, now: i64, requests: Vec<Request>) -> Result<(), String> {
    for request in requests {
        answer_one(regions, now, &request)?;
    }
    Ok(())
}

/// Dispatches one generic completion and settles its resulting envelope.
fn answer_one(regions: &mut Regions, now: i64, request: &Request) -> Result<(), String> {
    let completion = completion(now, request)?;
    let next = envelope::parse(&Library::dispatch(regions.state(), &completion)?)?;
    let requests = regions.apply(next)?;
    answer_all(regions, now, requests)
}

/// Builds the generic smoke answer for one supported capability.
fn completion(now: i64, request: &Request) -> Result<String, String> {
    match request.kind.as_str() {
        "animate" | "store-write" => Ok(events::completed_ok(now, &request.ticket, Value::Null)),
        "store-read" => Ok(events::completed_err(
            now,
            &request.ticket,
            "no such store entry",
        )),
        // Nothing is started: the program printed nothing and succeeded.
        "run" => Ok(events::completed_ok(
            now,
            &request.ticket,
            serde_json::json!({"status": 0, "stderr": ""}),
        )),
        kind => Err(format!(
            "unsupported capability {} under {}",
            kind, request.ticket
        )),
    }
}

/// Checks patch membership and the ordered capability request list.
fn check_envelope(envelope: &Envelope, expect: &Expect) -> Result<(), String> {
    let patches: Vec<&str> = envelope
        .patches
        .iter()
        .map(|patch| patch.key.as_str())
        .collect();
    if let Some(expected) = &expect.patched {
        for key in expected {
            if !patches.contains(&key.as_str()) {
                return Err(format!(
                    "region {key} was not patched; patches were {patches:?}"
                ));
            }
        }
    }
    check_requests(envelope, expect.requests.as_deref())
}

/// Checks the exact ordered request list when a step supplies one.
fn check_requests(envelope: &Envelope, expected: Option<&[String]>) -> Result<(), String> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual: Vec<&str> = envelope
        .requests
        .iter()
        .map(|request| request.kind.as_str())
        .collect();
    let wanted: Vec<&str> = expected.iter().map(String::as_str).collect();
    match actual == wanted {
        true => Ok(()),
        false => Err(format!("requests were {actual:?}, expected {wanted:?}")),
    }
}

/// Checks current region existence, text and accumulated diagnostics.
fn check_regions(regions: &Regions, expect: &Expect) -> Result<(), String> {
    if let Some(expected) = &expect.regions {
        for key in expected {
            if regions.get(key).is_none() {
                let keys: Vec<&str> = regions.keys().collect();
                return Err(format!("region {key} is absent; regions were {keys:?}"));
            }
        }
    }
    check_text(regions, expect.text.as_ref())?;
    check_diagnostics(regions, expect.diagnostics)
}

/// Checks one substring against all strings in the selected current region.
fn check_text(regions: &Regions, expect: Option<&TextExpect>) -> Result<(), String> {
    let Some(expect) = expect else {
        return Ok(());
    };
    let region = regions
        .get(&expect.region)
        .ok_or_else(|| format!("region {} is absent for a text assertion", expect.region))?;
    match has_text(&region.node, &expect.contains, true) {
        true => Ok(()),
        false => Err(format!(
            "region {} has no string containing {:?}: {}",
            expect.region, expect.contains, region.node
        )),
    }
}

/// Checks the exact accumulated diagnostic count when a step supplies one.
fn check_diagnostics(regions: &Regions, expected: Option<usize>) -> Result<(), String> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = regions.diagnostics();
    match actual.len() == expected {
        true => Ok(()),
        false => Err(format!(
            "diagnostics were {actual:?}, expected {expected} entries"
        )),
    }
}

/// Builds either a verbatim recorded event or a current control event.
fn recorded_event(regions: &Regions, now: i64, send: &Value) -> Result<String, String> {
    match send.get("control") {
        Some(_) => control_event(regions, now, send),
        None => verbatim_event(now, send),
    }
}

/// Copies a full event verbatim except for filling an absent timestamp.
fn verbatim_event(now: i64, send: &Value) -> Result<String, String> {
    let mut event = send
        .as_object()
        .cloned()
        .ok_or_else(|| "send must be an event object or control lookup".to_owned())?;
    event
        .entry("now")
        .or_insert_with(|| Value::Number(now.into()));
    Ok(Value::Object(event).to_string())
}

/// Resolves a control against the current region tree and revision.
fn control_event(regions: &Regions, now: i64, send: &Value) -> Result<String, String> {
    let control = send
        .get("control")
        .and_then(Value::as_object)
        .ok_or_else(|| "control must be an object".to_owned())?;
    let key = control
        .get("region")
        .and_then(Value::as_str)
        .ok_or_else(|| "control.region must be a string".to_owned())?;
    let label = control_label(control.get("label"))?;
    let value = send.get("value").cloned().ok_or("control needs value")?;
    current_control(regions, now, key, label, value)
}

/// Resolves a validated control against its current region.
fn current_control(
    regions: &Regions,
    now: i64,
    key: &str,
    label: Option<&str>,
    value: Value,
) -> Result<String, String> {
    let region = regions
        .get(key)
        .ok_or_else(|| format!("region {key} is absent"))?;
    let kind = if label.is_none() { Some("scene") } else { None };
    let ordinal = node_event(&region.node, kind, label)
        .ok_or_else(|| format!("region {key} has no matching bound node"))?;
    Ok(events::control(now, key, region.revision, ordinal, value))
}

/// Validates the optional control label without confusing absence with a nonstring.
fn control_label(label: Option<&Value>) -> Result<Option<&str>, String> {
    match label {
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| "control.label must be a string".to_owned()),
        None => Ok(None),
    }
}

/// Drives the original probe and checks its Add button and resulting count.
fn drive() -> Result<(), String> {
    let now = clock()?;
    Library::init()?;
    wire(now)?;
    let mut regions = Regions::new();
    let started = envelope::parse(&Library::start(&launch_context(now)?)?)?;
    settled(&mut regions, started)?;
    let keys: Vec<&str> = regions.keys().collect();
    for expected in ["root", "note"] {
        if !keys.contains(&expected) {
            return Err(format!(
                "the start envelope has no {expected} region; it holds [{}]",
                keys.join(", ")
            ));
        }
    }
    if regions.theme().is_none() {
        return Err("the start envelope carried no theme".to_owned());
    }
    let root = regions
        .get("root")
        .ok_or("the start envelope has no root region")?;
    let revision = root.revision;
    let ordinal = node_event(&root.node, Some("button"), Some("Add"))
        .ok_or("the root region has no button labelled Add")?;
    let event = events::control(now, "root", revision, ordinal, Value::Null);
    let next = envelope::parse(&Library::dispatch(regions.state(), &event)?)?;
    settled(&mut regions, next)?;
    let root = regions
        .get("root")
        .ok_or("the root region disappeared after the tap")?;
    match has_text(&root.node, "Count 1", false) {
        true => Ok(()),
        false => Err(format!(
            "the root region does not read \"Count 1\" after the tap: {}",
            root.node
        )),
    }
}

/// Applies one probe envelope, refusing capabilities and diagnostics.
fn settled(regions: &mut Regions, envelope: Envelope) -> Result<(), String> {
    if let Some(request) = regions.apply(envelope)?.first() {
        return Err(format!(
            "the application asked for {} under {}, and this smoke run answers no capability: {}",
            request.kind,
            request.ticket,
            Value::Object(request.fields.clone())
        ));
    }
    match regions.diagnostics().is_empty() {
        true => Ok(()),
        false => Err(format!(
            "the application reported {}",
            regions.diagnostics().join("; ")
        )),
    }
}

/// Builds every event kind once, so a builder the smoke run never dispatches
/// still cannot drift from `ui::Events`.
fn wire(now: i64) -> Result<(), String> {
    let built = [
        (
            "completed",
            events::completed_ok(now, "smoke:1", Value::Null),
        ),
        (
            "completed",
            events::completed_err(now, "smoke:1", "no adapter in the smoke run"),
        ),
        ("lifecycle", events::lifecycle(now, "visible")),
    ];
    for (kind, text) in built {
        let decoded: Value = serde_json::from_str(&text)
            .map_err(|problem| format!("The host built an event that is not JSON: {problem}"))?;
        let named = decoded.get("kind").and_then(Value::as_str) == Some(kind);
        if !named || decoded.get("now").and_then(Value::as_i64) != Some(now) {
            return Err(format!("The host built a malformed {kind} event: {text}"));
        }
    }
    Ok(())
}

/// The launch context the framework reads: the host platform, the person's
/// home folder and the arguments the application was opened with.
fn launch_context(now: i64) -> Result<String, String> {
    let home = crate::home()?;
    let arguments = crate::self_test::app_arguments();
    // The probe reads the platform and ignores the offset, so the smoke run
    // reports UTC rather than take a dependency to resolve local time.
    Ok(events::start_context(
        now,
        0,
        std::env::consts::OS,
        &home,
        &arguments,
    ))
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

/// Finds the first node matching an optional kind and label and reads its binding.
fn node_event(node: &Value, kind: Option<&str>, label: Option<&str>) -> Option<u64> {
    let kind_matches =
        kind.is_none_or(|kind| node.get("kind").and_then(Value::as_str) == Some(kind));
    let label_matches =
        label.is_none_or(|label| node.get("label").and_then(Value::as_str) == Some(label));
    match kind_matches && label_matches {
        true => node.get("event").and_then(Value::as_u64),
        false => children(node).find_map(|child| node_event(child, kind, label)),
    }
}

/// Whether a tree has exact text, or any string containing it when `substring` is true.
fn has_text(node: &Value, text: &str, substring: bool) -> bool {
    let candidate = match substring {
        true => node.as_str(),
        false if node.get("kind").and_then(Value::as_str) == Some("text") => {
            node.get("text").and_then(Value::as_str)
        }
        false => None,
    };
    let shown = candidate.is_some_and(|value| value == text || (substring && value.contains(text)));
    shown || children(node).any(|child| has_text(child, text, substring))
}

/// Everything held inside a node, so a walk never depends on where a container
/// keeps its children.
pub(crate) fn children(node: &Value) -> impl Iterator<Item = &Value> {
    let inside: Vec<&Value> = match node {
        Value::Object(members) => members.values().collect(),
        Value::Array(items) => items.iter().collect(),
        _ => Vec::new(),
    };
    inside.into_iter()
}

/// Unit tests for recorded script syntax and source-line diagnostics.
#[cfg(test)]
#[path = "smoke_tests.rs"]
mod tests;
