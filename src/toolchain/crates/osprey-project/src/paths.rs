//! The one place the toolchain resolves and spells filesystem paths [PROJECT-PATHS].
//!
//! `std::fs::canonicalize` spells a Windows drive path in its verbatim form, `\\?\C:\…`. Under
//! that prefix `/` is not a separator and `..` is not resolved, so a path that leaves the compiler
//! in that form breaks whatever joins onto it: a discipline checker appending `/erased-…`, `cl.exe`,
//! or a source root such as `../lib` that would then sort differently on Windows alone. Every
//! canonical path the toolchain makes therefore comes from [`canonical`], which keeps the plain
//! drive spelling, and every path handed to another process or compared with one goes through
//! [`plain`]. Nothing else calls `canonicalize`.

use std::path::{Path, PathBuf};

/// The verbatim prefix Windows puts on a canonical path.
const VERBATIM: &str = r"\\?\";

/// `path` resolved through every link, in plain spelling: `C:\proj`, never `\\?\C:\proj`.
///
/// # Errors
///
/// Returns the filesystem's error when `path` does not exist or cannot be resolved.
pub fn canonical(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path).map(|resolved| plain(&resolved))
}

/// [`canonical`], or `path` as given when it cannot be resolved (it does not exist yet).
#[must_use]
pub fn canonical_or_given(path: &Path) -> PathBuf {
    canonical(path).unwrap_or_else(|_| plain(path))
}

/// `path` without the verbatim prefix of a Windows drive path. A verbatim UNC or device path
/// (`\\?\UNC\…`, `\\?\GLOBALROOT\…`) has no plain drive spelling and is returned as it is, as is
/// every path on a system without the prefix.
#[must_use]
pub fn plain(path: &Path) -> PathBuf {
    path.to_str()
        .and_then(|text| text.strip_prefix(VERBATIM))
        .filter(|rest| is_drive(rest))
        .map_or_else(|| path.to_path_buf(), PathBuf::from)
}

/// The folder a project's discipline checkers, their builds and their answers live in.
#[must_use]
pub fn discipline_cache(project: &Path) -> PathBuf {
    project.join(".osprey").join("cache").join("disciplines")
}

/// [`plain`] as the text a request or command line carries.
#[must_use]
pub fn plain_text(path: &Path) -> String {
    plain(path).to_string_lossy().into_owned()
}

// `C:` followed by nothing or a separator.
fn is_drive(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':')
        && bytes.get(2).is_none_or(|next| *next == b'\\' || *next == b'/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verbatim_drive_path_loses_only_its_prefix() {
        let cases = [
            (r"\\?\C:\proj", r"C:\proj"),
            (r"\\?\d:\a\b.ospml", r"d:\a\b.ospml"),
            (r"\\?\C:", "C:"),
            (r"\\?\UNC\server\share\x", r"\\?\UNC\server\share\x"),
            (r"\\?\GLOBALROOT\Device\x", r"\\?\GLOBALROOT\Device\x"),
            (r"\\?\CC:\x", r"\\?\CC:\x"),
            (r"C:\already\plain", r"C:\already\plain"),
            ("/home/user/proj", "/home/user/proj"),
            ("relative/src", "relative/src"),
            ("", ""),
        ];
        for (given, expected) in cases {
            assert_eq!(plain(Path::new(given)), PathBuf::from(expected), "{given}");
            assert_eq!(plain_text(Path::new(given)), expected, "{given}");
        }
    }

    #[test]
    fn a_canonical_path_is_plain_resolved_and_existing() -> std::io::Result<()> {
        let here = canonical(Path::new("."))?;
        assert!(here.is_absolute(), "{}", here.display());
        assert!(!here.to_string_lossy().starts_with(VERBATIM), "{}", here.display());
        assert_eq!(canonical(&here.join("src").join(".."))?, here);
        assert!(canonical(Path::new("no/such/folder/anywhere")).is_err());
        let missing = Path::new("no/such/folder/anywhere");
        assert_eq!(canonical_or_given(missing), missing.to_path_buf());
        assert_eq!(canonical_or_given(Path::new(".")), here);
        Ok(())
    }
}
