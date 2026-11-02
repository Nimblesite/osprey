//! The application-driven display clock and frame measurements ([GPUI-HOST-FRAME]).

use std::time::{Duration, Instant};

use gpui_kit::{Context, Window};

use crate::envelope::Request;
use crate::events;
use crate::host::{now, HostView};

/// Measurements accumulated by one `--bench` run.
pub struct Bench {
    pub started: Instant,
    pub duration: Duration,
    pub frames: u64,
    pub dispatch_total: f64,
    pub paint_total: f64,
    /// Main-thread time from each frame event to the end of the paint that shows it: dispatch,
    /// envelope apply, layout and paint. Unlike `fps`, it does not depend on the display clock,
    /// which macOS slows for a hidden window or a locked screen.
    pub frame_total: f64,
}

impl Bench {
    /// Starts a run whose report follows `seconds` of wall-clock frames.
    pub fn new(seconds: f64) -> Self {
        Self {
            started: Instant::now(),
            duration: Duration::from_secs_f64(seconds.max(0.)),
            frames: 0,
            dispatch_total: 0.,
            paint_total: 0.,
            frame_total: 0.,
        }
    }
}

/// Completes an animate request immediately and arms or disarms the clock.
pub fn answer(view: &mut HostView, request: &Request) -> String {
    let on = request
        .fields
        .get("on")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if on && !view.animating {
        view.last_frame = None;
        view.frame_times.clear();
    }
    view.animating = on;
    if !on {
        view.last_frame = None;
        view.frame_times.clear();
        view.fps = 0;
    }
    events::completed_ok(now(), &request.ticket, serde_json::Value::Null)
}

/// Schedules frame delivery outside render, where application dispatch is forbidden.
pub fn schedule(view: &mut HostView, window: &mut Window, cx: &mut Context<HostView>) {
    if !view.animating || view.frame_pending {
        return;
    }
    view.frame_pending = true;
    cx.on_next_frame(window, |view, window, cx| {
        view.frame_pending = false;
        tick(view, window, cx);
    });
}

/// Delivers one clamped frame event and records its synchronous dispatch cost.
fn tick(view: &mut HostView, window: &mut Window, cx: &mut Context<HostView>) {
    if !view.animating {
        return;
    }
    let arrived = Instant::now();
    let elapsed = view
        .last_frame
        .map_or(1. / 60., |last| arrived.duration_since(last).as_secs_f64())
        .min(0.1);
    view.last_frame = Some(arrived);
    view.frame_times.push_back(arrived);
    while view
        .frame_times
        .front()
        .is_some_and(|time| arrived.duration_since(*time) > Duration::from_secs(1))
    {
        view.frame_times.pop_front();
    }
    view.fps = view.frame_times.len();
    view.frame_started = Some(arrived);
    let started = Instant::now();
    crate::host::deliver_opened(view, window, cx);
    view.dispatch(events::frame(now(), elapsed), window, cx);
    view.dispatch_ms = started.elapsed().as_secs_f64() * 1000.;
    record_bench(view, arrived);
    cx.notify();
}

/// Accumulates frame metrics and exits only after the requested bench duration.
fn record_bench(view: &mut HostView, arrived: Instant) {
    let Some(bench) = &mut view.bench else {
        return;
    };
    bench.frames += 1;
    bench.dispatch_total += view.dispatch_ms;
    bench.paint_total += view.paint_ms;
    bench.frame_total += view.frame_ms;
    if arrived.duration_since(bench.started) < bench.duration {
        return;
    }
    report(bench, view.envelope_bytes);
    std::process::exit(0);
}

/// Prints the single machine-readable bench result.
fn report(bench: &Bench, envelope: usize) {
    let frames = bench.frames.max(1) as f64;
    let seconds = bench.started.elapsed().as_secs_f64().max(f64::EPSILON);
    println!(
        "bench fps={} dispatch_ms={} paint_ms={} frame_ms={} envelope_bytes={} frames={}",
        bench.frames as f64 / seconds,
        bench.dispatch_total / frames,
        bench.paint_total / frames,
        bench.frame_total / frames,
        envelope,
        bench.frames
    );
}

/// Schedules a view redraw after every dispatch, including outside the render phase.
pub fn redraw(view: &HostView, window: &Window, cx: &Context<HostView>) {
    if !view.animating {
        return;
    }
    let entity = cx.entity_id();
    window.on_next_frame(move |_, cx| cx.notify(entity));
}
