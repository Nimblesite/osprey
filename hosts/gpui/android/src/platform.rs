//! The Android `Platform`: a NativeActivity event loop that maps Android
//! lifecycle commands onto GPUI's, and the trait methods gpui-pre 0.3.10 requires.
//!
//! The mapping follows the table in `gpui::AppLifecyclePhase` (platform.rs):
//! `onResume` → `Active`, `onPause` → `Inactive`, `onStop` → `Background`,
//! `onStart` → `Foreground`, destroy → `Disconnected`.

use android_activity::{AndroidApp, MainEvent, PollEvent};
use anyhow::anyhow;
use futures::channel::oneshot;
use gpui::{
    Action, AnyWindowHandle, AppLifecyclePhase, BackgroundExecutor, ClipboardItem, CursorStyle,
    DummyKeyboardMapper, ForegroundExecutor, Keymap, Menu, MenuItem, PathPromptOptions, Platform,
    PlatformDisplay, PlatformKeyboardLayout, PlatformKeyboardMapper, PlatformTextSystem,
    PlatformWindow, Result, Task, ThermalState, WindowAppearance, WindowParams,
    OperatingSystem, PlatformInfo, PlatformTarget,
};
use gpui_wgpu::{CosmicTextSystem, GpuContext};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::display::AndroidDisplay;
use crate::dispatcher::AndroidDispatcher;
use crate::keyboard::AndroidKeyboardLayout;
use crate::slot::Slot;
use crate::window::{AndroidPlatformWindow, AndroidWindow};

/// The poll timeout, which also paces frames until Choreographer-grade vsync:
/// a short fixed timeout keeps the looper responsive (an indefinite poll starves
/// the input queue on some devices and triggers ANRs).
const POLL_TIMEOUT: Duration = Duration::from_millis(16);

/// The soonest another frame is drawn; the loop draws at most this often.
const FRAME_INTERVAL: Duration = Duration::from_millis(15);

/// The platform text system: cosmic-text over Android's system fonts, which
/// its font database does not discover on its own (fontdb scans no Android
/// font directory), so every readable font under `/system/fonts` is loaded
/// explicitly before the first layout asks for one.
fn android_text_system() -> Arc<CosmicTextSystem> {
    // `.SystemUIFont` resolves to this family (gpui's font_name_with_fallbacks),
    // and Roboto is the UI font every Android ships.
    let system = Arc::new(CosmicTextSystem::new("Roboto"));
    let mut fonts = Vec::new();
    for entry in std::fs::read_dir("/system/fonts").into_iter().flatten().flatten() {
        let path = entry.path();
        let loadable = matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("ttf") | Some("otf") | Some("ttc")
        );
        if loadable {
            match std::fs::read(&path) {
                Ok(bytes) => fonts.push(std::borrow::Cow::Owned(bytes)),
                Err(problem) => log::warn!("skipping font {}: {problem}", path.display()),
            }
        }
    }
    if let Err(problem) = system.add_fonts(fonts) {
        log::warn!("Android system fonts failed to load: {problem:#}");
    }
    system
}

/// The Android platform backend: install with
/// `Application::with_platform(android::current_platform(&app))`.
pub struct AndroidPlatform(Rc<State>);

struct State {
    app: AndroidApp,
    dispatcher: Arc<AndroidDispatcher>,
    gpu: GpuContext,
    text: Arc<CosmicTextSystem>,
    window: Mutex<Option<Arc<AndroidWindow>>>,
    finish_launching: Slot<dyn FnOnce()>,
    open_urls: Slot<dyn FnMut(Vec<String>)>,
    quit: Slot<dyn FnMut() -> bool>,
    lifecycle: Slot<dyn FnMut(AppLifecyclePhase)>,
    memory_warning: Slot<dyn FnMut()>,
    keyboard_layout_changed: Slot<dyn FnMut()>,
    displays_changed: Slot<dyn FnMut()>,
    running: Cell<bool>,
    stop: Cell<bool>,
    last_frame: Cell<Option<Instant>>,
}

