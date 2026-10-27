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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--smoke") {
        std::process::exit(smoke::run());
    }
    std::process::exit(host::run());
}
