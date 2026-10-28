//! The GPUI host of Osprey Native ([GPUI-HOST-SEAM]). Rust is only the seam:
//! it loads the Osprey library, keeps regions by key, draws them and sends
//! events back by the ids the framework issued. No application rule lives here.

mod adapters;
mod clock;
#[cfg(test)]
mod coverage;
mod envelope;
mod events;
mod ffi;
mod host;
mod regions;
mod render;
mod scene;
mod smoke;
mod store;

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

fn main() {
    log_panics();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--smoke") {
        std::process::exit(smoke::run());
    }
    std::process::exit(host::run());
}
