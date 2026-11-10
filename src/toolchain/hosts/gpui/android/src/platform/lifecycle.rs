//! The activity's main-thread lifecycle commands, mapped onto the window and onto GPUI's
//! `AppLifecyclePhase` as the platform trait's table prescribes.

use android_activity::MainEvent;
use gpui::AppLifecyclePhase;

use super::AndroidPlatform;
use crate::display::AndroidDisplay;
use crate::slot::locked;
use crate::window::AndroidWindow;

impl AndroidPlatform {
    /// One main-thread lifecycle command.
    pub(super) fn main_event(&self, event: MainEvent<'_>) {
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
}
