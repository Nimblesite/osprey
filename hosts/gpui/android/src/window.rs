//! The activity's single window: an `ANativeWindow` rendered through
//! `gpui-pre-wgpu`, with raw touch delivery and content-rect insets.

use android_activity::input::{Axis, InputEvent, KeyAction, KeyEvent, MotionAction, MotionEvent};
use android_activity::ndk::native_window::NativeWindow;
use android_activity::{AndroidApp, AndroidAppWaker, InputStatus};
use gpui::{
    AtlasKey, AtlasTile, Capslock, Corners, DevicePixels, DispatchEventResult, Edges, Modifiers,
    PlatformAtlas, PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformWindow, Point,
    PromptButton, PromptLevel, Pixels, RequestFrameOptions, Scene, Size, TouchEvent, TouchId,
    size,
    TouchPhase, WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowControlArea,
    WindowInsets, WindowVisibility, point, px,
};
use gpui_wgpu::{GpuContext, WgpuAtlas, WgpuRenderer, WgpuSurfaceConfig};
use raw_window_handle::{
    AndroidDisplayHandle, AndroidNdkWindowHandle, HasDisplayHandle, HasWindowHandle,
};
use std::borrow::Cow;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use anyhow::anyhow;
use crate::keyboard;
use crate::slot::Slot;

/// Every callback GPUI registers on the window after `open_window`.
pub(crate) struct Callbacks {
    pub request_frame: Slot<dyn FnMut(RequestFrameOptions)>,
    pub input: Slot<dyn FnMut(PlatformInput) -> DispatchEventResult>,
    pub active_status_change: Slot<dyn FnMut(bool)>,
    pub visibility_change: Slot<dyn FnMut(WindowVisibility)>,
    pub hover_status_change: Slot<dyn FnMut(bool)>,
    pub resize: Slot<dyn FnMut(Size<Pixels>, f32)>,
    pub moved: Slot<dyn FnMut()>,
    pub display_changed: Slot<dyn FnMut()>,
    pub should_close: Slot<dyn FnMut() -> bool>,
    pub hit_test: Slot<dyn FnMut() -> Option<WindowControlArea>>,
    pub close: Slot<dyn FnOnce()>,
    pub appearance_changed: Slot<dyn FnMut()>,
    pub insets_changed: Slot<dyn FnMut(WindowInsets)>,
}

impl Default for Callbacks {
    fn default() -> Self {
        Self {
            request_frame: Slot::default(),
            input: Slot::default(),
            active_status_change: Slot::default(),
            visibility_change: Slot::default(),
            hover_status_change: Slot::default(),
            resize: Slot::default(),
            moved: Slot::default(),
            display_changed: Slot::default(),
            should_close: Slot::default(),
            hit_test: Slot::default(),
            close: Slot::default(),
            appearance_changed: Slot::default(),
            insets_changed: Slot::default(),
        }
    }
}

/// The window's mutable state, shared between the event loop and the boxed
/// [`PlatformWindow`] GPUI holds.
pub(crate) struct WindowState {
    native: Option<NativeWindow>,
    renderer: Option<WgpuRenderer>,
    atlas: Option<Arc<WgpuAtlas>>,
    size: Size<DevicePixels>,
    scale_factor: f32,
    insets: WindowInsets,
    appearance: WindowAppearance,
    background: WindowBackgroundAppearance,
    active: bool,
    visibility: WindowVisibility,
    input_handler: Option<PlatformInputHandler>,
    mouse: Point<Pixels>,
}

/// The activity's window: one per process under NativeActivity.
pub(crate) struct AndroidWindow {
    pub state: Mutex<WindowState>,
    pub callbacks: Callbacks,
    app: AndroidApp,
    waker: AndroidAppWaker,
}

