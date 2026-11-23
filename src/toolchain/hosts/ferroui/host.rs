//! The host: the application's last checked frame and the FerroUI tree that shows it
//! ([NATIVE-SHM-BOUNDARY], docs/osprey-prism/README.md). Every call into the application goes
//! through `ffi::Library`, whose frame is read in place from shared memory (`frame.rs`) and
//! checked whole (`rows.rs`). Nothing is encoded in either direction. The tree is one component
//! that runs again after every frame; the reconciler of `ferroui-view` patches the controls it
//! made before, so a control whose place and class stay keeps focus, caret and selection.

use std::cell::{Cell, Ref, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use ferroui_base::layout::{HorizontalAlignment, VerticalAlignment};
use ferroui_base::media::Color;
use ferroui_base::platform::storage::IStorageProvider;
use ferroui_base::threading::{Dispatcher, DispatcherPriority};
use ferroui_view::prelude::*;

use crate::events::Touch;
use crate::ffi::{self, Library};
use crate::regions::Regions;
use crate::rows::{Ask, Shown};
use crate::theme::Look;
use crate::{adapters, store};

/// What the window does when the application turns the frame clock on.
pub type ClockHandler = Box<dyn Fn(&Rc<Host>)>;

/// What the window does when the application's window menus change.
pub type MenusHandler = Box<dyn Fn(&[Menu])>;

/// One command of a window menu: its label, its shortcut (`Cmd+S`, empty for none), whether it
/// can be chosen, whether it is the current choice, and the number its tap is sent under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub label: String,
    pub keys: String,
    pub enabled: bool,
    /// The current one of a set of choices: shown with a check mark.
    pub checked: bool,
    pub event: i64,
}

/// One menu of the window's menu bar ([UI-COMPONENTS-CONTEXT-MENU]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub title: String,
    pub commands: Vec<Command>,
}

thread_local! {
    /// The host of this UI thread, so a capability answered on a worker thread finds it again.
    static CURRENT: RefCell<Weak<Host>> = const { RefCell::new(Weak::new()) };
}

/// The application's regions, its last frame and the state of the tree that shows them.
pub struct Host {
    regions: RefCell<Regions>,
    shown: RefCell<Rc<Shown>>,
    /// Bumped after every frame: the tree's component reads it, so it runs again.
    version: Signal<u64>,
    dark: Signal<bool>,
    /// True while the host itself changes controls: their change events are not sent.
    patching: Cell<bool>,
    /// True while a call into the application runs: an event raised by the tree it patches is
    /// dropped rather than re-entering the library.
    calling: Cell<bool>,
    animating: Cell<bool>,
    clock: RefCell<Option<ClockHandler>>,
    #[cfg(feature = "desktop")]
    pub windows: RefCell<Option<ClockHandler>>,
    storage: RefCell<Option<Rc<dyn IStorageProvider>>>,
    messages: RefCell<Vec<String>>,
    rendered: RefCell<Vec<String>>,
    pending: RefCell<Vec<Ask>>,
    runs: RefCell<Vec<RunJob>>,
    /// The window menus the tree being built names, those of the last tree, and what shows them.
    gathered: RefCell<Vec<Menu>>,
    menus: RefCell<Vec<Menu>>,
    menu_bar: RefCell<Option<MenusHandler>>,
    /// Milliseconds the last call into the application took, and the last tree build.
    pub dispatch_ms: Cell<f64>,
    pub build_ms: Cell<f64>,
    /// Milliseconds the last event took whole: the call, the frame applied and the tree patched.
    pub send_ms: Cell<f64>,
}

impl Host {
    /// A host before the application's first frame. Signals belong to the current owner, so
    /// this is called where the tree is mounted.
    pub fn new() -> Rc<Self> {
        let host = Rc::new(Self {
            regions: RefCell::new(Regions::new()),
            shown: RefCell::new(Rc::new(Shown::default())),
            version: signal(0),
            dark: signal(false),
            patching: Cell::new(false),
            calling: Cell::new(false),
            animating: Cell::new(false),
            clock: RefCell::new(None),
            #[cfg(feature = "desktop")]
            windows: RefCell::new(None),
            storage: RefCell::new(None),
            messages: RefCell::new(Vec::new()),
            rendered: RefCell::new(Vec::new()),
            pending: RefCell::new(Vec::new()),
            runs: RefCell::new(Vec::new()),
            gathered: RefCell::new(Vec::new()),
            menus: RefCell::new(Vec::new()),
            menu_bar: RefCell::new(None),
            dispatch_ms: Cell::new(0.),
            build_ms: Cell::new(0.),
            send_ms: Cell::new(0.),
        });
        CURRENT.with(|current| *current.borrow_mut() = Rc::downgrade(&host));
        host
    }

