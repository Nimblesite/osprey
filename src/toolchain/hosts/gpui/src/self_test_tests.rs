//! The self-test's reading of a script and of what the app offers, with every verb and its refusals.

use std::collections::HashMap;

use super::{attempt, parse, tree, Attempt};
use crate::events::Touch;
use crate::rows::{flags, Control, Fit, Kind, Look, Scene, Shown};
use crate::scene::access::fixtures::{button, slider};

#[test]
fn packaged_launch_keeps_overrides_and_rejects_invalid_argument_documents() {
    // [CAST-MOBILE-GPUI] The same host reads bundled defaults on mobile and desktop.
    let folder = std::path::Path::new("/Applications/A player's ☄.app");
    let given = vec!["--skin".to_owned(), "midnight".to_owned()];
    let result = crate::launch::configured(
        given.clone(),
        folder,
        r#"["--machine","${RESOURCES}/player.machine.json","--skin","sherbet"]"#,
    );
    assert_eq!(
        result,
        Ok(vec![
            "--skin".into(),
            "midnight".into(),
            "--machine".into(),
            "/Applications/A player's ☄.app/player.machine.json".into(),
            "--skin".into(),
            "sherbet".into()
        ])
    );
    assert_eq!(
        crate::launch::configured(given.clone(), folder, "[]"),
        Ok(given.clone())
    );
    for invalid in ["null", "{}", "[37]", "[true]", "[\"valid\",null]", "[", ""] {
        assert!(
            crate::launch::configured(given.clone(), folder, invalid).is_err(),
            "{invalid}"
        );
    }
}

/// One control as the shared frame carries it.
fn named(
    kind: Kind,
    action: Option<i64>,
    words: &str,
    hint: &str,
    traits: u32,
    children: Vec<usize>,
) -> Control {
    Control {
        kind,
        action,
        words: words.to_owned(),
        hint: hint.to_owned(),
        look: Look::default(),
        fit: Fit {
            flags: traits,
            ..Fit::default()
        },
        value: 0,
        children,
    }
}

/// The scene both frames carry: the elements a self-test reaches by name.
fn scene() -> Scene {
    Scene {
        width: 400.,
        height: 300.,
        zoom: 1.,
        atlas: vec![],
        shapes: vec![],
        access: vec![
            button("Play", [10., 20., 100., 40.]),
            slider("Volume", [100., 0., 200., 10.], [0.5, 0., 1.]),
        ],
    }
}

fn shown() -> Shown {
    Shown {
        controls: vec![
            named(Kind::Column, None, "", "", 0, vec![1, 3, 4, 5, 7, 8]),
            named(Kind::Press, Some(1), "Add", "", 0, vec![]),
            named(Kind::Label, None, "Note", "", 0, vec![]),
            named(Kind::Entry, Some(2), "", "Write a note", 0, vec![]),
            named(Kind::Toggle, Some(3), "Loud", "", flags::CHOSEN, vec![]),
            named(Kind::Picker, Some(4), "Alpha", "Rate", 0, vec![6]),
            named(Kind::Choice, None, "Alpha", "a", 0, vec![]),
            named(Kind::Label, None, "Count 0", "", 0, vec![]),
            named(Kind::Canvas, Some(5), "", "", 0, vec![]),
        ],
        roots: vec![0],
        scenes: HashMap::from([(8, scene())]),
        asks: vec![],
    }
}

/// One told row: its kind, the row it names as its parent, its name and the value its weight types.
fn told_row(
    kind: Kind,
    name: &str,
    (weight, words, value): (f32, &str, i64),
    children: Vec<usize>,
) -> Control {
    Control {
        words: words.to_owned(),
        hint: name.to_owned(),
        look: Look {
            weight,
            ..Look::default()
        },
        value,
        ..named(kind, None, "", "", 0, children)
    }
}

