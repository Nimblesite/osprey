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
#[path = "common/debug_scope.rs"]
mod debug_scope;
use debug_scope::{scope_chain, scope_parent};

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

/// [DEBUGGER-SOURCE-MAP] A return-only block still has an executable body line.
#[test]
fn single_expression_blocks_keep_their_return_locations() -> Result<(), String> {
    for (default, ml, scope, line) in RETURN_ONLY_BLOCKS {
        for (source, extension) in [(default, "osp"), (ml, "ospml")] {
            assert_return_location(&return_debug_ir(source, extension)?, scope, line)?;
        }
    }
    Ok(())
}

fn return_debug_ir(source: &str, extension: &str) -> Result<String, String> {
    let path = format!("return_location.{extension}");
    let parsed = osprey_syntax::parse_program_for_path(&path, source);
    assert!(parsed.errors.is_empty(), "{extension}: {:?}", parsed.errors);
    let errors = osprey_types::check_program(&parsed.program);
    assert!(errors.is_empty(), "{extension}: {errors:?}");
    osprey_codegen::compile_program_debug(
        &parsed.program,
        osprey_codegen::DebugSource::from_path(&path),
    )
    .map_err(|error| format!("{extension}: {error}"))
}

fn assert_return_location(ir: &str, name: &str, line: usize) -> Result<(), String> {
    let scope = ir
        .lines()
        .find(|line| line.contains(&format!("!DISubprogram(name: \"{name}")))
        .and_then(|line| line.split_once(" = "))
        .map(|(id, _)| id)
        .ok_or_else(|| format!("missing scope {name}"))?;
    let location = format!("!DILocation(line: {line},");
    let locations = ir.lines().filter(|metadata| metadata.contains(&location));
    let owners = locations
        .map(|metadata| scope_parent(metadata).and_then(|owner| scope_chain(ir, owner)))
        .collect::<Result<Vec<_>, _>>()?;
    assert!(
        owners.iter().any(|chain| chain.last() == Some(&scope)),
        "missing return line in {name}: {ir}"
    );
    Ok(())
}

const RETURN_ONLY_BLOCKS: [(&str, &str, &str, usize); 3] = [
    (
        "let value = 42\nfn read() = {\n    value\n}\nfn main() = print(read())\n",
        "value = 42\nread () =\n    value\nmain () = print (read ())\n",
        "read", 3,
    ),
    (
        "fn answer() = {\n    42\n}\nfn main() = print(answer())\n",
        "answer () =\n    42\nmain () = print (answer ())\n",
        "answer", 2,
    ),
    (
        "fn make() = fn(x) => {\n    wrapAdd(x, 0)\n}\nfn main() = {\n let f = make()\n print(f(42))\n}\n",
        "make () = \\x =>\n    wrapAdd x 0\nmain () =\n    f = make ()\n    print (f 42)\n",
        "__closure_fn_", 2,
    ),

];

/// [DEBUGGER-LAMBDA-SCOPES] A generic function value retains its real definition.
#[test]
fn generic_function_values_keep_their_source_scope() -> Result<(), String> {
    for (source, extension) in [(GENERIC_RECORD.0, "osp"), (GENERIC_RECORD.1, "ospml")] {
        let ir = return_debug_ir(source, extension)?;
        assert_return_location(&ir, "identity", 5)?;
        assert!(ir.contains("!DILocalVariable(name: \"x\", arg: 2,"));
    }
    Ok(())
}

const GENERIC_RECORD: (&str, &str) = (
    "type IntFunction = { run: fn(int) -> int }\n\nfn identity(x) = {\n    let value = x\n    value\n}\nfn main() = {\n    let holder = IntFunction { run: identity }\n    print(holder.run(42))\n}\n",
    "type IntFunction =\n    run : int -> int\nidentity x =\n    value = x\n    value\nmain () =\n    holder = IntFunction(run = identity)\n    print (holder.run 42)\n",
);

/// [FFI-CALLBACKS] Specializing a named function preserves its C ABI debug arguments.
#[test]
fn generic_c_callbacks_keep_their_source_scope() -> Result<(), String> {
    for (source, extension) in GENERIC_C_CALLBACKS {
        let ir = return_debug_ir(source, extension)?;
        assert_return_location(&ir, "identity", 4)?;
        assert!(ir.contains("!DILocalVariable(name: \"x\", arg: 1,"));
    }
    Ok(())
}

const GENERIC_C_CALLBACKS: [(&str, &str); 2] = [
    ("extern fn invoke(f: (int) -> int, n: int) -> int\nfn identity(x) = {\n    let value = x\n    value\n}\nfn main() = print(invoke(identity, 42))\n", "osp"),
    ("extern invoke (f : int -> int) (n : int) -> int\nidentity x =\n    value = x\n    value\nmain () = print (invoke (identity, 42))\n", "ospml"),
];

