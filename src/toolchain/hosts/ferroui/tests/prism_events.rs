//! Prism's interaction budget, end to end through the real shared-memory boundary
//! ([NATIVE-SHM-EVENT], [PRISM-ROUTING-EDITOR]): the linked Prism synth opens its studio, a module is
//! dragged the way a hand drags it, and every pointer event must leave the application's `event`
//! export within one 120 Hz display frame. Only the export's own time is measured
//! (`ffi::APP_MICROS`), so the host and its toolkit are not part of the budget.
//!
//! Needs Prism linked: `scripts/prism.sh` builds `target/prism-app/libOspreyApp.a`, and the test
//! runs with `OSPREY_NATIVE_APP_DIR` naming it (README.md).

use ferroui_osprey_native::events::Touch;
use ferroui_osprey_native::ffi::Library;
use ferroui_osprey_native::regions::Regions;
use ferroui_osprey_native::rows::Shown;
use ferroui_osprey_native::scene::Shape;

mod common;
use common::{repo, scene, shapes};

/// One display frame at 120 Hz, in milliseconds.
const FRAME_MS: f64 = 1000.0 / 120.0;
/// The Oscillator module's title bar in the studio preset, in scene units.
const OSCILLATOR: (f64, f64) = (131.0, 376.0);
/// Drag moves per run, and the first few left out of the budget while caches fill.
const MOVES: usize = 60;
const WARM_UP: usize = 5;
/// The shapes the studio's canvas and keyboard draw, at least.
const STUDIO_SHAPES: usize = 250;

fn machine() -> String {
    repo("osprey/packages/signal_flow/signal_flow.machine.json")
}

/// One pointer event; returns its frame and the milliseconds the application took.
fn point(canvas: i64, (x, y): (f64, f64), down: bool) -> (Shown, f64) {
    common::point("Prism", canvas, x, y, down)
}

/// The scene point of move `index`: around a circle from the module's title bar.
fn along(index: usize) -> (f64, f64) {
    let turn = index as f64 / 30.0 * std::f64::consts::TAU;
    (OSCILLATOR.0 + 120.0 * turn.sin(), OSCILLATOR.1 + 60.0 * (1.0 - turn.cos()))
}

/// [DAW-LAYOUT-FILL] Drag by the title in three viewport shapes and trace the real output socket.
fn drag_across_resized_canvas(canvas: i64) {
    let center = |frame: &Shown, label: &str| {
        let area = frame.scenes.values().flat_map(|scene| &scene.access)
            .find(|area| area.label == label).unwrap_or_else(|| panic!("missing canvas control: {label}"));
        (area.x + area.w / 2.0, area.y + area.h / 2.0)
    };
    let sprite = |frame: &Shown| {
        frame.scenes.values().find_map(|scene| scene.shapes.iter().find_map(|shape| match shape {
            Shape::Sprite { atlas, x, y, .. } if scene.atlas.get(*atlas).is_some_and(|s| s.ends_with("/node-source.png")) => Some((f64::from(*x), f64::from(*y))),
            _ => None,
        })).expect("oscillator artwork")
    };
    for (width, height, dx, dy) in [
        (1536.0, 828.0, 450.0, 150.0),
        (1536.0, 1850.0, 400.0, 1130.0),
        (2260.0, 828.0, 1420.0, 140.0),
    ] {
        Library::event(0, &Touch::viewed(canvas, width, height)).expect("resize patch workspace");
        let start = point(canvas, OSCILLATOR, true).0;
        let end = (OSCILLATOR.0 + dx, OSCILLATOR.1 + dy);
        point(canvas, end, true);
        let release = point(canvas, end, false).0;
        let (sx, sy) = sprite(&start);
        assert_eq!(sprite(&release), (sx + dx, sy + dy), "painted card follows its sockets");
        assert_eq!(center(&release, "Inspector: Bypass Oscillator"), center(&start, "Inspector: Bypass Oscillator"));
        for label in ["Canvas Oscillator 0", "Canvas output 0:0", "Bypass Oscillator"] {
            let (x, y) = center(&start, label);
            assert_eq!(center(&release, label), (x + dx, y + dy), "{label} after drag in {width} × {height}");
        }
        if width > 2000.0 {
            let other = center(&release, "Canvas Filter 1");
            point(canvas, other, true);
            let deselected = point(canvas, other, false).0;
            assert_eq!(center(&deselected, "Bypass Oscillator"), center(&release, "Bypass Oscillator"), "the far-right card retains its controls after deselection");
        }
        point(canvas, end, true);
        point(canvas, OSCILLATOR, false);
    }
}

#[test]
fn dragging_a_prism_module_answers_every_pointer_event_within_one_display_frame() {
    assert_eq!(Library::init(), Ok(()), "Prism's top level must run");
    let arguments = vec!["--machine".to_owned(), machine()];
    assert_eq!(Library::arguments(&arguments), Ok(()), "Prism takes its machine document");
    let opened = Library::open(0, 0, "macos", "/tmp", true);
    let Ok(first) = opened else {
        panic!("Prism must draw its first frame: {opened:?}");
    };
    assert!(shapes(&first) >= STUDIO_SHAPES, "the studio draws {} shapes", shapes(&first));
    let mut regions = Regions::new();
    assert_eq!(regions.adopt(&first), Ok(()), "the first frame tells its regions");
    let Some(canvas) = regions.get("root").and_then(|root| scene(&root.node)) else {
        panic!("Prism's studio must draw a canvas");
    };

    let (pressed, _) = point(canvas, OSCILLATOR, true);
    assert!(shapes(&pressed) >= STUDIO_SHAPES, "pressing keeps the studio drawn");
    let mut costs = Vec::with_capacity(MOVES);
    for index in 1..=MOVES {
        let (moved, ms) = point(canvas, along(index), true);
        assert!(shapes(&moved) >= STUDIO_SHAPES, "move {index} keeps the studio drawn");
        assert!(moved.diagnostics().next().is_none(), "move {index} is not refused");
        costs.push(ms);
    }
    let (released, _) = point(canvas, along(MOVES), false);
    assert!(shapes(&released) >= STUDIO_SHAPES, "releasing keeps the studio drawn");

    let measured = &costs[WARM_UP..];
    let mean = measured.iter().sum::<f64>() / measured.len() as f64;
    let worst = measured.iter().copied().fold(0.0, f64::max);
    println!("prism drag: mean {mean:.2} ms, worst {worst:.2} ms over {} moves", measured.len());
    assert!(mean < FRAME_MS, "a drag move takes {mean:.2} ms on average; one frame is {FRAME_MS:.2} ms");
    assert!(worst < 2.0 * FRAME_MS, "the slowest drag move takes {worst:.2} ms; two frames are {:.2} ms", 2.0 * FRAME_MS);
    drag_across_resized_canvas(canvas);
}
