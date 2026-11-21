//! An Osprey Native host on FerroUI over the shared-memory boundary (docs/osprey-prism/README.md).
//!
//! An Osprey application compiled as a C library (`osprey <app> --compile --target=macos`)
//! exports `argument`, `open` and `event`. Each call's frame (view rows, scene rows, asks and
//! their text) is written once by the Osprey runtime into a shared bridge; when the call returns
//! the host seals it, leases it and reads the same bytes in place. Nothing is encoded or parsed
//! in either direction ([NATIVE-SHM-BOUNDARY]). The boundary modules are ported from Osprey's
//! GPUI host (MIT) and are toolkit-free:
//!
//! - [`ffi`], [`frame`], [`rows`]: the C calls, the leased frame and its whole-frame check;
//! - [`tree`], [`regions`]: the told component trees by region;
//! - [`events`]: the typed host events; [`adapters`], [`store`]: the capabilities answered;
//!
//! and the FerroUI side is:
//!
//! - [`view`]: one mapping per told node kind to FerroUI controls, in one tree [`host`] keeps;
//! - [`surface`]: a scene's shapes drawn with FerroUI's drawing context, its input sent back.

pub mod adapters;
pub mod events;
pub mod ffi;
mod frame;
pub mod host;
pub mod regions;
// Both desktop renderers consume the same checked native scene protocol.
#[path = "../gpui/src/rows.rs"]
pub mod rows;
pub mod scene;
pub mod store;
pub mod surface;
pub mod symbols;
pub mod theme;
pub mod tree;
pub mod view;

#[cfg(feature = "desktop")]
pub mod desktop;
#[cfg(feature = "desktop")]
pub mod bench;

/// The person's home folder: `HOME`, or `USERPROFILE` on Windows.
pub fn home() -> Result<String, String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|problem| format!("neither HOME nor USERPROFILE is set: {problem}"))
}

/// Unix seconds for an event or the launch context.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
}

/// The local UTC offset in seconds, 0 when the system cannot tell.
#[cfg(unix)]
pub fn local_offset() -> i64 {
    let timestamp: libc::time_t = now();
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: both pointers are live, aligned and owned for the call.
    if unsafe { libc::localtime_r(&timestamp, local.as_mut_ptr()) }.is_null() {
        return 0;
    }
    // SAFETY: localtime_r initialised the structure when it returned non-null.
    unsafe { local.assume_init() }.tm_gmtoff
}

/// The local UTC offset; Windows is not a target of this port yet.
#[cfg(not(unix))]
pub fn local_offset() -> i64 {
    0
}