/// [DEBUGGER-SOURCE-MAP] Runtime callback instantiations keep the original definition.
#[test]
fn generic_runtime_callbacks_keep_their_source_scope() -> Result<(), String> {
    for (source, extension) in [
        (INFERRED.replace("handler", "respond"), "osp"),
        (INFERRED_ML.to_owned(), "ospml"),
    ] {
        let ir = return_debug_ir(&source, extension)?;
        assert_return_location(&ir, "respond", 1)?;
        for (index, name) in ["method", "path", "headers", "body"].iter().enumerate() {
            let arg = index + 1;
            assert!(ir.contains(&format!("!DILocalVariable(name: \"{name}\", arg: {arg},")));
        }
    }
    Ok(())
}

const INFERRED_ML: &str = "respond (method, path, headers, body) = HttpResponse(status = 200, headers = \"\", contentType = \"text/plain\", streamFd = 0, isComplete = true, partialBody = \"ok\")\nserver = httpCreateServer (18201, \"127.0.0.1\")\nlistening = httpListen (server, respond)\nprint \"${listening}\"\n";

/// [TYPE-GENERICS-FN] Named C callbacks read their declaration's global bindings.
#[test]
fn generic_c_callbacks_do_not_capture_shadowing_callers() -> Result<(), String> {
    for (source, extension) in [
        ("extern fn invoke(f: (int) -> int, n: int) -> int\nlet anchor = 2\nfn declared(ignored) = anchor\nfn main() = {\n    let anchor = 100\n    print(invoke(declared, anchor))\n}\n", "osp"),
        ("extern invoke (f : int -> int) (n : int) -> int\nanchor = 2\ndeclared ignored = anchor\nmain () =\n    anchor = 100\n    print (invoke (declared, anchor))\n", "ospml"),
    ] {
        let ir = return_debug_ir(source, extension)?;
        assert!(ir.contains("load i64, i64* @osp.g.anchor"));
        assert!(ir.contains("!DILocalVariable(name: \"ignored\", arg: 1,"));
        assert!(undefined_symbols(&ir).is_empty());
    }
    Ok(())
}

type DebugTestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// [DEBUGGER-BUILD-OPTIONS] Artifact controls work on both authoring surfaces.
#[test]
fn debug_build_controls_preserve_inspectable_ir_and_runnable_output() -> DebugTestResult {
    for (extension, source) in [
        ("osp", "fn main() = print(42)\n"),
        ("ospml", "main () = print 42\n"),
    ] {
        for info in ["dwarf", "none"] {
            assert_debug_artifacts(extension, source, info)?;
        }
    }
    Ok(())
}

fn debug_fixture(
    label: &str,
    extension: &str,
    source: &str,
) -> DebugTestResult<(std::path::PathBuf, std::path::PathBuf)> {
    let dir = std::env::temp_dir().join(format!(
        "osprey-debug-{}-{label}-{extension}",
        std::process::id()
    ));
    fs::create_dir_all(&dir)?;
    let input = dir.join(format!("source.{extension}"));
    fs::write(&input, source)?;
    Ok((input, dir.join("debug program.exe")))
}

fn debug_command(input: &Path, output: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = command
        .current_dir(repo_root())
        .arg(input)
        .arg("--debug-out")
        .arg(output);
    command
}

fn assert_debug_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn compile_debug_artifact(
    input: &Path,
    output: &Path,
    info: &str,
) -> DebugTestResult<std::process::Output> {
    Ok(debug_command(input, output)
        .args([
            "--debug",
            "--compile",
            "--debug-preserve-ir",
            "--debug-opt=none",
            &format!("--debug-info={info}"),
        ])
        .env("OSPREY_DEBUG_OPT", "-O3")
        .output()?)
}

fn assert_debug_artifacts(extension: &str, source: &str, info: &str) -> DebugTestResult {
    let (input, output) = debug_fixture(info, extension, source)?;
    let built = compile_debug_artifact(&input, &output, info)?;
    assert_debug_success(&built);
    let ir = fs::read_to_string(output.with_extension("exe.ll"))?;
    assert_eq!(ir.contains("!DICompileUnit"), info == "dwarf");
    assert_eq!(ir.contains("!DILocation"), info == "dwarf");
    let run = std::process::Command::new(&output).output()?;
    assert_debug_success(&run);
    assert_eq!(run.stdout, b"42\n");
    if let Some(dir) = input.parent() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

/// --run preserves explicitly named output and IR, with unchanged program stdout.
#[test]
fn debug_run_retains_requested_artifacts_in_both_flavors() -> DebugTestResult {
    for extension in ["osp", "ospml"] {
        let (input, output) = debug_fixture("run", extension, "print(42)\n")?;
        let run = debug_command(&input, &output)
            .args(["--run", "--debug-preserve-ir", "--debug-preserve-symbols"])
            .output()?;
        assert_debug_success(&run);
        assert_eq!(run.stdout, b"42\n");
        assert!(output.is_file());
        assert!(output.with_extension("exe.ll").is_file());
        if cfg!(target_os = "macos") {
            assert!(output.with_extension("exe.dSYM").is_dir());
        }
        if let Some(dir) = input.parent() {
            fs::remove_dir_all(dir)?;
        }
    }
    Ok(())
}
