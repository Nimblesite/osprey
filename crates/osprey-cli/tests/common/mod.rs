//! Corpus-walking helpers shared by the gates that read every committed
//! program.
//!
//! `redundant_annotations` and `emitted_ir_is_wellformed` both sweep `tests/`,
//! `examples/` and `benchmarks/`, and both arrived carrying a byte-identical
//! copy of this walk. One copy, so a fix to the walk reaches both gates.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The repository root, from this crate's manifest directory.
///
/// Left un-canonicalized on purpose: there is no fallible call to unwrap, and
/// the `..` components resolve the same way for every consumer here.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Every file with extension `ext` under `dir`, recursively, sorted so a
/// failure names the same program on every machine.
pub fn sources(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(dir, ext, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// The symbol in `@name` position starting at `start`, if it is a plain
/// identifier. Quoted and numeric symbols are skipped rather than guessed at.
///
/// `$` is part of the name. Omitting it truncated every monomorphised symbol
/// (`@handler$mono0`) back to its generic stem (`handler`), so a reference to
/// an instantiation that was never emitted collapsed onto one that was and the
/// gate reported a clean module — the exact dangling reference it exists to
/// catch ([`crate::monofn::specialize_callback`]).
pub fn symbol_at(rest: &str) -> Option<String> {
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.' || *c == '$')
        .collect();
    let first = name.chars().next()?;
    (!name.is_empty() && !first.is_ascii_digit()).then_some(name)
}

/// Every `@symbol` the module BINDS — defined, declared or a global.
pub fn bound_symbols(ir: &str) -> BTreeSet<String> {
    let mut bound = BTreeSet::new();
    for line in ir.lines() {
        let trimmed = line.trim_start();
        let binding = trimmed.starts_with("define")
            || trimmed.starts_with("declare")
            || (trimmed.starts_with('@') && trimmed.contains(" = "));
        if !binding {
            continue;
        }
        if let Some(at) = line.find('@') {
            if let Some(name) = symbol_at(&line[at + 1..]) {
                let _ = bound.insert(name);
            }
        }
    }
    bound
}

/// Every `@symbol` the module USES but never binds. A non-empty result is IR
/// that cannot link.
///
/// A binding line is not skipped wholesale — only the name it BINDS is. A
/// global initializer is a use like any other (`@table = global i8* @missing`),
/// and skipping the whole line let exactly that reference through: the one form
/// where a dangling symbol is written on the same line as a definition.
pub fn undefined_symbols(ir: &str) -> BTreeSet<String> {
    let bound = bound_symbols(ir);
    let mut missing = BTreeSet::new();
    for line in ir.lines() {
        for index in uses(line) {
            if let Some(name) = symbol_at(&line[index + 1..]) {
                if !bound.contains(&name) {
                    let _ = missing.insert(name);
                }
            }
        }
    }
    missing
}

/// The `@` positions on `line` that are USES rather than the binder itself.
///
/// `define`/`declare` headers name the function they introduce and then only
/// spell types, so nothing on them is a use. A global's initializer sits after
/// the binder, so the FIRST `@` is dropped and the rest are kept.
///
/// Positions inside a `c"…"` data literal are dropped whatever the line is: an
/// `@` there is a byte of the program's own text, not a symbol. Every corpus
/// program with an email or a URL in a string — `example.com`, `osprey.dev` —
/// reported a dangling `@example` the moment initializers began to be read.
fn uses(line: &str) -> Vec<usize> {
    let trimmed = line.trim_start();
    let all = symbol_positions(line);
    // One rule for every binding line: the FIRST `@` is the name being bound,
    // everything after it is a use. Skipping `define`/`declare` lines outright
    // missed `define ... @f() personality ptr @handler {`, where a real
    // reference shares the line with the definition it belongs to.
    let binds = trimmed.starts_with("define")
        || trimmed.starts_with("declare")
        || (trimmed.starts_with('@') && trimmed.contains(" = "));
    if binds {
        return all.into_iter().skip(1).collect();
    }
    all
}

/// The byte offset of every `@` on `line` that sits OUTSIDE a `c"…"` literal,
/// found by walking the line once rather than indexing into it.
fn symbol_positions(line: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut inside = false;
    let mut previous = ' ';
    for (index, ch) in line.char_indices() {
        match ch {
            // A `"` inside the data is escaped as `\22`, so the next raw quote
            // is always the one that closes the literal.
            '"' if inside => inside = false,
            '"' if previous == 'c' => inside = true,
            '@' if !inside => positions.push(index),
            _ => {}
        }
        previous = ch;
    }
    positions
}
