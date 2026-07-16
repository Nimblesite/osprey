//! Generic debugger support primitives.
//!
//! This crate intentionally avoids Osprey parser, type-checker, codegen, and
//! editor APIs. It holds small debugger concepts that are candidates to move to
//! `lspkit` once the shape proves useful across languages.

use std::path::{Path, PathBuf};

/// Source file identity used by debug-info producers and editor debug launches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugSource {
    /// Basename of the source file.
    pub filename: String,
    /// Directory containing the source file.
    pub directory: String,
}

impl DebugSource {
    /// Build a source identity from a source path. The directory is always
    /// absolute: an empty or relative `DW_AT_comp_dir` makes the macOS linker
    /// silently skip the debug map (no `N_OSO` stabs), so `dsymutil` produces an
    /// empty dSYM and profiler/debugger line info vanishes [PROF-BUILD-MODE].
    #[must_use]
    pub fn from_path(path: &str) -> Self {
        let path = Path::new(path);
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("input.osp")
            .to_string();
        DebugSource {
            filename,
            directory: absolute_directory(path.parent()),
        }
    }

    /// The full source path represented by this identity.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        Path::new(&self.directory).join(&self.filename)
    }
}

/// The absolute form of a source file's parent directory, resolving empty and
/// relative parents against the working directory (falling back to `.` only
/// when the working directory itself is unavailable).
fn absolute_directory(parent: Option<&Path>) -> String {
    let parent = parent.filter(|p| !p.as_os_str().is_empty());
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let abs = match parent {
        Some(p) if p.is_absolute() => p.to_path_buf(),
        Some(p) => base.join(p),
        None => base,
    };
    abs.to_str().map_or_else(|| ".".to_string(), str::to_string)
}

/// The native build kinds a compiler front-end can request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildKind {
    /// Optimized build, no debug info.
    Release,
    /// Source-level debugging: full debug info at `-O0`.
    Debug,
    /// CPU profiling [PROF-BUILD-MODE]: DWARF line info + frame pointers at
    /// FULL optimization — a profile of an unoptimized program misleads, so
    /// unlike `Debug` this keeps the release optimizer flag.
    Profile,
}

impl BuildKind {
    /// The optimizer flag to use for this build.
    #[must_use]
    pub fn opt_flag(self, release_default: String, debug_override: Option<String>) -> String {
        match self {
            BuildKind::Debug => debug_override.unwrap_or_else(|| "-O0".to_string()),
            BuildKind::Release | BuildKind::Profile => release_default,
        }
    }

    /// Extra C/LLVM driver flags for native builds of this kind.
    #[must_use]
    pub fn native_driver_flags(self) -> Vec<String> {
        match self {
            BuildKind::Release => Vec::new(),
            BuildKind::Debug | BuildKind::Profile => {
                vec!["-g".to_string(), "-fno-omit-frame-pointer".to_string()]
            }
        }
    }

    /// Whether codegen should emit source-level debug metadata (DWARF).
    #[must_use]
    pub fn wants_debug_info(self) -> bool {
        !matches!(self, BuildKind::Release)
    }
}

/// Debug-build switches shared by native compiler front-ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugBuild {
    /// Whether source-level debug information is requested.
    pub enabled: bool,
}

impl DebugBuild {
    /// A non-debug build.
    pub const OFF: DebugBuild = DebugBuild { enabled: false };

    /// A source-level debug build.
    pub const ON: DebugBuild = DebugBuild { enabled: true };

    /// The [`BuildKind`] this two-state switch corresponds to.
    #[must_use]
    pub fn kind(self) -> BuildKind {
        if self.enabled {
            BuildKind::Debug
        } else {
            BuildKind::Release
        }
    }

    /// The optimizer flag to use for this build.
    #[must_use]
    pub fn opt_flag(self, release_default: String, debug_override: Option<String>) -> String {
        self.kind().opt_flag(release_default, debug_override)
    }

    /// Extra C/LLVM driver flags for native debug builds.
    #[must_use]
    pub fn native_driver_flags(self) -> Vec<String> {
        self.kind().native_driver_flags()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_source_splits_file_and_directory() {
        let src = DebugSource::from_path("/tmp/example.osp");
        assert_eq!(src.filename, "example.osp");
        assert_eq!(src.directory, "/tmp");
        assert_eq!(src.path(), PathBuf::from("/tmp/example.osp"));
    }

    #[test]
    fn debug_source_falls_back_for_a_rootless_path() {
        // The filesystem root has neither a file name nor a parent, so both
        // fallbacks fire: a stable basename and the (absolute) working
        // directory. This pins the NO-PLACEHOLDER fallback values.
        let src = DebugSource::from_path("/");
        assert_eq!(src.filename, "input.osp");
        assert!(Path::new(&src.directory).is_absolute());
    }

    // An empty comp_dir kills the macOS linker's debug map, so a bare relative
    // filename must resolve its directory against the working directory.
    #[test]
    fn debug_source_makes_relative_parents_absolute() {
        let src = DebugSource::from_path("solo.osp");
        assert_eq!(src.filename, "solo.osp");
        assert!(Path::new(&src.directory).is_absolute());
        let nested = DebugSource::from_path("examples/nested.osp");
        assert!(Path::new(&nested.directory).is_absolute());
        assert!(nested.directory.ends_with("examples"));
    }

    #[test]
    fn debug_build_selects_flags() {
        assert_eq!(
            DebugBuild::OFF.opt_flag("-O2".to_string(), Some("-O0".to_string())),
            "-O2"
        );
        assert_eq!(DebugBuild::ON.opt_flag("-O2".to_string(), None), "-O0");
        assert_eq!(
            DebugBuild::ON.opt_flag("-O2".to_string(), Some("-Og".to_string())),
            "-Og"
        );
        assert!(DebugBuild::OFF.native_driver_flags().is_empty());
        assert!(DebugBuild::ON
            .native_driver_flags()
            .iter()
            .any(|f| f == "-g"));
        assert_eq!(DebugBuild::OFF.kind(), BuildKind::Release);
        assert_eq!(DebugBuild::ON.kind(), BuildKind::Debug);
    }

    // [PROF-BUILD-MODE]: profiling keeps the release optimizer but adds debug
    // info + frame pointers — the combination neither Release nor Debug gives.
    #[test]
    fn profile_build_keeps_optimizer_with_debug_flags() {
        let profile = BuildKind::Profile;
        assert_eq!(
            profile.opt_flag("-O2".to_string(), Some("-O0".to_string())),
            "-O2"
        );
        let flags = profile.native_driver_flags();
        assert!(flags.contains(&"-g".to_string()));
        assert!(flags.contains(&"-fno-omit-frame-pointer".to_string()));
        assert!(profile.wants_debug_info());
        assert!(BuildKind::Debug.wants_debug_info());
        assert!(!BuildKind::Release.wants_debug_info());
    }
}
