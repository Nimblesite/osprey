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
        scenes: HashMap::from([(
            8,
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
            },
        )]),
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
        kind,
        action: None,
        words: words.to_owned(),
        hint: name.to_owned(),
        look: Look {
            weight,
            ..Look::default()
        },
        fit: Fit::default(),
        value,
        children,
    }
}

/// A frame that told main's JSON fixture as components ([NATIVE-SHM-NODES]): one region holding a
/// stack of a button, a field, a toggle, a picker, a text and a scene, plus the scene's own rows.
fn told() -> Shown {
    let rows = told_row;
    Shown {
        controls: vec![
            rows(Kind::Record, "region", (0., "", 0), vec![1, 2]),
            rows(Kind::Field, "key", (0., "root", 0), vec![]),
            rows(Kind::Record, "node", (0., "", 0), vec![3, 4]),
            rows(Kind::Field, "kind", (0., "stack", 0), vec![]),
            rows(
                Kind::List,
                "children",
                (0., "", 0),
                vec![5, 9, 13, 18, 26, 29],
            ),
            rows(Kind::Record, "", (0., "", 0), vec![6, 7, 8]),
            rows(Kind::Field, "kind", (0., "button", 0), vec![]),
            rows(Kind::Field, "label", (0., "Add", 0), vec![]),
            rows(Kind::Field, "event", (1., "", 1), vec![]),
            rows(Kind::Record, "", (0., "", 0), vec![10, 11, 12]),
            rows(Kind::Field, "kind", (0., "field", 0), vec![]),
            rows(Kind::Field, "label", (0., "Note", 0), vec![]),
            rows(Kind::Field, "event", (1., "", 2), vec![]),
            rows(Kind::Record, "", (0., "", 0), vec![14, 15, 16, 17]),
            rows(Kind::Field, "kind", (0., "toggle", 0), vec![]),
            rows(Kind::Field, "label", (0., "Loud", 0), vec![]),
            rows(Kind::Field, "on", (3., "", 1), vec![]),
            rows(Kind::Field, "event", (1., "", 3), vec![]),
            rows(Kind::Record, "", (0., "", 0), vec![19, 20, 21, 22]),
            rows(Kind::Field, "kind", (0., "picker", 0), vec![]),
            rows(Kind::Field, "label", (0., "Rate", 0), vec![]),
            rows(Kind::Field, "event", (1., "", 4), vec![]),
            rows(Kind::List, "choices", (0., "", 0), vec![23]),
            rows(Kind::Record, "", (0., "", 0), vec![24, 25]),
            rows(Kind::Field, "key", (0., "a", 0), vec![]),
            rows(Kind::Field, "label", (0., "Alpha", 0), vec![]),
            rows(Kind::Record, "", (0., "", 0), vec![27, 28]),
            rows(Kind::Field, "kind", (0., "text", 0), vec![]),
            rows(Kind::Field, "text", (0., "Count 0", 0), vec![]),
            rows(Kind::Record, "", (0., "", 0), vec![30, 31]),
            rows(Kind::Field, "kind", (0., "scene", 0), vec![]),
            rows(Kind::Field, "event", (1., "", 5), vec![]),
        ],
        roots: vec![0],
        scenes: HashMap::from([(
            29,
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
            },
        )]),
        asks: vec![],
    }
}

fn step(verb: &str, words: &[&str]) -> Result<Attempt, String> {
    let words: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    attempt(&shown(), verb, &words)
}

fn told_step(verb: &str, words: &[&str]) -> Result<Attempt, String> {
    let words: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    attempt(&told(), verb, &words)
}

fn values(attempt: Result<Attempt, String>) -> Vec<Touch> {
    match attempt.unwrap() {
        Attempt::Sending(sends) => sends,
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
            Touch::pointer(5, 60., 40., true, false),
            Touch::pointer(5, 60., 40., false, false)
        ]
    );
    assert_eq!(
        values(step("invoke", &["Add"])),
        vec![Touch::control(1, "")]
    );
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
    assert_eq!(
        values(step("toggle", &["Loud"])),
        vec![Touch::control(3, "false")]
    );
    assert_eq!(
        values(step("type", &["Note", "hi"])),
        vec![Touch::control(2, "hi")]
    );
    assert_eq!(
        values(step("choose", &["Alpha"])),
        vec![Touch::control(4, "a")]
    );
    assert_eq!(
        values(step("key", &["p"])),
        vec![Touch::key(5, "p", true), Touch::key(5, "p", false)]
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
    let text = tree(&shown());
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

/// Every verb reaches a component the frame told, and sends only its ordinal and words.
#[test]
fn every_verb_reaches_the_components_a_frame_tells() {
    assert_eq!(
        told_step("wait", &["Count 0"]),
        Ok(Attempt::Sending(vec![]))
    );
    assert_eq!(
        values(told_step("invoke", &["Play"])),
        vec![
            Touch::pointer(5, 60., 40., true, false),
            Touch::pointer(5, 60., 40., false, false)
        ]
    );
    assert_eq!(
        values(told_step("invoke", &["Add"])),
        vec![Touch::control(1, "")]
    );
    assert_eq!(
        values(told_step("toggle", &["Loud"])),
        vec![Touch::control(3, "false")]
    );
    assert_eq!(
        values(told_step("type", &["Note", "hi"])),
        vec![Touch::control(2, "hi")]
    );
    assert_eq!(
        values(told_step("choose", &["Alpha"])),
        vec![Touch::control(4, "a")]
    );
    assert_eq!(
        values(told_step("key", &["p"])),
        vec![Touch::key(5, "p", true), Touch::key(5, "p", false)]
    );
    assert!(matches!(
        told_step("invoke", &["Nope"]),
        Ok(Attempt::Waiting(_))
    ));
}

/// The told tree names every actionable component, its role and name, and a scene's elements.
#[test]
fn the_told_tree_names_every_role_label_value_and_range() {
    let text = tree(&told());
    for line in [
        "button \"Add\"",
        "field \"Note\"",
        "toggle \"Loud\"",
        "picker \"Rate\"",
        "text \"Count 0\"",
        "scene \"\"",
        "button \"Play\"",
        "slider \"Volume\" value 0.5 of 0 to 1",
    ] {
        assert!(
            text.lines().any(|seen| seen == line),
            "{line} not in:\n{text}"
        );
    }
}
