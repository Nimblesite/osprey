//! Keep shipped examples and the language corpus free of unused-binding noise.

use std::{fs, path::Path, path::PathBuf};

/// A walk that finds nothing would pass silently, so the corpus has a floor.
const SOURCE_FLOORS: [(&str, usize); 2] = [("examples", 59), ("tests", 213)];

#[test]
fn shipped_sources_have_no_unused_bindings() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut warnings = Vec::new();
    for path in shipped_sources(&root) {
        let name = display_name(&root, &path);
        let source = fs::read_to_string(&path).expect("read example");
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
            let sources = example_sources(&root.join(directory));
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

/// Every Osprey source under `dir`, in a stable order.
fn example_sources(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect_sources(dir, &mut found);
    found.sort();
    found
}

fn collect_sources(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sources(&path, found);
        } else if is_osprey_source(&path) {
            found.push(path);
        }
    }
}

fn is_osprey_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("osp" | "ospml")
    )
}
