//! Every check `rows.rs` makes, with frames built as the Osprey runtime writes them
//! ([NATIVE-SHM-SAFETY]): a good frame reads back whole, and each malformed row refuses it.

use super::*;

/// The text run and a writer that appends to it, answering each text's offset and length.
#[derive(Default)]
struct Frame {
    views: Vec<ViewRow>,
    shapes: Vec<Streamed>,
    asks: Vec<AskRow>,
    text: Vec<u8>,
}

impl Frame {
    fn text(&mut self, words: &str) -> (i64, i64) {
        let at = self.text.len() as i64;
        self.text.extend_from_slice(words.as_bytes());
        (at, words.len() as i64)
    }

    fn view(&mut self, kind: i64, parent: i64, action: i64, words: &str, hint: &str) -> usize {
        let (text_offset, text_length) = self.text(words);
        let (hint_offset, hint_length) = self.text(hint);
        self.views.push(ViewRow {
            kind,
            parent,
            action,
            text_offset,
            text_length,
            colour: 0x112233ff,
            background: 0,
            size: 13.,
            padding: 4.,
            spacing: 8.,
            radius: 6.,
            weight: 400,
            width: 0.,
            height: 0.,
            grow: 0.,
            flags: 0,
            border: 0,
            hint_offset,
            hint_length,
        });
        self.views.len() - 1
    }

    fn shape(
        &mut self,
        code: i64,
        index: i64,
        values: [f64; 5],
        colour: i64,
        depth: i64,
        words: &str,
    ) {
        let (text_offset, text_length) = self.text(words);
        self.shapes.push(Streamed {
            code,
            index,
            values,
            colour,
            depth,
            text_offset,
            text_length,
        });
    }

    fn ask(&mut self, field: i64, number: i64, words: &str) {
        let (text_offset, text_length) = self.text(words);
        self.asks.push(AskRow {
            field,
            number,
            text_offset,
            text_length,
        });
    }

    fn read(&self) -> Result<Shown, String> {
        checked(&self.views, &self.shapes, &self.text, &self.asks)
    }
}

/// A column holding a label, a press, a picker with two options and a canvas, plus its scene and asks.
fn sample() -> Frame {
    let mut f = Frame::default();
    let root = f.view(0, -1, -1, "", "") as i64;
    f.view(2, root, -1, "Count 3", "star");
    f.view(3, root, 0, "Add", "add");
    let picker = f.view(9, root, 1, "Kitchen", "Receiver") as i64;
    f.view(10, picker, -1, "Kitchen", "kitchen");
    f.view(10, picker, -1, "Hallway", "hallway");
    f.views[4].flags = i64::from(flags::CHOSEN);
    let canvas = f.view(13, root, 2, "", "") as i64;
    f.shape(20, canvas, [200., 100., 0.5, 0., 0.], 0, 0, "");
    f.shape(21, 0, [0.; 5], 0, 0, "bird.png");
    f.shape(0, 0, [1., 2., 3., 4., 0.], 0xff0000ff, 2, "");
    f.shape(6, 1, [10., 20., 14., 700., 0.], 0x000000ff, 3, "Play");
    f.shape(5, 2, [0., 0., 1., 0., 0.], 0, 1, "");
    f.shape(8, 0, [1., 2., 0., 0., 0.], 0, 0, "");
    f.shape(8, 3, [9., 8., 7., 6., 5.], 0, 0, "");
    f.shape(7, 0, [4., 0., 0., 0., 0.], 0, 0, "");
    f.shape(9, 1, [2., 0., 0., 0., 0.], 1, 2, "");
    f.shape(7, 0, [3., 0., 0., 0., 0.], 0, 0, "");
    f.shape(7, 0, [1., 0., 0., 0., 0.], 0, 0, "");
    f.shape(10, 0, [0.; 5], 0, 0, "");
    f.shape(11, 0, [1., 0., 0., 1., 0.], 0, 0, "");
    f.shape(22, 3, [0., 50., 10., 10., 0.], 1, 0, "Kitchen");
    f.shape(7, 0, [0., 1., 0., 0., 0.], 0, 0, "");
    f.shape(22, 4, [0., 0., 400., 400., 0.], 0, 0, "Dismiss");
    f.shape(7, 0, [0.; 5], 0, 0, "");
    f.ask(0, 6, "appearance:read");
    f.ask(1, 0, "casting-appearance.txt");
    f.ask(2, 128, "");
    f.ask(3, 0, "casting.saved-skin");
    f.ask(0, SLICE_WRITE, "casting.selected-skin");
    f.ask(1, 0, "poolside");
    f.ask(0, DIAGNOSTIC, "Unknown control 9");
    f
}

