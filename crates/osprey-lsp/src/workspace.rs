//! The project a document belongs to.
//!
//! URI/path conversion and project-root discovery are shared by the editor's
//! live source snapshots and diagnostic mapping. Implements [LSP-WORKSPACE].

use std::path::{Path, PathBuf};

use osprey_ast::Program;

/// A project file other than the one under the cursor.
#[derive(Debug, Clone)]
pub struct Sibling {
    /// The file's `file://` URI, for locations sent back to the editor.
    pub(crate) uri: String,
    /// The file's text, for occurrence scanning.
    pub(crate) source: String,
    /// The parsed program, for symbol collection.
    pub(crate) program: Program,
    pub(crate) flavor: osprey_syntax::Flavor,
}

/// The directory of the nearest enclosing `osprey.toml`, if any.
#[must_use]
pub(crate) fn project_root(file: &Path) -> Option<PathBuf> {
    file.parent()?
        .ancestors()
        .find(|directory| directory.join("osprey.toml").is_file())
        .map(Path::to_path_buf)
}

/// Whether two paths name the same file, resolving `..` and symlinks when the
/// filesystem can. A path that cannot be canonicalized (an unsaved buffer)
/// compares literally rather than erroring.
#[must_use]
pub(crate) fn same_path(left: &Path, right: &Path) -> bool {
    let normalize =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    normalize(left) == normalize(right)
}

/// The filesystem path a `file://` URI names, percent-decoded.
#[must_use]
pub(crate) fn file_path(uri: &str) -> Option<PathBuf> {
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

/// The percent-encoded URI for an absolute UTF-8 path, including spaces and
/// non-ASCII names. Unrepresentable paths have no editor location.
#[must_use]
pub(crate) fn uri_of(path: &Path) -> Option<String> {
    lspkit_server::uri::path_to_uri(path).ok()
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
        let (plain, uri, escaped) = if cfg!(windows) {
            (
                "C:\\tmp\\a.osp",
                "file:///C:/tmp/a.osp",
                "C:\\tmp\\my app\\#λ(main).osp",
            )
        } else {
            (
                "/tmp/a.osp",
                "file:///tmp/a.osp",
                "/tmp/my app/#λ(main).osp",
            )
        };
        assert_eq!(uri_of(Path::new(plain)).as_deref(), Some(uri));
        let escaped = PathBuf::from(escaped);
        assert_eq!(
            uri_of(&escaped).as_deref().and_then(file_path),
            Some(escaped)
        );
        // A non-file scheme and a truncated escape are refused, not guessed.
        assert!(file_path("untitled:Untitled-1").is_none());
        assert!(file_path("file:///a%2").is_none());
    }

    #[test]
    fn a_file_under_no_manifest_has_no_siblings() {
        // The standalone case must stay free: no project, no work, no answers
        // invented from files that are not part of any program.
        assert!(crate::test_support::view("file:///nonexistent/scratch.osp")
            .siblings
            .is_empty());
        assert!(crate::test_support::view("untitled:Untitled-1")
            .siblings
            .is_empty());
        assert!(project_root(Path::new("/nonexistent/scratch.osp")).is_none());
    }
}

/// A feature request's live project inputs; standalone documents use defaults.
#[derive(Debug, Default)]
pub(crate) struct View {
    pub(crate) siblings: Vec<Sibling>,
    pub(crate) current: Option<osprey_project::SourceFile>,
    pub(crate) configured: Option<osprey_syntax::Flavor>,
}

impl View {
    pub(crate) fn flavor(&self, path: &str, text: &str) -> osprey_syntax::Flavor {
        match osprey_syntax::resolve_flavor(self.configured, path, text) {
            Ok(flavor) => flavor,
            Err(_) => osprey_syntax::Flavor::Default,
        }
    }

    pub(crate) fn program(&self, path: &str, text: &str) -> Program {
        self.parsed(path, text).program
    }

    pub(crate) fn parsed(&self, path: &str, text: &str) -> osprey_syntax::Parsed {
        match self.current.as_ref().filter(|file| file.source == text) {
            Some(file) => osprey_syntax::Parsed {
                program: file.program.clone(),
                errors: Vec::new(),
                flavor: file.flavor,
            },
            None => osprey_syntax::parse_program_with_flavor(text, self.flavor(path, text)),
        }
    }
}
