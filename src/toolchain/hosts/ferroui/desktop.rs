//! The window of the host (feature `desktop`): the application's regions as the window's content,
//! the frame clock on FerroUI's animation frames, and the window's appearance choosing the
//! theme's colours. `OSPREY_NATIVE_TITLE` and `OSPREY_NATIVE_WINDOW` (`WIDTHxHEIGHT`) set the title
//! and content size, `OSPREY_NATIVE_CHROME=bare` runs the content under the title bar, `OSPREY_NATIVE_APPEARANCE=dark`
//! keeps the controls dark whatever the system's appearance (an application that draws a dark canvas); `OSPREY_NATIVE_BENCH=<seconds>` prints frame timings and closes the process.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::time::Instant;

use ferroui_base::media::MediaContext;
use ferroui_base::styling::ThemeVariant;
use ferroui_base::Ref;
use ferroui_base::input::KeyGesture;
use ferroui_controls::{AppBuilder, Application, MenuItemToggleType, NativeMenu, NativeMenuItem, Window};
use ferroui_controls::application_lifetimes::ClassicDesktopStyleApplicationLifetime;
use ferroui_desktop::AppBuilderDesktopExtensions;
use ferroui_themes_fluent::FluentTheme;
use ferroui_view::prelude::*;

use crate::bench::Bench;
use crate::events::Touch;
use crate::ffi::Library;
use crate::host::{Command, Host, Menu};

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
    let status = launch(
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
            watch_detached(&host, window.clone());
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
    status
}

/// Each generation opens once, so closing a patch by its title bar lasts until it is reopened.
fn watch_detached(host: &Rc<Host>, owner: Ref<Window>) {
    let owner = owner.downgrade();
    let active = RefCell::new(None::<(String, Ref<Window>)>);
    *host.windows.borrow_mut() = Some(Box::new(move |host| {
        let Some(owner) = owner.upgrade() else { return };
        let key = host.regions().detached();
        if active.borrow().as_ref().map(|(old, _)| old) == key.as_ref() { return; }
        let previous = active.borrow_mut().take();
        if let Some((_, old)) = previous { if old.is_visible() { old.close(); } }
        if let Some(key) = key {
            let title = key.splitn(3, ':').nth(2).unwrap_or("Patch").to_owned();
            let content = host.detached_element(key.clone());
            let panel = ferroui_view::mount_window(|| window().title(title).width(1200.).height(800.).child(content).into());
            let weak = Rc::downgrade(host);
            let closed = key.clone();
            let subscription = panel.closed(move || {
                if let Some(host) = weak.upgrade() {
                    if host.regions().detached().as_ref() == Some(&closed) {
                        host.send(Touch::phase(&format!("closed:{closed}")));
                    }
                }
            });
            subscription.detach();
            panel.show_with_owner(&owner);
            *active.borrow_mut() = Some((key, panel));
        }
    }));
}

/// Runs the application as `ferroui_view::launch_with` does (the operating system's platform, the
/// Fluent theme, the main loop until the window closes), named `name` before the platform reads the
/// name: the application menu is the application's, not the toolkit's.
fn launch(name: String, view: impl FnOnce() -> Element + 'static, started: impl FnOnce(&Ref<Window>) + 'static) -> i32 {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let lifetime = ClassicDesktopStyleApplicationLifetime::new();
    lifetime.set_args(Some(arguments.clone()));
    let builder = AppBuilder::configure::<Application>()
        .after_setup(move |_| {
            if let Some(application) = Application::current() {
                application.set_name(Some(name.clone()));
            }
        })
        .use_platform_detect()
        .setup_with_lifetime(lifetime.clone());
    let Some(application) = builder.instance() else {
        eprintln!("FerroUI did not initialize the desktop application");
        return 1;
    };
    own_application_menu(&application);
    if std::env::var("OSPREY_NATIVE_APPEARANCE").is_ok_and(|look| look == "dark") {
        application.set_requested_theme_variant(Some(ThemeVariant::dark()));
    }
    application.styles().add(FluentTheme::new().as_style());
    let window = ferroui_view::mount_window(view);
    lifetime.set_main_window(Some(window.clone()));
    started(&window);
    lifetime.start_with_own_args()
}

/// Drops the toolkit's "About FerroUI" and the rule under it from the application menu: its panel
/// names the toolkit and the toolkit's version, not the application.
fn own_application_menu(application: &Application) {
    let Some(menu) = NativeMenu::get_menu(application) else {
        return;
    };
    let items = menu.items();
    let about = (items.count() > 1)
        .then(|| items.get(0).cast::<NativeMenuItem>())
        .flatten()
        .is_some_and(|item| item.header().as_deref() == Some("About FerroUI"));
    if about {
        items.remove_at(0);
        items.remove_at(0);
    }
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
        let keyed = keyed(menus);
        for menu in menus {
            let entry = NativeMenuItem::with_header(&menu.title);
            let list = NativeMenu::new();
            for command in &menu.commands {
                list.add(menu_item(command, keyed.contains(&command.event), &sender));
            }
            entry.set_menu(Some(list));
            bar.add(entry);
        }
    }));
}

/// The commands that carry their shortcut. Commands that are never available together may share
/// one (Play and Stop on Space): of those sharing keys the first that can be chosen has them, so the
/// keys always run what is possible now.
fn keyed(menus: &[Menu]) -> HashSet<i64> {
    let commands: Vec<&Command> = menus.iter().flat_map(|menu| &menu.commands).collect();
    let mut owners: HashMap<&str, &Command> = HashMap::new();
    for command in commands.iter().filter(|command| !command.keys.is_empty()) {
        let owner = owners.entry(command.keys.as_str()).or_insert(command);
        if !owner.enabled && command.enabled {
            *owner = command;
        }
    }
    owners.values().map(|command| command.event).collect()
}

fn menu_item(command: &Command, keyed: bool, host: &Weak<Host>) -> Ref<NativeMenuItem> {
    let item = NativeMenuItem::with_header(&command.label);
    item.set_is_enabled(command.enabled);
    if command.checked {
        item.set_toggle_type(MenuItemToggleType::CheckBox);
        item.set_is_checked(true);
    }
    item.set_gesture(keyed.then(|| KeyGesture::parse(&command.keys).ok()).flatten());
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
