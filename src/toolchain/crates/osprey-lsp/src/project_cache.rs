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
    /// The assembled program's types and where it declares each function,
    /// inferred when an editor view first asks ([LSP-HOVER-PROJECT]).
    typed: std::sync::OnceLock<(Arc<osprey_types::ProgramTypes>, crate::inferred::Functions)>,
}

/// Collects where an assembled program declares each function.
#[derive(Default)]
struct Declared(std::collections::BTreeMap<(u32, u32), String>);

impl osprey_ast::AstVisitor for Declared {
    fn statement(&mut self, statement: &osprey_ast::Stmt) {
        if let osprey_ast::Stmt::Function {
            name,
            position: Some(at),
            ..
        } = statement
        {
            let _ = self.0.insert((at.line, at.column), name.clone());
        }
    }
}

impl Checked {
    /// The project's answers for the source file at `path`; `None` when the
    /// project does not assemble or does not contain it.
    pub(crate) fn inferred(&self, path: &std::path::Path) -> Option<crate::inferred::Inferred> {
        let project = self.assembled.as_ref().ok()?;
        let source = project
            .sources
            .iter()
            .find(|source| crate::workspace::same_path(&source.path, path))?;
        let (types, functions) = self.typed.get_or_init(|| {
            let mut declared = Declared::default();
            osprey_ast::walk_program(&project.program, &mut declared);
            (
                Arc::new(osprey_types::infer_program(&project.program)),
                Arc::new(declared.0),
            )
        });
        Some(crate::inferred::Inferred::placed(
            Arc::clone(types),
            Arc::clone(functions),
            source.global_line(1).saturating_sub(1),
        ))
    }
}

/// The latest analysis of one project root, locked on its own.
type Slot = Arc<Mutex<Option<(Inputs, Arc<Checked>)>>>;

/// Each engine owns its memo: unrelated servers and parallel tests cannot
/// evict the project while its open siblings are being refreshed.
#[derive(Debug, Default)]
pub(crate) struct ProjectCache {
    latest: Mutex<std::collections::BTreeMap<PathBuf, Slot>>,
    sources: Mutex<crate::project_sources::SourceCache>,
    #[cfg(test)]
    computed: std::sync::atomic::AtomicUsize,
}

