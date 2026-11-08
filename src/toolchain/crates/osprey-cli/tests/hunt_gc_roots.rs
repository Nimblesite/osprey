//! Real GC root retention and reclamation under KLEE [SYMEX-GC-ROOTS]: the
//! `osprey-gc-roots` LLVM pass roots exactly the frames that can collect, and
//! the KLEE runtime keeps those roots live on both backends. Needs LLVM 19 and
//! KLEE, so it is ignored by default and run by `make _test_hunt_crashes`,
//! which passes `OSPREY_KLEE_LLVM` (LLVM's bin directory) and `OSPREY_KLEE`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

type Checked<T = ()> = Result<T, Box<dyn std::error::Error>>;

const SINGLE_PASS: &str = "-passes=osprey-gc-roots";
const DOUBLE_PASS: &str = "-passes=osprey-gc-roots,osprey-gc-roots";

const CALL_GRAPH: &str = r#"target datalayout = "e-p:64:64"
@collector_tag = private constant [20 x i8] c"osprey.gc.collector\00"
@llvm.global.annotations = appending global [1 x { ptr, ptr, ptr, i32, ptr }] [{ ptr, ptr, ptr, i32, ptr } { ptr @internal_collect, ptr @collector_tag, ptr null, i32 0, ptr null }]
declare void @collect()
define internal void @internal_collect() { ret void }
define internal void @trigger() { call void @internal_collect() ret void }
define internal ptr @holder(ptr %p) { call void @trigger() ret ptr %p }
define internal i64 @leaf(i64 %n) { %sum = add i64 %n, 1 ret i64 %sum }
define internal i64 @pure_chain(i64 %n) { %sum = call i64 @leaf(i64 %n) ret i64 %sum }
define internal void @collect_chain() { call void @collect() ret void }
define i64 @integer_root(ptr %p) {
  %raw = ptrtoint ptr %p to i64
  call void @collect_chain()
  ret i64 %raw
}
define weak void @replaceable() { ret void }
define void @weak_caller() { call void @replaceable() ret void }
define void @indirect_caller(ptr %callee) { call void %callee() ret void }
define ptr @memory_root(ptr %p) {
  %slots = alloca [2 x ptr]
  store ptr %p, ptr %slots
  call void @collect()
  %result = load ptr, ptr %slots
  ret ptr %result
}
"#;

const ATTRIBUTES: &str = "target datalayout = \"e-p:64:64\"
@immutable = constant i64 7
declare void @collect()
define ptr @identity(ptr %p, i32 %small) memory(none) speculatable { call void @collect() %wide = zext i32 %small to i64 %result = getelementptr i8, ptr %p, i64 %wide ret ptr %result }
declare i64 @free(ptr)
define void @unused_return(ptr %p, i1 %flag) { entry: br i1 %flag, label %yes, label %no yes: %ignored = call i64 @free(ptr %p) br label %end no: br label %end end: %dead = phi i64 [ %ignored, %yes ], [ 0, %no ] ret void }
";

const UNTERMINATED: &str = "#include <stdint.h>
long osp_hunt_input_bytes = 2;
char *osp_input(void);
int64_t osp_parse_int_strict(const char *, int64_t *);
int main(void) { char *s = osp_input(); for (int i = 0; i < 3; i++) s[i] = 49; int64_t value = 0; return (int)osp_parse_int_strict(s, &value); }
";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn tool(var: &str) -> Checked<PathBuf> {
    Ok(PathBuf::from(std::env::var(var).map_err(|error| {
        format!("{var} must name the tool: {error}")
    })?))
}

