//! A frame is refused when this host cannot draw it, exactly as main's envelope refused it
//! ([UI-PROTOCOL]): only the content-sized inset and no vertical scrolling are drawn, and a
//! present member of the wrong type is malformed rather than read as its default.

use super::{frame, viewports, Frame, Theme, Token, Tree};
use crate::rows::{Control, Fit, Kind, Look, Shown};

/// A record of the named members, as the told rows of a record read back.
fn record(members: &[(&str, Tree)]) -> Tree {
    Tree::Record(
        members
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    )
}

fn text(words: &str) -> Tree {
    Tree::Text(words.to_owned())
}

/// A framed component with the given style, or with no style at all.
fn framed(style: Option<Tree>) -> Tree {
    let body = record(&[("kind", text("empty"))]);
    match style {
        None => record(&[("kind", text("framed")), ("body", body)]),
        Some(style) => record(&[("kind", text("framed")), ("style", style), ("body", body)]),
    }
}

fn style(members: &[(&str, Tree)]) -> Tree {
    record(members)
}

/// The refusal for one component drawn as region `root`.
fn refusal(node: &Tree) -> String {
    match viewports(node, "root") {
        Ok(()) => String::from("accepted a frame this host cannot draw"),
        Err(problem) => problem,
    }
}

/// A frame the host cannot draw is refused by name, never drawn as if the member were unset.
#[test]
fn a_frame_sized_to_the_window_is_refused() {
    for inset in [0, 400] {
        let problem = refusal(&framed(Some(style(&[(
            "viewportHeightInset",
            Tree::Whole(inset),
        )]))));
        assert!(problem.contains("viewportHeightInset"), "{problem}");
        assert!(problem.contains("-1"), "{problem}");
    }
}

/// A frame that asks to scroll vertically is refused by name.
#[test]
fn a_scrolling_frame_is_refused() {
    let problem = refusal(&framed(Some(style(&[("scrollY", Tree::Bool(true))]))));
    assert!(problem.contains("scrollY"), "{problem}");
    assert!(problem.contains("region root"), "{problem}");
}

/// A frame nested in a container, or inside another frame's body, is refused too.
#[test]
fn a_nested_frame_is_refused() {
    let child = refusal(&record(&[
        ("kind", text("stack")),
        (
            "children",
            Tree::Items(vec![framed(Some(style(&[(
                "viewportHeightInset",
                Tree::Whole(12),
            )])))]),
        ),
    ]));
    assert!(child.contains("viewportHeightInset"), "{child}");
    let body = record(&[
        ("kind", text("framed")),
        ("style", style(&[])),
        (
            "body",
            framed(Some(style(&[("scrollY", Tree::Bool(true))]))),
        ),
    ]);
    let body = refusal(&body);
    assert!(body.contains("scrollY"), "{body}");
}

/// A present member of the wrong kind is malformed, not read as its default.
#[test]
fn a_malformed_viewport_field_is_refused_not_guessed() {
    for (member, value) in [
        ("viewportHeightInset", text("4")),
        ("viewportHeightInset", Tree::Real(2.5)),
        ("scrollY", Tree::Whole(1)),
        ("scrollY", text("true")),
    ] {
        let problem = refusal(&framed(Some(style(&[(member, value)]))));
        assert!(problem.contains("where it needs"), "{problem}");
    }
}

/// Only absent or exactly the content-sized default is legal: any other inset is refused, and a
/// style that is not a record is malformed.
#[test]
fn only_absent_or_the_content_sized_default_is_accepted() {
    for (wanted, node) in [
        ("an object style", framed(Some(Tree::Text(String::new())))),
        (
            "an integer viewportHeightInset",
            framed(Some(style(&[("viewportHeightInset", Tree::Real(2.5))]))),
        ),
        (
            "a boolean scrollY",
            framed(Some(style(&[("scrollY", Tree::Whole(1))]))),
        ),
        (
            "viewportHeightInset",
            framed(Some(style(&[("viewportHeightInset", Tree::Whole(-2))]))),
        ),
        (
            "viewportHeightInset",
            framed(Some(style(&[(
                "viewportHeightInset",
                Tree::Whole(i64::MIN),
            )]))),
        ),
    ] {
        let problem = refusal(&node);
        assert!(problem.contains(wanted), "{wanted}: {problem}");
    }
}

