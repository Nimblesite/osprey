//! Content-addressed native test executables [TESTING-NATIVE-CACHE].
use crate::invocation::TEST_CACHE_DIR_ENV;
use crate::linking::{directive, find_runtime_lib};
use crate::native::{build_input, c_compiler, opt_flag};
use crate::project::CompilationInput;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub(super) fn test_cache_path(
    input: &CompilationInput,
    memory: &str,
    kind: osprey_debug::BuildKind,
) -> Option<PathBuf> {
    let dir = cache_directory(input)?;
    Some(dir.join(format!(
        "suite-{:016x}.out",
        test_cache_key(input, memory, kind)
    )))
}

fn cache_directory(input: &CompilationInput) -> Option<PathBuf> {
    // [TESTING-NATIVE-CACHE] every input affecting the native artifact is
    // represented by the cache key; untracked external links bypass caching.
    let dir = std::env::var_os(TEST_CACHE_DIR_ENV).map(PathBuf::from)?;
    if dir.as_os_str().is_empty() || !cacheable_test_source(input.source()) {
        return None;
    }
    if std::fs::create_dir_all(&dir).is_err() || !directory_is_safe(&dir) {
        return None;
    }
    Some(dir)
}

pub(super) fn codegen_cache_path(
    input: &CompilationInput,
    kind: osprey_debug::BuildKind,
) -> Option<PathBuf> {
    use std::hash::Hasher;
    let dir = cache_directory(input)?;
    Some(dir.join(format!(
        "module-{:016x}.ll",
        codegen_key(input, kind).finish()
    )))
}

pub(super) fn cacheable_test_source(source: &str) -> bool {
    !source
        .lines()
        .any(|line| directive(line, "linkdir").is_some())
}

pub(super) fn directory_is_safe(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.file_type().is_dir() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o700);
        if std::fs::set_permissions(path, permissions).is_err() {
            return false;
        }
    }
    true
}

pub(super) fn test_cache_key(
    input: &CompilationInput,
    memory: &str,
    kind: osprey_debug::BuildKind,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut state = codegen_key(input, kind);
    memory.hash(&mut state);
    hash_runtime_identity(memory, &mut state);
    state.finish()
}

fn codegen_key(
    input: &CompilationInput,
    kind: osprey_debug::BuildKind,
) -> std::collections::hash_map::DefaultHasher {
    use std::hash::Hash;
    let mut state = std::collections::hash_map::DefaultHasher::new();
    "osprey-test-cache-v2".hash(&mut state);
    input.source().hash(&mut state);
    // Assembly options can change qualified symbols without changing source text.
    format!("{:?}", input.backend_program()).hash(&mut state);
    input.debug_path().hash(&mut state);
    // Both lowerings must produce distinct cached artifacts [GPU-KERNEL-EXTRACT].
    std::env::var_os(osprey_codegen::GPU_KERNELS_ENV).hash(&mut state);
    std::mem::discriminant(&kind).hash(&mut state);
    opt_flag(kind).hash(&mut state);
    hash_toolchain_identity(&mut state);
    state
}

fn hash_toolchain_identity<H: std::hash::Hasher>(state: &mut H) {
    use std::hash::Hash;
    let compiler = c_compiler();
    compiler.hash(state);
    hash_file_identity(compiler_path(&compiler).as_deref(), state);
    std::env::var_os("PATH").hash(state);
    hash_file_identity(std::env::current_exe().ok().as_deref(), state);
}

fn compiler_path(compiler: &str) -> Option<PathBuf> {
    let requested = Path::new(compiler);
    if requested.is_absolute() || requested.components().count() > 1 {
        return Some(requested.to_path_buf());
    }
    let search = std::env::var_os("PATH")?;
    let suffix = if cfg!(windows) && requested.extension().is_none() {
        std::env::consts::EXE_SUFFIX
    } else {
        ""
    };
    std::env::split_paths(&search)
        .map(|directory| directory.join(format!("{compiler}{suffix}")))
        .find(|path| path.is_file())
}

pub(super) fn hash_runtime_identity<H: std::hash::Hasher>(memory: &str, state: &mut H) {
    let suffix = match memory {
        "gc" => "_gc",
        "arc" => "_arc",
        _ => "",
    };
    for prefix in ["libfiber_runtime", "libhttp_runtime"] {
        let runtime = find_runtime_lib(&format!("{prefix}{suffix}.a")).map(PathBuf::from);
        hash_file_identity(runtime.as_deref(), state);
    }
}

pub(super) fn hash_file_identity<H: std::hash::Hasher>(path: Option<&Path>, state: &mut H) {
    use std::hash::Hash;

    path.hash(state);
    let Some(metadata) = path.and_then(|file| std::fs::metadata(file).ok()) else {
        return;
    };
    metadata.len().hash(state);
    metadata.modified().ok().hash(state);
}

pub(super) fn ensure_cached_executable(
    input: &CompilationInput,
    memory: &str,
    kind: osprey_debug::BuildKind,
    cached: &Path,
) -> Result<(), ExitCode> {
    if is_nonempty_file(cached) {
        return Ok(());
    }
    let staging = cached.with_extension(format!("{}.tmp", std::process::id()));
    let build = build_input(
        input,
        &staging,
        crate::native::NativeOptions::new(memory, kind),
    );
    if let Err(code) = build {
        let _ = std::fs::remove_file(&staging);
        return Err(code);
    }
    publish_cached_artifact(&staging, cached)
}

pub(super) fn is_nonempty_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.file_type().is_file() && metadata.len() > 0)
}

pub(super) fn publish_cached_artifact(staging: &Path, cached: &Path) -> Result<(), ExitCode> {
    match std::fs::rename(staging, cached) {
        Ok(()) => Ok(()),
        Err(_) if is_nonempty_file(cached) => {
            let _ = std::fs::remove_file(staging);
            Ok(())
        }
        Err(error) => {
            eprintln!(
                "error: cannot publish cached artifact {}: {error}",
                cached.display()
            );
            let _ = std::fs::remove_file(staging);
            Err(ExitCode::FAILURE)
        }
    }
}