/// The platform for this activity. Call from `android_main`'s thread.
pub fn current_platform(app: &AndroidApp) -> Rc<dyn Platform> {
    Rc::new(AndroidPlatform(Rc::new(State {
        app: app.clone(),
        dispatcher: AndroidDispatcher::new(app),
        gpu: Default::default(),
        // Roboto is the font Android's layout is designed around.
        text: android_text_system(),
        window: Mutex::new(None),
        finish_launching: Slot::default(),
        open_urls: Slot::default(),
        quit: Slot::default(),
        lifecycle: Slot::default(),
        memory_warning: Slot::default(),
        keyboard_layout_changed: Slot::default(),
        displays_changed: Slot::default(),
        running: Cell::new(false),
        stop: Cell::new(false),
        last_frame: Cell::new(None),
    })))
}

impl AndroidPlatform {
    /// The activity's window, created at `InitWindow`.
    fn window(&self) -> Option<Arc<AndroidWindow>> {
        locked(&self.0.window).clone()
    }

    /// The display as currently configured.
    fn display(&self) -> Rc<dyn PlatformDisplay> {
        Rc::new(AndroidDisplay::current(&self.0.app))
    }

    /// The appearance Android's night mode selects.
    fn appearance(&self) -> WindowAppearance {
        use android_activity::ndk::configuration::UiModeNight;
        match self.0.app.config().ui_mode_night() {
            UiModeNight::Yes => WindowAppearance::Dark,
            _ => WindowAppearance::Light,
        }
    }

    /// One event-loop iteration: wait for events or the timeout, run main-thread
    /// tasks, then draw when the frame interval allows.
    fn tick(&self) -> bool {
        let app = &self.0.app;
        app.poll_events(Some(POLL_TIMEOUT), |event| match event {
            PollEvent::Main(event) => self.main_event(event),
            PollEvent::Wake | PollEvent::Timeout => {}
            _ => {}
        });
        self.0.dispatcher.run_until_idle();
        self.frame();
        !self.0.stop.get()
    }

    /// One main-thread lifecycle command.
    fn main_event(&self, event: MainEvent<'_>) {
        match event {
            MainEvent::InitWindow { .. } => self.init_window(),
            MainEvent::TerminateWindow { .. } => self.terminate_window(),
            MainEvent::WindowResized { .. } => {
                if let Some(window) = self.window() {
                    window.resized(&self.0.app);
                    window.update_insets(&self.0.app);
                }
            }
            MainEvent::RedrawNeeded { .. } => {
                // Android expects the redraw within this callback; GPUI's frame
                // paints synchronously through the request-frame callback.
                if let Some(window) = self.window() {
                    window.request_frame();
                }
            }
            MainEvent::ContentRectChanged { .. } | MainEvent::InsetsChanged { .. } => {
                if let Some(window) = self.window() {
                    window.update_insets(&self.0.app);
                }
            }
            MainEvent::GainedFocus => {
                if let Some(window) = self.window() {
                    window.set_active(true);
                }
            }
            MainEvent::LostFocus => {
                if let Some(window) = self.window() {
                    window.set_active(false);
                }
            }
            MainEvent::ConfigChanged { .. } => {
                if let Some(window) = self.window() {
                    window.set_appearance(self.appearance());
                }
            }
            MainEvent::LowMemory => {
                self.0.memory_warning.with(|warn| warn());
            }
            MainEvent::Start => self.phase(AppLifecyclePhase::Foreground),
            MainEvent::Resume { .. } => self.phase(AppLifecyclePhase::Active),
            MainEvent::Pause => self.phase(AppLifecyclePhase::Inactive),
            MainEvent::Stop => self.phase(AppLifecyclePhase::Background),
            MainEvent::Destroy => self.phase(AppLifecyclePhase::Disconnected),
            MainEvent::SaveState { .. } => {}
            MainEvent::InputAvailable => self.input(),
            _ => {}
        }
    }

    /// Creates the activity window and launches GPUI once the surface exists.
    fn init_window(&self) {
        let display = AndroidDisplay::current(&self.0.app);
        {
            let mut window = locked(&self.0.window);
            if window.is_none() {
                *window = Some(AndroidWindow::new(
                    &self.0.app,
                    display.physical_size(),
                    display.scale_factor(),
                ));
            }
        }
        if let Some(window) = self.window() {
            window.init(&self.0.app, &self.0.gpu);
        }
        // GPUI opens its window inside this callback, which finds the native
        // window the loop just created.
        if let Some(launch) = self.0.finish_launching.take() {
            launch();
        }
    }

