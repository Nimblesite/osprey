//! Test-only helpers shared by the editor-feature unit tests.
//!
//! Every feature test opens the same way: parse a snippet, fail loudly on any
//! syntax error, then ask one editor question about a cursor position. Each
//! `mod tests` used to carry its own copy of that preamble, so the copies
//! drifted apart on their failure messages while asserting the same thing.

use osprey_ast::Program;

/// The feature tests' shared document: one fully annotated function and one
/// call. `satAdd` is total ([ARITH-EFFECT-TOTAL-HELPERS]), so the file-scope
/// call needs no `Arith` policy and the program type-checks with exactly its
/// three redundant-annotation warnings.
pub(crate) const ADD_SRC: &str =
    "fn add(a: int, b: int) -> int = satAdd(a, b)\nlet total = add(1, 2)\n";

/// Parse `src`, asserting it is syntactically valid. A snippet the PARSER
/// rejects must never be scored as a passing editor view.
pub(crate) fn parsed(src: &str) -> Program {
    let parsed = osprey_syntax::parse_program(src);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    parsed.program
}

/// The symbol view of a valid snippet — the rendering the inference-view tests
/// inspect.
pub(crate) fn symbols(src: &str) -> String {
    crate::analysis::symbols_json(&parsed(src))
}

/// The 0-based column just inside the first occurrence of `needle` on 0-based
/// `line` of `src` — a cursor position over that word.
pub(crate) fn col_of(src: &str, line: usize, needle: &str) -> u32 {
    let text = src.lines().nth(line).expect("line exists");
    let at = text.find(needle).expect("needle on line");
    u32::try_from(at).expect("column fits") + 1
}

/// An isolated project removed even when a regression assertion fails.
pub(crate) struct ProjectFixture {
    pub(crate) root: std::path::PathBuf,
    extension: String,
}

impl ProjectFixture {
    pub(crate) fn new(extension: &str) -> std::io::Result<Self> {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "osprey-live-{}-{:?}-{extension}-{id}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::write(root.join("osprey.toml"), format!("[project]\nsource_roots = [\"src\"]\ndefault_namespace = \"live\"\nentry = \"src/main.{extension}\"\n"))?;
        Ok(Self {
            root,
            extension: extension.to_string(),
        })
    }

    pub(crate) fn write(&self, name: &str, source: &str) -> std::io::Result<String> {
        let path = self.root.join(format!("src/{name}.{}", self.extension));
        std::fs::write(&path, source)?;
        lspkit_server::uri::path_to_uri(&path).map_err(std::io::Error::other)
    }
}

impl Drop for ProjectFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn view(uri: &str) -> crate::workspace::View {
    let vfs = lspkit_vfs::Vfs::new(lspkit_vfs::PositionEncoding::Utf16);
    crate::project_cache::ProjectCache::default().view(uri, &vfs)
}

macro_rules! disk_feature {
    ($module:ident::$name:ident -> $result:ty $(, $extra:ident: $ty:ty)?) => {
        pub(crate) fn $name(text: &str, uri: &str, line: u32, character: u32, encoding: lspkit_vfs::PositionEncoding $(, $extra: $ty)?) -> $result {
            crate::$module::$name(text, uri, line, character, encoding $(, $extra)?, &view(uri))
        }
    };
}

disk_feature!(features::definition -> Vec<crate::model::Location>);
disk_feature!(features::references -> Vec<crate::model::Location>, include_declaration: bool);
disk_feature!(features::signature_help -> Option<crate::model::SignatureInfo>);
disk_feature!(hover::hover -> Option<String>);
disk_feature!(complete::completion -> Vec<crate::model::CompletionItem>);
disk_feature!(effects::implementations -> Vec<crate::model::Location>);
