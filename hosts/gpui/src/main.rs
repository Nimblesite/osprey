//! The desktop binary of the GPUI host; the same crate as a library is the
//! Android entry on mobile. See [`osprey_gpui::run`].

fn main() {
    std::process::exit(osprey_gpui::run());
}
