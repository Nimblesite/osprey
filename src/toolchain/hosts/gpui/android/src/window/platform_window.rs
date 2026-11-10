//! The [`PlatformWindow`] GPUI holds for the activity's window, delegating every method to the
//! shared [`AndroidWindow`] state, and the empty atlas reported before the renderer starts.

use gpui::{
    px, size, AtlasKey, AtlasTile, Capslock, DevicePixels, DispatchEventResult, Modifiers, Pixels,
    PlatformAtlas, PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformWindow, Point,
    PromptButton, PromptLevel, RequestFrameOptions, Scene, Size, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowInsets, WindowVisibility,
};
use raw_window_handle::{
    AndroidDisplayHandle, AndroidNdkWindowHandle, HasDisplayHandle, HasWindowHandle,
};
use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;

use super::{locked, AndroidWindow};

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
    fn a11y_init(&self, callbacks: gpui::A11yCallbacks) {
        if let Err(problem) = self.window.open_accessibility(callbacks) {
            log::error!("TalkBack cannot reach this window: {problem}");
        }
    }

    fn a11y_tree_update(&self, tree_update: accesskit::TreeUpdate) {
        // Android throws when an event is sent with no accessibility service left, and that
        // exception would abort the app the moment a person turned TalkBack off.
        if !self.window.accessibility_enabled().unwrap_or(false) {
            return;
        }
        if let Some(adapter) = locked(&self.window.accessibility).as_mut() {
            adapter.update_if_active(|| tree_update);
        }
    }

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

    fn on_hit_test_window_control(&self, callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
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
