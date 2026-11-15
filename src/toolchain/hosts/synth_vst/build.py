#!/usr/bin/env python3
"""[SYNTH-HOST-VST] Reproducible bounded SDK/worker build; never installs plug-ins."""
import argparse
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[4]
SDK = ROOT / "target/vst3sdk"
BUILD = ROOT / "target/vst-worker"
REVISION = "3cdf9ca5d1f5b1b21e0a86832aa4abe55607bd96"
SOURCE = "https://github.com/steinbergmedia/vst3sdk.git"


def run(*args, timeout=180):
    subprocess.run(args, cwd=ROOT, check=True, timeout=timeout)


def sdk():
    SDK.parent.mkdir(parents=True, exist_ok=True)
    if not SDK.exists():
        run("git", "clone", "--filter=blob:none", "--no-checkout", SOURCE, str(SDK))
        run("git", "-C", str(SDK), "checkout", "--detach", REVISION)
    actual = subprocess.check_output(
        ["git", "-C", str(SDK), "rev-parse", "HEAD"], text=True, timeout=10
    ).strip()
    if actual != REVISION:
        raise RuntimeError(f"SDK revision mismatch: expected {REVISION}, found {actual}")
    dirty = subprocess.check_output(
        ["git", "-C", str(SDK), "status", "--porcelain", "--untracked-files=no"],
        text=True, timeout=10)
    if dirty.strip():
        raise RuntimeError("Pinned SDK checkout has modified tracked files")
    run("git", "-C", str(SDK), "submodule", "update", "--init", "--depth", "1",
        "base", "cmake", "pluginterfaces", "public.sdk")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--test", action="store_true")
    args = parser.parse_args()
    sdk()
    run("cmake", "-S", str(pathlib.Path(__file__).parent), "-B", str(BUILD),
        f"-DVST3_SDK={SDK}", "-DCMAKE_BUILD_TYPE=Release")
    targets = ["osprey-vst-worker", "adelay", "mda-vst3"] if args.test else ["osprey-vst-worker"]
    run("cmake", "--build", str(BUILD), "--parallel", "4", "--target", *targets, timeout=600)
    if args.test:
        run("ctest", "--test-dir", str(BUILD), "--output-on-failure", timeout=120)


if __name__ == "__main__":
    main()