    /// Releases the surface Android is destroying.
    fn terminate_window(&self) {
        if let Some(window) = self.window() {
            window.terminate();
        }
    }

    /// Reads and delivers queued input events.
    fn input(&self) {
        let Ok(events) = self.0.app.input_events_iter() else {
            return;
        };
        let mut events = events;
        while events.next(|event| match self.window() {
            Some(window) => window.handle_input(&self.0.app, event),
            None => android_activity::InputStatus::Unhandled,
        }) {}
    }

    /// Reports one lifecycle phase to GPUI.
    fn phase(&self, phase: AppLifecyclePhase) {
        if matches!(phase, AppLifecyclePhase::Disconnected) {
            self.0.stop.set(true);
        }
        self.0.lifecycle.with(|report| report(phase));
    }

    /// Draws when at least one frame interval has passed.
    fn frame(&self) {
        let Some(window) = self.window() else {
            return;
        };
        if !window.is_presenting() {
            return;
        }
        let now = Instant::now();
        if self.0.last_frame.get().is_some_and(|last| now - last < FRAME_INTERVAL) {
            return;
        }
        self.0.last_frame.set(Some(now));
        window.request_frame();
    }
}

impl Platform for AndroidPlatform {
    fn platform_info(&self) -> PlatformInfo {
        PlatformInfo {
            os: OperatingSystem::Android,
            target: PlatformTarget::Native,
        }
    }

    fn background_executor(&self) -> BackgroundExecutor {
        BackgroundExecutor::new(self.0.dispatcher.clone())
    }

