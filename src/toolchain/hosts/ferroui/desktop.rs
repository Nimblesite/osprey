//! The window of the host (feature `desktop`): the application's regions as the window's content,
//! the frame clock on FerroUI's animation frames, and the window's appearance choosing the
//! theme's colours. `OSPREY_NATIVE_TITLE` and `OSPREY_NATIVE_WINDOW` (`WIDTHxHEIGHT`) set the title
//! and content size, `OSPREY_NATIVE_CHROME=bare` runs the content under the title bar; `OSPREY_NATIVE_BENCH=<seconds>` prints frame timings and closes the process.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use ferroui_base::media::MediaContext;
use ferroui_base::styling::ThemeVariant;
use ferroui_base::Ref;
use ferroui_controls::Window;
use ferroui_view::prelude::*;

use crate::bench::Bench;
use crate::events::Touch;
use crate::ffi::Library;
use crate::host::Host;

/// Default content size: the GPUI host's.
const DEFAULT_SIZE: (f64, f64) = (1200.0, 800.0);

fn size() -> (f64, f64) {
    std::env::var("OSPREY_NATIVE_WINDOW")
        .ok()
        .and_then(|size| {
            let (width, height) = size.split_once('x')?;
            Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
        })
        .unwrap_or(DEFAULT_SIZE)
}

/// Starts the application, opens its window and runs until the window closes.
pub fn run(arguments: &[String]) -> i32 {
    if let Err(problem) = Library::init().and_then(|()| Library::arguments(arguments)) {
        eprintln!("the Osprey application did not start: {problem}");
        return 1;
    }
    let (width, height) = size();
    let title = std::env::var("OSPREY_NATIVE_TITLE").unwrap_or_else(|_| "Osprey".to_owned());
    let made: Rc<RefCell<Option<Rc<Host>>>> = Rc::new(RefCell::new(None));
    let kept = made.clone();
    ferroui_view::launch_with(
        move || {
            let host = Host::new();
            *kept.borrow_mut() = Some(host.clone());
            window()
                .title(title)
                .width(width)
                .height(height)
                .extend_client_area_to_decorations_hint(bare())
                .extend_client_area_title_bar_height_hint(title_bar())
                .child(host.element())
                .into()
        },
        move |window| {
            let Some(host) = made.borrow_mut().take() else {
                return;
            };
            host.open(is_dark(window));
            follow_appearance(&host, window);
            let clock = Rc::new(FrameClock::new());
            host.on_clock(Box::new(move |host| clock.clone().schedule(host.clone())));
            host.send(Touch::phase("active"));
            crate::bench::tap(&host);
            crate::bench::repaint(host.clone());
            crate::bench::drag(host);
        },
    );
    drop(crate::adapters::Cleanup);
    0
}

/// Whether the application draws the whole window itself (`OSPREY_NATIVE_CHROME=bare`): its
/// content runs under a transparent title bar, as the GPUI host's bare chrome does.
fn bare() -> bool {
    std::env::var("OSPREY_NATIVE_CHROME").is_ok_and(|chrome| chrome == "bare")
}

/// The height of a bare window's title bar, `OSPREY_NATIVE_TITLE_BAR` in points (default -1, the
/// system's): the close, minimise and zoom buttons are centred in it, so a bar of 50 points puts
/// their top 18 points down, where the GPUI host's `OSPREY_GPUI_LIGHTS=16,18` puts them.
fn title_bar() -> f64 {
    std::env::var("OSPREY_NATIVE_TITLE_BAR")
        .ok()
        .and_then(|height| height.parse().ok())
        .unwrap_or(-1.0)
}

fn is_dark(window: &Ref<Window>) -> bool {
    window.actual_theme_variant() == Some(ThemeVariant::dark())
}

/// Keeps the theme's colours in the window's appearance.
fn follow_appearance(host: &Rc<Host>, window: &Ref<Window>) {
    let (weak, host) = (window.downgrade(), host.clone());
    let subscription = window.actual_theme_variant_changed(move || {
        if let Some(window) = weak.upgrade() {
            host.set_dark(is_dark(&window));
        }
    });
    let subscription = Cell::new(Some(subscription));
    window.closed(move || drop(subscription.take())).detach();
}

/// The frame clock: while the application has it on, one `frame` event per animation frame, with
/// the seconds since the one before (at most 0.1), and the optional bench.
struct FrameClock {
    scheduled: Cell<bool>,
    last: Cell<Option<Instant>>,
    bench: Option<RefCell<Bench>>,
}

impl FrameClock {
    fn new() -> Self {
        Self {
            scheduled: Cell::new(false),
            last: Cell::new(None),
            bench: Bench::requested().map(RefCell::new),
        }
    }

    fn schedule(self: Rc<Self>, host: Rc<Host>) {
        if self.scheduled.replace(true) {
            return;
        }
        MediaContext::instance().request_animation_frame(move |_| {
            self.scheduled.set(false);
            if !host.animating() {
                self.last.set(None);
                return;
            }
            let now = Instant::now();
            let elapsed = self
                .last
                .get()
                .map_or(1.0 / 60.0, |last| now.duration_since(last).as_secs_f64())
                .min(0.1);
            self.last.set(Some(now));
            host.send(Touch::frame(elapsed));
            if let Some(bench) = &self.bench {
                bench.borrow_mut().record(&host, now);
            }
            self.schedule(host);
        });
    }
}