#[test]
fn a_whole_frame_reads_back_as_a_tree_with_its_scene_and_asks() {
    let shown = sample().read().expect("the sample frame is well formed");
    assert_eq!(shown.roots, vec![0]);
    assert_eq!(shown.controls[0].children, vec![1, 2, 3, 6]);
    assert_eq!(shown.controls[3].children, vec![4, 5]);
    let label = &shown.controls[1];
    assert_eq!(
        (
            label.kind,
            label.words.as_str(),
            label.hint.as_str(),
            label.action
        ),
        (Kind::Label, "Count 3", "star", None)
    );
    assert_eq!(
        (shown.controls[2].kind, shown.controls[2].action),
        (Kind::Press, Some(0))
    );
    assert_eq!(
        (
            shown.controls[3].words.as_str(),
            shown.controls[3].hint.as_str()
        ),
        ("Kitchen", "Receiver")
    );
    assert_eq!(shown.controls[4].fit.flags, flags::CHOSEN);
    assert_eq!(shown.controls[4].hint, "kitchen");
    assert_eq!(
        shown.controls[2].look,
        Look {
            ink: 0x112233ff,
            fill: 0,
            size: 13.,
            padding: 4.,
            spacing: 8.,
            radius: 6.,
            weight: 400.
        }
    );
    let scene = &shown.scenes[&6];
    assert_eq!((scene.width, scene.height, scene.zoom), (200., 100., 0.5));
    assert_eq!(scene.atlas, vec!["bird.png".to_owned()]);
    assert_eq!(scene.shapes.len(), 3);
    assert_eq!(
        scene.shapes[0],
        Shape::Rect {
            x: 1.,
            y: 2.,
            w: 3.,
            h: 4.,
            angle: 0.,
            colour: 0xff0000ff,
            depth: 2.
        }
    );
    assert_eq!(
        scene.shapes[1],
        Shape::Lettering {
            text: "Play".to_owned(),
            x: 10.,
            y: 20.,
            size: 14.,
            colour: 255,
            depth: 3.,
            anchor: 1,
            weight: 700.
        }
    );
    match &scene.shapes[2] {
        Shape::Path {
            steps,
            outline,
            paint,
            transform,
            depth,
        } => {
            assert_eq!(
                steps.as_slice(),
                &[
                    Step::Move((1., 2.)),
                    Step::Cubic((9., 8.), (7., 6.), (5., 4.))
                ]
            );
            assert_eq!(
                *outline,
                Outline::Stroke {
                    width: 2.,
                    bevel: true,
                    dashes: vec![3., 1.]
                }
            );
            assert_eq!(*paint, Paint::Solid(Tint::Rgb([1., 0., 0., 1.])));
            assert_eq!((*transform, *depth), (None, 1.));
        }
        other => panic!("expected a path, got {other:?}"),
    }
    assert_eq!(
        scene.access.len(),
        1,
        "an unannounced element is not offered"
    );
    assert_eq!(
        (
            scene.access[0].role,
            scene.access[0].label.as_str(),
            scene.access[0].selected
        ),
        (Role::MenuItem, "Kitchen", true)
    );
    assert_eq!((scene.access[0].min, scene.access[0].max), (0., 1.));
    let requests: Vec<_> = shown.requests().collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        (requests[0].kind(), requests[0].ticket.as_str()),
        ("store-read", "appearance:read")
    );
    assert_eq!(
        (
            requests[0].texts(1),
            requests[0].number(2),
            requests[0].texts(3)
        ),
        (
            vec!["casting-appearance.txt".to_owned()],
            128,
            vec!["casting.saved-skin".to_owned()]
        )
    );
    assert_eq!(
        shown.slices().collect::<Vec<_>>(),
        vec![("casting.selected-skin", "poolside")]
    );
    assert_eq!(
        shown.diagnostics().collect::<Vec<_>>(),
        vec!["Unknown control 9"]
    );
}

