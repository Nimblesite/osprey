//! Linking support for the compiler driver.

use std::path::Path;

/// Assemble the link arguments — everything a compiled binary needs beyond
/// libc: the prebuilt C runtime static library (the HTTP superset when the
/// program touches HTTP/WebSocket, else the fiber runtime), OpenSSL for HTTP,
/// and any `// @link:` / `// @linkdir:` FFI directives (e.g. `-lsqlite3`).
/// Implements [FFI-LINK-DIRECTIVES].
pub(super) fn link_args(ir: &str, source: &str, memory: &str) -> Vec<String> {
    // The extern-callable TLS session and file server live in the HTTP
    // archive with OpenSSL [FFI-TLS-CLIENT] [FFI-FILE-SERVER].
    let uses_http = ir.contains("@http")
        || ir.contains("@websocket")
        || ir.contains("@osprey_tls_")
        || ir.contains("@osprey_net_")
        || ir.contains("@osprey_file_server_");
    let mut args: Vec<_> = runtime_archive(uses_http, memory).into_iter().collect();
    if uses_http {
        args.extend(openssl_flags());
    }
    args.extend(platform_flags(uses_http, needs_libm(ir)));
    if cfg!(target_os = "macos") && ir.contains("@osprey_gpu_") {
        args.extend(["-framework", "Foundation", "-framework", "Metal"].map(str::to_owned));
    }
    if cfg!(target_os = "linux") && ir.contains("@osprey_gpu_") {
        args.extend(["-ldl", "-pthread"].map(str::to_owned));
    }
    args.extend(source.lines().filter_map(ffi_flag));
    args
}

fn runtime_archive(http: bool, memory: &str) -> Option<String> {
    let name = if http { "http" } else { "fiber" };
    find_runtime_lib(&runtime_name(&format!("lib{name}_runtime"), memory))
        .or_else(|| find_runtime_lib(&runtime_name("libfiber_runtime", memory)))
}

pub(crate) fn runtime_name(base: &str, memory: &str) -> String {
    // [MEM-BACKENDS]: the reclaiming backend is an archive swap.
    let suffix = match memory {
        "gc" => "_gc",
        "arc" => "_arc",
        _ => "",
    };
    format!("{}{suffix}.a", base.trim_end_matches(".a"))
}

/// The IR spellings that reach libm: `frem` may lower to `fmod`
/// [FLOAT-IEEE-RESULTS], and the float functions [BUILTIN-MATH] and its
/// sections are libm calls — the trig/log/pow intrinsics lower to them and
/// the rest are called by name. `llvm.sqrt.f64` is deliberately absent: sqrt
/// lives in libc on every target the compiler links against.
const LIBM_SPELLINGS: [&str; 28] = [
    "frem ",
    "@llvm.exp.f64",
    "@llvm.log.f64",
    "@llvm.sin.f64",
    "@llvm.cos.f64",
    "@llvm.log2.f64",
    "@llvm.log10.f64",
    "@llvm.pow.f64",
    "@llvm.copysign.f64",
    "@llvm.floor.f64",
    "@llvm.ceil.f64",
    "@llvm.trunc.f64",
    "@llvm.roundeven.f64",
    "@tanh(",
    "@asin(",
    "@acos(",
    "@atan(",
    "@atan2(",
    "@sinh(",
    "@cosh(",
    "@asinh(",
    "@acosh(",
    "@atanh(",
    "@log1p(",
    "@expm1(",
    "@erf(",
    "@erfc(",
    "@hypot(",
];

fn needs_libm(ir: &str) -> bool {
    LIBM_SPELLINGS.iter().any(|spelling| ir.contains(spelling))
}

fn platform_flags(http: bool, libm: bool) -> Vec<String> {
    let mut flags = Vec::new();
    // Place libraries after the object/archive that references their symbols.
    if cfg!(unix) && libm {
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

/// The operating system a native build runs on, as a qualified directive
/// names it: `// @link(windows): ospgfx` links only a Windows executable.
const NATIVE_OS: &str = if cfg!(target_os = "windows") {
    "windows"
} else if cfg!(target_os = "macos") {
    "macos"
} else {
    "linux"
};

/// The trimmed value of a `// @<key>:` FFI directive line, or of one qualified
/// with this build's operating system (accepting the space-less `//@` spelling
/// too), or `None` if `line` is neither. Implements [FFI-LINK-DIRECTIVES].
pub(super) fn directive<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    unscoped(line, key).or_else(|| unscoped(line, &format!("{key}({NATIVE_OS})")))
}

fn unscoped<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let t = line.trim();
    t.strip_prefix(&format!("// @{key}:"))
        .or_else(|| t.strip_prefix(&format!("//@{key}:")))
        .map(str::trim)
}

