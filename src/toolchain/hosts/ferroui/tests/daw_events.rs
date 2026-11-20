//! The daw's Timeline tap budget, end to end through the real shared-memory boundary
//! ([NATIVE-SHM-EVENT], [DAW-PAGES]): the linked daw opens a real song project on its Timeline, lanes
//! are tapped across every track, the ruler is tapped, every Mix gain fader is dragged, and each event
//! must leave the application's `event` export within one 60 Hz display frame. A separate file from `prism_events.rs` because the linked application
//! holds one session per process and this one opens with a song.
//!
//! Needs the daw linked: `scripts/prism.sh` builds `target/prism-app/libOspreyApp.a`, and the test
//! runs with `OSPREY_NATIVE_APP_DIR` naming it (README.md).

use std::sync::atomic::Ordering;

use ferroui_osprey_native::events::Touch;
use ferroui_osprey_native::ffi::{self, Library};
use ferroui_osprey_native::regions::Regions;
use ferroui_osprey_native::rows::Shown;
use ferroui_osprey_native::tree::Tree;

/// One display frame at 60 Hz, in milliseconds.
const FRAME_MS: f64 = 1000.0 / 60.0;
/// The song's tracks, and the first taps left out of the budget while caches fill.
const TRACKS: usize = 17;
const WARM_UP: usize = 3;
/// The shapes the Timeline draws for the song, at least.
const TIMELINE_SHAPES: usize = 300;
/// The shapes the Mix page draws for the song, at least, and the middle of its header tab.
const MIX_SHAPES: usize = 200;
const MIX_TAB: (f64, f64) = (680.0, 32.0);

fn repo(path: &str) -> String {
    format!("{}/../../../{path}", env!("CARGO_MANIFEST_DIR"))
}

/// The control number of the first scene at or below `node`, depth first.
fn scene(node: &Tree) -> Option<i64> {
    match node {
        Tree::Record(members) => match members.get("kind") {
            Some(Tree::Text(kind)) if kind == "scene" => match members.get("event") {
                Some(Tree::Whole(ordinal)) => Some(*ordinal),
                _ => None,
            },
            _ => members.values().find_map(scene),
        },
        Tree::Items(items) => items.iter().find_map(scene),
        _ => None,
    }
}

fn shapes(shown: &Shown) -> usize {
    shown.scenes.values().map(|scene| scene.shapes.len()).sum()
}

/// One pointer event; returns its frame and the milliseconds the application took.
fn point(canvas: i64, (x, y): (f64, f64), down: bool) -> (Shown, f64) {
    match Library::event(0, &Touch::pointer(canvas, x, y, down, false)) {
        Ok(shown) => (shown, ffi::APP_MICROS.load(Ordering::Relaxed) as f64 / 1000.0),
        Err(problem) => panic!("the daw refused a pointer event at {x},{y}: {problem}"),
    }
}

/// The scene point of lane `track` at fraction `along` of the song (lanes span x 230 to 1200 and
/// share y 146 to 476).
fn lane(track: usize, along: f64) -> (f64, f64) {
    let height = 330.0 / TRACKS as f64;
    (230.0 + along * 970.0, 146.0 + (track as f64 + 0.5) * height)
}