#[test]
fn an_empty_frame_is_an_empty_view() {
    let shown = Frame::default()
        .read()
        .expect("nothing drawn is well formed");
    assert!(
        shown.controls.is_empty()
            && shown.roots.is_empty()
            && shown.scenes.is_empty()
            && shown.asks.is_empty()
    );
}

/// One malformation made to a good frame.
type Change = Box<dyn Fn(&mut Frame)>;

/// Each change, made to an otherwise good frame, refuses the whole frame with the reason.
#[test]
fn every_malformed_row_refuses_the_frame() {
    let cases: Vec<(&str, Change)> = vec![
        ("unknown kind", Box::new(|f| f.views[1].kind = 17)),
        ("unknown kind", Box::new(|f| f.views[1].kind = 36)),
        ("unknown kind", Box::new(|f| f.views[1].kind = -1)),
        (
            "a told value of an unknown type",
            Box::new(|f| {
                f.views[1].kind = 33;
                f.views[1].weight = 400;
            }),
        ),
        (
            "text range outside",
            Box::new(|f| {
                f.views[1].kind = 34;
                f.views[1].weight = 0;
                f.views[1].text_length = 1 << 40;
            }),
        ),
        ("not an earlier row", Box::new(|f| f.views[1].parent = 1)),
        ("not an earlier row", Box::new(|f| f.views[1].parent = 9)),
        ("not an earlier row", Box::new(|f| f.views[1].parent = -2)),
        (
            "text range outside",
            Box::new(|f| f.views[1].text_length = 1 << 40),
        ),
        (
            "text offset below zero",
            Box::new(|f| f.views[1].text_offset = -1),
        ),
        (
            "text length below zero",
            Box::new(|f| f.views[1].hint_length = -3),
        ),
        ("unknown flags", Box::new(|f| f.views[2].flags = 128)),
        ("font weight", Box::new(|f| f.views[2].weight = 950)),
        ("colour outside", Box::new(|f| f.views[2].colour = 1 << 33)),
        ("colour outside", Box::new(|f| f.views[2].border = -1)),
        ("extent", Box::new(|f| f.views[2].size = f64::NAN)),
        ("extent", Box::new(|f| f.views[2].width = -1.)),
        ("extent", Box::new(|f| f.views[2].grow = f64::INFINITY)),
        (
            "scene that names no canvas",
            Box::new(|f| f.shapes[0].index = 2),
        ),
        (
            "scene row before any scene",
            Box::new(|f| {
                f.shapes.remove(0);
            }),
        ),
        ("not finite", Box::new(|f| f.shapes[2].values[1] = f64::NAN)),
        ("unknown scene code", Box::new(|f| f.shapes[2].code = 12)),
        ("runs past its rows", Box::new(|f| f.shapes[4].index = 99)),
        ("should have code", Box::new(|f| f.shapes[6].code = 9)),
        ("unknown path step", Box::new(|f| f.shapes[5].index = 7)),
        (
            "unknown lettering anchor",
            Box::new(|f| f.shapes[3].index = 3),
        ),
        (
            "unknown accessible role",
            Box::new(|f| f.shapes[13].index = 5),
        ),
        (
            "window-pixel scene",
            Box::new(|f| f.shapes[0].values[1] = 0.),
        ),
        (
            "unknown capability code",
            Box::new(|f| f.asks[0].number = 26),
        ),
        (
            "before any ask",
            Box::new(|f| {
                f.asks.remove(0);
            }),
        ),
        ("below zero", Box::new(|f| f.asks[1].field = -1)),
    ];
    for (reason, change) in cases {
        let mut frame = sample();
        change(&mut frame);
        let refused = frame.read().expect_err(reason);
        assert!(refused.contains(reason), "{reason}: refused with {refused}");
    }
}

