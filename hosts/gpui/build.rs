// Links the Osprey application archive ([GPUI-HOST-BOUNDARY], [GPUI-HOST-TARGETS]).
// `OSPREY_GPUI_APP_DIR` holds `libOspreyApp.a` (default: the probe's build folder).
// The macOS archive is linked beside the Mac ARC runtime in `OSPREY_RUNTIME_DIR`;
// a Linux or Windows archive already bundles its native runtime and needs only the
// system libraries that runtime calls. An application that needs more, such as the
// casting player's TLS and file-server runtime with OpenSSL, names them in
// `OSPREY_GPUI_LINK_SEARCH` (colon-separated folders) and `OSPREY_GPUI_LINK_LIBS`
// (comma-separated cargo link specs such as `static=osprey_cast_net,dylib=ssl`).
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
    println!("cargo:rerun-if-env-changed=OSPREY_GPUI_LINK_SEARCH");
    println!("cargo:rerun-if-env-changed=OSPREY_GPUI_LINK_LIBS");
    let search = env::var("OSPREY_GPUI_LINK_SEARCH").unwrap_or_default();
    for folder in search.split(':').filter(|folder| !folder.is_empty()) {
        println!("cargo:rustc-link-search=native={folder}");
    }
    let libs = env::var("OSPREY_GPUI_LINK_LIBS").unwrap_or_default();
    for lib in libs.split(',').filter(|lib| !lib.is_empty()) {
        println!("cargo:rustc-link-lib={lib}");
    }
    println!(
        "cargo:rerun-if-changed={}",
        app.join("libOspreyApp.a").display()
    );
    println!("cargo:rustc-link-search=native={}", app.display());
    println!("cargo:rustc-link-search=native={}", runtime.display());
    println!("cargo:rustc-link-lib=static=OspreyApp");
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => println!("cargo:rustc-link-lib=static=osprey_runtime_macos_arc"),
        Ok("windows") => println!("cargo:rustc-link-lib=dylib=ws2_32"),
        _ => {
            for system in ["pthread", "dl", "m"] {
                println!("cargo:rustc-link-lib=dylib={system}");
            }
        }
    }
}