    fn foreground_executor(&self) -> ForegroundExecutor {
        ForegroundExecutor::new(self.0.dispatcher.clone())
    }

    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        self.0.text.clone()
    }

    fn run(&self, on_finish_launching: Box<dyn FnOnce()>) {
        if self.0.running.replace(true) {
            log::warn!("the Android application is already running");
            return;
        }
        self.0.finish_launching.set(on_finish_launching);
        while self.tick() {}
        self.0.running.set(false);
    }

    fn quit(&self) {
        // Android owns the process; the loop stops after this activity's destroy.
        self.0.stop.set(true);
        let vetoed = self.0.quit.with(|quit| quit()).unwrap_or(false);
        if vetoed {
            log::info!("quit vetoed, but Android terminates the activity regardless");
        }
        self.0.app.create_waker().wake();
    }

    fn restart(&self, _binary_path: Option<PathBuf>, _arguments: Vec<std::ffi::OsString>) {
        log::warn!("Android apps cannot restart themselves");
    }

    fn activate(&self, _ignoring_other_apps: bool) {}

    fn hide(&self) {}

    fn hide_other_apps(&self) {}

    fn unhide_other_apps(&self) {}

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        vec![self.display()]
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(self.display())
    }

    fn on_displays_changed(&self, callback: Box<dyn FnMut()>) {
        // A fold or display plug-in arrives as ConfigChanged plus a new native
        // window, which rereads the display; this callback is kept for parity.
        self.0.displays_changed.set(callback);
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        None
    }

    fn open_window(
        &self,
        _handle: AnyWindowHandle,
        _options: WindowParams,
    ) -> anyhow::Result<Box<dyn PlatformWindow>> {
        let window = self
            .window()
            .ok_or_else(|| anyhow!("no Android native window; open windows after InitWindow"))?;
        Ok(Box::new(AndroidPlatformWindow::new(window, Some(self.display()))))
    }

    fn window_appearance(&self) -> WindowAppearance {
        self.appearance()
    }

    fn open_url(&self, url: &str) {
        // Launching an Intent needs the JVM; the Java-main-thread bridge is not
        // wired yet, so the request is visible in the log instead of lost.
        log::warn!("Android open_url({url}) needs an Intent bridge; not wired");
    }

    fn on_open_urls(&self, callback: Box<dyn FnMut(Vec<String>)>) {
        // NativeActivity delivers deep links through Java; not wired yet.
        self.0.open_urls.set(callback);
    }

    fn register_url_scheme(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    fn prompt_for_paths(
        &self,
        _options: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        unimplemented_prompt("the Android file picker")
    }

    fn prompt_for_new_path(
        &self,
        _directory: &Path,
        _suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        unimplemented_prompt("the Android save dialog")
    }

    fn can_select_mixed_files_and_dirs(&self) -> bool {
        false
    }

    fn reveal_path(&self, _path: &Path) {}

    fn open_with_system(&self, _path: &Path) {}

    fn on_quit(&self, callback: Box<dyn FnMut() -> bool>) {
        self.0.quit.set(callback);
    }

    fn on_reopen(&self, _callback: Box<dyn FnMut()>) {}

    fn on_system_sleep(&self, _callback: Box<dyn FnMut()>) {}

    fn on_system_wake(&self, _callback: Box<dyn FnMut()>) {}

    fn on_app_lifecycle(&self, callback: Box<dyn FnMut(AppLifecyclePhase)>) {
        self.0.lifecycle.set(callback);
    }

    fn on_memory_warning(&self, callback: Box<dyn FnMut()>) {
        self.0.memory_warning.set(callback);
    }

    fn set_menus(&self, _menus: Vec<Menu>, _keymap: &Keymap) {}

    fn set_dock_menu(&self, _menu: Vec<MenuItem>, _keymap: &Keymap) {}

    fn on_app_menu_action(&self, _callback: Box<dyn FnMut(&dyn Action)>) {}

    fn on_will_open_app_menu(&self, _callback: Box<dyn FnMut()>) {}

    fn on_validate_app_menu_command(&self, _callback: Box<dyn FnMut(&dyn Action) -> bool>) {}

    fn thermal_state(&self) -> ThermalState {
        // PowerManager's thermal listener needs the JVM; not wired yet.
        ThermalState::Nominal
    }

    fn on_thermal_state_change(&self, _callback: Box<dyn FnMut()>) {
        // Delivered by PowerManager once the JNI bridge exists.
    }

    fn prevent_idle_sleep(&self, _reason: &str) -> Task<Result<gpui::ActivityGuard>> {
        Task::ready(Err(anyhow!(
            "idle sleep prevention is not implemented for Android"
        )))
    }

    fn app_path(&self) -> Result<PathBuf> {
        self.0
            .app
            .internal_data_path()
            .ok_or_else(|| anyhow!("the Android app data path is unavailable"))
    }

    fn path_for_auxiliary_executable(&self, _name: &str) -> Result<PathBuf> {
        Err(anyhow!("auxiliary executables are not supported on Android"))
    }

    fn set_cursor_style(&self, _style: CursorStyle) {}

    fn hide_cursor_until_mouse_moves(&self) {}

    fn is_cursor_visible(&self) -> bool {
        false
    }

    fn should_auto_hide_scrollbars(&self) -> bool {
        true
    }

    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        // ClipboardManager needs the JVM; absent until the bridge exists, and
        // reporting nothing is honest where a fake string would not be.
        None
    }

    fn write_to_clipboard(&self, _item: ClipboardItem) {
        log::warn!("Android clipboard writes need a JNI bridge; not wired");
    }

    fn write_credentials(&self, _url: &str, _username: &str, _password: &[u8]) -> Task<Result<()>> {
        Task::ready(Err(anyhow!(
            "the Android Keystore credential store is not implemented"
        )))
    }

    fn read_credentials(&self, _url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        Task::ready(Err(anyhow!(
            "the Android Keystore credential store is not implemented"
        )))
    }

    fn delete_credentials(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Err(anyhow!(
            "the Android Keystore credential store is not implemented"
        )))
    }

    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(AndroidKeyboardLayout)
    }

    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        Rc::new(DummyKeyboardMapper)
    }

    fn on_keyboard_layout_change(&self, callback: Box<dyn FnMut()>) {
        self.0.keyboard_layout_changed.set(callback);
    }
}

/// A ready receiver for a prompt this backend does not implement.
fn unimplemented_prompt<T>(what: &str) -> oneshot::Receiver<Result<T>> {
    let (sender, receiver) = oneshot::channel();
    let _ = sender.send(Err(anyhow!("{what} is not implemented for Android")));
    receiver
}

/// Locks without panicking on a poisoned mutex.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
