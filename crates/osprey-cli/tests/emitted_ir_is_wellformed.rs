//! `compile_program` must never report success and hand back broken IR.
//!
//! A function passed as a VALUE (an HTTP handler, a callback) is referenced by
//! address. When its type is inferred rather than written, codegen emits the
//! reference and never emits the body — then returns `Ok`. clang rejects the
//! result with `use of undefined value '@localResponse'`, so the failure lands
//! in the LINKER, far from the compiler that caused it.
//!
//! That is the silent-failure class: success reported, garbage produced. It hid
//! behind four annotations for as long as they happened to be written down, and
//! surfaced only when `ec6a5cac` deleted them as "redundant" — taking 14 corpus
//! programs from green to a clang error in one commit.
//!
//! Type-checking cannot see this and neither can `compile_program`'s own return
//! value, which is precisely why these assertions read the emitted IR.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn sources(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(dir, ext, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
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

/// Lower `source`, or `None` if it never got as far as IR.
fn ir_for(path: &Path, source: &str) -> Option<String> {
    let parsed = osprey_syntax::parse_program_for_path(&path.to_string_lossy(), source);
    if !parsed.errors.is_empty() || !osprey_types::check_program(&parsed.program).is_empty() {
        return None;
    }
    osprey_codegen::compile_program(&parsed.program).ok()
}

/// The symbol in `@name` position starting at `start`, if it is a plain
/// identifier. Quoted and numeric symbols are skipped rather than guessed at.
fn symbol_at(rest: &str) -> Option<String> {
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    let first = name.chars().next()?;
    (!name.is_empty() && !first.is_ascii_digit()).then_some(name)
}

/// Every `@symbol` the module BINDS — defined, declared or a global.
fn bound_symbols(ir: &str) -> BTreeSet<String> {
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
fn undefined_symbols(ir: &str) -> BTreeSet<String> {
    let bound = bound_symbols(ir);
    let mut missing = BTreeSet::new();
    for line in ir.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("define")
            || trimmed.starts_with("declare")
            || (trimmed.starts_with('@') && trimmed.contains(" = "))
        {
            continue;
        }
        for (index, _) in line.match_indices('@') {
            if let Some(name) = symbol_at(&line[index + 1..]) {
                if !bound.contains(&name) {
                    let _ = missing.insert(name);
                }
            }
        }
    }
    missing
}

/// A callback whose type is written out — the control. Its body IS emitted.
const ANNOTATED: &str = r#"fn handler(method: string, path: string, headers: string, body: string) -> HttpResponse = HttpResponse {
    status: 200, headers: "", contentType: "text/plain",
    streamFd: 0, isComplete: true, partialBody: "ok"
}
let server = httpCreateServer(18201, "127.0.0.1")
let listening = httpListen(server, handler)
print("${listening}")
"#;

/// The same program with the inferable annotations deleted, exactly as
/// CLAUDE.md requires. Identical meaning; the body stops being emitted.
const INFERRED: &str = r#"fn handler(method, path, headers, body) = HttpResponse {
    status: 200, headers: "", contentType: "text/plain",
    streamFd: 0, isComplete: true, partialBody: "ok"
}
let server = httpCreateServer(18201, "127.0.0.1")
let listening = httpListen(server, handler)
print("${listening}")
"#;

#[test]
fn a_callback_referenced_by_the_ir_must_also_be_defined_by_it() {
    let probe = Path::new("callback_probe.osp");

    // The control proves the program is well-formed and that codegen CAN emit
    // this body. Any difference below is the annotation's presence alone.
    let annotated = ir_for(probe, ANNOTATED).expect("annotated probe must lower");
    assert!(
        annotated.contains("define") && annotated.contains("@handler"),
        "control must both define and reference @handler; got {annotated}"
    );
    assert!(
        undefined_symbols(&annotated).is_empty(),
        "control IR must be self-contained; dangling: {:?}",
        undefined_symbols(&annotated)
    );

    // The defect. `compile_program` returning Ok is itself part of the bug, so
    // it is asserted rather than relied on.
    let inferred = ir_for(probe, INFERRED)
        .expect("codegen reports SUCCESS here — that is half the defect being pinned");
    assert!(
        inferred.contains("@handler"),
        "the probe must still reference the callback, or it pins nothing"
    );

    let dangling = undefined_symbols(&inferred);
    assert!(
        !dangling.contains("handler"),
        "codegen emitted a reference to @handler without emitting its body and \
         still returned Ok — clang rejects this with `use of undefined value`. \
         Success reported, garbage produced. dangling={dangling:?}"
    );
    assert!(
        dangling.is_empty(),
        "emitted IR must never reference a symbol it does not bind; dangling={dangling:?}"
    );
    assert!(
        inferred
            .lines()
            .any(|l| l.trim_start().starts_with("define") && l.contains("@handler")),
        "the callback body must be emitted whether or not its type was written"
    );
}

#[test]
fn no_corpus_program_emits_a_reference_to_an_undefined_symbol() {
    // Guards the tree as it stands: whatever else is true, nothing currently
    // committed may lower to IR that cannot link.
    let root = repo_root();
    let mut broken = Vec::new();
    for dir in ["tests", "examples", "benchmarks"] {
        for path in sources(&root.join(dir), "osp") {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            let Some(ir) = ir_for(&path, &source) else {
                continue;
            };
            let dangling = undefined_symbols(&ir);
            if !dangling.is_empty() {
                let display = path.strip_prefix(&root).unwrap_or(&path).display();
                broken.push(format!("{display}: {dangling:?}"));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "codegen returned Ok for {} program(s) whose IR references symbols it \
         never emitted; each of these fails at clang, not at the compiler:\n{}",
        broken.len(),
        broken.join("\n")
    );
}
