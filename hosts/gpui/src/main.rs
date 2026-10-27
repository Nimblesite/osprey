//! The GPUI host of Osprey Native ([GPUI-HOST-SEAM]). Rust is only the seam:
//! it loads the Osprey library, keeps regions by key, draws them and sends
//! events back by the ids the framework issued. No application rule lives here.

mod clock;
mod envelope;
mod events;
mod ffi;
mod host;
mod regions;
mod render;
mod scene;
mod smoke;
mod store;

/// Appends every panic to a log file as well as stderr, so a crash in a window run leaves a trace.
fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let home = std::env::var("HOME").unwrap_or_default();
        let folder = std::path::Path::new(&home).join("Library/Logs/Osprey");
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