impl AndroidWindow {
    /// Creates the window around the activity's future native surface.
    pub fn new(app: &AndroidApp, size: Size<DevicePixels>, scale_factor: f32) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(WindowState {
                native: None,
                renderer: None,
                atlas: None,
                size,
                scale_factor,
                insets: WindowInsets::default(),
                appearance: WindowAppearance::Light,
                background: WindowBackgroundAppearance::Opaque,
                active: false,
                visibility: WindowVisibility::Hidden,
                input_handler: None,
                mouse: Point::default(),
            }),
            callbacks: Callbacks::default(),
            app: app.clone(),
            waker: app.create_waker(),
        })
    }

    /// Binds the native window Android just created. The first window builds the
    /// renderer; later ones resume it on the new surface, keeping the device,
    /// pipelines and atlas gpui-pre-wgpu holds across `TerminateWindow`.
    pub fn init(&self, app: &AndroidApp, gpu: &GpuContext) {
        let Some(native) = app.native_window() else {
            log::error!("InitWindow delivered no native window; this frame is lost");
            return;
        };
        let size = window_size(&native);
        let source = SurfaceSource {
            window: native.ptr().cast(),
        };
        let mut state = locked(&self.state);
        state.size = size;
        state.native = Some(native);
        let config = surface_config(size, state.background != WindowBackgroundAppearance::Opaque);
        let outcome = match state.renderer.as_mut() {
            Some(renderer) => match instance(gpu) {
                Some(instance) => renderer.replace_surface(&source, config, &instance),
                None => Err(anyhow!("the wgpu instance is unavailable")),
            },
            None => WgpuRenderer::new(gpu.clone(), &source, config, None).map(|renderer| {
                state.atlas = Some(renderer.sprite_atlas().clone());
                state.renderer = Some(renderer);
            }),
        };
        if let Err(problem) = outcome {
            log::error!("Android wgpu surface failed to start: {problem:#}");
        }
        drop(state);
        self.set_visibility(WindowVisibility::Visible);
    }

    /// Releases the surface Android is about to destroy. The renderer survives
    /// unconfigured so the next `InitWindow` can resume it without recompiling.
    pub fn terminate(&self) {
        self.set_visibility(WindowVisibility::Hidden);
        let mut state = locked(&self.state);
        if let Some(renderer) = state.renderer.as_mut() {
            renderer.unconfigure_surface();
        }
        state.native = None;
    }

    /// Follows a native-window resize: new surface size, then GPUI's callback.
    pub fn resized(&self, app: &AndroidApp) {
        let Some(native) = app.native_window() else {
            return;
        };
        let (physical, scale) = {
            let mut state = locked(&self.state);
            state.size = window_size(&native);
            let next = state.size;
            if let Some(renderer) = state.renderer.as_mut() {
                renderer.update_drawable_size(next);
            }
            (state.size, state.scale_factor)
        };
        self.callbacks.resize.with(|resize| {
            resize(
                size(px(physical.width.0 as f32 / scale), px(physical.height.0 as f32 / scale)),
                scale,
            )
        });
    }

    /// Rereads the content rect Android reserves (status bar, navigation bar,
    /// keyboard) and reports inset changes: the safe-area contract GPUI expects.
    pub fn update_insets(&self, app: &AndroidApp) {
        let insets = {
            let state = locked(&self.state);
            let rect = app.content_rect();
            WindowInsets {
                safe_area: Edges {
                    top: px(rect.top as f32 / state.scale_factor),
                    right: px((state.size.width.0 - rect.right) as f32 / state.scale_factor),
                    bottom: px((state.size.height.0 - rect.bottom) as f32 / state.scale_factor),
                    left: px(rect.left as f32 / state.scale_factor),
                },
                ime: Edges::default(),
                corner_radii: Corners::default(),
            }
        };
        let changed = {
            let mut state = locked(&self.state);
            (state.insets != insets).then(|| state.insets = insets.clone())
        };
        if changed.is_some() {
            self.callbacks.insets_changed.with(|report| report(insets));
        }
    }

    /// Reports focus changes to GPUI as window activity.
    pub fn set_active(&self, active: bool) {
        locked(&self.state).active = active;
        self.callbacks
            .active_status_change
            .with(|report| report(active));
    }

    /// Records and reports a visibility change.
    fn set_visibility(&self, visibility: WindowVisibility) {
        let changed = {
            let mut state = locked(&self.state);
            let changed = state.visibility != visibility;
            state.visibility = visibility;
            changed
        };
        if changed {
            self.callbacks
                .visibility_change
                .with(|report| report(visibility));
        }
    }

    /// Reports night mode as the window's appearance.
    pub fn set_appearance(&self, appearance: WindowAppearance) {
        if locked(&self.state).appearance == appearance {
            return;
        }
        locked(&self.state).appearance = appearance;
        self.callbacks.appearance_changed.with(|report| report());
    }

    /// Whether the window is presenting: surfaced, active and visible.
    pub fn is_presenting(&self) -> bool {
        let state = locked(&self.state);
        state.renderer.is_some() && state.active && state.visibility == WindowVisibility::Visible
    }

    /// Draws one frame by running GPUI's request-frame callback, which paints
    /// synchronously and calls [`PlatformWindow::draw`] with the finished scene.
    pub fn request_frame(&self) {
        if locked(&self.state).renderer.is_none() {
            return;
        }
        self.callbacks
            .request_frame
            .with(|request| request(RequestFrameOptions::default()));
    }

    /// Wakes the event loop so a demanded frame is drawn without waiting out the
    /// poll timeout.
    pub fn wake(&self) {
        self.waker.wake();
    }

    /// Delivers one input event; the return value tells Android whether the
    /// event was consumed or should keep its default system interpretation.
    pub fn handle_input(&self, app: &AndroidApp, event: &InputEvent) -> InputStatus {
        match event {
            InputEvent::MotionEvent(motion) => self.motion(motion),
            InputEvent::KeyEvent(key) => self.key(app, key),
            _ => InputStatus::Unhandled,
        }
    }

    /// Turns one motion event into raw GPUI touch events. GPUI itself recognizes
    /// taps, pans, pinches and long presses from these.
    fn motion(&self, event: &MotionEvent) -> InputStatus {
        let Some(phase) = touch_phase(event.action()) else {
            return InputStatus::Handled;
        };
        for pointer in touched_pointers(event) {
            let (position, force) = {
                let mut state = locked(&self.state);
                let position = point(
                    px(pointer.x / state.scale_factor),
                    px(pointer.y / state.scale_factor),
                );
                state.mouse = position;
                (position, (pointer.pressure > 0.).then_some(pointer.pressure))
            };
            let touch = TouchEvent {
                id: TouchId(pointer.id as u64),
                phase,
                position,
                predicted_position: None,
                force,
            };
            self.callbacks
                .input
                .with(|input| input(PlatformInput::Touch(touch)));
        }
        InputStatus::Handled
    }

    /// Turns one key event into a GPUI key press, resolving the printable
    /// character through the device's key character map.
    fn key(&self, app: &AndroidApp, event: &KeyEvent) -> InputStatus {
        let unicode = unicode_for(app, event);
        let input = match event.action() {
            KeyAction::Down => keyboard::key_down(event, unicode).map(PlatformInput::KeyDown),
            KeyAction::Up => keyboard::key_up(event, unicode).map(PlatformInput::KeyUp),
            _ => None,
        };
        match input {
            Some(pressed) => {
                self.callbacks.input.with(|input| input(pressed));
                InputStatus::Handled
            }
            None => InputStatus::Unhandled,
        }
    }
}

