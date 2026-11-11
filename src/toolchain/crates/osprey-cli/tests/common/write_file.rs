//! Writing project fixture files, parents first.

use std::path::Path;

/// Writes `text` to `path`, creating its parent directories.
#[expect(
    clippy::expect_used,
    reason = "fixture setup failures must fail the test immediately"
)]
pub(crate) fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create project directory");
    }
    std::fs::write(path, text).expect("write project source");
}
