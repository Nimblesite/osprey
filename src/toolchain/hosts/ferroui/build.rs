//! Links the Osprey application the host runs over the shared-memory boundary
//! ([NATIVE-SHM-BOUNDARY], docs/osprey-prism/README.md).
//!
//! `OSPREY_NATIVE_APP_DIR` holds `libOspreyApp.a`, compiled with
//! `osprey <app> --compile --target=macos -o <folder>/libOspreyApp.a`. On macOS it links beside the
//! Mac ARC runtime `libosprey_runtime_macos_arc.a` from `OSPREY_RUNTIME_DIR`. `OSPREY_NATIVE_ABI`
//! names the runtime's generated `native_abi.h`: its shared fingerprint is compiled into the host,
//! and the linked runtime refuses to open a bridge for any other layout ([NATIVE-SHM-LAYOUT]).
//! On Windows nothing is linked: the host loads the application's self-contained `app.dll`
//! (`osprey build <app> --target=windows-x64`) at run time, from `OSPREY_NATIVE_APP_DLL` or
//! beside the executable, so only `OSPREY_NATIVE_ABI` is needed to build.
//! More libraries an application needs are named in `OSPREY_NATIVE_LINK_SEARCH` (folders
//! separated as in `PATH`: `:` on Unix, `;` on Windows) and `OSPREY_NATIVE_LINK_LIBS`
//! (`,`-separated cargo link specs).

use std::env;
use std::path::PathBuf;

const VARIABLES: [&str; 5] = [
    "OSPREY_NATIVE_APP_DIR",
    "OSPREY_RUNTIME_DIR",
    "OSPREY_NATIVE_ABI",
    "OSPREY_NATIVE_LINK_SEARCH",
    "OSPREY_NATIVE_LINK_LIBS",
];

fn main() {
    // `app_dll`: the application is a DLL the host loads, so only its boundary can be tested.
    println!("cargo::rustc-check-cfg=cfg(app_dll)");
    for name in VARIABLES {
        println!("cargo::rerun-if-env-changed={name}");
    }
    let required = |name: &str| {
        env::var_os(name)
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("{name} must be set (see the crate's README)"))
    };
    fingerprint(&required("OSPREY_NATIVE_ABI"));
    // On Windows the application is the self-contained DLL the host loads at run time (`app.rs`,
    // [WINUI-HOST-DLL]): its MinGW runtime stays inside it, so nothing is linked here.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo::rustc-cfg=app_dll");
        // FerroUI's ANGLE ships libEGL and libGLESv2 as static libraries, and libEGL still calls
        // libGLESv2 through DLL-import stubs. MSVC resolves each one locally and notes it (LNK4217,
        // once per EGL entry point); only that note is silenced, every other linker message stands.
        println!("cargo::rustc-link-arg=/IGNORE:4217");
        return;
    }
    let app = required("OSPREY_NATIVE_APP_DIR");
    let runtime = required("OSPREY_RUNTIME_DIR");
    println!("cargo::rerun-if-changed={}", app.join("libOspreyApp.a").display());
    println!("cargo::rustc-link-search=native={}", app.display());
    println!("cargo::rustc-link-search=native={}", runtime.display());
    // The platform's own separator: `:` would cut a Windows path such as `C:/msys64` in two.
    let search = env::var_os("OSPREY_NATIVE_LINK_SEARCH").unwrap_or_default();
    for folder in env::split_paths(&search).filter(|folder| !folder.as_os_str().is_empty()) {
        println!("cargo::rustc-link-search=native={}", folder.display());
    }
    let libs = env::var("OSPREY_NATIVE_LINK_LIBS").unwrap_or_default();
    for lib in libs.split(',').filter(|lib| !lib.is_empty()) {
        println!("cargo::rustc-link-lib={lib}");
    }
    println!("cargo::rustc-link-lib=static=OspreyApp");
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => {
            println!("cargo::rustc-link-lib=static=osprey_runtime_macos_arc");
            for framework in ["Metal", "Foundation"] {
                println!("cargo::rustc-link-lib=framework={framework}");
            }
        }
        _ => {
            for system in ["pthread", "dl", "m"] {
                println!("cargo::rustc-link-lib=dylib={system}");
            }
        }
    }
}

/// Passes the shared layout this host is compiled against to `frame.rs` ([NATIVE-SHM-LAYOUT]).
fn fingerprint(header: &std::path::Path) {
    println!("cargo::rerun-if-changed={}", header.display());
    let text = std::fs::read_to_string(header)
        .unwrap_or_else(|problem| panic!("{}: {problem}", header.display()));
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix("#define TD_SHARED_FINGERPRINT_64 "))
        .expect("the header's shared fingerprint")
        .trim_matches('"');
    println!("cargo::rustc-env=OSPREY_NATIVE_FINGERPRINT={value}");
}