/// The GPUI touch phase an Android motion action describes; `None` for hover and
/// scroll-wheel actions GPUI has no raw equivalent for.
fn touch_phase(action: MotionAction) -> Option<TouchPhase> {
    match action {
        MotionAction::Down | MotionAction::PointerDown => Some(TouchPhase::Started),
        MotionAction::Move => Some(TouchPhase::Moved),
        MotionAction::Up | MotionAction::PointerUp => Some(TouchPhase::Ended),
        MotionAction::Cancel => Some(TouchPhase::Cancelled),
        _ => None,
    }
}

/// One touch pointer's delivered state.
struct Touch {
    id: i32,
    x: f32,
    y: f32,
    pressure: f32,
}

/// The pointers one motion event describes: only the pointer that changed for
/// press and release actions, every pointer for moves and cancels.
fn touched_pointers(event: &MotionEvent) -> Vec<Touch> {
    fn read(pointer: android_activity::input::Pointer<'_>) -> Touch {
        Touch {
            id: pointer.pointer_id(),
            x: pointer.x(),
            y: pointer.y(),
            pressure: pointer.axis_value(Axis::Pressure),
        }
    }
    match event.action() {
        MotionAction::Down
        | MotionAction::Up
        | MotionAction::PointerDown
        | MotionAction::PointerUp => vec![read(event.pointer_at_index(event.pointer_index()))],
        _ => {
            let mut pointers = Vec::new();
            for pointer in event.pointers() {
                pointers.push(read(pointer));
            }
            pointers
        }
    }
}

