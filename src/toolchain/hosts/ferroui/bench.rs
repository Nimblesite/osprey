//! Bounded frame measurements (feature `desktop`). `OSPREY_NATIVE_BENCH=<seconds>` measures the
//! application's own animation frames; `OSPREY_NATIVE_DRAG=<x>,<y>` also drives a pointer drag on
//! the first canvas from that scene point, one move per animation frame, as a hand dragging a
//! module would. Each run prints one line of averages and closes the process when it ends.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ferroui_base::media::MediaContext;

use crate::events::Touch;
use crate::host::Host;
use crate::tree::{self, Tree};

/// The longest run allowed, whatever is asked.
const LONGEST: f64 = 600.0;

/// Frame measurements of one run.
pub struct Bench {
    started: Instant,
    duration: Duration,
    frames: u64,
    dispatch: f64,
    build: f64,
    paint: f64,
    send: f64,
    app: f64,
}

impl Bench {
    /// The run `OSPREY_NATIVE_BENCH` asks for, if any.
    pub fn requested() -> Option<Self> {
        let seconds: f64 = std::env::var("OSPREY_NATIVE_BENCH").ok()?.parse().ok()?;
        Some(Self {
            started: Instant::now(),
            duration: Duration::from_secs_f64(seconds.clamp(0., LONGEST)),
            frames: 0,
            dispatch: 0.,
            build: 0.,
            paint: 0.,
            send: 0.,
            app: 0.,
        })
    }

    /// Adds one frame; at the end of the run prints the averages and exits.
    pub fn record(&mut self, host: &Host, now: Instant) {
        self.frames += 1;
        self.dispatch += host.dispatch_ms.get();
        self.build += host.build_ms.get();
        self.send += host.send_ms.get();
        self.app +=
            crate::ffi::APP_MICROS.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1000.;
        self.paint += crate::surface::PAINT_MS.with(|total| total.replace(0.));
        if now.duration_since(self.started) < self.duration {
            return;
        }
        let frames = self.frames.max(1) as f64;
        let seconds = self.started.elapsed().as_secs_f64().max(f64::EPSILON);
        let shapes: usize = host.shown().scenes.values().map(|s| s.shapes.len()).sum();
        println!(
            "bench fps={:.1} app_ms={:.2} dispatch_ms={:.2} build_ms={:.2} paint_ms={:.2} event_total_ms={:.2} shapes={shapes} frames={}",
            self.frames as f64 / seconds,
            self.app / frames,
            self.dispatch / frames,
            self.build / frames,
            self.paint / frames,
            self.send / frames,
            self.frames
        );
        std::process::exit(0);
    }
}

/// Starts the repaint run `OSPREY_NATIVE_REPAINT=1` asks for: every animation frame rebuilds and
/// repaints the whole view from the frame already shown, with no call into the application.
pub fn repaint(host: Rc<Host>) {
    if std::env::var("OSPREY_NATIVE_REPAINT").as_deref() != Ok("1") {
        return;
    }
    let Some(bench) = Bench::requested() else {
        eprintln!("repaint bench: OSPREY_NATIVE_BENCH is not set");
        std::process::exit(2);
    };
    again(host, Rc::new(RefCell::new(bench)));
}

fn again(host: Rc<Host>, bench: Rc<RefCell<Bench>>) {
    MediaContext::instance().request_animation_frame(move |_| {
        host.redraw();
        bench.borrow_mut().record(&host, Instant::now());
        again(host, bench);
    });
}

/// Drives native click events on the current canvas at each scene point in `OSPREY_NATIVE_TAP`.
/// Semicolon-separated points can change pages between clicks.
pub fn tap(host: &Rc<Host>) {
    let Ok(sequence) = std::env::var("OSPREY_NATIVE_TAP") else {
        return;
    };
    if sequence.is_empty() {
        return;
    }
    let Some(points) = sequence
        .split(';')
        .map(coordinates)
        .collect::<Option<Vec<_>>>()
    else {
        host.diagnose("Expected a tap sequence of x,y scene coordinates".to_owned());
        return;
    };
    for (x, y) in points {
        let Some(canvas) = first_canvas(host) else {
            return;
        };
        host.send(Touch::pointer(canvas, x, y, true, false));
        host.send(Touch::pointer(canvas, x, y, false, false));
    }
}

fn coordinates(at: &str) -> Option<(f64, f64)> {
    let (x, y) = at.split_once(',')?;
    let point = (x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?);
    (point.0.is_finite() && point.1.is_finite()).then_some(point)
}

/// A scene point `<x>,<y>` an environment variable names.
fn point(name: &str) -> Option<(f64, f64)> {
    coordinates(&std::env::var(name).ok()?)
}

/// Starts the drag `OSPREY_NATIVE_DRAG` asks for, if any, measured as `OSPREY_NATIVE_BENCH` says.
pub fn drag(host: Rc<Host>) {
    let Some((x, y)) = point("OSPREY_NATIVE_DRAG") else {
        return;
    };
    let (Some(canvas), Some(bench)) = (first_canvas(&host), Bench::requested()) else {
        eprintln!("drag bench: no canvas, or OSPREY_NATIVE_BENCH is not set");
        std::process::exit(2);
    };
    host.send(Touch::pointer(canvas, x, y, true, false));
    step(host, Rc::new(RefCell::new(bench)), canvas, (x, y), 0);
}

/// One drag move per animation frame, around a circle of 120 scene units from the start.
fn step(host: Rc<Host>, bench: Rc<RefCell<Bench>>, canvas: i64, (x, y): (f64, f64), frame: u64) {
    MediaContext::instance().request_animation_frame(move |_| {
        let turn = frame as f64 / 90.0 * std::f64::consts::TAU;
        let at = (x + 120.0 * turn.sin(), y + 60.0 * (1.0 - turn.cos()));
        host.send(Touch::pointer(canvas, at.0, at.1, true, false));
        bench.borrow_mut().record(&host, Instant::now());
        step(host, bench, canvas, (x, y), frame + 1);
    });
}

/// The control number of the first scene in the root region.
fn first_canvas(host: &Host) -> Option<i64> {
    let root = host.regions().get("root")?.node.clone();
    find(&root, 0)
}

fn find(node: &Tree, depth: usize) -> Option<i64> {
    if depth > 64 {
        return None;
    }
    match tree::string(node, "kind") {
        "scene" => tree::ordinal(node),
        _ => tree::inside(node)
            .into_iter()
            .find_map(|child| find(child, depth + 1)),
    }
}
