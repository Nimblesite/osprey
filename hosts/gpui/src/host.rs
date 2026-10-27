//! The GPUI window and synchronous Osprey dispatch seam ([GPUI-HOST-SEAM]).

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::component::{input::InputState, ActiveTheme as _, Theme};
use gpui_kit::*;

use crate::envelope::{self, Request};
use crate::events;
use crate::ffi::Library;
use crate::regions::Regions;
use crate::{clock, render, scene, store};

/// The one native view: framework regions, input editing state and host diagnostics.
pub struct HostView {
    /// The opaque Osprey session and its current region trees.
    pub regions: Regions,
    /// One editor per framework region key, revision and binding ordinal.
    pub inputs: HashMap<(String, u64, u64), Entity<InputState>>,
    /// Decoded scenes and their persistent region-keyed input adapters.
    pub scenes: HashMap<String, scene::Decoded>,
    pub scene_focus: HashMap<String, FocusHandle>,
    pub scene_transforms: HashMap<String, scene::Transform>,
    pub scene_hover: BTreeSet<String>,
    /// Host-only frame and paint measurements.
    pub animating: bool,
    pub last_frame: Option<Instant>,
    pub frame_times: VecDeque<Instant>,
    pub frame_pending: bool,
    pub fps: usize,
    pub dispatch_ms: f64,
    pub paint_ms: f64,
    pub envelope_bytes: usize,
    pub bench: Option<clock::Bench>,
    pub(crate) messages: Vec<String>,
    pub(crate) collapsed: bool,
    seen: BTreeSet<String>,
    pending: Vec<Request>,
}

impl HostView {
    /// Creates the view with any capability requests from the start envelope pending.
    pub fn new(regions: Regions, pending: Vec<Request>, bench: Option<clock::Bench>) -> Self {
        Self {
            regions,
            inputs: HashMap::new(),
            scenes: HashMap::new(),
            scene_focus: HashMap::new(),
            scene_transforms: HashMap::new(),
            scene_hover: BTreeSet::new(),
            animating: false,
            last_frame: None,
            frame_times: VecDeque::new(),
            frame_pending: false,
            fps: 0,
            dispatch_ms: 0.,
            paint_ms: 0.,
            envelope_bytes: 0,
            bench,
            messages: Vec::new(),
            collapsed: false,
            seen: BTreeSet::new(),
            pending,
        }
    }

    /// Sends one event, applies its envelope and answers each requested capability.
    pub fn dispatch(&mut self, event: String, window: &mut Window, cx: &mut Context<Self>) {
        let next = Library::dispatch(self.regions.state(), &event);
        self.envelope_bytes = next.as_ref().map_or(0, String::len);
        self.settle(next.and_then(|text| envelope::parse(&text)));
        for request in std::mem::take(&mut self.pending) {
            if request.kind != "animate" && !request.kind.starts_with("store-") {
                self.diagnose(format!(
                    "unsupported capability {} ({})",
                    request.kind, request.ticket
                ));
            }
            let completion = match request.kind.as_str() {
                "animate" => clock::answer(self, &request),
                _ => store::completion(&request),
            };
            self.dispatch(completion, window, cx);
        }
        clock::redraw(self, window, cx);
        cx.notify();
    }

    /// Applies a decoded result and discards stale editor bindings.
    fn settle(&mut self, next: Result<envelope::Envelope, String>) {
        match next {
            Ok(envelope) => self.pending.extend(self.regions.apply(envelope)),
            Err(problem) => self.diagnose(problem),
        }
        self.retain_inputs();
        self.retain_scenes();
    }

    /// Adds a renderer diagnostic in encounter order.
    pub(crate) fn diagnose(&mut self, message: String) {
        self.messages.push(message);
    }

    /// Adds a recurring renderer diagnostic only on its first encounter.
    pub(crate) fn diagnose_once(&mut self, message: String) {
        if self.seen.insert(message.clone()) {
            self.diagnose(message);
        }
    }

    /// Drops scene adapters whose region disappeared in the latest envelope.
    fn retain_scenes(&mut self) {
        let regions = &self.regions;
        self.scenes.retain(|key, _| regions.get(key).is_some());
        self.scene_focus.retain(|key, _| regions.get(key).is_some());
        self.scene_transforms
            .retain(|key, _| regions.get(key).is_some());
        self.scene_hover.retain(|key| regions.get(key).is_some());
    }

    /// Drops editors whose region or revision disappeared in the latest envelope.
    fn retain_inputs(&mut self) {
        let regions = &self.regions;
        self.inputs.retain(|(key, revision, _), _| {
            regions
                .get(key)
                .is_some_and(|region| region.revision == *revision)
        });
    }
}