/// One told row to be: its kind, the row it names as its parent, its weight, its words, its value
/// and the rows it holds.
type ToldSpec = (
    Kind,
    &'static str,
    (f32, &'static str, i64),
    &'static [usize],
);

/// A told field that carries only words.
const fn field(name: &'static str, words: &'static str) -> ToldSpec {
    (Kind::Field, name, (0., words, 0), &[])
}

/// A told field whose weight types a whole value.
const fn typed(name: &'static str, weight: f32, value: i64) -> ToldSpec {
    (Kind::Field, name, (weight, "", value), &[])
}

/// An unnamed told record holding only rows.
const fn record(children: &'static [usize]) -> ToldSpec {
    (Kind::Record, "", (0., "", 0), children)
}

/// A frame that told main's JSON fixture as components ([NATIVE-SHM-NODES]): one region holding a
/// stack of a button, a field, a toggle, a picker, a text and a scene, plus the scene's own rows.
fn told() -> Shown {
    let rows: &[ToldSpec] = &[
        (Kind::Record, "region", (0., "", 0), &[1, 2]),
        field("key", "root"),
        (Kind::Record, "node", (0., "", 0), &[3, 4]),
        field("kind", "stack"),
        (Kind::List, "children", (0., "", 0), &[5, 9, 13, 18, 26, 29]),
        record(&[6, 7, 8]),
        field("kind", "button"),
        field("label", "Add"),
        typed("event", 1., 1),
        record(&[10, 11, 12]),
        field("kind", "field"),
        field("label", "Note"),
        typed("event", 1., 2),
        record(&[14, 15, 16, 17]),
        field("kind", "toggle"),
        field("label", "Loud"),
        typed("on", 3., 1),
        typed("event", 1., 3),
        record(&[19, 20, 21, 22]),
        field("kind", "picker"),
        field("label", "Rate"),
        typed("event", 1., 4),
        (Kind::List, "choices", (0., "", 0), &[23]),
        record(&[24, 25]),
        field("key", "a"),
        field("label", "Alpha"),
        record(&[27, 28]),
        field("kind", "text"),
        field("text", "Count 0"),
        record(&[30, 31]),
        field("kind", "scene"),
        typed("event", 1., 5),
    ];
    Shown {
        controls: rows
            .iter()
            .map(|&(kind, name, typed, children)| told_row(kind, name, typed, children.to_vec()))
            .collect(),
        roots: vec![0],
        scenes: HashMap::from([(29, scene())]),
        asks: vec![],
    }
}

/// One verb and its words, run against one frame.
fn attempt_on(shown: &Shown, verb: &str, words: &[&str]) -> Result<Attempt, String> {
    let words: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    attempt(shown, verb, &words)
}

fn step(verb: &str, words: &[&str]) -> Result<Attempt, String> {
    attempt_on(&shown(), verb, words)
}

fn values(attempt: Result<Attempt, String>) -> Vec<Touch> {
    match attempt.unwrap() {
        Attempt::Sending(sends) => sends,
        other => panic!("not a send: {other:?}"),
    }
}

/// One verb sends one control touch, whatever frame carries the control.
fn sends_control(frame: &Shown, verb: &str, words: &[&str], ordinal: i64, sent: &str) {
    assert_eq!(
        values(attempt_on(frame, verb, words)),
        vec![Touch::control(ordinal, sent)]
    );
}

/// An `invoke` lands a pointer where a hand would, on the scene's element a word names.
fn pointer_lands_where_a_hand_would(frame: &Shown) {
    assert_eq!(
        values(attempt_on(frame, "invoke", &["Play"])),
        vec![
            Touch::pointer(5, 60., 40., true, false),
            Touch::pointer(5, 60., 40., false, false)
        ]
    );
}

/// Every verb that reaches an ordinary control sends only its ordinal and words.
fn verbs_reach_ordinary_controls(frame: &Shown) {
    sends_control(frame, "invoke", &["Add"], 1, "");
    sends_control(frame, "toggle", &["Loud"], 3, "false");
    sends_control(frame, "type", &["Note", "hi"], 2, "hi");
    sends_control(frame, "choose", &["Alpha"], 4, "a");
    assert_eq!(
        values(attempt_on(frame, "key", &["p"])),
        vec![Touch::key(5, "p", true), Touch::key(5, "p", false)]
    );
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

/// A `wait` or `gone` verb sends once its word is on screen and waits while it is missing.
fn waits_for_the_missing_word(verb: &str, present: &str, missing: &str) {
    assert_eq!(step(verb, &[present]), Ok(Attempt::Sending(vec![])));
    assert!(matches!(step(verb, &[missing]), Ok(Attempt::Waiting(_))));
}

/// Every word on screen is found, and a missing one waits rather than acts.
#[test]
fn wait_and_gone_read_every_string_on_screen() {
    waits_for_the_missing_word("wait", "Count 0", "Count 1");
    assert_eq!(step("wait", &["Play"]), Ok(Attempt::Sending(vec![])));
    waits_for_the_missing_word("gone", "Count 1", "Count 0");
}

/// One verb reaches a scene's element and an ordinary control alike, and a pointer lands where a hand would.
#[test]
fn invoke_presses_scene_elements_and_controls_by_label() {
    pointer_lands_where_a_hand_would(&shown());
    verbs_reach_ordinary_controls(&shown());
    assert!(matches!(step("invoke", &["Nope"]), Ok(Attempt::Waiting(_))));
}

/// A slider lands at its value, clamped to its range; what has no range refuses it.
#[test]
fn set_lands_along_a_slider_and_refuses_anything_else() {
    let at = |value: &str| values(step("set", &["Volume", value]))[0].x;
    assert_eq!(at("0.25"), 150.);
    assert_eq!(at("9"), 300.);
    assert_eq!(at("-1"), 100.);
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
    assert_eq!(sent[0].x, 60.);
    assert_eq!((sent[1].x, sent[1].y), (65., 35.));
    assert!(!sent[2].down);
}

/// Switches flip, fields take their words, pickers choose by what a row shows and scenes take keys.
#[test]
fn toggle_type_choose_and_key_send_what_the_control_takes() {
    verbs_reach_ordinary_controls(&shown());
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

/// The tree names every line it is asked for, whatever frame carries the rows.
fn names_every_line(frame: &Shown, lines: &[&str]) {
    let text = tree(frame);
    for &line in lines {
        assert!(
            text.lines().any(|seen| seen == line),
            "{line} not in:\n{text}"
        );
    }
}

/// What an agent reads: every actionable thing, its role and name, and a slider's value and range.
#[test]
fn the_tree_names_every_role_label_value_and_range() {
    names_every_line(
        &shown(),
        &[
            "button \"Add\"",
            "field \"Note\"",
            "toggle \"Loud\"",
            "picker \"Rate\"",
            "button \"Play\"",
            "slider \"Volume\" value 0.5 of 0 to 1",
        ],
    );
}

/// Every verb reaches a component the frame told, and sends only its ordinal and words.
#[test]
fn every_verb_reaches_the_components_a_frame_tells() {
    let told = told();
    assert_eq!(
        attempt_on(&told, "wait", &["Count 0"]),
        Ok(Attempt::Sending(vec![]))
    );
    // A told row's words that no component draws, like a region's key or a theme token's
    // name, are not on screen: signal flow's "Bypassed" token once held `gone` forever.
    assert_eq!(
        attempt_on(&told, "gone", &["root"]),
        Ok(Attempt::Sending(vec![]))
    );
    assert!(matches!(
        attempt_on(&told, "wait", &["root"]),
        Ok(Attempt::Waiting(_))
    ));
    pointer_lands_where_a_hand_would(&told);
    verbs_reach_ordinary_controls(&told);
    assert!(matches!(
        attempt_on(&told, "invoke", &["Nope"]),
        Ok(Attempt::Waiting(_))
    ));
}

/// The told tree names every actionable component, its role and name, and a scene's elements.
#[test]
fn the_told_tree_names_every_role_label_value_and_range() {
    names_every_line(
        &told(),
        &[
            "button \"Add\"",
            "field \"Note\"",
            "toggle \"Loud\"",
            "picker \"Rate\"",
            "text \"Count 0\"",
            "scene \"\"",
            "button \"Play\"",
            "slider \"Volume\" value 0.5 of 0 to 1",
        ],
    );
}
