//! The window of the host (feature `desktop`): the application's regions as the window's content,
//! the frame clock on FerroUI's animation frames, and the window's appearance choosing the
//! theme's colours. `OSPREY_NATIVE_TITLE` and `OSPREY_NATIVE_WINDOW` (`WIDTHxHEIGHT`) set the title
//! and content size, `OSPREY_NATIVE_CHROME=bare` runs the content under the title bar; `OSPREY_NATIVE_BENCH=<seconds>` prints frame timings and closes the process.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Instant;

use ferroui_base::media::MediaContext;
use ferroui_base::styling::ThemeVariant;
use ferroui_base::Ref;
use ferroui_base::input::KeyGesture;
use ferroui_controls::{AppBuilder, Application, NativeMenu, NativeMenuItem, Window};
use ferroui_desktop::AppBuilderDesktopExtensions;
use ferroui_themes_fluent::FluentTheme;
use ferroui_view::prelude::*;

use crate::bench::Bench;
use crate::events::Touch;
use crate::ffi::Library;
use crate::host::{Command, Host};

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
    launch(
        title.clone(),
        move || {
            let host = Host::new();
            *kept.borrow_mut() = Some(host.clone());
            window()
                .title(title)
                .width(width)
                .height(height)
                .extend_client_area_to_decorations_hint(crate::surface::bare())
                .extend_client_area_title_bar_height_hint(crate::surface::title_bar())
                .child(host.element())
                .into()
        },
        move |window| {
            let Some(host) = made.borrow_mut().take() else {
                return;
            };
            host.set_storage(window.storage_provider());
            host.open(is_dark(window));
            follow_appearance(&host, window);
            show_menus(&host, window);
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

/// Runs the application as `ferroui_view::launch_with` does (the operating system's platform, the
/// Fluent theme, the main loop until the window closes), named `name` before the platform reads the
/// name: the application menu is the application's, not the toolkit's.
fn launch(name: String, view: impl FnOnce() -> Element + 'static, started: impl FnOnce(&Ref<Window>) + 'static) {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    AppBuilder::configure::<Application>()
        .after_setup(move |_| {
            if let Some(application) = Application::current() {
                application.set_name(Some(name.clone()));
            }
        })
        .use_platform_detect()
        .start(
            move |application, _| {
                application.styles().add(FluentTheme::new().as_style());
                let window = ferroui_view::mount_window(view);
                started(&window);
                application.run_window(window);
            },
            &arguments,
        );
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

/// Keeps the window's menu bar the menus the application names: each command sends its tap, and
/// its shortcut is the key gesture its symbol spells (none when it spells no gesture).
fn show_menus(host: &Rc<Host>, window: &Ref<Window>) {
    let (weak, sender) = (window.downgrade(), Rc::downgrade(host));
    host.on_menus(Box::new(move |menus| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        // The platform exporter keeps this root's identity for the life of the window.
        let bar = match NativeMenu::get_menu(&window) {
            Some(bar) => bar,
            None if menus.is_empty() => return,
            None => {
                let bar = NativeMenu::new();
                NativeMenu::set_menu(&window, Some(bar.clone()));
                bar
            }
        };
        bar.items().clear();
        for menu in menus {
            let entry = NativeMenuItem::with_header(&menu.title);
            let list = NativeMenu::new();
            for command in &menu.commands {
                list.add(menu_item(command, &sender));
            }
            entry.set_menu(Some(list));
            bar.add(entry);
        }
    }));
}

fn menu_item(command: &Command, host: &Weak<Host>) -> Ref<NativeMenuItem> {
    let item = NativeMenuItem::with_header(&command.label);
    item.set_is_enabled(command.enabled);
    item.set_gesture(KeyGesture::parse(&command.keys).ok());
    let (host, event) = (host.clone(), command.event);
    item.click(move |_| {
        if let Some(host) = host.upgrade() {
            host.control(event, "");
        }
    })
    .detach();
    item
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