#[test]
fn tapping_timeline_lanes_answers_every_tap_within_one_display_frame() {
    assert_eq!(Library::init(), Ok(()), "the daw's top level must run");
    // A copy: a Mix edit saves into the project it opened, and the tracked song must stay as it is.
    let folder = std::env::temp_dir().join("osprey-daw-events");
    let song = folder.join("song.json");
    assert!(std::fs::create_dir_all(&folder).is_ok(), "the test keeps its song in {}", folder.display());
    let copied = std::fs::copy(repo("osprey/examples/daw/tracks/codex-version.json"), &song);
    assert!(copied.is_ok(), "the song is copied: {copied:?}");
    let arguments = vec![
        "--machine".to_owned(),
        repo("osprey/packages/signal_flow/signal_flow.machine.json"),
        "--song".to_owned(),
        song.display().to_string(),
    ];
    assert_eq!(Library::arguments(&arguments), Ok(()), "the daw takes its machine and song");
    let opened = Library::open(0, 0, "macos", "/tmp", true);
    let Ok(first) = opened else {
        panic!("the daw must draw its first frame: {opened:?}");
    };
    assert!(shapes(&first) >= TIMELINE_SHAPES, "the Timeline draws {} shapes", shapes(&first));
    let mut regions = Regions::new();
    assert_eq!(regions.adopt(&first), Ok(()), "the first frame tells its regions");
    let Some(canvas) = regions.get("root").and_then(|root| scene(&root.node)) else {
        panic!("the Timeline must draw a canvas");
    };

    let mut costs = Vec::new();
    let mut drawn = std::collections::BTreeSet::new();
    for round in 0..3 {
        for track in 0..TRACKS {
            let at = lane(track, (track * 5 + round * 13) as f64 % 40.0 / 40.0);
            let (pressed, down) = point(canvas, at, true);
            assert!(shapes(&pressed) >= TIMELINE_SHAPES, "track {track} keeps the Timeline drawn");
            assert!(pressed.diagnostics().next().is_none(), "the tap on track {track} is not refused");
            let (released, up) = point(canvas, at, false);
            assert!(shapes(&released) >= TIMELINE_SHAPES, "releasing keeps the Timeline drawn");
            drawn.insert(shapes(&released));
            costs.push(down);
            costs.push(up);
        }
    }

    // [DAW-TRANSPORT-PLAYHEAD] Ruler taps move the playhead; the lanes stay drawn.
    for bar in 0..10 {
        let at = (230.0 + f64::from(bar) * 97.0, 130.0);
        let (pressed, down) = point(canvas, at, true);
        assert!(pressed.diagnostics().next().is_none(), "the ruler tap at bar step {bar} is not refused");
        let (released, up) = point(canvas, at, false);
        assert!(shapes(&released) >= TIMELINE_SHAPES, "moving the playhead keeps the Timeline drawn");
        costs.push(down);
        costs.push(up);
    }

    // The Mix tab, then every track's gain fader pressed, dragged and released (rows start at y 150,
    // 36 units each; the gain fader spans x 310 to 460).
    point(canvas, MIX_TAB, true);
    let (mix, _) = point(canvas, MIX_TAB, false);
    assert!(shapes(&mix) >= MIX_SHAPES, "the Mix page draws {} shapes", shapes(&mix));
    let mut drags = Vec::new();
    for track in 0..TRACKS {
        let y = 150.0 + (track as f64 + 0.5) * 36.0;
        for step in 0..=6 {
            let (moved, cost) = point(canvas, (320.0 + f64::from(step) * 20.0, y), true);
            assert!(shapes(&moved) >= MIX_SHAPES, "dragging track {track}'s gain keeps the Mix drawn");
            assert!(moved.diagnostics().next().is_none(), "the drag on track {track} is not refused");
            drags.push(cost);
        }
        let (released, cost) = point(canvas, (440.0, y), false);
        assert!(shapes(&released) >= MIX_SHAPES, "releasing track {track}'s gain keeps the Mix drawn");
        drags.push(cost);
    }
    let mean = drags.iter().sum::<f64>() / drags.len() as f64;
    let worst = drags.iter().copied().fold(0.0, f64::max);
    println!("mix fader drags: mean {mean:.2} ms, worst {worst:.2} ms over {} events", drags.len());
    assert!(mean < FRAME_MS, "a Mix fader event takes {mean:.2} ms on average; one frame is {FRAME_MS:.2} ms");
    assert!(worst < 2.0 * FRAME_MS, "the slowest Mix fader event takes {worst:.2} ms; two frames are {:.2} ms", 2.0 * FRAME_MS);

    // Each track's piano roll draws its own notes, so taps that select tracks redraw different scenes.
    assert!(drawn.len() >= 8, "the taps selected tracks: {} distinct scenes", drawn.len());
    let measured = &costs[WARM_UP..];
    let mean = measured.iter().sum::<f64>() / measured.len() as f64;
    let worst = measured.iter().copied().fold(0.0, f64::max);
    println!("timeline taps: mean {mean:.2} ms, worst {worst:.2} ms over {} events", measured.len());
    assert!(mean < FRAME_MS, "a Timeline tap takes {mean:.2} ms on average; one frame is {FRAME_MS:.2} ms");
    assert!(worst < 2.0 * FRAME_MS, "the slowest Timeline tap takes {worst:.2} ms; two frames are {:.2} ms", 2.0 * FRAME_MS);
}
