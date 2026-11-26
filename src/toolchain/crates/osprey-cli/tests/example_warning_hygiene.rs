//! Keep shipped examples and the language corpus free of unused-binding noise.

use std::{fs, path::Path, path::PathBuf};

/// A walk that finds nothing would pass silently, so the corpus has a floor.
const SOURCE_FLOORS: [(&str, usize); 2] = [("src/osprey/examples", 59), ("src/osprey/tests", 213)];

#[test]
fn shipped_sources_have_no_unused_bindings() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let mut warnings = Vec::new();
    for path in shipped_sources(&root) {
        let name = display_name(&root, &path);
        let source = fs::read_to_string(&path).expect("read example");
        // A disciplined module is its checker's language until erased
        // [TYPES-DISCIPLINE-SEAM]; this lint is for Osprey's own bindings.
        if osprey_project::discipline::marker(&source)
            .is_some_and(|name| name != osprey_project::discipline::HM)
        {
            continue;
        }
        let parsed = osprey_syntax::parse_program_for_path(&name, &source);
        assert!(parsed.errors.is_empty(), "{name}: {:?}", parsed.errors);
        warnings.extend(
            osprey_types::unused_symbols(&parsed.program)
                .into_iter()
                .map(|symbol| format!("{name}: {}", symbol.warning.message)),
        );
    }
    assert!(
        warnings.is_empty(),
        "{} unused-binding warning(s) in shipped examples and tests:\n{}",
        warnings.len(),
        warnings.join("\n")
    );
}

fn shipped_sources(root: &Path) -> Vec<PathBuf> {
    SOURCE_FLOORS
        .into_iter()
        .flat_map(|(directory, minimum)| {
            let mut sources = osprey_project::native_source_files(&root.join(directory));
            sources.sort();
            assert!(
                sources.len() >= minimum,
                "{directory}: found {} source(s), expected at least {minimum}",
                sources.len()
            );
            sources
        })
        .collect()
}

fn display_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
