//! The compiler knows no discipline ([TYPES-DISCIPLINE-REGISTRY]): no code
//! under `crates/` names a registered discipline in a string literal or
//! mentions the theory words `tactic`, `theorem`, `refinement`, `lean4`.
//! Registered names are read from the reference packages under
//! `examples/disciplines/` and the first consumer of spec 0049, so adding a
//! discipline extends the search without touching this test.
//!
//! `kernel` is not searched: it is an Osprey keyword (the `kernel` handler
//! region of [STAGE-KERNEL]) and appears throughout the GPU pipeline; the
//! proof kernel of plan 0041 lives under `examples/`, never under `crates/`.
//! A registered name is searched in string literals only, because the words
//! `bounded` and `passthrough` are ordinary English (`bounded workers`, the
//! malloc passthrough of `--memory=none`) and a branch on a discipline name
//! compares against a literal. Comments and test modules are skipped: a
//! comment cannot branch, and test fixtures are allowed by the spec.

#![expect(
    clippy::expect_used,
    reason = "test assertions: an unreadable tree is a test failure, not a production panic"
)]

use std::path::{Path, PathBuf};

const THEORY_WORDS: [&str; 4] = ["tactic", "theorem", "refinement", "lean4"];
const FIRST_CONSUMER: &str = "lean4";
const SOURCE_EXTENSIONS: [&str; 3] = ["rs", "c", "h"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

/// `name = "<word>"` of every `[discipline]` table under `examples/disciplines`.
fn registered_names(root: &Path) -> Vec<String> {
    let mut names = vec![FIRST_CONSUMER.to_string()];
    let packages = root.join("examples/disciplines");
    for entry in std::fs::read_dir(&packages).into_iter().flatten().flatten() {
        let manifest = entry.path().join("osprey.toml");
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            names.extend(discipline_name(&text));
        }
    }
    names
}

fn discipline_name(manifest: &str) -> Option<String> {
    let table = manifest.split("[discipline]").nth(1)?;
    table
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("name"))
        .and_then(|rest| rest.split('"').nth(1))
        .map(ToString::to_string)
}

fn is_test_file(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.contains("/tests/") || text.ends_with("tests.rs") || text.contains("_tests/")
}

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                source_files(&path, out);
            }
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| SOURCE_EXTENSIONS.contains(&extension))
            && !is_test_file(&path)
        {
            out.push(path);
        }
    }
}

/// The code lines of a file: inline test modules and comments removed.
fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split("#[cfg(test)]")
        .next()
        .unwrap_or("")
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.split("//").next().unwrap_or("")))
        .filter(|(_, line)| !line.trim().is_empty())
}

fn is_word_boundary(byte: Option<u8>) -> bool {
    byte.is_none_or(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
}

fn contains_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(start, _)| {
        is_word_boundary(text.as_bytes().get(start.wrapping_sub(1)).copied())
            && is_word_boundary(text.as_bytes().get(start + word.len()).copied())
    })
}

/// The double-quoted segments of a line.
fn literals(line: &str) -> impl Iterator<Item = &str> {
    line.split('"')
        .enumerate()
        .filter(|(index, _)| index % 2 == 1)
        .map(|(_, segment)| segment)
}

fn offending(line: &str, names: &[String]) -> Option<String> {
    if let Some(word) = THEORY_WORDS.iter().find(|word| contains_word(line, word)) {
        return Some(format!("theory word `{word}`"));
    }
    names
        .iter()
        .find(|name| literals(line).any(|literal| contains_word(literal, name)))
        .map(|name| format!("discipline name `{name}` in a string literal"))
}

#[test]
fn no_code_under_crates_names_a_discipline_or_its_theory() {
    let root = repo_root();
    let names = registered_names(&root);
    assert!(
        names.len() >= 3,
        "expected lean4 and both reference disciplines, found {names:?}"
    );
    let mut files = Vec::new();
    source_files(&root.join("crates"), &mut files);
    assert!(
        files.len() > 100,
        "the walk found only {} files",
        files.len()
    );
    let mut hits = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).expect("read source");
        for (line_number, line) in code_lines(&text) {
            if let Some(reason) = offending(line, &names) {
                hits.push(format!("{}:{line_number}: {reason}", path.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "discipline knowledge below the seam:\n{}",
        hits.join("\n")
    );
}

#[test]
fn the_scan_sees_a_literal_and_ignores_comments_and_english() {
    let names = vec!["bounded".to_string()];
    assert!(offending("let x = select(\"bounded\");", &names).is_some());
    assert!(offending("let bounded = workers.len();", &names).is_none());
    assert!(offending("prove(theorem_count)", &names).is_none());
    assert!(offending("let theorem = 1;", &names).is_some());
    let text = "fn a() {} // a theorem here\n#[cfg(test)]\nmod t { const N: &str = \"bounded\"; }";
    assert!(code_lines(text).all(|(_, line)| offending(line, &names).is_none()));
}
