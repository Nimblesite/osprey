//! A test file that belongs to a project runs against that project's code.
//! Implements [TESTING-FILE-PROJECT] (docs/specs/tooling/TestRunner.md).

use super::CompilationInput;
use crate::test_discovery::is_test_file;
use osprey_project::ProjectError;
use std::path::{Path, PathBuf};

impl CompilationInput {
    /// The suite for `path` when it is a `*.test.*` file its nearest
    /// `osprey.toml` lists as a source: every non-test source of that project
    /// plus this file as the entry, so its imports resolve while sibling
    /// suites stay out of the run. `None` leaves the file standalone.
    pub(crate) fn owned_test_suite(path: &str) -> Option<Result<Self, Vec<ProjectError>>> {
        let file = Path::new(path);
        if !is_test_file(file) {
            return None;
        }
        let root = osprey_project::project_root(file)?;
        let (config, _) = osprey_project::discover(&root).ok()?;
        if !config.contains_source(&root, file) {
            return None;
        }
        let suite = canonical(file);
        Some(Self::load_suite(path, &root, &suite))
    }

    fn load_suite(path: &str, root: &Path, suite: &Path) -> Result<Self, Vec<ProjectError>> {
        let (mut config, sources) = osprey_project::load_selected(root, |source| {
            !is_test_file(source) || canonical(source) == suite
        })?;
        config.entry = sources
            .iter()
            .find(|source| canonical(&source.path) == suite)
            .map(|source| source.path.clone());
        Self::assembled_source(path, &config, &sources)
    }
}

/// One spelling per file, so `tests/./a.test.osp` and `tests/a.test.osp` match.
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
