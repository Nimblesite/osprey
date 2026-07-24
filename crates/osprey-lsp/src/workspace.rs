//! The project a document belongs to.
//!
//! Every feature used to take only the open buffer's text, so a symbol declared
//! in a sibling file was invisible: hover said nothing, go-to-definition
//! returned nothing, and completion never offered it — even though the compiler
//! links the two files into one program. This module supplies the missing
//! context by reusing the **same** project loader the CLI and the diagnostics
//! path already use (`osprey_project::load`), so the editor's idea of "the
//! program" cannot drift from the compiler's. Implements [LSP-WORKSPACE].

use std::path::{Path, PathBuf};

use osprey_ast::Program;

/// A project file other than the one under the cursor.
#[derive(Debug, Clone)]
pub struct Sibling {
    /// The file's `file://` URI, for locations sent back to the editor.
    pub uri: String,
    /// The file's text, for occurrence scanning.
    pub source: String,
    /// The parsed program, for symbol collection.
    pub program: Program,
}

/// Every *other* source file of the project that claims `uri`.
///
/// Empty for a standalone script — a file under no `osprey.toml` has no
/// siblings, which is the common case and costs one `is_file` check per
/// ancestor. Loading is deliberately not cached: the manifest's own file set is
/// the source of truth, and a stale index answering "no such symbol" is worse
/// than re-reading a handful of files.
#[must_use]
pub fn siblings(uri: &str) -> Vec<Sibling> {
    let Some(file) = file_path(uri) else {
        return Vec::new();
    };
    let Some(root) = project_root(&file) else {
        return Vec::new();
    };
    let Ok((_, sources)) = osprey_project::load(&root) else {
        return Vec::new();
    };
    sources
        .into_iter()
        .filter(|source| !same_path(&source.path, &file))
        .map(|source| Sibling {
            uri: uri_of(&source.path),
            source: source.source,
            program: source.program,
        })
        .collect()
}

/// The directory of the nearest enclosing `osprey.toml`, if any.
#[must_use]
pub fn project_root(file: &Path) -> Option<PathBuf> {
    file.parent()?
        .ancestors()
        .find(|directory| directory.join("osprey.toml").is_file())
        .map(Path::to_path_buf)
}

/// Whether two paths name the same file, resolving `..` and symlinks when the
/// filesystem can. A path that cannot be canonicalized (an unsaved buffer)
/// compares literally rather than erroring.
#[must_use]
pub fn same_path(left: &Path, right: &Path) -> bool {
    let normalize =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    normalize(left) == normalize(right)
}

/// The filesystem path a `file://` URI names, percent-decoded.
#[must_use]
pub fn file_path(uri: &str) -> Option<PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    let decoded = percent_decode(encoded)?;
    #[cfg(windows)]
    let decoded = decoded
        .strip_prefix('/')
        .filter(|path| path.as_bytes().get(1) == Some(&b':'))
        .unwrap_or(&decoded)
        .to_string();
    Some(PathBuf::from(decoded))
}

/// The `file://` URI for `path` — the inverse of [`file_path`] for the ASCII
/// paths a project loader yields.
#[must_use]
pub fn uri_of(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn percent_decode(encoded: &str) -> Option<String> {
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte == b'%' {
            let high = hex(*bytes.get(index.saturating_add(1))?)?;
            let low = hex(*bytes.get(index.saturating_add(2))?)?;
            decoded.push((high << 4) | low);
            index = index.saturating_add(3);
        } else {
            decoded.push(byte);
            index = index.saturating_add(1);
        }
    }
    String::from_utf8(decoded).ok()
}

const fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uri_round_trips_through_a_path_including_escapes() {
        let path = file_path("file:///tmp/my%20app/main.osp").expect("path");
        assert_eq!(path, PathBuf::from("/tmp/my app/main.osp"));
        assert_eq!(uri_of(Path::new("/tmp/a.osp")), "file:///tmp/a.osp");
        // A non-file scheme and a truncated escape are refused, not guessed.
        assert!(file_path("untitled:Untitled-1").is_none());
        assert!(file_path("file:///a%2").is_none());
    }

    #[test]
    fn a_file_under_no_manifest_has_no_siblings() {
        // The standalone case must stay free: no project, no work, no answers
        // invented from files that are not part of any program.
        assert!(siblings("file:///nonexistent/scratch.osp").is_empty());
        assert!(siblings("untitled:Untitled-1").is_empty());
        assert!(project_root(Path::new("/nonexistent/scratch.osp")).is_none());
    }
}