impl Render for HostView {
    /// Draws the root region above the host-only diagnostic strip.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        clock::schedule(self, window, cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .bg(self.background(window, cx))
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex_grow(1.)
                    .child(render::region(self, "root", window, cx)),
            )
            .child(render::diagnostics(self, cx))
    }
}

impl HostView {
    /// Resolves the window background in the current system appearance.
    fn background(&self, window: &Window, cx: &App) -> Rgba {
        render::colour(self.regions.theme(), "background", render::is_dark(window))
            .unwrap_or_else(|| Rgba::from(cx.theme().background))
    }
}

/// Starts Osprey, opens the single window and returns the process status.
pub fn run() -> i32 {
    match bench().and_then(|bench| start().map(|(regions, pending)| (regions, pending, bench))) {
        Ok((regions, pending, bench)) => {
            open(regions, pending, bench);
            0
        }
        Err(problem) => {
            eprintln!("GPUI host failed: {problem}");
            1
        }
    }
}

/// Performs all pre-window C calls on the main thread.
fn start() -> Result<(Regions, Vec<Request>), String> {
    Library::init()?;
    let home = std::env::var("HOME").map_err(|problem| format!("HOME is not set: {problem}"))?;
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let context = events::start_context(now(), local_offset()?, "macos", &home, &arguments);
    let started = envelope::parse(&Library::start(&context)?)?;
    let mut regions = Regions::new();
    let pending = regions.apply(started);
    Ok((regions, pending))
}

/// Opens the native window; this is the only permitted infallible window-open boundary.
fn open(regions: Regions, pending: Vec<Request>, bench: Option<clock::Bench>) {
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(options(), cx, move |window, cx| {
            Theme::sync_system_appearance(Some(window), cx);
            cx.new(|cx| new_view(regions, pending, bench, window, cx))
        })
        .expect("failed to open the Osprey window");
    });
}

/// Creates the root view and keeps kit colours synchronized with the window.
fn new_view(
    regions: Regions,
    pending: Vec<Request>,
    bench: Option<clock::Bench>,
    window: &mut Window,
    cx: &mut Context<HostView>,
) -> HostView {
    cx.observe_window_appearance(window, |_, window, cx| {
        Theme::sync_system_appearance(Some(window), cx);
    })
    .detach();
    let mut view = HostView::new(regions, pending, bench);
    view.dispatch(events::lifecycle(now(), "active"), window, cx);
    view
}

/// The fixed native window dimensions and title.
fn options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Default::default(),
            size: size(px(1200.), px(800.)),
        })),
        titlebar: Some(TitlebarOptions {
            title: Some("Osprey".into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Parses the optional frame-run duration without changing normal or smoke launch.
fn bench() -> Result<Option<clock::Bench>, String> {
    let arguments: Vec<String> = std::env::args().collect();
    let Some(index) = arguments.iter().position(|argument| argument == "--bench") else {
        return Ok(None);
    };
    let value = arguments
        .get(index + 1)
        .ok_or("--bench needs a duration in seconds")?;
    let seconds = value
        .parse::<f64>()
        .map_err(|_| "--bench duration is not a number")?;
    if !seconds.is_finite() || seconds < 0. {
        return Err("--bench duration must be finite and nonnegative".to_owned());
    }
    Ok(Some(clock::Bench::new(seconds)))
}

/// Unix seconds for an event or launch context.
pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

/// Reads the platform timezone offset without adding a date/time dependency.
fn local_offset() -> Result<i64, String> {
    let output = Command::new("date")
        .arg("+%z")
        .output()
        .map_err(|problem| format!("Could not read the local UTC offset: {problem}"))?;
    if !output.status.success() {
        return Err("The local UTC offset command failed".to_owned());
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|problem| format!("The local UTC offset was not UTF-8: {problem}"))?;
    parse_offset(text.trim())
}

/// Converts a `date +%z` value such as `+1030` to seconds.
fn parse_offset(text: &str) -> Result<i64, String> {
    let valid = text.len() == 5 && matches!(text.as_bytes()[0], b'+' | b'-');
    if !valid || !text[1..].bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("Invalid local UTC offset {text:?}"));
    }
    let hours = text[1..3]
        .parse::<i64>()
        .map_err(|problem| problem.to_string())?;
    let minutes = text[3..5]
        .parse::<i64>()
        .map_err(|problem| problem.to_string())?;
    if hours > 23 || minutes > 59 {
        return Err(format!("Invalid local UTC offset {text:?}"));
    }
    let sign = if text.starts_with('-') { -1 } else { 1 };
    Ok(sign * (hours * 3600 + minutes * 60))
}
