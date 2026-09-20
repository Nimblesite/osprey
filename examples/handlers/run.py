#!/usr/bin/env python3
"""Run the real language tools; --check compares their actual output."""

import argparse
import os
from pathlib import Path
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
BUILD = ROOT / "target" / "handler-demo"
TOOLS = ROOT / "target" / "demo-tools"
LANGUAGES = ("osprey", "osprey-ml", "koka", "ocaml", "eff", "effekt")


def command(arguments, directory, log_name):
    result = subprocess.run(list(map(str, arguments)), cwd=directory, text=True,
                            capture_output=True, timeout=120)
    (BUILD / f"{log_name}.log").write_text(result.stdout + result.stderr)
    if result.returncode:
        raise SystemExit(f"{log_name} failed:\n{result.stdout}{result.stderr}")
    return result.stdout


def tool(name, fallback):
    location = os.environ.get(name.upper()) or shutil.which(name) or str(fallback)
    if not Path(location).is_file():
        raise SystemExit(f"Missing {name}: install it or set {name.upper()} to its executable (see README.md)")
    return location


def run(language, demo):
    directory = BUILD / language
    directory.mkdir(parents=True, exist_ok=True)
    extension = {"osprey": "osp", "osprey-ml": "ospml", "koka": "kk", "ocaml": "ml", "eff": "eff", "effekt": "effekt"}[language]
    source = HERE / f"{demo}.{extension}"
    if language.startswith("osprey"):
        return command([os.environ.get("OSPREY", ROOT / "target/release/osprey"), source, "--run"], ROOT, language)
    if language == "eff":
        output = command([tool("eff", TOOLS / "eff/eff.exe"), "--no-wrapper", source], ROOT, language)
        # Eff echoes top-level bindings and expression types. Keep the complete
        # interpreter transcript in target/handler-demo/eff.log.
        return "".join(line for line in output.splitlines(keepends=True)
                       if not line.startswith(("val ", "- : ")))
    if language == "effekt":
        return command([tool("effekt", TOOLS / "effekt/node_modules/.bin/effekt"), source], directory, language)
    executable = directory / demo
    if language == "koka":
        command([tool("koka", TOOLS / "koka/bin/koka"), "-o", executable, source], directory, "koka-build")
        executable.chmod(executable.stat().st_mode | 0o100)
    else:
        local_source = directory / source.name
        shutil.copy2(source, local_source)
        command([tool("ocamlopt", Path("/opt/homebrew/bin/ocamlopt")), "-o", executable, local_source], directory, "ocaml-build")
    return command([executable], directory, language)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("language", nargs="?", default="all", choices=("all", *LANGUAGES))
    parser.add_argument("--demo", default="handlers", choices=("handlers", "semantics", "returns"))
    parser.add_argument("--check", action="store_true", help="assert the checked-in expected output")
    args = parser.parse_args()
    BUILD.mkdir(parents=True, exist_ok=True)
    selected = LANGUAGES if args.language == "all" else (args.language,)
    failed = []
    for language in selected:
        output = run(language, args.demo)
        print(f"[{language}]\n{output}", end="" if output.endswith("\n") else "\n")
        if args.check:
            expected = (HERE / f"{args.demo}.expectedoutput").read_text()
            if output != expected:
                failed.append(language)
    if failed:
        raise SystemExit(f"Output differs from {args.demo}.expectedoutput: {', '.join(failed)}")


if __name__ == "__main__":
    main()
