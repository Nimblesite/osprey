//! The self-test's reading of a script and of what the app offers, with every verb and its refusals.

use serde_json::json;

use super::{attempt, parse, tree, Attempt};
use crate::envelope;
use crate::regions::Regions;

fn regions() -> Regions {
    let envelope = json!({"version": envelope::VERSION, "state": "s", "requests": [], "patches": [{"key": "root", "revision": 7, "node": {"kind": "stack", "children": [
        {"kind": "button", "label": "Add", "event": 1},
        {"kind": "field", "label": "Note", "event": 2},
        {"kind": "toggle", "label": "Loud", "on": true, "event": 3},
        {"kind": "picker", "label": "Rate", "event": 4, "choices": [{"key": "a", "label": "Alpha"}]},
        {"kind": "text", "text": "Count 0"},
        {"kind": "scene", "event": 5, "access": [
            {"role": "button", "label": "Play", "x": 10, "y": 20, "w": 100, "h": 40},
            {"role": "slider", "label": "Volume", "x": 100, "y": 0, "w": 200, "h": 10, "value": 0.5, "min": 0, "max": 1}
        ]}
    ]}}]});
    let mut regions = Regions::new();
    regions
        .apply(envelope::parse(&envelope.to_string()).unwrap())
        .unwrap();
    regions
}

fn step(verb: &str, words: &[&str]) -> Result<Attempt, String> {
    let words: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    attempt(&regions(), verb, &words)
}

fn values(attempt: Result<Attempt, String>) -> Vec<(u64, serde_json::Value)> {
    match attempt.unwrap() {
        Attempt::Sending(sends) => sends.into_iter().map(|s| (s.ordinal, s.value)).collect(),
        other => panic!("not a send: {other:?}"),
    }
}

/// A script is a list of one-verb steps, and anything else says what is wrong.
#[test]
fn parses_steps_and_refuses_malformed_scripts() {
    let steps = parse(r#"{"steps":[{"wait":"Go"},{"set":["Volume","0.5"]}]}"#).unwrap();
    assert_eq!(steps[0], ("wait".to_owned(), vec!["Go".to_owned()]));
    assert_eq!(steps[1].1, vec!["Volume".to_owned(), "0.5".to_owned()]);
    assert!(parse("nope").unwrap_err().contains("malformed"));
    assert!(parse("{}").unwrap_err().contains("list of steps"));
    assert!(parse(r#"{"steps":[{"a":"b","c":"d"}]}"#)
        .unwrap_err()
        .contains("one verb"));
}

/// Every word on screen is found, and a missing one waits rather than acts.
#[test]
fn wait_and_gone_read_every_string_on_screen() {
    assert_eq!(step("wait", &["Count 0"]), Ok(Attempt::Sending(vec![])));
    assert_eq!(step("wait", &["Play"]), Ok(Attempt::Sending(vec![])));
    assert!(matches!(
        step("wait", &["Count 1"]),
        Ok(Attempt::Waiting(_))
    ));
    assert_eq!(step("gone", &["Count 1"]), Ok(Attempt::Sending(vec![])));
    assert!(matches!(
        step("gone", &["Count 0"]),
        Ok(Attempt::Waiting(_))
    ));
}

/// One verb reaches a scene's element and an ordinary control alike, and a pointer lands where a hand would.
#[test]
fn invoke_presses_scene_elements_and_controls_by_label() {
    assert_eq!(
        values(step("invoke", &["Play"])),
        vec![
            (
                5,
                json!({"x": 60.0, "y": 40.0, "down": true, "shift": false})
            ),
            (
                5,
                json!({"x": 60.0, "y": 40.0, "down": false, "shift": false})
            )
        ]
    );
    assert_eq!(values(step("invoke", &["Add"])), vec![(1, json!(null))]);
    assert!(matches!(step("invoke", &["Nope"]), Ok(Attempt::Waiting(_))));
}

/// A slider lands at its value, clamped to its range; what has no range refuses it.
#[test]
fn set_lands_along_a_slider_and_refuses_anything_else() {
    let at = |value: &str| values(step("set", &["Volume", value]))[0].1["x"].clone();
    assert_eq!(at("0.25"), json!(150.0));
    assert_eq!(at("9"), json!(300.0));
    assert_eq!(at("-1"), json!(100.0));
    assert!(step("set", &["Play", "0.5"])
        .unwrap_err()
        .contains("no range"));
    assert!(matches!(
        step("set", &["Nope", "1"]),
        Ok(Attempt::Waiting(_))
    ));
    assert!(step("set", &["Volume", "loud"])
        .unwrap_err()
        .contains("number"));
    assert!(step("set", &["Volume"]).unwrap_err().contains("needs 2"));
}

/// A drag grips the middle, moves and releases where it moved to.
#[test]
fn drag_presses_moves_and_releases() {
    let sent = values(step("drag", &["Play", "5", "-5"]));
    assert_eq!(sent[0].1["x"], json!(60.0));
    assert_eq!(
        (sent[1].1["x"].clone(), sent[1].1["y"].clone()),
        (json!(65.0), json!(35.0))
    );
    assert_eq!(sent[2].1["down"], json!(false));
}

/// Switches flip, fields take their words, pickers choose by what a row shows and scenes take keys.
#[test]
fn toggle_type_choose_and_key_send_what_the_control_takes() {
    assert_eq!(values(step("toggle", &["Loud"])), vec![(3, json!(false))]);
    assert_eq!(
        values(step("type", &["Note", "hi"])),
        vec![(2, json!("hi"))]
    );
    assert_eq!(values(step("choose", &["Alpha"])), vec![(4, json!("a"))]);
    assert_eq!(
        values(step("key", &["p"])),
        vec![
            (5, json!({"key": "p", "down": true})),
            (5, json!({"key": "p", "down": false}))
        ]
    );
    assert!(matches!(step("choose", &["Nope"]), Ok(Attempt::Waiting(_))));
}

/// Frames and sleeps count display frames, a shot records and an unknown verb is refused.
#[test]
fn time_shots_and_unknown_verbs() {
    assert_eq!(step("frames", &["180"]), Ok(Attempt::Framing(180)));
    assert_eq!(step("sleep", &["1"]), Ok(Attempt::Framing(60)));
    assert_eq!(
        step("shot", &["1-go"]),
        Ok(Attempt::Recording("1-go".to_owned()))
    );
    assert!(step("tap", &["1", "2"])
        .unwrap_err()
        .contains("no step called tap"));
}

/// What an agent reads: every actionable thing, its role and name, and a slider's value and range.
#[test]
fn the_tree_names_every_role_label_value_and_range() {
    let text = tree(&regions());
    for line in [
        "button \"Add\"",
        "field \"Note\"",
        "toggle \"Loud\"",
        "picker \"Rate\"",
        "button \"Play\"",
        "slider \"Volume\" value 0.5 of 0 to 1",
    ] {
        assert!(
            text.lines().any(|seen| seen == line),
            "{line} not in:\n{text}"
        );
    }
}
