//! Links the Osprey application the host runs over the shared-memory boundary
//! ([NATIVE-SHM-BOUNDARY], docs/osprey-prism/README.md).
//!
//! `OSPREY_NATIVE_APP_DIR` holds `libOspreyApp.a`, compiled with
//! `osprey <app> --compile --target=macos -o <folder>/libOspreyApp.a`. On macOS it links beside the
//! Mac ARC runtime `libosprey_runtime_macos_arc.a` from `OSPREY_RUNTIME_DIR`. `OSPREY_NATIVE_ABI`
//! names the runtime's generated `native_abi.h`: its shared fingerprint is compiled into the host,
//! and the linked runtime refuses to open a bridge for any other layout ([NATIVE-SHM-LAYOUT]).
//! More libraries an application needs are named in `OSPREY_NATIVE_LINK_SEARCH` (`:`-separated
//! folders) and `OSPREY_NATIVE_LINK_LIBS` (`,`-separated cargo link specs).

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
    for name in VARIABLES {
        println!("cargo::rerun-if-env-changed={name}");
    }
    let required = |name: &str| {
        env::var_os(name)
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("{name} must be set (see the crate's README)"))
    };
    fingerprint(&required("OSPREY_NATIVE_ABI"));
    let app = required("OSPREY_NATIVE_APP_DIR");
    let runtime = required("OSPREY_RUNTIME_DIR");
    println!("cargo::rerun-if-changed={}", app.join("libOspreyApp.a").display());
    println!("cargo::rustc-link-search=native={}", app.display());
    println!("cargo::rustc-link-search=native={}", runtime.display());
    let search = env::var("OSPREY_NATIVE_LINK_SEARCH").unwrap_or_default();
    for folder in search.split(':').filter(|folder| !folder.is_empty()) {
        println!("cargo::rustc-link-search=native={folder}");
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
        Ok("windows") => println!("cargo::rustc-link-lib=dylib=ws2_32"),
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