fn scratch(name: &str) -> Checked<PathBuf> {
    let dir = repo_root().join(format!(
        "target/hunt-gc-roots/{}-{name}",
        std::process::id()
    ));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn run(command: &mut Command) -> Checked {
    let output = command.output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{command:?} failed:\n{stderr}");
    Ok(())
}

/// Runs the GC-roots pass over `source` into `output`, as text when `text`.
fn gc_roots_opt(source: &Path, output: &Path, passes: &str, text: bool) -> Checked {
    let plugin = repo_root().join("src/toolchain/compiler/bin/libosprey_gc_roots.so");
    let mut opt = Command::new(tool("OSPREY_KLEE_LLVM")?.join("opt"));
    let _ = opt
        .arg(format!("-load-pass-plugin={}", plugin.display()))
        .args([passes, "-verify-each"]);
    let _ = opt
        .args(if text { &["-S"][..] } else { &[] })
        .arg(source)
        .arg("-o")
        .arg(output);
    run(&mut opt)
}

/// The rooted text of `ir` after `passes`.
fn rooted(name: &str, ir: &str, passes: &str) -> Checked<String> {
    let dir = scratch(name)?;
    std::fs::write(dir.join("in.ll"), ir)?;
    gc_roots_opt(&dir.join("in.ll"), &dir.join("out.ll"), passes, true)?;
    Ok(std::fs::read_to_string(dir.join("out.ll"))?)
}

/// The body of `define ... @name(...) { ... }` in `ir`, up to its closing brace line.
fn body(ir: &str, name: &str) -> Checked<String> {
    let head = format!("@{name}(");
    let mut lines = ir
        .lines()
        .skip_while(|l| !(l.starts_with("define") && l.contains(&head)));
    let first = lines
        .next()
        .ok_or_else(|| format!("@{name} missing from the rooted IR"))?;
    let rest: Vec<&str> = lines.take_while(|l| *l != "}").collect();
    Ok(format!("{first}\n{}", rest.join("\n")))
}

#[test]
#[ignore = "needs LLVM 19 and KLEE"]
fn only_frames_that_can_collect_retain_their_roots() -> Checked {
    let ir = rooted("calls", CALL_GRAPH, DOUBLE_PASS)?;
    for name in ["leaf", "pure_chain"] {
        assert!(
            !body(&ir, name)?.contains("osp.gc.frame"),
            "{name} cannot collect during its activation"
        );
    }
    for name in [
        "collect_chain",
        "integer_root",
        "weak_caller",
        "indirect_caller",
        "memory_root",
        "trigger",
        "holder",
    ] {
        assert!(
            body(&ir, name)?.contains("osp.gc.frame"),
            "{name} must retain its roots across collection"
        );
    }
    assert!(
        body(&ir, "integer_root")?.contains("store volatile i64 %raw"),
        "integer-held pointers are roots"
    );
    assert!(
        body(&ir, "memory_root")?.contains("ptrtoint ptr %slots"),
        "memory-held pointers remain scanned"
    );
    Ok(())
}

#[test]
#[ignore = "needs LLVM 19 and KLEE"]
fn rooting_drops_stale_effects_and_is_idempotent() -> Checked {
    let ir = rooted("attributes", ATTRIBUTES, DOUBLE_PASS)?;
    assert!(
        !ir.contains("memory(none)") && !ir.contains("speculatable"),
        "stale LLVM effects after rooting"
    );
    assert!(
        !ir.contains("define internal void @osp.gc.roots.register"),
        "immutable globals registered"
    );
    assert_eq!(
        ir.matches("alloca { ptr, i64, ptr }").count(),
        2,
        "root pass must be idempotent"
    );
    assert!(
        !ir.contains("alloca i32"),
        "sub-word values cannot contain a GC root"
    );
    assert!(
        !ir.contains("store volatile i64 %ignored"),
        "rooting must not invent a use of a void C return"
    );
    Ok(())
}

#[test]
#[ignore = "needs LLVM 19 and KLEE"]
fn one_module_registers_every_mutable_global_in_one_table() -> Checked {
    let mut ir = String::from("target datalayout = \"e-p:64:64\"\n");
    for i in 0..64 {
        writeln!(ir, "@root{i} = global ptr null")?;
    }
    let ir = rooted("globals", &ir, SINGLE_PASS)?;
    let tables = ir
        .lines()
        .filter(|l| l.starts_with("@osp.gc.global.table") && l.contains(" ="))
        .count();
    assert_eq!(tables, 1, "one module needs one global root table");
    assert!(
        ir.contains("i64 64"),
        "every mutable global must remain registered"
    );
    Ok(())
}

/// Compiles C `source` to KLEE bitcode for `memory`, rooted under GC.
fn compile_module(source: &Path, module: &Path, memory: &str) -> Checked {
    let mut clang = Command::new(tool("OSPREY_KLEE_LLVM")?.join("clang"));
    let _ = clang.current_dir(repo_root().join("src/toolchain/compiler"));
    let _ = clang.args([
        "--target=x86_64-unknown-linux-gnu",
        "-nostdlibinc",
        "-isystem",
        "runtime/klee/include",
    ]);
    let _ = clang.args(["-emit-llvm", "-c", "-g", "-O1", "-DOSP_SYMBOLIC"]);
    let _ = clang.args(if memory == "arc" {
        &["-DOSP_HUNT_ARC"][..]
    } else {
        &[]
    });
    run(clang.arg(source).arg("-o").arg(module))?;
    if memory == "gc" {
        gc_roots_opt(module, module, SINGLE_PASS, false)?;
    }
    Ok(())
}

/// Runs `module` under KLEE against the `memory` runtime; returns KLEE's output directory.
fn execute_module(dir: &Path, memory: &str, module: &Path) -> Checked<PathBuf> {
    let out = dir.join("klee");
    let runtime = repo_root().join(format!(
        "src/toolchain/compiler/bin/libosprey_runtime_klee_{memory}.bc"
    ));
    let mut klee = Command::new(tool("OSPREY_KLEE")?);
    let _ = klee.args(["--libc=klee", "--ubsan-runtime", "--optimize", "--kdalloc"]);
    let _ = klee.args(["--external-calls=none", "--max-time=30s", "--watchdog"]);
    let _ = klee.arg(format!("-link-llvm-lib={}", runtime.display()));
    run(klee
        .arg(format!("--output-dir={}", out.display()))
        .arg(module))?;
    Ok(out)
}

fn outputs(out: &Path, suffix: &str) -> Checked<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(out)? {
        let path = entry?.path();
        if path.to_string_lossy().ends_with(suffix) {
            found.push(path);
        }
    }
    Ok(found)
}

