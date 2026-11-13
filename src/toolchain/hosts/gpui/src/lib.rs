//! The GPUI host of Osprey Native ([GPUI-HOST-SEAM]). Rust is only the seam: it
//! loads the Osprey library, reads each frame's shared rows in place, draws them
//! and sends typed events back by the control numbers the framework issued. No
//! application rule and no style lives here.
//!
//! The crate is a library plus the `osprey-gpui` binary on desktop; on Android
//! the same library is linked into the host's shared library, whose
//! [`android_main`] NativeActivity calls instead of `main`.

mod adapters;
mod clock;
#[cfg(test)]
mod coverage;
mod diagnostics;
mod events;
mod ffi;
mod frame;
mod host;
mod launch;
mod regions;
mod render;
mod rows;
mod scene;
mod screen;
mod self_test;
mod smoke;
mod store;
mod tree;
mod view;

/// The person's home folder: `HOME`, or `USERPROFILE` on Windows.
pub(crate) fn home() -> Result<String, String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|problem| format!("neither HOME nor USERPROFILE is set: {problem}"))
}

/// Where this platform keeps application logs: `~/Library/Logs/Osprey` on macOS,
/// `$XDG_STATE_HOME/osprey` (or `~/.local/state/osprey`) on Linux, `%LOCALAPPDATA%\Osprey\Logs` on Windows.
fn log_folder() -> std::path::PathBuf {
    let home = std::path::PathBuf::from(home().unwrap_or_default());
    if cfg!(target_os = "macos") {
        home.join("Library/Logs/Osprey")
    } else if cfg!(windows) {
        std::env::var("LOCALAPPDATA")
            .map_or_else(|_| home.join("AppData/Local"), std::path::PathBuf::from)
            .join("Osprey/Logs")
    } else {
        std::env::var("XDG_STATE_HOME")
            .map_or_else(|_| home.join(".local/state"), std::path::PathBuf::from)
            .join("osprey")
    }
}

/// Appends every panic to a log file as well as stderr, so a crash in a window run leaves a trace.
fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let folder = log_folder();
        let _ = std::fs::create_dir_all(&folder);
        let line = format!("{info}\n{}\n", std::backtrace::Backtrace::force_capture());
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(folder.join("gpui-panic.log"))
            .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
        default(info);
    }));
}

/// The desktop entry: the windowless smoke run, the scripted self-test, or the
/// GPUI window ([GPUI-HOST-SEAM]).
pub fn run() -> i32 {
    log_panics();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--smoke") {
        return smoke::run();
    }
    if self_test::requested() == Some(true) {
        return self_test::windowless();
    }
    host::run()
}

/// Writes out the files `assets/osprey-files.txt` names, so the host reads them as on desktop.
#[cfg(target_os = "android")]
fn unpack_assets(
    app: &android_activity::AndroidApp,
    folder: &std::path::Path,
) -> Result<(), String> {
    use std::io::Read;
    let assets = app.asset_manager();
    let read = |name: &str| -> Result<Vec<u8>, String> {
        let path = std::ffi::CString::new(name)
            .map_err(|error| format!("Bad asset name {name}: {error}"))?;
        let mut asset = assets
            .open(&path)
            .ok_or_else(|| format!("The APK has no asset {name}"))?;
        let mut bytes = Vec::new();
        asset
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Cannot read asset {name}: {error}"))?;
        Ok(bytes)
    };
    let manifest =
        String::from_utf8(read("osprey-files.txt")?).map_err(|error| error.to_string())?;
    launch::unpack(&manifest, read, folder)
}

/// NativeActivity's entry, resolved by android-activity's glue from this
/// library: the same window flow with the Android platform backend
/// ([CAST-MOBILE-GPUI]).
#[cfg(target_os = "android")]
#[no_mangle]
pub fn android_main(app: android_activity::AndroidApp) {
    log_panics();
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    // The sandbox starts the process with no HOME, and an APK's files have no path until
    // they are written out.
    if let Some(home) = app.internal_data_path() {
        std::env::set_var("HOME", &home);
        if let Err(problem) = unpack_assets(&app, &home.join("resources")) {
            log::error!("{problem}");
        }
    }
    host::set_android_app(app);
    let status = host::run();
    log::info!("the GPUI host finished with status {status}");
    // GPUI runs once in a process. Back ends the activity but Android keeps the process, and would
    // start the next activity in it; ending the process lets it start cleanly instead.
    std::process::exit(status);
}
