//! The one display Android hands a NativeActivity: geometry from its native
//! window, pixel density from the activity configuration.

use android_activity::AndroidApp;
use gpui::{DevicePixels, DisplayId, Pixels, PlatformDisplay, Size, px, size, Bounds};
use std::time::Duration;
use uuid::Uuid;

/// The activity's screen. Android's NDK exposes no multi-display API at this
/// level: a foldable or plugged-in display arrives as a new native window and
/// configuration change, which rebuilds this display.
#[derive(Debug)]
pub(crate) struct AndroidDisplay {
    id: u64,
    size: Size<DevicePixels>,
    scale_factor: f32,
}

impl AndroidDisplay {
    /// Reads the display from the activity's current window and configuration.
    pub fn current(app: &AndroidApp) -> Self {
        let size = app
            .native_window()
            .map_or(size(DevicePixels(0), DevicePixels(0)), |window| {
                size(DevicePixels(window.width()), DevicePixels(window.height()))
            });
        Self {
            id: 1,
            size,
            scale_factor: scale(app),
        }
    }

    /// The window's physical pixel size.
    pub fn physical_size(&self) -> Size<DevicePixels> {
        self.size
    }

    /// The density scale factor.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// The display's logical bounds, which on Android are also the window's bounds.
    pub fn logical_size(&self) -> Size<Pixels> {
        size(
            px(self.size.width.0 as f32 / self.scale_factor),
            px(self.size.height.0 as f32 / self.scale_factor),
        )
    }
}

/// The activity's density scale factor: DPI over the 160 DPI reference density.
pub(crate) fn scale(app: &AndroidApp) -> f32 {
    let dpi = app.config().density().unwrap_or(REFERENCE_DPI) as f32;
    (dpi / REFERENCE_DPI as f32).max(1.)
}

/// The density Android's layout system is defined against.
const REFERENCE_DPI: u32 = 160;

impl PlatformDisplay for AndroidDisplay {
    fn id(&self) -> DisplayId {
        DisplayId::new(self.id)
    }

    /// Android names no persistent display UUID; this one is stable for the
    /// window it was read from, like the iOS backend's property-derived UUID.
    fn uuid(&self) -> gpui::Result<Uuid> {
        Ok(Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!(
                "android-display-{}-{}-{}",
                self.id,
                self.size.width.0,
                self.size.height.0
            )
            .as_bytes(),
        ))
    }

    fn bounds(&self) -> Bounds<Pixels> {
        let size = self.logical_size();
        Bounds {
            origin: Default::default(),
            size,
        }
    }

    /// The NDK does not report the refresh rate; the event loop paces frames
    /// itself instead (see `platform::FRAME_INTERVAL`).
    fn refresh_interval(&self) -> Option<Duration> {
        None
    }
}