/// The content-sized frame the desktop hosts draw stays decodable, members absent or default.
#[test]
fn a_content_sized_frame_keeps_the_defaults() {
    for node in [
        framed(None),
        framed(Some(style(&[]))),
        framed(Some(style(&[
            ("viewportHeightInset", Tree::Whole(-1)),
            ("scrollY", Tree::Bool(false)),
        ]))),
    ] {
        assert_eq!(viewports(&node, "root"), Ok(()));
    }
}

/// One told row: its kind, its name, the type its weight gives, its value and the rows inside it.
fn row(
    kind: Kind,
    name: &str,
    (weight, size, words, value): (f32, f32, &str, i64),
    children: Vec<usize>,
) -> Control {
    Control {
        kind,
        action: None,
        words: words.to_owned(),
        hint: name.to_owned(),
        look: Look {
            size,
            weight,
            ..Look::default()
        },
        fit: Fit::default(),
        value,
        children,
    }
}

fn shown(controls: Vec<Control>) -> Shown {
    Shown {
        controls,
        roots: vec![0],
        ..Shown::default()
    }
}

/// The theme the frame tells reads back as main's JSON theme did: its tokens, spacing and motion
/// durations, with a member that carries no name ignored.
#[test]
fn the_theme_a_frame_tells_reads_back() {
    let controls = vec![
        row(Kind::Record, "theme", (0., 0., "", 0), vec![1, 2, 3, 4]),
        row(Kind::List, "tokens", (0., 0., "", 0), vec![5]),
        row(Kind::List, "spacing", (0., 0., "", 0), vec![8, 9]),
        row(Kind::Field, "feedback", (1., 0., "", 250), vec![]),
        row(Kind::Field, "settle", (1., 0., "", 90), vec![]),
        row(Kind::Record, "", (0., 0., "", 0), vec![6, 7]),
        row(Kind::Field, "name", (0., 0., "text", 0), vec![]),
        row(Kind::Field, "light", (0., 0., "#ffffff", 0), vec![]),
        row(Kind::Field, "", (1., 0., "", 4), vec![]),
        row(Kind::Field, "", (1., 0., "", 8), vec![]),
    ];
    let frame = frame(&shown(controls)).expect("a told frame").expect("one");
    assert_eq!(
        frame,
        Frame {
            theme: Some(Theme {
                tokens: vec![Token {
                    name: "text".to_owned(),
                    light: "#ffffff".to_owned(),
                    dark: String::new(),
                }],
                spacing: vec![4, 8],
                feedback: 250,
                settle: 90,
            }),
            regions: vec![],
        }
    );
}

/// A real keeps its type and its sign, and a member with no name belongs to no component.
#[test]
fn a_told_real_and_a_nameless_member_read_back() {
    let controls = vec![
        row(Kind::Record, "region", (0., 0., "", 0), vec![1, 2]),
        row(Kind::Field, "key", (0., 0., "root", 0), vec![]),
        row(Kind::Record, "node", (0., 0., "", 0), vec![3, 4, 5, 6]),
        row(Kind::Field, "kind", (0., 0., "stack", 0), vec![]),
        row(Kind::Field, "real", (2., 0.5, "", 0), vec![]),
        row(Kind::Field, "negative", (4., 0.25, "", 0), vec![]),
        row(Kind::Field, "", (0., 0., "ignored", 0), vec![]),
    ];
    let frame = frame(&shown(controls)).expect("a told frame").expect("one");
    let node = record(&[
        ("kind", text("stack")),
        ("real", Tree::Real(0.5)),
        ("negative", Tree::Real(-0.25)),
    ]);
    assert_eq!(frame.regions, vec![("root".to_owned(), node)]);
}
