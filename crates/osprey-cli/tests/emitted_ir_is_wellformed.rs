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

mod common;

use common::{bound_symbols, repo_root, sources, symbol_at, undefined_symbols};
use std::fs;
use std::path::Path;

/// Lower `source`, or `None` if it never got as far as IR.
fn ir_for(path: &Path, source: &str) -> Option<String> {
    let parsed = osprey_syntax::parse_program_for_path(&path.to_string_lossy(), source);
    if !parsed.errors.is_empty() || !osprey_types::check_program(&parsed.program).is_empty() {
        return None;
    }
    osprey_codegen::compile_program(&parsed.program).ok()
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
fn a_reference_to_an_unemitted_instantiation_is_not_absorbed_by_its_generic_stem() {
    // The gate reads symbols out of raw IR, so its own name-scanning decides
    // what it can see. `$` separates a generic from its instantiation, and
    // dropping it made `@handler$mono99` indistinguishable from `@handler`.
    // A module defining ONLY `mono0` while calling `mono99` is precisely the
    // dangling reference clang rejects, and it must not read as clean because
    // some other instantiation of the same function happens to exist.
    let ir = "\
define i64 @handler$mono0(i8* %a) {
entry:
  ret i64 0
}
define i64 @main() {
entry:
  %r = call i64 @handler$mono99(i8* null)
  ret i64 0
}
";
    assert_eq!(
        symbol_at("handler$mono0("),
        Some("handler$mono0".to_string())
    );
    assert!(
        bound_symbols(ir).contains("handler$mono0"),
        "the emitted instantiation must be recorded under its full name"
    );
    assert!(
        undefined_symbols(ir).contains("handler$mono99"),
        "a call to an instantiation that was never emitted must be reported, \
         not absorbed by the stem it shares with an emitted one"
    );
}

#[test]
fn a_global_initializer_is_scanned_for_uses_like_any_other_line() {
    // The scanner skipped every BINDING line wholesale, on the reasoning that
    // such a line introduces a name rather than using one. A global's
    // initializer breaks that: it sits on the same line as the binder and is a
    // reference like any other, so `@table = global i8* @missing` — the one
    // shape where a dangling symbol shares a line with a definition — read as
    // clean. Only the bound name is skipped now; the rest of the line is scanned.
    let ir = "\
@present = global i64 0
@table = global i8* bitcast (i64* @present to i8*)
@broken = global i8* bitcast (i64* @missing to i8*)
define i64 @withPersonality() personality i8* @absentPersonality {
entry:
  ret i64 0
}
@.str.0 = private unnamed_addr constant [17 x i8] c\"a@example.com\\00\"
define i64 @main() {
entry:
  ret i64 0
}
";
    let bound = bound_symbols(ir);
    assert!(
        bound.contains("present") && bound.contains("table") && bound.contains("broken"),
        "every global still binds its own name: {bound:?}"
    );
    let missing = undefined_symbols(ir);
    assert!(
        missing.contains("missing"),
        "an initializer naming an unbound symbol must be reported: {missing:?}"
    );
    assert!(
        !missing.contains("present"),
        "an initializer naming a BOUND symbol must not be: {missing:?}"
    );
    assert!(
        !missing.contains("table") && !missing.contains("broken"),
        "and a global must never report itself as undefined: {missing:?}"
    );
    // An `@` inside a `c"…"` literal is a byte of the program's own text. Every
    // corpus program carrying an email or a URL reported a dangling `@example`
    // the moment initializers began to be read.
    assert!(
        !missing.contains("example"),
        "an `@` inside a data literal is not a symbol: {missing:?}"
    );
    // A `define` header can carry a real reference after the name it binds —
    // `personality` is the common one — so skipping those lines wholesale hid
    // it. One rule now covers every binding line: skip the binder, read the rest.
    assert!(
        missing.contains("absentPersonality"),
        "a personality clause naming an unbound symbol must be reported: {missing:?}"
    );
    assert!(
        !missing.contains("withPersonality"),
        "while the function that clause belongs to is bound, not missing: {missing:?}"
    );
}

#[test]
fn no_corpus_program_emits_a_reference_to_an_undefined_symbol() {
    // Guards the tree as it stands: whatever else is true, nothing currently
    // committed may lower to IR that cannot link.
    // BOTH flavors. The break that motivated this gate reached ML too, and a
    // sweep of `osp` alone cannot see it: an `.ospml` twin lowers through the
    // same codegen, so a dangling reference there is the same unlinkable module
    // ([FLAVOR-IR-EQUIV]).
    let root = repo_root();
    let mut broken = Vec::new();
    for dir in ["tests", "examples", "benchmarks"] {
        for path in ["osp", "ospml"]
            .iter()
            .flat_map(|ext| sources(&root.join(dir), ext))
        {
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