/// The first unqualified link directive in `source`, as a capability name. An
/// app-library archive carries no library and its host does the linking, so
/// such a target refuses the directive instead of silently dropping it; a
/// directive qualified with a native operating system does not apply there.
/// Implements [FFI-LINK-DIRECTIVES] and [IOS-TARGET-CAPABILITIES].
pub(crate) fn unscoped_link_directive(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        ["link", "linkdir"].iter().find_map(|key| {
            unscoped(line, key).map(|value| format!("link directive `// @{key}: {value}`"))
        })
    })
}

/// Search the conventional install/build locations for a runtime static lib:
/// the working directory's repo layout, then next to the `osprey` executable
/// and below each of its ancestors (covering arbitrary in-workspace Cargo
/// target/profile nesting, release-tarball layouts and an installed
/// `<prefix>/bin` + `<prefix>/lib` pair such as Homebrew's), the compile-time
/// workspace as a development fallback, then the system lib dir.
pub(crate) fn find_runtime_lib(lib: &str) -> Option<String> {
    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf));
    runtime_lib_candidates(lib, executable_dir.as_deref())
        .into_iter()
        .find(|candidate| Path::new(candidate).exists())
}

/// `archive`, unless it was built in a checkout whose runtime sources have
/// changed since: such an archive lacks the objects those sources added, and
/// the host linker would report their symbols undefined. A shipped archive has
/// no sources beside it and is taken as built.
pub(crate) fn fresh_runtime(archive: &str) -> Result<String, String> {
    let path = Path::new(archive);
    let built = modified(path);
    let newer = checkout_runtime(path)
        .and_then(|sources| newest_source(&sources))
        .filter(|(_, changed)| built.is_some_and(|built| *changed > built));
    match newer {
        Some((source, _)) => Err(format!(
            "{archive} is older than {}; rebuild it with `make {}`",
            source.display(),
            rebuild_goal(path)
        )),
        None => Ok(archive.to_owned()),
    }
}

/// `compiler/runtime` beside an archive in `compiler/bin` or `compiler/lib`.
fn checkout_runtime(archive: &Path) -> Option<std::path::PathBuf> {
    let directory = archive.parent()?;
    let compiler = directory.parent()?;
    let in_checkout = matches!(directory.file_name()?.to_str()?, "bin" | "lib")
        && compiler.file_name()?.to_str()? == "compiler";
    in_checkout.then(|| compiler.join("runtime"))
}

