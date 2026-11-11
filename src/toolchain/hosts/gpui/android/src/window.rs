//! The activity's single window: an `ANativeWindow` rendered through
//! `gpui-pre-wgpu`, with raw touch delivery and content-rect insets.

use android_activity::input::InputEvent;
use android_activity::ndk::native_window::NativeWindow;
use android_activity::{AndroidApp, AndroidAppWaker, InputStatus};
use anyhow::anyhow;
use gpui::{
    px, size, Corners, DevicePixels, DispatchEventResult, Edges, Pixels, PlatformInput,
    PlatformInputHandler, Point, RequestFrameOptions, Size, WindowAppearance,
    WindowBackgroundAppearance, WindowControlArea, WindowInsets, WindowVisibility,
};
use gpui_wgpu::{GpuContext, WgpuAtlas, WgpuRenderer, WgpuSurfaceConfig};
use raw_window_handle::{
    AndroidDisplayHandle, AndroidNdkWindowHandle, HasDisplayHandle, HasWindowHandle,
};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use crate::slot::{locked, Slot};

mod input;
mod platform_window;
pub(crate) use platform_window::AndroidPlatformWindow;

/// Every callback GPUI registers on the window after `open_window`.
#[derive(Default)]
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
    /// TalkBack's view of the window, once GPUI has opened accessibility.
    accessibility: Mutex<Option<accesskit_android::InjectingAdapter>>,
}

/// GPUI's activation callback as AccessKit asks for it.
struct Activation(Box<dyn Fn() -> Option<accesskit::TreeUpdate> + Send + 'static>);

impl accesskit::ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<accesskit::TreeUpdate> {
        (self.0)()
    }
}

/// GPUI's action callback as AccessKit asks for it.
struct Action(Box<dyn Fn(accesskit::ActionRequest) + Send + 'static>);

impl accesskit::ActionHandler for Action {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        (self.0)(request);
    }
}

/// The JNI name and signature of one method that returns an object.
type ObjectCall = (&'static str, &'static str);

/// The activity methods the accessibility bridge calls.
const SYSTEM_SERVICE: ObjectCall = ("getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;");
const WINDOW: ObjectCall = ("getWindow", "()Landroid/view/Window;");
const DECOR_VIEW: ObjectCall = ("getDecorView", "()Landroid/view/View;");

/// Calls the method `call` on `object`, handing back the object it returns.
fn call_object<'a>(
    env: &mut jni::JNIEnv<'a>,
    object: &jni::objects::JObject,
    (name, signature): ObjectCall,
    args: &[jni::objects::JValue],
) -> jni::errors::Result<jni::objects::JObject<'a>> {
    Ok(env.call_method(object, name, signature, args)?.l()?)
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
            accessibility: Mutex::new(None),
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
                size(
                    px(physical.width.0 as f32 / scale),
                    px(physical.height.0 as f32 / scale),
                ),
                scale,
            )
        });
    }

    /// Whether the surface Android holds is now another size than the one drawn to. A rotation
    /// this activity handles itself resizes the window without always raising `WindowResized`.
    pub fn size_changed(&self, app: &AndroidApp) -> bool {
        app.native_window()
            .is_some_and(|native| window_size(&native) != locked(&self.state).size)
    }

    /// Rereads the content rect Android reserves (status bar, navigation bar,
    /// keyboard) and reports inset changes: the safe-area contract GPUI expects.
    pub fn update_insets(&self, app: &AndroidApp) {
        let insets = {
            let state = locked(&self.state);
            let rect = app.content_rect();
            log::info!(
                "safe area top {} bottom {} px",
                rect.top,
                state.size.height.0 - rect.bottom
            );
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

    /// Whether any accessibility service is connected right now.
    fn accessibility_enabled(&self) -> jni::errors::Result<bool> {
        let (vm, activity) = self.jni()?;
        let mut env = vm.attach_current_thread()?;
        let service = env.new_string("accessibility")?;
        let manager = call_object(&mut env, &activity, SYSTEM_SERVICE, &[(&service).into()])?;
        env.call_method(&manager, "isEnabled", "()Z", &[])?.z()
    }

    /// Hands TalkBack the activity's decor view, which covers the same pixels as the surface,
    /// so the tree's window coordinates are its own.
    fn open_accessibility(&self, callbacks: gpui::A11yCallbacks) -> jni::errors::Result<()> {
        let (vm, activity) = self.jni()?;
        let mut env = vm.attach_current_thread()?;
        let window = call_object(&mut env, &activity, WINDOW, &[])?;
        let host = call_object(&mut env, &window, DECOR_VIEW, &[])?;
        let adapter = accesskit_android::InjectingAdapter::new(
            &mut env,
            &host,
            Activation(callbacks.activation),
            Action(callbacks.action),
        );
        *locked(&self.accessibility) = Some(adapter);
        Ok(())
    }

    /// The activity as a global JNI reference, with the VM the process runs on.
    fn jni(&self) -> jni::errors::Result<(jni::JavaVM, jni::objects::JObject<'static>)> {
        // SAFETY: both pointers stay valid for the process, and the activity is a global
        // reference this borrow never deletes.
        unsafe {
            Ok((
                jni::JavaVM::from_raw(self.app.vm_as_ptr().cast())?,
                jni::objects::JObject::from_raw(self.app.activity_as_ptr().cast()),
            ))
        }
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
}

/// The wgpu instance the shared GPU context created, for surface replacement.
fn instance(gpu: &GpuContext) -> Option<gpui_wgpu::wgpu::Instance> {
    gpu.borrow()
        .as_ref()
        .map(|context| context.instance.clone())
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
