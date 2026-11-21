//! Profiling probe ([DAW-PERF]): times the daw's `event` export for each kind of user action on every
//! page, through the real shared-memory boundary, and prints one row per action. `PROBE_HOLD=<scenario>`
//! repeats one scenario for `PROBE_SECONDS` (default 8) so a sampler can attach; `PROBE_SONG` names the
//! project. Ignored by default: `scripts/make/daw-profile.sh` runs it (`make daw-profile`).
use std::sync::atomic::Ordering;
use std::time::Instant;

use ferroui_osprey_native::events::Touch;
use ferroui_osprey_native::ffi::{self, Library};
use ferroui_osprey_native::regions::Regions;
use ferroui_osprey_native::rows::Shown;
use ferroui_osprey_native::tree::Tree;

fn repo(path: &str) -> String { format!("{}/../../../{path}", env!("CARGO_MANIFEST_DIR")) }

fn scene(node: &Tree) -> Option<i64> {
    match node {
        Tree::Record(members) => match members.get("kind") {
            Some(Tree::Text(kind)) if kind == "scene" => match members.get("event") { Some(Tree::Whole(o)) => Some(*o), _ => None },
            _ => members.values().find_map(scene),
        },
        Tree::Items(items) => items.iter().find_map(scene),
        _ => None,
    }
}
fn named(node: &Tree, label: &str) -> Option<i64> {
    match node {
        Tree::Record(fields) => {
            if matches!(fields.get("kind"), Some(Tree::Text(v)) if v == "button") && matches!(fields.get("label"), Some(Tree::Text(v)) if v == label) {
                if let Some(Tree::Whole(event)) = fields.get("event") { return Some(*event); }
            }
            fields.values().find_map(|child| named(child, label))
        }
        Tree::Items(items) => items.iter().find_map(|child| named(child, label)),
        _ => None,
    }
}
fn shapes(shown: &Shown) -> usize { shown.scenes.values().map(|scene| scene.shapes.len()).sum() }

/// (application ms, whole call ms, shapes)
fn send(touch: &Touch) -> (Shown, f64, f64) {
    let started = Instant::now();
    let shown = Library::event(0, touch).expect("event");
    let whole = started.elapsed().as_secs_f64() * 1000.0;
    (shown, ffi::APP_MICROS.load(Ordering::Relaxed) as f64 / 1000.0, whole)
}
fn command(regions: &mut Regions, label: &str) -> (f64, f64, usize) {
    let event = named(&regions.get("root").expect("root").node, label).unwrap_or_else(|| panic!("missing command {label}"));
    let (shown, app, whole) = send(&Touch::control(event, ""));
    let count = shapes(&shown);
    regions.adopt(&shown).expect("adopt");
    (app, whole, count)
}

const SCENARIOS: &[(&str, &str)] = &[
    ("timeline lane tap", "Timeline"), ("timeline ruler tap", "Timeline"), ("timeline hover", "Timeline"),
    ("roll note drag", "Timeline"), ("timeline frame tick", "Timeline"), ("timeline resize", "Timeline"),
    ("mix hover", "Bus Routing"), ("mix fader move", "Bus Routing"), ("mix frame tick", "Bus Routing"),
    ("patch hover", "Track Patch"), ("patch module drag", "Track Patch"), ("patch key", "Track Patch"),
    ("patch frame tick", "Track Patch"), ("patch resize", "Track Patch"),
];

fn touches(name: &str, canvas: i64, i: usize) -> Vec<Touch> {
    let p = |x: f64, y: f64, down: bool| Touch::pointer(canvas, x, y, down, false);
    let k = i as f64;
    match name {
        "timeline lane tap" => { let (x, y) = (198.0 + (k * 37.0) % 894.0, 221.0 + ((i % 7) as f64 + 0.5) * 31.0); vec![p(x, y, true), p(x, y, false)] }
        "timeline ruler tap" => { let x = 198.0 + (k * 89.4) % 894.0; vec![p(x, 207.0, true), p(x, 207.0, false)] }
        "timeline hover" => vec![p(198.0 + (k * 11.0) % 894.0, 230.0 + (k * 7.0) % 600.0, false)],
        "roll note drag" => vec![p(218.0, 606.0, true), p(230.0 + k % 20.0, 600.0, true), p(247.0, 597.0, true), p(282.6, 582.4, true), Touch::key(canvas, "escape", true), p(282.6, 582.4, false)],
        "timeline frame tick" | "mix frame tick" | "patch frame tick" => vec![Touch::frame(1.0 / 60.0)],
        "timeline resize" | "patch resize" => vec![Touch::viewed(canvas, 1152.0 + (k * 16.0) % 480.0, 706.0 + (k * 8.0) % 200.0)],
        "mix hover" => vec![p(100.0 + (k * 13.0) % 1000.0, 235.0 + (k * 7.0) % 600.0, false)],
        "mix fader move" => { let y = 235.0 + ((i / 8 % 17) as f64 + 0.5) * 37.0; vec![p(216.0 + (i % 8) as f64 * 6.0, y, true)] }
        "patch hover" => vec![p(60.0 + (k * 13.0) % 1300.0, 120.0 + (k * 7.0) % 600.0, false)],
        "patch module drag" => { let s = i % 40; if s == 0 { vec![p(131.0, 376.0, true)] } else if s == 39 { vec![p(131.0, 376.0, true), p(131.0, 376.0, false)] } else { vec![p(131.0 + (s as f64) * 3.0, 376.0 + (s as f64), true)] } }
        "patch key" => vec![Touch::key(canvas, "a", i % 2 == 0)],
        _ => panic!("no scenario {name}"),
    }
}

