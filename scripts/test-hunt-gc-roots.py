#!/usr/bin/env python3
"""Require real GC root retention and reclamation under KLEE [SYMEX-GC-ROOTS]."""
import argparse
import pathlib
import re
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]


def run(command, cwd=ROOT):
    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, timeout=180)
    if result.returncode:
        raise RuntimeError(f"{command[0]} failed:\n{result.stdout}\n{result.stderr}")
    return result


def verify(out):
    errors = list(out.glob("*.err"))
    info = (out / "info").read_text()
    if errors or not re.search(r"completed paths = 1$", info, re.M) or not re.search(r"partially completed paths = 0$", info, re.M):
        raise RuntimeError(f"GC roots regression failed: {errors}\n{info}")


def pass_regressions(llvm, folder):
    source = folder / "attributes.ll"
    source.write_text('target datalayout = "e-p:64:64"\n'
                      '@immutable = constant i64 7\n'
                      'define ptr @identity(ptr %p) memory(none) speculatable { ret ptr %p }\n')
    output = folder / "attributes-rooted.ll"
    run([str(llvm / "opt"), "-load-pass-plugin=" + str(ROOT / "compiler/bin/libosprey_gc_roots.so"),
         "-passes=osprey-gc-roots,osprey-gc-roots", "-verify-each", "-S", str(source), "-o", str(output)])
    text = output.read_text()
    assert "memory(none)" not in text and "speculatable" not in text, "stale LLVM effects after rooting"
    assert text.count("define internal void @osp.gc.roots.register") == 0, "immutable globals registered"
    assert text.count("alloca { ptr, i64, ptr }") == 1, "root pass must be idempotent"


def regression(llvm, klee, folder, memory):
    folder = folder / memory
    folder.mkdir()
    module = folder / "roots.bc"
    source = ROOT / "compiler/runtime/memory_gc_stack_root_tests.c"
    run([str(llvm / "clang"), "--target=x86_64-unknown-linux-gnu", "-nostdlibinc",
         "-isystem", "runtime/klee/include", "-emit-llvm", "-c", "-g", "-O1",
         "-DOSP_SYMBOLIC", *( ["-DOSP_HUNT_ARC"] if memory == "arc" else []), str(source), "-o", str(module)], ROOT / "compiler")
    if memory == "gc":
        run([str(llvm / "opt"), "-load-pass-plugin=" + str(ROOT / "compiler/bin/libosprey_gc_roots.so"),
             "-passes=osprey-gc-roots", "-verify-each", str(module), "-o", str(module)])
    out = folder / "klee"
    run([str(klee), "--libc=klee", "--ubsan-runtime", "--optimize", "--kdalloc",
                  "--external-calls=none", "--max-time=30s", "--watchdog",
                  "-link-llvm-lib=" + str(ROOT / f"compiler/bin/libosprey_runtime_klee_{memory}.bc"),
                  "--output-dir=" + str(out), str(module)])
    verify(out)
    print(f"{memory.upper()} retention and reclamation: passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--llvm", type=pathlib.Path, required=True)
    parser.add_argument("--klee", type=pathlib.Path, required=True)
    options = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="osprey-gc-roots-") as directory:
        folder = pathlib.Path(directory)
        pass_regressions(options.llvm, folder)
        for memory in ("gc", "arc"):
            regression(options.llvm, options.klee, folder, memory)


if __name__ == "__main__":
    main()