fn newest_source(directory: &Path) -> Option<(std::path::PathBuf, std::time::SystemTime)> {
    std::fs::read_dir(directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| matches!(path.extension().and_then(|e| e.to_str()), Some("c" | "h")))
        .filter_map(|path| modified(&path).map(|changed| (path, changed)))
        .max_by_key(|(_, changed)| *changed)
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

/// The make goal that builds an archive: `libosprey_runtime_macos.a` is
/// built by `_runtime_macos`.
fn rebuild_goal(archive: &Path) -> String {
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    format!(
        "_{}",
        name.trim_start_matches("libosprey_").trim_end_matches(".a")
    )
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
                &["compiler/lib", "compiler/bin", "bin", "lib"],
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

#[cfg(test)]
mod tests {
    use super::{fresh_runtime, needs_libm};
    use std::time::{Duration, SystemTime};

    #[test]
    fn an_archive_older_than_its_runtime_sources_is_refused_with_the_rebuild() {
        // A stale archive lacks the objects newer sources added, and the host
        // linker would report their symbols undefined instead.
        let root = std::env::temp_dir().join(format!("osprey-stale-rt-{}", std::process::id()));
        let (bin, runtime) = (root.join("compiler/bin"), root.join("compiler/runtime"));
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::create_dir_all(&runtime).expect("runtime");
        let archive = bin.join("libosprey_runtime_macos.a");
        let source = runtime.join("json_codec.c");
        std::fs::write(&archive, b"!<arch>\n").expect("archive");
        std::fs::write(&source, b"int json;\n").expect("source");
        let stamp = |path: &std::path::Path, age: u64| {
            let file = std::fs::File::options()
                .write(true)
                .open(path)
                .expect("open");
            file.set_modified(SystemTime::now() - Duration::from_secs(age))
                .expect("mtime");
        };
        stamp(&archive, 600);
        stamp(&source, 60);
        let stale = fresh_runtime(&archive.display().to_string());
        stamp(&source, 1200);
        let fresh = fresh_runtime(&archive.display().to_string());
        let shipped = fresh_runtime("/usr/local/lib/libosprey_runtime_macos.a");
        std::fs::remove_dir_all(&root).expect("cleanup");
        let message = stale.expect_err("stale archive");
        assert!(
            message.contains("json_codec.c") && message.contains("make _runtime_macos"),
            "{message}"
        );
        assert!(fresh.is_ok() && shipped.is_ok());
    }

    #[test]
    fn libm_is_linked_for_each_spelling_that_reaches_it_and_nothing_else() {
        // [BUILTIN-MATH] / [FLOAT-IEEE-RESULTS]: every IR spelling whose
        // lowering needs libm, and near misses that must not drag it in.
        for ir in [
            "%r = frem double %a, %b",
            "call double @llvm.exp.f64(double 1.0)",
            "call double @llvm.log.f64(double 1.0)",
            "call double @tanh(double 1.0)",
            "call double @llvm.sin.f64(double 1.0)",
            "call double @llvm.cos.f64(double 1.0)",
            "call double @llvm.log2.f64(double 1.0)",
            "call double @llvm.log10.f64(double 1.0)",
            "call double @llvm.pow.f64(double 1.0, double 2.0)",
            "call double @llvm.copysign.f64(double 1.0, double -1.0)",
            "call double @llvm.floor.f64(double 1.0)",
            "call double @llvm.ceil.f64(double 1.0)",
            "call double @llvm.trunc.f64(double 1.0)",
            "call double @llvm.roundeven.f64(double 1.0)",
            "call double @asin(double 1.0)",
            "call double @acos(double 1.0)",
            "call double @atan(double 1.0)",
            "call double @atan2(double 1.0, double 2.0)",
            "call double @sinh(double 1.0)",
            "call double @cosh(double 1.0)",
            "call double @asinh(double 1.0)",
            "call double @acosh(double 1.0)",
            "call double @atanh(double 1.0)",
            "call double @log1p(double 1.0)",
            "call double @expm1(double 1.0)",
            "call double @erf(double 1.0)",
            "call double @erfc(double 1.0)",
            "call double @hypot(double 1.0, double 2.0)",
        ] {
            assert!(needs_libm(ir), "{ir}");
        }
        for ir in [
            "",
            "call double @llvm.sqrt.f64(double 1.0)",
            "%r = fadd double %a, %b",
            "call float @tanhf(float 1.0)",
            "declare double @tanh2(double)",
            "call double @sinf(float 1.0)",
            "call double @powf(float 1.0, float 2.0)",
        ] {
            assert!(!needs_libm(ir), "{ir}");
        }
    }
}

#[cfg(test)]
mod install_layout_tests {
    use super::runtime_lib_candidates;
    use std::path::Path;

    #[test]
    fn a_homebrew_prefix_lib_beside_bin_is_searched() {
        // `brew install` lays the formula out as `<prefix>/bin/osprey` and
        // `<prefix>/lib/lib*.a`, and links both into the Homebrew prefix. A
        // lookup that only knows `bin` and `compiler/{bin,lib}` fails the
        // very first native link on an installed compiler (issue #304).
        let bin = Path::new("/opt/homebrew/Cellar/osprey/0.22.0/bin");
        let candidates = runtime_lib_candidates("libfiber_runtime_gc.a", Some(bin));
        assert!(
            candidates.contains(
                &"/opt/homebrew/Cellar/osprey/0.22.0/lib/libfiber_runtime_gc.a".to_owned()
            ),
            "{candidates:#?}"
        );
        let linked = Path::new("/opt/homebrew/bin");
        let candidates = runtime_lib_candidates("libosprey_runtime_macos.a", Some(linked));
        assert!(
            candidates.contains(&"/opt/homebrew/lib/libosprey_runtime_macos.a".to_owned()),
            "{candidates:#?}"
        );
    }
}