#[test]
#[ignore = "a measurement, run by scripts/make/daw-profile.sh"]
fn profile() {
    assert_eq!(Library::init(), Ok(()));
    let folder = std::env::temp_dir().join(format!("osprey-daw-profile-{}", std::process::id()));
    let song = folder.join("song.json");
    std::fs::create_dir_all(&folder).expect("folder");
    let project = std::env::var("PROBE_SONG").unwrap_or_else(|_| repo("osprey/examples/daw/tracks/codex-version.json"));
    std::fs::copy(project, &song).expect("copy");
    let arguments = vec!["--machine".to_owned(), repo("osprey/packages/signal_flow/signal_flow.machine.json"), "--song".to_owned(), song.display().to_string()];
    assert_eq!(Library::arguments(&arguments), Ok(()));
    let started = Instant::now();
    let first = Library::open(0, 0, "macos", folder.to_str().expect("utf8"), true).expect("open");
    println!("PROBE open: {:.1} ms, {} shapes", started.elapsed().as_secs_f64() * 1000.0, shapes(&first));
    let mut regions = Regions::new();
    regions.adopt(&first).expect("adopt");
    let canvas = regions.get("root").and_then(|root| scene(&root.node)).expect("canvas");
    // Select the keys track so the roll and the patch have content.
    send(&Touch::pointer(canvas, 380.0, 391.0, true, false));
    send(&Touch::pointer(canvas, 380.0, 391.0, false, false));
    let hold = std::env::var("PROBE_HOLD").ok();
    let seconds: f64 = std::env::var("PROBE_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(8.0);
    println!("PROBE {:<22} {:>6} {:>8} {:>8} {:>8} {:>8} {:>7}", "scenario", "events", "app mean", "app p95", "app max", "call mean", "shapes");
    let mut page = "";
    for (name, wanted) in SCENARIOS {
        if hold.as_deref().is_some_and(|only| only != *name) { continue; }
        if page != *wanted {
            let (app, whole, count) = command(&mut regions, wanted);
            println!("PROBE {:<22} {:>6} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>7}", format!("open page {wanted}"), 1, app, app, app, whole, count);
            page = wanted;
        }
        let canvas = regions.get("root").and_then(|root| scene(&root.node)).expect("canvas");
        let (mut apps, mut wholes, mut count) = (Vec::new(), Vec::new(), 0);
        let begun = Instant::now();
        let mut i = 0;
        while if hold.is_some() { begun.elapsed().as_secs_f64() < seconds } else { i < 120 } {
            for touch in touches(name, canvas, i) {
                let (shown, app, whole) = send(&touch);
                count = shapes(&shown);
                if i >= 3 { apps.push(app); wholes.push(whole); }
            }
            i += 1;
        }
        apps.sort_by(f64::total_cmp);
        let mean = apps.iter().sum::<f64>() / apps.len().max(1) as f64;
        let call = wholes.iter().sum::<f64>() / wholes.len().max(1) as f64;
        println!("PROBE {:<22} {:>6} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>7}", name, apps.len(), mean, apps[apps.len() * 95 / 100], apps[apps.len() - 1], call, count);
    }
    if hold.is_none() {
        for round in 0..6 {
            for label in ["Track Patch", "Bus Routing", "Timeline", "Patch Controls", "Keyboard", "Modules"] {
                if round < 1 { continue; }
                let (app, whole, count) = command(&mut regions, label);
                if round == 5 { println!("PROBE {:<22} {:>6} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>7}", format!("command {label}"), 1, app, app, app, whole, count); }
            }
        }
    }
}
