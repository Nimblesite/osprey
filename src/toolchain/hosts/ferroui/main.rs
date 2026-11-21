//! The host binary: `ferroui-osprey-native [arguments]` opens the application's window; the
//! arguments are the application's launch arguments, in order.

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(ferroui_osprey_native::desktop::run(&arguments));
}
