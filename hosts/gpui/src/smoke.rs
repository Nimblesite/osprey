//! The deterministic phase-0 check of the real boundary ([GPUI-HOST-VERIFICATION]):
//! start the probe, tap its Add button by the ordinal the node carries and
//! require "Count 1", without a window.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::envelope::{self, Envelope};
use crate::events;
use crate::ffi::Library;
use crate::regions::Regions;

/// Drives the probe and returns the process exit status.
pub fn run() -> i32 {
    match drive() {
        Ok(()) => {
            println!("smoke ok");
            0
        }
        Err(problem) => {
            eprintln!("smoke failed: {problem}");
            1
        }
    }
}

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
    let ordinal =
        button_event(&root.node, "Add").ok_or("the root region has no button labelled Add")?;
    let event = events::control(now, "root", revision, ordinal, Value::Null);
    let next = envelope::parse(&Library::dispatch(regions.state(), &event)?)?;
    settled(&mut regions, next)?;
    let root = regions
        .get("root")
        .ok_or("the root region disappeared after the tap")?;
    match has_text(&root.node, "Count 1") {
        true => Ok(()),
        false => Err(format!(
            "the root region does not read \"Count 1\" after the tap: {}",
            root.node
        )),
    }
}

/// Applies one envelope, refusing a capability request or diagnostic the smoke
/// run cannot answer.
fn settled(regions: &mut Regions, envelope: Envelope) -> Result<(), String> {
    if let Some(request) = regions.apply(envelope).first() {
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
    let home = std::env::var("HOME").map_err(|problem| format!("HOME is not set: {problem}"))?;
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    // The probe reads the platform and ignores the offset, so the smoke run
    // reports UTC rather than take a dependency to resolve local time.
    Ok(events::start_context(now, 0, "macos", &home, &arguments))
}

/// Unix seconds, the clock every event carries.
fn clock() -> Result<i64, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|problem| format!("The system clock is before the Unix epoch: {problem}"))?
        .as_secs();
    i64::try_from(seconds).map_err(|_| "The system clock is beyond i64 seconds".to_owned())
}

/// The ordinal the framework issued to the button labelled `label`; it is read
/// from the node, never assumed.
fn button_event(node: &Value, label: &str) -> Option<u64> {
    let named = node.get("kind").and_then(Value::as_str) == Some("button")
        && node.get("label").and_then(Value::as_str) == Some(label);
    match named {
        true => node.get("event").and_then(Value::as_u64),
        false => children(node).find_map(|child| button_event(child, label)),
    }
}

/// Whether a text node reading exactly `text` appears in `node`'s tree.
fn has_text(node: &Value, text: &str) -> bool {
    let shown = node.get("kind").and_then(Value::as_str) == Some("text")
        && node.get("text").and_then(Value::as_str) == Some(text);
    shown || children(node).any(|child| has_text(child, text))
}

/// Everything held inside a node, so a walk never depends on where a container
/// keeps its children.
fn children(node: &Value) -> impl Iterator<Item = &Value> {
    let inside: Vec<&Value> = match node {
        Value::Object(members) => members.values().collect(),
        Value::Array(items) => items.iter().collect(),
        _ => Vec::new(),
    };
    inside.into_iter()
}
