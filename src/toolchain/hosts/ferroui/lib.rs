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
mod app;
#[path = "../gpui/src/events.rs"]
pub mod events;
#[path = "../gpui/src/ffi.rs"]
pub mod ffi;
#[path = "../gpui/src/frame.rs"]
mod frame;
pub mod host;
#[path = "../gpui/src/regions.rs"]
pub mod regions;
// Both desktop renderers consume the same checked native scene protocol.
#[path = "../gpui/src/rows.rs"]
pub mod rows;
pub mod scene;
#[path = "../gpui/src/store.rs"]
pub mod store;
pub mod surface;
pub mod symbols;
pub mod theme;
#[path = "../gpui/src/tree.rs"]
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

/// The local UTC offset in seconds, daylight saving included, 0 when the system cannot tell.
/// The C runtime's local wall-clock time for now, read back as if it were UTC, is ahead of now by
/// exactly the offset.
#[cfg(windows)]
pub fn local_offset() -> i64 {
    extern "C" {
        /// The UCRT's inverse of `gmtime`: a broken-down UTC time to seconds since the epoch.
        fn _mkgmtime64(time: *mut libc::tm) -> i64;
    }
    let timestamp: libc::time_t = now();
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: both pointers are live, aligned and owned for the call.
    if unsafe { libc::localtime_s(local.as_mut_ptr(), &timestamp) } != 0 {
        return 0;
    }
    // SAFETY: localtime_s initialised the structure when it returned 0.
    let mut wall = unsafe { local.assume_init() };
    // SAFETY: a fully initialised structure the call may normalise in place.
    let as_utc = unsafe { _mkgmtime64(&mut wall) };
    if as_utc == -1 {
        0
    } else {
        as_utc - timestamp
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    /// The application's clock is local time on Windows, not UTC: the offset handed to it at
    /// launch is the one Windows itself reports for now, daylight saving included.
    #[test]
    fn local_offset_matches_the_windows_time_zone() {
        let reported = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "[int][TimeZoneInfo]::Local.GetUtcOffset([DateTime]::UtcNow).TotalSeconds",
            ])
            .output()
            .ok()
            .and_then(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .parse::<i64>()
                    .ok()
            });
        assert_eq!(Some(super::local_offset()), reported);
    }
}
