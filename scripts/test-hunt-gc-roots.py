#!/usr/bin/env python3
"""Require real GC root retention and reclamation under KLEE [SYMEX-GC-ROOTS]."""
import argparse
import pathlib
import re
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
CALL_GRAPH = '''target datalayout = "e-p:64:64"
@collector_tag = private constant [20 x i8] c"osprey.gc.collector\\00"
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
'''


def run(command, cwd=ROOT):
    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, timeout=180)
    if result.returncode:
        raise RuntimeError(f"{command[0]} failed:\n{result.stdout}\n{result.stderr}")
    return result


def verify(out):
    errors = [f"{path.name}: {path.read_text()}" for path in out.glob("*.err")]
    info = (out / "info").read_text()
    if errors or not re.search(r"completed paths = 1$", info, re.M) or not re.search(r"partially completed paths = 0$", info, re.M):
        raise RuntimeError(f"GC roots regression failed: {errors}\n{info}")


def call_graph_regressions(llvm, folder):
    source, output = folder / "calls.ll", folder / "calls-rooted.ll"
    source.write_text(CALL_GRAPH)
    run([str(llvm / "opt"), "-load-pass-plugin=" + str(ROOT / "compiler/bin/libosprey_gc_roots.so"),
         "-passes=osprey-gc-roots,osprey-gc-roots", "-verify-each", "-S", str(source), "-o", str(output)])
    text = output.read_text()
    bodies = dict(re.findall(r"define[^@]*@(\w+)\([^\n]*\).*?\{(.*?)^\}", text, re.S | re.M))
    for name in ("leaf", "pure_chain"):
        assert "osp.gc.frame" not in bodies[name], f"{name} cannot collect during its activation"
    for name in ("collect_chain", "integer_root", "weak_caller", "indirect_caller", "memory_root", "trigger", "holder"):
        assert "osp.gc.frame" in bodies[name], f"{name} must retain its roots across collection"
    assert "store volatile i64 %raw" in bodies["integer_root"], "integer-held pointers are roots"
    assert "ptrtoint ptr %slots" in bodies["memory_root"], "memory-held pointers remain scanned"


def pass_regressions(llvm, folder):
    source = folder / "attributes.ll"
    source.write_text('target datalayout = "e-p:64:64"\n'
                      '@immutable = constant i64 7\n'
                      'declare void @collect()\n'
                      'define ptr @identity(ptr %p, i32 %small) memory(none) speculatable { call void @collect() %wide = zext i32 %small to i64 %result = getelementptr i8, ptr %p, i64 %wide ret ptr %result }\n'
                      'declare i64 @free(ptr)\n'
                      'define void @unused_return(ptr %p, i1 %flag) { entry: br i1 %flag, label %yes, label %no yes: %ignored = call i64 @free(ptr %p) br label %end no: br label %end end: %dead = phi i64 [ %ignored, %yes ], [ 0, %no ] ret void }\n')
    output = folder / "attributes-rooted.ll"
    run([str(llvm / "opt"), "-load-pass-plugin=" + str(ROOT / "compiler/bin/libosprey_gc_roots.so"),
         "-passes=osprey-gc-roots,osprey-gc-roots", "-verify-each", "-S", str(source), "-o", str(output)])
    text = output.read_text()
    assert "memory(none)" not in text and "speculatable" not in text, "stale LLVM effects after rooting"
    assert text.count("define internal void @osp.gc.roots.register") == 0, "immutable globals registered"
    assert text.count("alloca { ptr, i64, ptr }") == 2, "root pass must be idempotent"
    assert "alloca i32" not in text, "sub-word values cannot contain a GC root"
    assert "store volatile i64 %ignored" not in text, "rooting must not invent a use of a void C return"


def execute_module(klee, folder, memory, module):
    out = folder / "klee"
    run([str(klee), "--libc=klee", "--ubsan-runtime", "--optimize", "--kdalloc",
                  "--external-calls=none", "--max-time=30s", "--watchdog",
                  "-link-llvm-lib=" + str(ROOT / f"compiler/bin/libosprey_runtime_klee_{memory}.bc"),
                  "--output-dir=" + str(out), str(module)])
    return out


def compile_module(llvm, source, module, memory):
    run([str(llvm / "clang"), "--target=x86_64-unknown-linux-gnu", "-nostdlibinc",
         "-isystem", "runtime/klee/include", "-emit-llvm", "-c", "-g", "-O1",
         "-DOSP_SYMBOLIC", *( ["-DOSP_HUNT_ARC"] if memory == "arc" else []), str(source), "-o", str(module)], ROOT / "compiler")
    if memory == "gc":
        run([str(llvm / "opt"), "-load-pass-plugin=" + str(ROOT / "compiler/bin/libosprey_gc_roots.so"),
             "-passes=osprey-gc-roots", "-verify-each", str(module), "-o", str(module)])


def regression(llvm, klee, folder, memory):
    folder = folder / memory
    folder.mkdir()
    module = folder / "roots.bc"
    source = ROOT / "compiler/runtime/memory_gc_stack_root_tests.c"
    compile_module(llvm, source, module, memory)
    out = execute_module(klee, folder, memory, module)
    verify(out)
    print(f"{memory.upper()} retention and reclamation: passed")


def parser_bounds_regression(llvm, klee, folder, memory):
    folder = folder / f"{memory}-parser-bounds"
    folder.mkdir()
    source, module = folder / "unterminated.c", folder / "unterminated.bc"
    source.write_text('#include <stdint.h>\nlong osp_hunt_input_bytes = 2;\nchar *osp_input(void);\n'
                      'int64_t osp_parse_int_strict(const char *, int64_t *);\n'
                      'int main(void) { char *s = osp_input(); for (int i = 0; i < 3; i++) s[i] = 49; '
                      'int64_t value = 0; return (int)osp_parse_int_strict(s, &value); }\n')
    compile_module(llvm, source, module, memory)
    out = execute_module(klee, folder, memory, module)
    errors = list(out.glob("*.err"))
    assert len(errors) == 1 and errors[0].name.endswith(".ptr.err"), "unterminated numeric input must retain its bounds finding"
    assert "out of bound" in errors[0].read_text(), errors[0].read_text()
    assert not list(out.glob("*.early")), "the bounds control must finish"
    print(f"{memory.upper()} unterminated parser bounds control: passed")


def global_regressions(llvm, folder):
    source = folder / "globals.ll"
    source.write_text('target datalayout = "e-p:64:64"\n' +
                      ''.join(f'@root{i} = global ptr null\n' for i in range(64)))
    output = folder / "globals-rooted.ll"
    run([str(llvm / "opt"), "-load-pass-plugin=" + str(ROOT / "compiler/bin/libosprey_gc_roots.so"),
         "-passes=osprey-gc-roots", "-verify-each", "-S", str(source), "-o", str(output)])
    text = output.read_text()
    assert len(re.findall(r'@osp\.gc\.global\.table(?:\.\d+)? =', text)) == 1, "one module needs one global root table"
    assert "i64 64" in text, "every mutable global must remain registered"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--llvm", type=pathlib.Path, required=True)
    parser.add_argument("--klee", type=pathlib.Path, required=True)
    options = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="osprey-gc-roots-") as directory:
        folder = pathlib.Path(directory)
        call_graph_regressions(options.llvm, folder)
        pass_regressions(options.llvm, folder)
        global_regressions(options.llvm, folder)
        for memory in ("gc", "arc"):
            regression(options.llvm, options.klee, folder, memory)
            parser_bounds_regression(options.llvm, options.klee, folder, memory)


if __name__ == "__main__":
    main()