/// The printable character for a key event, from the device's character map.
fn unicode_for(app: &AndroidApp, event: &KeyEvent) -> Option<char> {
    let map = app.device_key_character_map(event.device_id()).ok()?;
    match map.get(event.key_code(), event.meta_state()) {
        Ok(android_activity::input::KeyMapChar::Unicode(character)) => Some(character),
        _ => None,
    }
}

/// The wgpu instance the shared GPU context created, for surface replacement.
fn instance(gpu: &GpuContext) -> Option<gpui_wgpu::wgpu::Instance> {
    gpu.borrow().as_ref().map(|context| context.instance.clone())
}

/// The renderer's surface configuration for one size.
fn surface_config(size: Size<DevicePixels>, transparent: bool) -> WgpuSurfaceConfig {
    WgpuSurfaceConfig {
        size,
        transparent,
        // `None` selects Fifo (VSync), which the field documents as the default.
        preferred_present_mode: None,
    }
}

/// The native window's size in device pixels.
fn window_size(native: &NativeWindow) -> Size<DevicePixels> {
    Size {
        width: DevicePixels(native.width()),
        height: DevicePixels(native.height()),
    }
}

/// The raw-window-handle wrapper `WgpuRenderer::new` and `replace_surface` need.
#[derive(Debug, Clone)]
struct SurfaceSource {
    window: NonNull<c_void>,
}

// Safety: the pointer is only borrowed to create a wgpu surface while the
// owning `NativeWindow` (held by `WindowState`) is alive; `terminate` releases
// the renderer's surface before the window is dropped.
unsafe impl Send for SurfaceSource {}
unsafe impl Sync for SurfaceSource {}

impl HasWindowHandle for SurfaceSource {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let handle = AndroidNdkWindowHandle::new(self.window);
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(handle.into()) })
    }
}

impl HasDisplayHandle for SurfaceSource {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let handle = AndroidDisplayHandle::new();
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(handle.into()) })
    }
}

/// The [`PlatformWindow`] GPUI holds: every method delegated to the shared state.
pub(crate) struct AndroidPlatformWindow {
    window: Arc<AndroidWindow>,
    display: Option<Rc<dyn PlatformDisplay>>,
}

impl AndroidPlatformWindow {
    /// Wraps the window the event loop created at `MainEvent::InitWindow`.
    pub fn new(window: Arc<AndroidWindow>, display: Option<Rc<dyn PlatformDisplay>>) -> Self {
        Self { window, display }
    }
}

impl HasWindowHandle for AndroidPlatformWindow {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let native = locked(&self.window.state).native.clone();
        let Some(native) = native else {
            return Err(raw_window_handle::HandleError::Unavailable);
        };
        let handle = AndroidNdkWindowHandle::new(native.ptr().cast());
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(handle.into()) })
    }
}

impl HasDisplayHandle for AndroidPlatformWindow {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let handle = AndroidDisplayHandle::new();
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(handle.into()) })
    }
}

impl PlatformWindow for AndroidPlatformWindow {
    fn bounds(&self) -> gpui::Bounds<Pixels> {
        let state = locked(&self.window.state);
        gpui::Bounds {
            origin: Default::default(),
            size: size(
                px(state.size.width.0 as f32 / state.scale_factor),
                px(state.size.height.0 as f32 / state.scale_factor),
            ),
        }
    }

    fn is_maximized(&self) -> bool {
        false
    }

    fn window_bounds(&self) -> WindowBounds {
        WindowBounds::Fullscreen(self.bounds())
    }

    fn content_size(&self) -> Size<Pixels> {
        self.bounds().size
    }

    fn resize(&mut self, _size: Size<Pixels>) {
        // Android owns the window size; changes arrive through `resized` instead.
    }

    fn scale_factor(&self) -> f32 {
        locked(&self.window.state).scale_factor
    }

