#!/usr/bin/env python3
"""Compile the actual flavor port and compare every tiny tensor to its oracle."""
import argparse
import importlib.util
import math
import os
from pathlib import Path
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor

ROOT = Path(__file__).resolve().parents[1]
PORT = ROOT / "examples/gpt2/python"
MEMORIES = ("gc", "arc")


def reference():
    spec = importlib.util.spec_from_file_location("gpt2_oracle", ROOT / "examples/gpt2/reference.py")
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load independent GPT-2 reference")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def vector(size, scale):
    return [(i % 7) * scale for i in range(size)]


def layer(module):
    w, q, m = module.WIDTH, module.QKV, module.MLP
    return {"ln_1.weight": vector(w, 0.1), "ln_1.bias": vector(w, -0.05),
            "attn.c_attn.weight": vector(w * q, 0.01), "attn.c_attn.bias": vector(q, -0.02),
            "attn.c_proj.weight": vector(w * w, 0.02), "attn.c_proj.bias": vector(w, 0.03),
            "ln_2.weight": vector(w, 0.2), "ln_2.bias": vector(w, -0.01),
            "mlp.c_fc.weight": vector(w * m, 0.01), "mlp.c_fc.bias": vector(m, 0.02),
            "mlp.c_proj.weight": vector(m * w, -0.01), "mlp.c_proj.bias": vector(w, 0.01)}


def expected(module):
    a, b, bias = vector(6, 0.25), vector(6, -0.125), vector(2, 0.1)
    result = {"transpose": module.transpose(a, 2, 3), "matmul": module.matmul(a, b, 2, 3, 2),
              "linear": module.linear(a, 2, 3, 2, b, bias), "residual": module.residual(a, b),
              "gelu": [module.gelu((i - 3) / 2.0) for i in range(7)]}
    for t in (1, 2, 3):
        x, qkv = vector(t * module.WIDTH, 0.25), vector(t * module.QKV, 0.125)
        result[f"norm{t}"] = module.layer_norm(x, t, vector(module.WIDTH, 0.1), vector(module.WIDTH, -0.05))
        result[f"probs{t}"], result[f"attention{t}"] = module.attention(qkv, t)
    result.update({f"block.{name}": values for name, values in module.block(vector(3 * module.WIDTH, 0.25), 3, layer(module)).items()})
    return result


def project(directory):
    """The check program over the self-contained Python-flavor modules: no `.osp` source."""
    (directory / "check.py").write_text((PORT / "check.py").read_text())
    modules = ", ".join(f'"{Path(os.path.relpath(PORT / name, directory)).as_posix()}"' for name in ("backends.py", "cpu.py", "gpu.py", "model.py"))
    (directory / "osprey.toml").write_text(f'''[project]
name = "python-gpt2-check"
source_roots = [".", "{PORT.as_posix()}"]
python_sources = [{modules}]
entry = "check.py"
''')


def run(command, **kwargs):
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False, **kwargs)
    if result.returncode:
        raise AssertionError(f"{' '.join(map(str, command))}\n{result.stdout}\n{result.stderr}")
    return result


def parsed(text):
    result = {}
    for line in text.splitlines():
        if not line:
            continue
        if line.startswith("@device:"):
            continue
        if line.startswith("check stopped:"):
            raise AssertionError(line)
        name, numbers = line.split(":", 1)
        if name in result:
            raise AssertionError(f"duplicate tensor {name}")
        result[name] = [float(value) for value in numbers.split(",") if value]
    return result


def compare(actual, wanted, device):
    if actual.keys() != wanted.keys():
        raise AssertionError(f"tensor inventory differs: {actual.keys()} != {wanted.keys()}")
    for name, values in wanted.items():
        if len(actual[name]) != len(values):
            raise AssertionError(f"{name}: wrong shape")
        for index, (received, value) in enumerate(zip(actual[name], values)):
            exact = device == "host" or not transcendental(name)
            equal = received == value if exact else math.isclose(received, value, rel_tol=0.0, abs_tol=1e-9)
            if not equal:
                raise AssertionError(f"{name}[{index}]: {received!r} != {value!r}")


def transcendental(name):
    # [GPU-CORE-SCALAR]: only values downstream of exp/tanh use the 1e-9 bound.
    return name == "gelu" or name.startswith(("probs", "attention")) or name in {
        "block.probs", "block.a", "block.x2", "block.h2", "block.f", "block.u", "block.y"}


def check_binary(binary, wanted, memory, backend, device):
    env = dict(os.environ, OSPREY_GPU=device, OSPREY_ARC_DEBUG="1", OSPREY_GPU_TRACE="1")
    result = run([str(binary)], input=backend + "\n", env=env)
    check_device(result, backend, device)
    compare(parsed(result.stdout), wanted, device if backend == "gpu" else "host")
    if memory == "arc" and "[osp-arc] exit: 0 live objects," not in result.stderr:
        raise AssertionError(f"ARC retained values: {result.stderr}")


def check_device(result, backend, device):
    labels = [line.removeprefix("@device:") for line in result.stdout.splitlines() if line.startswith("@device:")]
    if len(labels) != 1:
        raise AssertionError(f"missing or duplicated backend identity: {labels}")
    if backend == "lists" or device == "host":
        expected_label = "lists" if backend == "lists" else "host"
        if labels[0] != expected_label:
            raise AssertionError(f"expected {expected_label}, got {labels[0]}")
    elif not labels[0].startswith(("metal:", "opencl:")) or "unavailable" in labels[0] or "completed" not in result.stderr:
        raise AssertionError(f"missing actual device identity/dispatch evidence: {labels[0]}\n{result.stderr}")


def compile_memory(compiler, directory, memory):
    binary = directory / f"gpt2-{memory}"
    run([str(compiler), str(directory), "--compile", f"--memory={memory}", "-o", str(binary)])
    return binary


def check_matrix(compiler, directory, wanted, device):
    with ThreadPoolExecutor(max_workers=len(MEMORIES)) as pool:
        binaries = list(pool.map(lambda memory: compile_memory(compiler, directory, memory), MEMORIES))
    jobs = [(binary, memory, backend) for binary, memory in zip(binaries, MEMORIES) for backend in ("lists", "gpu")]
    with ThreadPoolExecutor(max_workers=len(jobs)) as pool:
        list(pool.map(lambda job: check_binary(job[0], wanted, job[1], job[2], device), jobs))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--device", default="host", choices=("host", "metal", "opencl", "auto"))
    args = parser.parse_args()
    wanted = expected(reference())
    with tempfile.TemporaryDirectory(prefix="osprey-python-gpt2-", dir=ROOT / "target") as temporary:
        directory = Path(temporary)
        project(directory)
        check_matrix(args.compiler.resolve(), directory, wanted, args.device)
    print(f"matched {len(wanted)} complete tensors across both handlers and GC/ARC memory modes")


if __name__ == "__main__":
    main()
