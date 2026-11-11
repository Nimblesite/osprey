//! The repository root, for the integration tests that build fixtures from it.

use std::path::{Path, PathBuf};

/// The repository root, from this crate's manifest directory. Left
/// un-canonicalized on purpose: the `..` components resolve the same way for
/// every consumer.
pub(crate) fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}
