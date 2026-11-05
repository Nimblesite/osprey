//! The GPUI window and synchronous Osprey dispatch seam ([GPUI-HOST-SEAM]).

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::component::{input::InputState, ActiveTheme as _, Theme};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::envelope::{self, Request};
use crate::events;
use crate::ffi::Library;
use crate::regions::Regions;
use crate::{adapters, clock, render, scene, store};

/// The one native view: framework regions, input editing state and host diagnostics.
pub struct HostView {
    /// The opaque Osprey session and its current region trees.
    pub regions: Regions,
    /// One editor per framework region key, revision and binding ordinal.
    pub inputs: HashMap<(String, u64, u64), Entity<InputState>>,
    /// Decoded scenes and their persistent region-keyed input adapters.
    pub scenes: HashMap<String, scene::Decoded>,
    /// Scene sprite images, decoded once and turned as drawn.
    pub sprites: scene::atlas::Sprites,
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
    /// When the frame event now being drawn arrived, until its paint ends.
    pub frame_started: Option<std::time::Instant>,
    /// Main-thread time from the last frame event to the end of its paint.
    pub frame_ms: f64,
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
            sprites: Default::default(),
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
            frame_started: None,
            frame_ms: 0.,
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
        self.answer_pending(window, cx);
        clock::redraw(self, window, cx);
        cx.notify();
    }

    /// Answers every capability the latest envelopes requested ([GPUI-HOST-CAPABILITIES]);
    /// an unknown kind is diagnosed and still completes with a protocol error.
    fn answer_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for request in std::mem::take(&mut self.pending) {
            match request.kind.as_str() {
                "animate" => {
                    let completion = clock::answer(self, &request);
                    self.dispatch(completion, window, cx);
                }
                "run" | "list-directories" | "read-texts" => {
                    self.answer_in_background(request, window, cx);
                }
                "cancel" => {
                    self.dispatch(adapters::completion(&request), window, cx);
                }
                "store-read" | "store-write" | "store-list" | "store-delete" => {
                    self.dispatch(store::completion(&request), window, cx);
                }
                // [CAST-MOBILE-DELIVERY] iOS has no GPUI path prompt: its document
                // picker is answered across the OS bridge like any blocking adapter.
                "choose-file" | "host-call"
                    if cfg!(any(target_os = "ios", target_os = "android")) =>
                {
                    self.answer_in_background(request, window, cx);
                }
                "choose-file" | "choose-folder" | "save-file" | "reveal" => {
                    if let Some(completion) = adapters::dispatch_platform(&request, window, cx) {
                        self.dispatch(completion, window, cx);
                    }
                }
                kind => {
                    self.diagnose(format!(
                        "unsupported capability {kind} ({})",
                        request.ticket
                    ));
                    let error = events::completed_err(
                        now(),
                        &request.ticket,
                        &format!("unsupported capability {kind}"),
                    );
                    self.dispatch(error, window, cx);
                }
            }
        }
    }

    /// Runs a blocking adapter (a process, a directory walk, file reads) on the background executor,
    /// so a slow capability never stalls the window, and delivers its completion on the main thread.
    fn answer_in_background(
        &mut self,
        request: Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = cx
            .background_executor()
            .spawn(async move { adapters::completion(&request) });
        cx.spawn_in(window, async move |view, cx| {
            let completion = task.await;
            let _ = view.update_in(cx, |view, window, cx| view.dispatch(completion, window, cx));
        })
        .detach();
    }

    /// Applies a decoded result and discards stale editor bindings.
    fn settle(&mut self, next: Result<envelope::Envelope, String>) {
        match next.and_then(|envelope| self.regions.apply(envelope)) {
            Ok(requests) => self.pending.extend(requests),
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
    /// Draws the root region above the host-only diagnostic strip, both contained in the window's
    /// safe area ([GPUI-HOST-RENDER]): the platform's insets clear the status bar, notch and home
    /// indicator, and a window that reports zero insets (every desktop window) keeps its framed
    /// padding unchanged.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        clock::schedule(self, window, cx);
        let framed = !bare();
        let safe = window.insets().safe_area;
        div()
            .size_full()
            .flex()
            .flex_col()
            .when(framed, |this| this.p_4())
            .bg(self.background(window, cx))
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_grow(1.)
                    .min_h_0()
                    .pt(safe.top)
                    .pr(safe.right)
                    .pb(safe.bottom)
                    .pl(safe.left)
                    .when(framed, |this| this.gap_4())
                    .child(render::region(self, "root", window, cx))
                    .child(render::diagnostics(self, cx)),
            )
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
    let home = crate::home()?;
    let arguments = crate::launch::arguments()?;
    let context = events::start_context(
        now(),
        local_offset()?,
        std::env::consts::OS,
        &home,
        &arguments,
    );
    let started = envelope::parse(&Library::start(&context)?)?;
    let mut regions = Regions::new();
    let pending = regions.apply(started)?;
    Ok((regions, pending))
}