    fn appearance(&self) -> WindowAppearance {
        locked(&self.window.state).appearance
    }

    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        self.display.clone()
    }

    fn mouse_position(&self) -> Point<Pixels> {
        locked(&self.window.state).mouse
    }

    fn modifiers(&self) -> Modifiers {
        Modifiers::default()
    }

    fn capslock(&self) -> Capslock {
        Capslock::default()
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        locked(&self.window.state).input_handler = Some(input_handler);
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        locked(&self.window.state).input_handler.take()
    }

    fn prompt(
        &self,
        _level: PromptLevel,
        _msg: &str,
        _detail: Option<&str>,
        _answers: &[PromptButton],
    ) -> Option<futures::channel::oneshot::Receiver<usize>> {
        None
    }

    fn activate(&self) {}

    fn is_active(&self) -> bool {
        locked(&self.window.state).active
    }

    fn visibility(&self) -> WindowVisibility {
        locked(&self.window.state).visibility
    }

    fn is_hovered(&self) -> bool {
        false
    }

    fn background_appearance(&self) -> WindowBackgroundAppearance {
        locked(&self.window.state).background
    }

    fn set_title(&mut self, _title: &str) {}

    fn set_background_appearance(&self, background: WindowBackgroundAppearance) {
        let mut state = locked(&self.window.state);
        state.background = background;
        let transparent = background != WindowBackgroundAppearance::Opaque;
        if let Some(renderer) = state.renderer.as_mut() {
            renderer.update_transparency(transparent);
        }
    }

    fn minimize(&self) {}

    fn zoom(&self) {}

    fn toggle_fullscreen(&self) {}

    fn is_fullscreen(&self) -> bool {
        true
    }

    fn update_ime_position(&self, _bounds: gpui::Bounds<Pixels>) {
        // The Android IME follows the focused view, not a platform-reported rect.
    }

    fn frame_waker(&self) -> Option<Rc<dyn Fn()>> {
        let window = Arc::downgrade(&self.window);
        Some(Rc::new(move || {
            if let Some(window) = window.upgrade() {
                window.wake();
            }
        }))
    }

    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        self.window.callbacks.request_frame.set(callback);
    }

    fn on_input(&self, callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>) {
        self.window.callbacks.input.set(callback);
    }

    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.window.callbacks.active_status_change.set(callback);
    }

    fn on_visibility_change(&self, callback: Box<dyn FnMut(WindowVisibility)>) {
        self.window.callbacks.visibility_change.set(callback);
    }

    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.window.callbacks.hover_status_change.set(callback);
    }

    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        self.window.callbacks.resize.set(callback);
    }

    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        self.window.callbacks.moved.set(callback);
    }

    fn on_display_changed(&self, callback: Box<dyn FnMut()>) {
        self.window.callbacks.display_changed.set(callback);
    }

    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        self.window.callbacks.should_close.set(callback);
    }

    fn on_hit_test_window_control(
        &self,
        callback: Box<dyn FnMut() -> Option<WindowControlArea>>,
    ) {
        self.window.callbacks.hit_test.set(callback);
    }

    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        self.window.callbacks.close.set(callback);
    }

    fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        self.window.callbacks.appearance_changed.set(callback);
    }

    fn draw(&self, scene: &Scene) {
        let mut state = locked(&self.window.state);
        if let Some(renderer) = state.renderer.as_mut() {
            renderer.draw(scene);
        }
    }

    fn schedule_frame(&self) {
        self.window.wake();
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        match locked(&self.window.state).atlas.clone() {
            Some(atlas) => atlas,
            None => Arc::new(Atlasless),
        }
    }

    fn is_subpixel_rendering_supported(&self) -> bool {
        false
    }

    fn gpu_specs(&self) -> Option<gpui::GpuSpecs> {
        let state = locked(&self.window.state);
        state.renderer.as_ref()?.gpu_specs()
    }

    fn insets(&self) -> WindowInsets {
        locked(&self.window.state).insets.clone()
    }

    fn on_insets_changed(&self, callback: Box<dyn FnMut(WindowInsets)>) {
        self.window.callbacks.insets_changed.set(callback);
    }

    fn show_soft_keyboard(&self) {
        self.window.app.show_soft_input(false);
    }

    fn hide_soft_keyboard(&self) {
        self.window.app.hide_soft_input(false);
    }
}

/// The atlas reported before the renderer has ever started: it can hold nothing,
/// which is exactly the state of a window that has never drawn.
struct Atlasless;

impl PlatformAtlas for Atlasless {
    fn get_or_insert_with<'a>(
        &self,
        _key: AtlasKey,
        _build: &mut dyn FnMut() -> gpui::Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> gpui::Result<Option<AtlasTile>> {
        Ok(None)
    }

    fn remove(&self, _key: &AtlasKey) {}
}

/// Locks without panicking on a poisoned mutex.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