#[test]
#[ignore = "needs LLVM 19 and KLEE"]
fn stack_roots_are_retained_and_reclaimed_on_both_backends() -> Checked {
    for memory in ["gc", "arc"] {
        let dir = scratch(&format!("roots-{memory}"))?;
        let source =
            repo_root().join("src/toolchain/compiler/runtime/memory_gc_stack_root_tests.c");
        compile_module(&source, &dir.join("roots.bc"), memory)?;
        let out = execute_module(&dir, memory, &dir.join("roots.bc"))?;
        let errors = outputs(&out, ".err")?;
        assert!(errors.is_empty(), "{memory}: {errors:?}");
        let info = std::fs::read_to_string(out.join("info"))?;
        assert!(
            info.lines().any(|l| l.ends_with("completed paths = 1")),
            "{memory}: {info}"
        );
        assert!(
            info.lines()
                .any(|l| l.ends_with("partially completed paths = 0")),
            "{memory}: {info}"
        );
    }
    Ok(())
}

#[test]
#[ignore = "needs LLVM 19 and KLEE"]
fn unterminated_numeric_input_keeps_its_bounds_finding_on_both_backends() -> Checked {
    for memory in ["gc", "arc"] {
        let dir = scratch(&format!("parser-bounds-{memory}"))?;
        std::fs::write(dir.join("unterminated.c"), UNTERMINATED)?;
        compile_module(
            &dir.join("unterminated.c"),
            &dir.join("unterminated.bc"),
            memory,
        )?;
        let out = execute_module(&dir, memory, &dir.join("unterminated.bc"))?;
        let errors = outputs(&out, ".err")?;
        let [error] = errors.as_slice() else {
            return Err(format!("{memory}: one bounds finding expected, got {errors:?}").into());
        };
        assert!(
            error.to_string_lossy().ends_with(".ptr.err"),
            "{memory}: {error:?}"
        );
        let detail = std::fs::read_to_string(error)?;
        assert!(detail.contains("out of bound"), "{memory}: {detail}");
        assert!(
            outputs(&out, ".early")?.is_empty(),
            "{memory}: the bounds control must finish"
        );
    }
    Ok(())
}
