//! One project analysis, shared by the open files that answer the same edit.
//! Implements [LSP-PROJECT-BATCH].

use osprey_project::{AssembledProject, ProjectConfig, ProjectError, SourceFile};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

/// Everything that decides the answer: the manifest settings and the text of
/// every file the project assembles. Compared exactly — a digest would let two
/// different projects collide and hand a reader another project's errors.
#[derive(Debug, PartialEq, Eq)]
struct Inputs {
    config: ProjectConfig,
    sources: Vec<(PathBuf, osprey_syntax::Flavor, String)>,
}

impl Inputs {
    fn of(config: &ProjectConfig, sources: &[SourceFile]) -> Self {
        Self {
            config: config.clone(),
            sources: sources
                .iter()
                .map(|file| (file.path.clone(), file.flavor, file.source.clone()))
                .collect(),
        }
    }
}

/// One project's assembly, type errors and warnings, shared by every open file
/// of that project. Each open sibling asks for its own diagnostics after every
/// keystroke, and each answer needs the whole project assembled, checked and
/// its warnings proved: without this, N open files cost N of those per edit.
/// The key is the whole input, so an edit to any buffer — or to the manifest —
/// misses and recomputes. The reuse is between siblings answering the SAME
/// edit; unchanged inputs can also be reused on later requests.
#[derive(Debug)]
pub(crate) struct Checked {
    pub(crate) assembled: Result<AssembledProject, Vec<ProjectError>>,
    pub(crate) errors: Vec<osprey_types::TypeError>,
    pub(crate) warnings: crate::warning_actions::Warnings,
}

/// Each engine owns its memo: unrelated servers and parallel tests cannot
/// evict the project while its open siblings are being refreshed.
#[derive(Debug, Default)]
pub(crate) struct ProjectCache {
    latest: Mutex<Option<(Inputs, Arc<Checked>)>>,
    #[cfg(test)]
    computed: std::sync::atomic::AtomicUsize,
}

impl ProjectCache {
    pub(crate) fn checked(&self, config: &ProjectConfig, sources: &[SourceFile]) -> Arc<Checked> {
        let inputs = Inputs::of(config, sources);
        // The entry is replaced whole, so a poisoned mutex cannot expose a
        // partially written analysis. Hold it through computation to avoid
        // duplicate work if two requests arrive together.
        let mut latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((_, value)) = latest.as_ref().filter(|(seen, _)| *seen == inputs) {
            return Arc::clone(value);
        }
        let value = Arc::new(analyse(config, sources));
        #[cfg(test)]
        let _ = self
            .computed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *latest = Some((inputs, Arc::clone(&value)));
        value
    }

    #[cfg(test)]
    pub(crate) fn analyses(&self) -> usize {
        self.computed.load(std::sync::atomic::Ordering::Relaxed)
    }
}

fn analyse(config: &ProjectConfig, sources: &[SourceFile]) -> Checked {
    let assembled = osprey_project::assemble(config, sources);
    let errors = match &assembled {
        Ok(project) => osprey_types::check_program(&project.program),
        Err(_) => Vec::new(),
    };
    // A file with type errors reports no warnings, so proving them would be
    // work nobody reads.
    let warnings = match (&assembled, errors.is_empty()) {
        (Ok(project), true) => crate::warning_actions::Warnings::of(&project.program),
        _ => crate::warning_actions::Warnings::none(),
    };
    Checked {
        assembled,
        errors,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectCache;
    use osprey_project::ProjectConfig;
    use std::path::{Path, PathBuf};

    /// A project on disk under a name no other test uses.
    fn project(suffix: &str, namespace: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "osprey-lsp-cache-{}-{:?}-{suffix}",
            std::process::id(),
            std::thread::current().id()
        ));
        let sources = root.join("src");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&sources).expect("create the project");
        std::fs::write(
            root.join("osprey.toml"),
            format!(
                "[project]\nname = \"cache\"\nsource_roots = [\"src\"]\ndefault_namespace = \"{namespace}\"\nentry = \"src/main.ospml\"\n"
            ),
        )
        .expect("write the manifest");
        std::fs::write(sources.join("main.ospml"), "main () = print \"one\"\n")
            .expect("write main");
        std::fs::write(sources.join("helper.ospml"), "double x = x * 2\n").expect("write helper");
        root
    }

    fn loaded(root: &Path) -> (ProjectConfig, Vec<osprey_project::SourceFile>) {
        match osprey_project::load(root) {
            Ok(loaded) => loaded,
            Err(errors) => panic!("the temporary project must load: {errors:?}"),
        }
    }

    /// Every open file of a project republishes on every keystroke, and each
    /// answer needs the whole project assembled, checked and proved. Measured
    /// on `examples/projects/modules`, one such answer takes about 6 s, so the
    /// tenth open file must not buy a tenth of those.
    #[test]
    fn open_siblings_of_one_project_share_a_single_analysis() {
        let root = project("siblings", "batch");
        let (config, sources) = loaded(&root);
        let cache = ProjectCache::default();
        for _ in 0..3 {
            let _ = cache.checked(&config, &sources);
        }
        let performed = cache.analyses();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            performed, 1,
            "three open siblings analysed {performed} times"
        );
    }

    /// The manifest decides namespaces, entry and flavor, so an edit to it
    /// changes the answer for buffers whose own text never moved. Keying on the
    /// sources alone republished the previous project's diagnostics.
    #[test]
    fn a_manifest_edit_is_not_answered_from_the_previous_analysis() {
        let root = project("manifest", "before");
        let (config, sources) = loaded(&root);
        let cache = ProjectCache::default();
        let first = cache.checked(&config, &sources);
        let renamed = ProjectConfig {
            default_namespace: Some("after".to_owned()),
            ..config
        };
        let second = cache.checked(&renamed, &sources);
        let performed = cache.analyses();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            performed, 2,
            "the renamed project was analysed {performed} times"
        );
        assert!(
            !std::sync::Arc::ptr_eq(&first, &second),
            "a manifest edit reused the previous analysis"
        );
    }
}
