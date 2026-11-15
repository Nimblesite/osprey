//! Math sources ([MATH-CODE-SOURCES], `docs/specs/tooling/MathToCode.md`).
//!
//! A `.lean` file under a source root holds mathematics. `osprey-math`
//! generates its module, which is stored under `.osprey/math/` and is what
//! the project compiles ([MATH-CODE-UNTRACKED]).

use crate::{ProjectError, SourceFile};
use osprey_syntax::Flavor;
use std::path::{Component, Path, PathBuf};

/// Where generated modules are stored, relative to the owning project.
const GENERATED: &str = ".osprey/math";

/// Whether `path` is a math source.
pub(crate) fn is_math(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == osprey_math::EXTENSION)
}

/// Generate, store and parse the module of one math source. A refusal is
/// reported at the `.lean` path; anything wrong inside the generated module
/// is reported at the generated path.
///
/// # Errors
/// The refusal of [MATH-CODE-REFUSALS], or a failure to store the module.
pub(crate) fn parse(path: &Path, source: &str) -> Result<SourceFile, Vec<ProjectError>> {
    let text = osprey_math::generate(path, source).map_err(|refusal| {
        vec![ProjectError {
            message: format!("{:?}: {}", refusal.code, refusal.message),
            path: Some(path.to_path_buf()),
            line: Some(refusal.line),
            column: Some(refusal.column),
        }]
    })?;
    let generated = generated_path(path);
    store(&generated, &text).map_err(|error| vec![ProjectError::io(&generated, &error)])?;
    crate::parse_text(generated, text, Some(Flavor::Ml))
}

/// The project a source belongs to: the nearest directory above it that
/// holds a manifest, else the source's own directory.
fn owner(path: &Path) -> &Path {
    let parent = path.parent().unwrap_or(Path::new("."));
    parent
        .ancestors()
        .find(|directory| directory.join("osprey.toml").is_file())
        .unwrap_or(parent)
}

/// `<project>/.osprey/math/<source path>.ospml`.
fn generated_path(path: &Path) -> PathBuf {
    let owner = owner(path);
    let relative = path.strip_prefix(owner).unwrap_or(path);
    let mut name = owner.join(GENERATED).join(relative).into_os_string();
    name.push(".ospml");
    PathBuf::from(name)
}

/// Write `text` only when it differs from what is stored, and keep the
/// generated tree out of version control by default.
fn store(generated: &Path, text: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(generated).is_ok_and(|stored| stored == text) {
        return Ok(());
    }
    generated.parent().map_or(Ok(()), std::fs::create_dir_all)?;
    let ignore = generated
        .ancestors()
        .find(|directory| directory.ends_with(GENERATED))
        .map(|directory| directory.join(".gitignore"));
    if let Some(ignore) = ignore.filter(|ignore| !ignore.exists()) {
        std::fs::write(ignore, "*\n")?;
    }
    std::fs::write(generated, text)
}

/// The folder a source counts as living in for layout advice: a generated
/// module lives beside the `.lean` source it came from, not in the store.
pub(crate) fn folder(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let parts: Vec<Component<'_>> = parent.components().collect();
    let store: Vec<Component<'_>> = Path::new(GENERATED).components().collect();
    let at = parts
        .windows(store.len())
        .position(|window| window == store.as_slice());
    Some(match at {
        Some(at) => parts
            .iter()
            .take(at)
            .chain(parts.iter().skip(at + store.len()))
            .collect(),
        None => parent.to_path_buf(),
    })
}