    /// Draws the application's first frame for this window's appearance.
    pub fn open(self: &Rc<Self>, dark: bool) {
        self.dark.set(dark);
        let first = crate::home().and_then(|home| {
            Library::open(
                crate::now(),
                crate::local_offset(),
                std::env::consts::OS,
                &home,
                dark,
            )
        });
        self.settle(first);
    }

    /// Sends one host event, applies the frame it draws and answers what that frame asks.
    pub fn send(self: &Rc<Self>, touch: Touch) {
        if self.calling.replace(true) {
            self.diagnose(format!("dropped a re-entrant event of kind {}", touch.kind));
            return;
        }
        let started = Instant::now();
        let next = Library::event(crate::now(), &touch);
        self.dispatch_ms
            .set(started.elapsed().as_secs_f64() * 1000.);
        trace(&touch, self.dispatch_ms.get());
        self.calling.set(false);
        self.settle(next);
        self.send_ms.set(started.elapsed().as_secs_f64() * 1000.);
    }

    /// A control delivering `words` under the number the framework issued; a change the host
    /// made itself is not sent.
    pub fn control(self: &Rc<Self>, ordinal: i64, words: &str) {
        if !self.patching.get() {
            self.send(Touch::control(ordinal, words));
        }
    }

    /// Switches between the theme's light and dark colours and tells the application.
    pub fn set_dark(self: &Rc<Self>, dark: bool) {
        if self.dark.get_untracked() != dark {
            self.patching.set(true);
            self.dark.set(dark);
            self.patching.set(false);
            self.send(Touch::appearance(dark));
        }
    }

    /// Sets what the window does when the application turns the frame clock on.
    pub fn on_clock(self: &Rc<Self>, handler: ClockHandler) {
        if self.animating.get() {
            handler(self);
        }
        *self.clock.borrow_mut() = Some(handler);
    }

    /// Runs the tree again for the frame already shown, without calling the application: what a
    /// repaint costs the toolkit alone.
    pub fn redraw(&self) {
        self.patching.set(true);
        self.version.update(|version| *version += 1);
        self.patching.set(false);
    }

    /// Whether the application has the frame clock on.
    pub fn animating(&self) -> bool {
        self.animating.get()
    }

    /// The last checked frame.
    pub fn shown(&self) -> Rc<Shown> {
        self.shown.borrow().clone()
    }