impl ProjectCache {
    pub(crate) fn checked(
        &self,
        root: &std::path::Path,
        config: &ProjectConfig,
        sources: &[SourceFile],
    ) -> Arc<Checked> {
        let inputs = Inputs::of(config, sources);
        let slot = Arc::clone(lock(&self.latest).entry(root.to_path_buf()).or_default());
        // The entry is replaced whole, so a poisoned mutex cannot expose a
        // partially written analysis. One lock per project, held through the
        // computation: siblings asking together share one analysis, while a
        // request in another project never waits for it.
        let mut latest = lock(&slot);
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

    pub(crate) fn sources(
        &self,
        uri: &str,
        vfs: Option<&lspkit::vfs::Vfs>,
        text: Option<&str>,
    ) -> Option<Result<crate::project_sources::Sources, Vec<ProjectError>>> {
        lock(&self.sources).load(uri, vfs, text)
    }

    pub(crate) fn view(&self, uri: &str, vfs: &lspkit::vfs::Vfs) -> crate::workspace::View {
        self.sources(uri, Some(vfs), None)
            .and_then(Result::ok)
            .map_or_else(crate::workspace::View::default, |sources| sources.view(uri))
    }

    /// [`Self::view`] with the project's checked types, for the features that
    /// display one ([LSP-HOVER-PROJECT]).
    pub(crate) fn typed_view(&self, uri: &str, vfs: &lspkit::vfs::Vfs) -> crate::workspace::View {
        let Some(Ok(sources)) = self.sources(uri, Some(vfs), None) else {
            return crate::workspace::View::default();
        };
        let mut view = sources.view(uri);
        if view.current.is_some() {
            view.checked = Some(self.checked(&sources.root, &sources.config, &sources.files));
        }
        view
    }

    pub(crate) fn opened(&self, uri: &str, open: bool) {
        lock(&self.sources).opened(uri, open);
    }

    pub(crate) fn documents(&self) -> Vec<lspkit::vfs::DocumentUri> {
        lock(&self.sources).documents()
    }

    pub(crate) fn affected(&self, document: &str, changed: &str) -> bool {
        lock(&self.sources).affected(document, changed)
    }

    #[cfg(test)]
    pub(crate) fn parses(&self) -> usize {
        lock(&self.sources).parses
    }

    #[cfg(test)]
    pub(crate) fn analyses(&self) -> usize {
        self.computed.load(std::sync::atomic::Ordering::Relaxed)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(error) => PoisonError::into_inner(error),
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
        (Ok(project), true) => crate::warning_actions::Warnings::of_project(project),
        _ => crate::warning_actions::Warnings::none(),
    };
    Checked {
        assembled,
        errors,
        warnings,
        typed: std::sync::OnceLock::new(),
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

    /// Each uri's project loaded and checked through `cache`, in order — the
    /// alternating entries are what prove separate cache slots, not repeats.
    fn checked_in_turn(
        cache: &ProjectCache,
        uris: &[&str],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for uri in uris {
            let sources = cache
                .sources(uri, None, None)
                .ok_or("missing project")?
                .map_err(|errors| format!("{errors:?}"))?;
            let checked = cache.checked(&sources.root, &sources.config, &sources.files);
            assert!(checked.assembled.is_ok(), "{checked:?}");
        }
        Ok(())
    }

    /// Every open file of a project republishes on every keystroke, and each
    /// answer needs the whole project assembled, checked and proved. Measured
    /// on `src/osprey/examples/projects/modules`, one such answer takes about 6 s, so the
    /// tenth open file must not buy a tenth of those.
    #[test]
    fn open_siblings_of_one_project_share_a_single_analysis() {
        let root = project("siblings", "batch");
        let (config, sources) = loaded(&root);
        let cache = ProjectCache::default();
        for _ in 0..3 {
            let _ = cache.checked(&root, &config, &sources);
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
        let first = cache.checked(&root, &config, &sources);
        let renamed = ProjectConfig {
            default_namespace: Some("after".to_owned()),
            ..config
        };
        let second = cache.checked(&root, &renamed, &sources);
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

    #[test]
    fn separate_projects_keep_their_analysis_when_requests_alternate(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let first = crate::test_support::ProjectFixture::new("osp")?;
        let second = crate::test_support::ProjectFixture::new("osp")?;
        let a = first.write("main", "fn main() = print(\"one\")\n")?;
        let b = second.write("main", "fn main() = print(\"two\")\n")?;
        let cache = ProjectCache::default();
        checked_in_turn(&cache, &[&a, &b, &a, &b])?;
        assert_eq!((cache.parses(), cache.analyses()), (2, 2));
        Ok(())
    }

    #[test]
    fn nested_projects_do_not_share_the_first_sources_cache_slot(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for relative in ["src/aaa", "nested"] {
            nested_project_cache(relative)?;
        }
        Ok(())
    }

    fn nested_project_cache(relative: &str) -> Result<(), Box<dyn std::error::Error>> {
        let outer = crate::test_support::ProjectFixture::new("osp")?;
        let entry = outer.write("main", "fn main() = print(\"outer\")\n")?;
        let nested = outer.root.join(relative);
        std::fs::create_dir_all(nested.join("src"))?;
        std::fs::write(nested.join("osprey.toml"), "[project]\nsource_roots = [\"src\"]\ndefault_namespace = \"nested\"\nentry = \"src/helper.osp\"\n")?;
        let helper = nested.join("src/helper.osp");
        std::fs::write(&helper, "fn helper(x) = x\n")?;
        let uri = lspkit_server::uri::path_to_uri(&helper)?;
        let cache = ProjectCache::default();
        checked_in_turn(&cache, &[&entry, &uri, &entry, &uri])?;
        assert_eq!(cache.analyses(), 2);
        assert_eq!(cache.parses(), 2);
        Ok(())
    }

    /// A type fixed only by ANOTHER file of the project reaches hover: the
    /// editor answers from the assembled project, as the compiler does, and
    /// not from the open file alone, which cannot see `ui::LineTo` and so
    /// showed every type it constrains as a hole (#391).
    /// Implements [LSP-HOVER-PROJECT].
    #[test]
    fn hover_infers_from_the_whole_project() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = crate::test_support::ProjectFixture::new("ospml")?;
        let _ui = fixture.write("ui", "namespace ui\n\ntype PathStep =\n    LineTo\n        toX : float\n        toY : float\n    Close\n")?;
        let _main = fixture.write(
            "main",
            "main () = print \"${listLength (painting::Logo::steps ())}\"\n",
        )?;
        let logo = "namespace painting\n\nmodule Logo\n    segment row = match row\n        [x, y, ..._rest] => ui::LineTo(toX = x, toY = y)\n        _ => ui::Close\n\n    export steps () =\n        first = [1.0, 2.0]\n        mapList [first] segment\n";
        let uri = fixture.write("logo", logo)?;
        for (line, needle, present) in [
            (4, "x, y", "x : float"),
            (4, "y, ...", "y : float"),
            (4, "_rest", "_rest : List<float>"),
            (4, "x, toY", "x : float"),
            (4, "y)", "y : float"),
            (3, "row =", "row : List<float>"),
            (3, "segment", "List<float> -> ui::PathStep"),
            (8, "first", "first : List<float>"),
        ] {
            let col = crate::test_support::col_of(logo, line, needle);
            let row = u32::try_from(line)?;
            let md = crate::test_support::hover(
                logo,
                &uri,
                row,
                col,
                lspkit::vfs::PositionEncoding::Utf16,
            )
            .ok_or_else(|| format!("no hover for `{needle}` on line {line}"))?;
            crate::testkit::contents(&md, &[present, "```osprey-ml"], &["_ ->", ": _", "__osp"]);
        }
        // Signature help and completion display the same types, so they
        // answer from the same project.
        let enc = lspkit::vfs::PositionEncoding::Utf16;
        let at = crate::test_support::col_of(logo, 9, "segment");
        let help = crate::test_support::signature_help(logo, &uri, 9, at, enc)
            .ok_or("no signature help for `segment`")?;
        crate::testkit::contents(
            &help.label,
            &["List<float> -> ui::PathStep"],
            &["_", "__osp"],
        );
        let items = crate::test_support::completion(logo, &uri, 9, at, enc);
        let segment = items
            .iter()
            .find(|item| item.label.ends_with("segment"))
            .ok_or("no completion for `segment`")?;
        let detail = format!("{:?}", segment.detail);
        crate::testkit::contents(&detail, &["List<float>", "PathStep"], &["__osp"]);
        Ok(())
    }
}