/// Opens the native window; this is the only permitted infallible window-open boundary.
fn open(regions: Regions, pending: Vec<Request>, bench: Option<clock::Bench>) {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let application = gpui_kit::application();
    // [CAST-MOBILE-GPUI] gpui-kit leaves the mobile backend to the application.
    #[cfg(target_os = "ios")]
    let application = gpui_kit::Application::with_platform(gpui_ios::current_platform(false));
    // [CAST-MOBILE-GPUI] the same host code draws through the Android backend over gpui-pre-wgpu.
    #[cfg(target_os = "android")]
    let Some(application) = android_application() else {
        eprintln!("GPUI host failed: no Android activity to draw through");
        return;
    };
    application.on_open_urls(|urls| opened().extend(urls));
    application.run(move |cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(options(), cx, move |window, cx| {
            Theme::sync_system_appearance(Some(window), cx);
            cx.new(|cx| new_view(regions, pending, bench, window, cx))
        })
        .expect("failed to open the Osprey window");
        // A window launched from a terminal opens behind it, where macOS throttles its display clock.
        cx.activate(true);
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
    if crate::self_test::requested().is_some() {
        match crate::self_test::load() {
            Ok(run) => crate::self_test::in_window(run, window, cx),
            Err(problem) => {
                eprintln!("self-test failed: {problem}");
                std::process::exit(1);
            }
        }
    }
    view
}

/// The NativeActivity handle `android_main` installed before the host starts
/// ([CAST-MOBILE-GPUI]).
#[cfg(target_os = "android")]
pub(crate) static ANDROID_APP: std::sync::OnceLock<android_activity::AndroidApp> =
    std::sync::OnceLock::new();

/// Installs the activity the Android platform backend draws through.
#[cfg(target_os = "android")]
pub fn set_android_app(app: android_activity::AndroidApp) {
    let _ = ANDROID_APP.set(app);
}

/// The application on this activity's GPUI backend.
#[cfg(target_os = "android")]
fn android_application() -> Option<gpui_kit::Application> {
    let app = ANDROID_APP.get()?.clone();
    Some(gpui_kit::Application::with_platform(
        gpui_android::current_platform(&app),
    ))
}

/// Files Finder asked the application to open and no frame has delivered yet ([GPUI-HOST-OPEN]).
static OPENED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn opened() -> std::sync::MutexGuard<'static, Vec<String>> {
    OPENED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Delivers each file Finder opened as the lifecycle phase `open:<path>`; a `file://` URL is
/// turned back into its path ([GPUI-HOST-OPEN]).
pub fn deliver_opened(view: &mut HostView, window: &mut Window, cx: &mut Context<HostView>) {
    let urls = std::mem::take(&mut *opened());
    for url in urls {
        let path = url
            .strip_prefix("file://")
            .map_or(url.clone(), events::unescaped);
        view.dispatch(
            events::lifecycle(now(), &format!("open:{path}")),
            window,
            cx,
        );
    }
}

/// Whether the application draws the whole window itself (`OSPREY_GPUI_CHROME=bare`): no host
/// padding or frame-time strip, and a transparent title bar its canvas runs under, with the
/// traffic lights where `OSPREY_GPUI_LIGHTS=x,y` puts them ([UI-MACHINE-CANVAS]).
pub fn bare() -> bool {
    cfg!(any(target_os = "ios", target_os = "android"))
        || std::env::var("OSPREY_GPUI_CHROME").is_ok_and(|chrome| chrome == "bare")
}

/// The traffic lights' origin for a bare window, `OSPREY_GPUI_LIGHTS=x,y` in points.
fn lights() -> Option<Point<Pixels>> {
    let requested = std::env::var("OSPREY_GPUI_LIGHTS").ok()?;
    let (x, y) = requested.split_once(',')?;
    Some(point(
        px(x.trim().parse().ok()?),
        px(y.trim().parse().ok()?),
    ))
}

/// The fixed native window dimensions and title, `OSPREY_GPUI_TITLE` or "Osprey".
fn options() -> WindowOptions {
    let bounds = window_size().map(|(width, height)| {
        WindowBounds::Windowed(Bounds {
            origin: Default::default(),
            size: size(px(width), px(height)),
        })
    });
    WindowOptions {
        window_bounds: bounds,
        titlebar: Some(TitlebarOptions {
            title: Some(
                std::env::var("OSPREY_GPUI_TITLE")
                    .unwrap_or_else(|_| "Osprey".to_owned())
                    .into(),
            ),
            appears_transparent: bare(),
            traffic_light_position: lights().filter(|_| bare()),
        }),
        ..Default::default()
    }
}

/// The content size: 1200 by 800, GPUI's default for OSPREY_GPUI_WINDOW=default, or its
/// `WIDTHxHEIGHT`, so a port can open at exactly the size of the example it is compared with.
fn window_size() -> Option<(f32, f32)> {
    let requested = std::env::var("OSPREY_GPUI_WINDOW").unwrap_or_default();
    if requested == "default" {
        return None;
    }
    let parsed = requested
        .split_once('x')
        .and_then(|(width, height)| Some((width.parse().ok()?, height.parse().ok()?)));
    Some(parsed.unwrap_or((1200., 800.)))
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
#[cfg(not(windows))]
fn local_offset() -> Result<i64, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?;
    let timestamp = libc::time_t::try_from(seconds.as_secs()).map_err(|error| error.to_string())?;
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: both pointers are live, correctly aligned, and owned for this call.
    if unsafe { libc::localtime_r(&timestamp, local.as_mut_ptr()) }.is_null() {
        return Err("Could not read the local UTC offset".to_owned());
    }
    // SAFETY: localtime_r initialized the structure after returning a non-null pointer.
    Ok(unsafe { local.assume_init() }.tm_gmtoff)
}

/// Reads the local UTC offset from the time zone Windows reports, with daylight saving applied:
/// Windows has no `date` command outside MSYS2. Windows gives the bias in minutes west of UTC.
#[cfg(windows)]
fn local_offset() -> Result<i64, String> {
    #[repr(C)]
    struct SystemTime([u16; 8]);
    #[repr(C)]
    struct TimeZone {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: SystemTime,
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: SystemTime,
        daylight_bias: i32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetTimeZoneInformation(zone: *mut TimeZone) -> u32;
    }
    const INVALID: u32 = u32::MAX;
    const STANDARD: u32 = 1;
    const DAYLIGHT: u32 = 2;
    let mut zone = std::mem::MaybeUninit::<TimeZone>::zeroed();
    // SAFETY: the call writes one TIME_ZONE_INFORMATION into the zeroed buffer it is given.
    let id = unsafe { GetTimeZoneInformation(zone.as_mut_ptr()) };
    if id == INVALID {
        return Err("Windows could not report the local time zone".to_owned());
    }
    // SAFETY: the buffer was zeroed and the call succeeded, so every field holds a value.
    let zone = unsafe { zone.assume_init() };
    let extra = match id {
        STANDARD => zone.standard_bias,
        DAYLIGHT => zone.daylight_bias,
        _ => 0,
    };
    Ok(-i64::from(zone.bias + extra) * 60)
}