    /// Every region the last frame told.
    pub fn regions(&self) -> Ref<'_, Regions> {
        self.regions.borrow()
    }

    /// Whether a change event comes from the host's own patch.
    pub fn is_patching(&self) -> bool {
        self.patching.get()
    }

    /// Records a host diagnostic once.
    pub fn diagnose(&self, message: String) {
        let mut messages = self.messages.borrow_mut();
        if !messages.contains(&message) {
            messages.push(message);
        }
    }

    /// Sets what shows the window's menus, and shows the ones already known.
    pub fn on_menus(&self, handler: MenusHandler) {
        handler(&self.menus.borrow());
        *self.menu_bar.borrow_mut() = Some(handler);
    }

    /// The window's menus as the last tree named them.
    pub fn menus(&self) -> Vec<Menu> {
        self.menus.borrow().clone()
    }

    /// Records one window menu of the tree being built.
    pub(crate) fn gather(&self, menu: Menu) {
        self.gathered.borrow_mut().push(menu);
    }

    /// Takes the built tree's menus as the window's; the menu bar is told only when they differ.
    fn commit_menus(&self) {
        let next = std::mem::take(&mut *self.gathered.borrow_mut());
        if *self.menus.borrow() == next {
            return;
        }
        *self.menus.borrow_mut() = next;
        if let Some(handler) = self.menu_bar.borrow().as_ref() {
            handler(&self.menus.borrow());
        }
    }

    /// Records what a render could not draw.
    pub(crate) fn refuse(&self, message: String) {
        let mut rendered = self.rendered.borrow_mut();
        if !rendered.contains(&message) {
            rendered.push(message);
        }
    }

    /// Applies a checked frame (slice writes, diagnostics, regions, requests), runs the tree
    /// again and answers what the frame asked.
    fn settle(self: &Rc<Self>, next: Result<Shown, String>) {
        match next {
            Ok(shown) => self.adopt(shown),
            Err(problem) => self.diagnose(problem),
        }
        self.patching.set(true);
        self.version.update(|version| *version += 1);
        self.patching.set(false);
        #[cfg(feature = "desktop")]
        if let Some(sync) = self.windows.borrow().as_ref() { sync(self); }
        self.answer_pending();
    }

    fn adopt(&self, shown: Shown) {
        if let Err(problem) = self.regions.borrow_mut().adopt(&shown) {
            self.diagnose(problem);
            return;
        }
        for (key, text) in shown.slices() {
            if let Err(problem) = ffi::put_slice(key, text) {
                self.diagnose(problem);
            }
        }
        for reason in shown.diagnostics() {
            self.diagnose(reason.to_owned());
        }
        self.pending.borrow_mut().extend(shown.requests().cloned());
        *self.shown.borrow_mut() = Rc::new(shown);
    }

    /// Answers every capability the latest frames requested; an unknown kind still completes,
    /// with an error, so the application is never left waiting.
    fn answer_pending(self: &Rc<Self>) {
        let asks = std::mem::take(&mut *self.pending.borrow_mut());
        for ask in asks {
            match ask.kind() {
                "animate" => self.animate(&ask),
                "choose-file" => self.choose_file(ask),
                "run" => self.run(ask),
                "list-directories" | "read-texts" | "host-call" => background(ask),
                "cancel" => self.cancel(&ask),
                "store-read" | "store-write" | "store-list" | "store-delete" => {
                    self.send(store::completion(&ask));
                }
                kind => {
                    let problem = format!("unsupported capability {kind} ({})", ask.ticket);
                    self.diagnose(problem.clone());
                    self.send(Touch::completed(&ask.ticket, Err(problem)));
                }
            }
        }
    }

    fn run(self: &Rc<Self>, ask: Ask) {
        let mut runs = self.runs.borrow_mut();
        let ticket = ask.ticket.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let dispatcher = Dispatcher::ui_thread();
        let worker = std::thread::spawn(move || {
            let answer = adapters::process::cancellable(&ask, &stop);
            dispatcher.post(
                move || {
                    if let Some(host) = CURRENT.with(|current| current.borrow().upgrade()) {
                        let mut jobs = host.runs.borrow_mut();
                        let Some(index) = jobs
                            .iter()
                            .position(|job| Arc::ptr_eq(&job.cancelled, &stop))
                        else {
                            return;
                        };
                        jobs.swap_remove(index);
                        drop(jobs);
                        if !stop.load(Ordering::Acquire) {
                            host.send(Touch::completed(&ask.ticket, answer));
                        }
                    }
                },
                DispatcherPriority::LOADED,
            );
        });
        runs.push(RunJob {
            ticket,
            cancelled,
            worker,
        });
    }

    fn cancel(self: &Rc<Self>, ask: &Ask) {
        let result = adapters::text(ask, 1, "cancelled").map(|ticket| {
            for run in self.runs.borrow().iter().filter(|run| run.ticket == ticket) {
                run.cancelled.store(true, Ordering::Release);
            }
            String::new()
        });
        self.send(Touch::completed(&ask.ticket, result));
    }

    /// Binds native pickers to the window hosting the Osprey surface.
    pub fn set_storage(&self, provider: Rc<dyn IStorageProvider>) {
        *self.storage.borrow_mut() = Some(provider);
    }

    fn choose_file(self: &Rc<Self>, ask: Ask) {
        let provider = self.storage.borrow().clone();
        let host = Rc::downgrade(self);
        drop(Dispatcher::ui_thread().spawn_local(async move {
            let result = match provider {
                Some(provider) => adapters::picker::choose(provider, &ask).await,
                None => Err("This surface has no window for a file picker.".into()),
            };
            if let Some(host) = host.upgrade() {
                host.send(Touch::completed(&ask.ticket, result));
            }
        }));
    }

    /// Completes an animate request at once and arms or disarms the frame clock.
    fn animate(self: &Rc<Self>, ask: &Ask) {
        let on = ask.flag(1);
        let was = self.animating.replace(on);
        self.send(Touch::completed(&ask.ticket, Ok(String::new())));
        if on && !was {
            if let Some(handler) = self.clock.borrow().as_ref() {
                handler(self);
            }
        }
    }

    #[cfg(feature = "desktop")]
    pub fn detached_element(self: &Rc<Self>, key: String) -> Element {
        let owner = Rc::downgrade(self);
        border().child(rerender(move || {
            let Some(host) = owner.upgrade() else { return border().into() };
            host.version.get();
            let dark = host.dark.get();
            let theme = host.regions().theme().cloned().map(Rc::new);
            if host.regions().get(&key).is_none() { return border().into(); }
            crate::view::region(&host, Look { theme, dark }, &key)
        })).into()
    }

    /// The element of the whole window: the root region above the diagnostics strip.
    pub fn element(self: &Rc<Self>) -> Element {
        let host = self.clone();
        border()
            .child(rerender(move || {
                host.version.get();
                let dark = host.dark.get();
                let started = Instant::now();
                host.rendered.borrow_mut().clear();
                let theme = host.regions.borrow().theme().cloned().map(Rc::new);
                let look = Look { theme, dark };
                let background = look.colour("background");
                host.gathered.borrow_mut().clear();
                let root = crate::view::root(&host, look.clone());
                host.commit_menus();
                let mut messages = host.messages.borrow().clone();
                messages.extend(host.rendered.borrow().iter().cloned());
                let mut grid = grid()
                    .row_definitions("*,Auto")
                    .child(border().grid_row(0).child(root));
                if !messages.is_empty() {
                    grid = grid.child(diagnostics(&messages, &look));
                }
                let mut frame = border();
                if let Some(background) = background {
                    frame = frame.background(background);
                }
                host.build_ms.set(started.elapsed().as_secs_f64() * 1000.);
                frame.child(grid)
            }))
            .into()
    }
}

