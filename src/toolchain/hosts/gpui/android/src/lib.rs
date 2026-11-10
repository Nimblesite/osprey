//! The Android platform backend for GPUI ([CAST-MOBILE-GPUI]).
//!
//! `gpui-pre` 0.3.10 has no Android `Platform`; mobile applications supply their
//! backend to [`Application::with_platform`]. This crate is that backend, written
//! against the same `gpui-pre-*` family the desktop host already renders with:
//!
//! - rendering, atlas and text come from `gpui-pre-wgpu` (`WgpuRenderer` over the
//!   activity's `ANativeWindow`, `CosmicTextSystem` over the system fonts),
//! - touch arrives as raw [`gpui::PlatformInput::Touch`] events, which GPUI itself
//!   recognizes into taps, scrolls and pinches,
//! - the process entry is a NativeActivity (`android-activity` 0.6), so the OS owns
//!   the lifecycle: [`crate::platform::AndroidPlatform::run`] drives the ALooper
//!   event loop and maps Android lifecycle commands onto
//!   [`gpui::AppLifecyclePhase`] exactly as the platform trait's table prescribes.
//!
//! Nothing here is drawn by Java views, widgets or a WebView: the same GPUI view
//! code as desktop composes every frame.

#[cfg(target_os = "android")]
mod dispatcher;
#[cfg(target_os = "android")]
mod display;
#[cfg(target_os = "android")]
mod keyboard;
#[cfg(target_os = "android")]
mod platform;
#[cfg(target_os = "android")]
mod slot;
#[cfg(target_os = "android")]
mod window;

#[cfg(target_os = "android")]
pub use platform::{current_platform, AndroidPlatform};