#[test]
fn text_that_is_not_utf8_is_refused() {
    let mut frame = sample();
    let (offset, _) = frame.text("\u{e9}");
    frame.views[1].text_offset = offset + 1;
    frame.views[1].text_length = 1;
    assert_eq!(
        frame.read().expect_err("half a character"),
        "text that is not UTF-8"
    );
}

#[test]
fn ask_fields_read_lists_pairs_and_flags_by_position() {
    let mut frame = Frame::default();
    frame.ask(0, 10, "fetch");
    for (field, number, text) in [
        (1, 0, "GET"),
        (2, 0, "https://example.test"),
        (3, 0, "Accept"),
        (3, 0, "text/plain"),
        (3, 0, "X-Id"),
        (3, 0, "7"),
        (5, 4096, ""),
        (6, 1, ""),
    ] {
        frame.ask(field, number, text);
    }
    let shown = frame.read().expect("an http ask is well formed");
    let http = shown.requests().next().expect("one request");
    assert_eq!(http.kind(), "http");
    assert_eq!(http.texts(3), vec!["Accept", "text/plain", "X-Id", "7"]);
    assert_eq!(
        (http.number(5), http.flag(6), http.flag(7), http.texts(4)),
        (4096, true, false, Vec::<String>::new())
    );
}

/// A told frame reads back as the typed component tree main's renderer drew: a field's weight says
/// whether it is text, a whole number, a real or a flag, and a record's members are named by the
/// hints of the rows under it ([NATIVE-SHM-NODES]).
#[test]
fn told_rows_read_as_the_component_tree() {
    let mut frame = Frame::default();
    let region = frame.view(34, -1, -1, "", "region");
    let key = frame.view(33, region as i64, -1, "root", "key");
    let node = frame.view(34, region as i64, -1, "", "node");
    let kind = frame.view(33, node as i64, -1, "stack", "kind");
    let count = frame.view(33, node as i64, -1, "", "count");
    let real = frame.view(33, node as i64, -1, "", "real");
    let on = frame.view(33, node as i64, -1, "", "on");
    let list = frame.view(35, node as i64, -1, "", "children");
    let item = frame.view(34, list as i64, -1, "", "");
    for row in [region, key, node, kind, list, item] {
        frame.views[row].weight = 0;
    }
    for (row, weight, size, colour) in [(count, 1, 0., -7), (real, 2, 0.5, 0), (on, 3, 0., 1)] {
        frame.views[row].weight = weight;
        frame.views[row].size = size;
        frame.views[row].colour = colour;
    }
    let shown = frame.read().expect("a told frame is well formed");
    assert_eq!(shown.controls[node].kind, Kind::Record);
    assert_eq!(shown.controls[list].kind, Kind::List);
    let told = crate::tree::frame(&shown)
        .expect("a drawable told frame")
        .expect("a frame that tells components");
    let mut members = std::collections::BTreeMap::new();
    members.insert(
        "kind".to_owned(),
        crate::tree::Tree::Text("stack".to_owned()),
    );
    members.insert("count".to_owned(), crate::tree::Tree::Whole(-7));
    members.insert("real".to_owned(), crate::tree::Tree::Real(0.5));
    members.insert("on".to_owned(), crate::tree::Tree::Bool(true));
    members.insert(
        "children".to_owned(),
        crate::tree::Tree::Items(vec![crate::tree::Tree::Record(
            std::collections::BTreeMap::new(),
        )]),
    );
    assert_eq!(
        told.regions,
        vec![("root".to_owned(), crate::tree::Tree::Record(members))]
    );
}