/// With `OSPREY_NATIVE_TRACE=1`, one stderr line per event: the wall clock in milliseconds, its
/// kind, whether a pointer was down, and how long the application took.
fn trace(touch: &Touch, took: f64) {
    thread_local! {
        static ON: bool = std::env::var("OSPREY_NATIVE_TRACE").as_deref() == Ok("1");
    }
    if ON.with(|on| *on) {
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        eprintln!(
            "trace {at} kind={} down={} ms={took:.1}",
            touch.kind, touch.down
        );
    }
}

struct RunJob {
    ticket: String,
    cancelled: Arc<AtomicBool>,
    worker: std::thread::JoinHandle<()>,
}

impl Drop for Host {
    fn drop(&mut self) {
        for job in self.runs.get_mut().iter() {
            job.cancelled.store(true, Ordering::Release);
        }
        for job in self.runs.get_mut().drain(..) {
            let _ = job.worker.join();
        }
    }
}

fn deliver(completion: Touch) {
    if let Some(host) = CURRENT.with(|current| current.borrow().upgrade()) {
        host.send(completion);
    }
}

/// Runs a blocking adapter on a worker thread so a slow capability never stalls the window, and
/// delivers its completion on the UI thread.
fn background(ask: Ask) {
    let dispatcher = Dispatcher::ui_thread();
    std::thread::spawn(move || {
        let completion = adapters::completion(&ask);
        dispatcher.post(
            move || deliver(completion),
            // Above input: an audio worker's exchange must not queue behind pointer moves.
            DispatcherPriority::LOADED,
        );
    });
}

/// The strip of what the application reported and the host could not draw, the latest last.
fn diagnostics(messages: &[String], look: &Look) -> Element {
    let ink = look
        .colour("danger")
        .unwrap_or(Color::from_argb(0xFF, 0xC4, 0x2B, 0x1C));
    let recent = &messages[messages.len().saturating_sub(4)..];
    stack_panel()
        .key("host:diagnostics")
        .grid_row(1)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .vertical_alignment(VerticalAlignment::Bottom)
        .children(
            recent
                .iter()
                .map(|message| {
                    text_block()
                        .text(message.clone())
                        .foreground(ink)
                        .font_size(11.0)
                        .into()
                })
                .collect::<Vec<Element>>(),
        )
        .into()
}
