//! Suite discovery for `osprey test`. Implements [TESTING-FILE-CONVENTION]
//! and [TESTING-PROJECT-SUITE] (docs/specs/tooling/TestRunner.md).

use std::path::{Path, PathBuf};

/// A single file runs as-is regardless of naming, and a directory holding an
/// `osprey.toml` is one project suite ([TESTING-PROJECT-SUITE]); any other
/// directory is searched for `[TESTING-FILE-CONVENTION]` files, sorted for a
/// deterministic run order.
pub(crate) fn discover(path: &Path) -> Vec<PathBuf> {
    if path.is_file() || path.join("osprey.toml").is_file() {
        return vec![path.to_path_buf()];
    }
    let mut out = Vec::new();
    visit(path, &mut out);
    out.sort();
    out
}

fn is_test_file(path: &Path) -> bool {
    matches_file_name(path, |name| {
        name.ends_with(".test.osp") || name.ends_with(".test.ospml")
    })
}

fn skipped_dir_entry(path: &Path) -> bool {
    matches_file_name(path, |name| {
        name.starts_with('.') || name == "target" || name == "node_modules"
    })
}

fn matches_file_name(path: &Path, predicate: impl FnOnce(&str) -> bool) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(predicate)
}

fn visit(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `file_type()` does not follow symlinks, so a symlinked directory is
        // never descended into — a link cycle cannot recurse forever.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if skipped_dir_entry(&path) {
            continue;
        }
        if file_type.is_dir() {
            visit(&path, out);
        } else if file_type.is_file() && is_test_file(&path) {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_are_classified() {
        // [TESTING-FILE-CONVENTION] both flavor suffixes are exact.
        let file_rule: fn(&Path) -> bool = is_test_file;
        let skip_rule: fn(&Path) -> bool = skipped_dir_entry;
        let cases = [
            (file_rule, "money.test.osp", true),
            (file_rule, "json.test.ospml", true),
            (file_rule, "money.osp", false),
            (file_rule, "notes.txt", false),
            (skip_rule, "proj/.git", true),
            (skip_rule, "proj/target", true),
            (skip_rule, "proj/node_modules", true),
            (skip_rule, "proj/src", false),
        ];
        for (rule, path, expected) in cases {
            assert_eq!(rule(Path::new(path)), expected, "{path}");
        }
    }

    #[test]
    fn discover_returns_a_single_file_as_is_and_walks_directories_sorted() {
        let root = std::env::temp_dir().join(format!("osprey-test-cmd-{}", std::process::id()));
        let nested = root.join("nested");
        let skipped = root.join("target");
        std::fs::create_dir_all(&nested).expect("mkdir nested");
        std::fs::create_dir_all(&skipped).expect("mkdir target");
        std::fs::write(root.join("b.test.osp"), "").expect("write b");
        std::fs::write(nested.join("a.test.ospml"), "").expect("write a");
        std::fs::write(root.join("ignore.osp"), "").expect("write ignore");
        std::fs::write(skipped.join("c.test.osp"), "").expect("write skipped");

        // A single file is returned verbatim, no matter its name.
        let plain = root.join("ignore.osp");
        assert_eq!(discover(&plain), vec![plain.clone()]);

        // A directory walk finds only *.test.* files, skips target/, and sorts.
        let found = discover(&root);
        assert_eq!(
            found,
            vec![root.join("b.test.osp"), nested.join("a.test.ospml")]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        );

        // An unreadable directory yields nothing rather than panicking.
        assert_eq!(
            discover(&root.join("does-not-exist")),
            Vec::<PathBuf>::new()
        );

        // A directory holding a manifest is one project suite, never walked:
        // its own `*.test.*` files are sources of that suite, not suites.
        std::fs::write(root.join("osprey.toml"), "").expect("write manifest");
        assert_eq!(discover(&root), vec![root.clone()]);

        let _ = std::fs::remove_dir_all(&root);
    }
}
