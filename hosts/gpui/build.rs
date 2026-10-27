// Links the Osprey application archive and the macOS ARC runtime it was
// compiled against ([GPUI-HOST-BOUNDARY]). `OSPREY_GPUI_APP_DIR` holds
// `libOspreyApp.a` (default: the probe's build folder) and
// `OSPREY_RUNTIME_DIR` holds `libosprey_runtime_macos_arc.a`.
use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let app = env::var("OSPREY_GPUI_APP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("probe/build"));
    let runtime = env::var("OSPREY_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("../../compiler/bin"));
    println!("cargo:rerun-if-env-changed=OSPREY_GPUI_APP_DIR");
    println!("cargo:rerun-if-env-changed=OSPREY_RUNTIME_DIR");
    println!(
        "cargo:rerun-if-changed={}",
        app.join("libOspreyApp.a").display()
    );
    println!("cargo:rustc-link-search=native={}", app.display());
    println!("cargo:rustc-link-search=native={}", runtime.display());
    println!("cargo:rustc-link-lib=static=OspreyApp");
    println!("cargo:rustc-link-lib=static=osprey_runtime_macos_arc");
}
