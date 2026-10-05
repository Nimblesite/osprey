//! Linking support for the compiler driver.

use std::path::Path;

/// Assemble the link arguments — everything a compiled binary needs beyond
/// libc: the prebuilt C runtime static library (the HTTP superset when the
/// program touches HTTP/WebSocket, else the fiber runtime), OpenSSL for HTTP,
/// and any `// @link:` / `// @linkdir:` FFI directives (e.g. `-lsqlite3`).
/// Implements [FFI-LINK-DIRECTIVES].
pub(super) fn link_args(ir: &str, source: &str, memory: &str) -> Vec<String> {
    let uses_http = ir.contains("@http") || ir.contains("@websocket");
    let mut args: Vec<_> = runtime_archive(uses_http, memory).into_iter().collect();
    if uses_http {
        args.extend(openssl_flags());
    }
    args.extend(platform_flags(uses_http, ir.contains("frem ")));
    args.extend(source.lines().filter_map(ffi_flag));
    args
}

fn runtime_archive(http: bool, memory: &str) -> Option<String> {
    // [MEM-BACKENDS]: the reclaiming backend is an archive swap.
    let suffix = match memory {
        "gc" => "_gc",
        "arc" => "_arc",
        _ => "",
    };
    let name = if http { "http" } else { "fiber" };
    find_runtime_lib(&format!("lib{name}_runtime{suffix}.a"))
        .or_else(|| find_runtime_lib(&format!("libfiber_runtime{suffix}.a")))
}

fn platform_flags(http: bool, float_remainder: bool) -> Vec<String> {
    let mut flags = Vec::new();
    // [FLOAT-IEEE-RESULTS]: LLVM may lower `frem` to libm's fmod.
    // Place libraries after the object/archive that references their symbols.
    if cfg!(unix) && float_remainder {
        flags.push("-lm".into());
    }
    if cfg!(windows) {
        flags.push("-lpthread".into());
        if http {
            flags.push("-lws2_32".into());
        }
    }
    flags
}

fn ffi_flag(line: &str) -> Option<String> {
    directive(line, "link")
        .map(|lib| format!("-l{lib}"))
        .or_else(|| directive(line, "linkdir").map(|dir| format!("-L{dir}")))
}

/// The trimmed value of a `// @<key>:` FFI directive line (accepting the
/// space-less `//@<key>:` spelling too), or `None` if `line` is not one.
pub(super) fn directive<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let t = line.trim();
    t.strip_prefix(&format!("// @{key}:"))
        .or_else(|| t.strip_prefix(&format!("//@{key}:")))
        .map(str::trim)
}

/// Search the conventional install/build locations for a runtime static lib:
/// the working directory's repo layout, then next to the `osprey` executable
/// and below each of its ancestors (covering arbitrary in-workspace Cargo
/// target/profile nesting and release-tarball layouts), the compile-time
/// workspace as a development fallback, then the system lib dir.
pub(crate) fn find_runtime_lib(lib: &str) -> Option<String> {
    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf));
    runtime_lib_candidates(lib, executable_dir.as_deref())
        .into_iter()
        .find(|candidate| Path::new(candidate).exists())
}

pub(super) fn runtime_lib_candidates(lib: &str, executable_dir: Option<&Path>) -> Vec<String> {
    let mut roots: Vec<_> = ["compiler/bin", "compiler/lib", "bin", "../bin", "../../bin"]
        .map(|directory| format!("{directory}/{lib}"))
        .into();
    if let Some(dir) = executable_dir {
        roots.push(dir.join(lib).display().to_string());
        for ancestor in dir.ancestors() {
            roots.extend(library_paths(
                ancestor,
                lib,
                &["compiler/lib", "compiler/bin", "bin"],
            ));
        }
    }
    roots.extend(workspace_libraries(lib));
    roots.push(format!("/usr/local/lib/{lib}"));
    roots
}

fn workspace_libraries(lib: &str) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    library_paths(&root, lib, &["compiler/lib", "compiler/bin"])
}

fn library_paths(root: &Path, library: &str, directories: &[&str]) -> Vec<String> {
    directories
        .iter()
        .map(|directory| root.join(directory).join(library).display().to_string())
        .collect()
}

/// OpenSSL link flags, searching the conventional Homebrew/system lib dirs.
pub(super) fn openssl_flags() -> Vec<String> {
    for dir in [
        "/opt/homebrew/opt/openssl@3/lib",
        "/opt/homebrew/lib",
        "/usr/local/opt/openssl@3/lib",
        "/usr/local/lib",
    ] {
        if Path::new(dir).join("libssl.dylib").exists() {
            return vec![format!("-L{dir}"), "-lssl".into(), "-lcrypto".into()];
        }
    }
    vec!["-lssl".into(), "-lcrypto".into()]
}
