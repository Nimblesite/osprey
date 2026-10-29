#!/usr/bin/env python3
"""Stage canonical proof exports for every workbench host [WORKBENCH-VSIX]."""
import argparse
import importlib
import json
from pathlib import Path
import shutil

PROOF = importlib.import_module("proof-openai")
ROOT = PROOF.ROOT
MANIFEST = ROOT / "apps/workbench/assets/proof-libraries.json"
LIBRARIES = "proof-libraries"


def groups():
    openai = json.loads((PROOF.CORPUS / "corpus.json").read_text())["groups"]
    strings = json.loads((ROOT / "examples/proof/kernel/exports/string_cases.json").read_text())
    return PROOF.core_groups() + openai + [strings]


def approved_exports():
    result = {}
    for group in groups():
        PROOF.check_export(group)
        data = PROOF.export_path(group).read_bytes()
        if PROOF.digest(data) != group["export_sha256"]:
            raise ValueError("proof source changed during staging")
        relative = f"{LIBRARIES}/{group['id']}.ndjson"
        if relative in result:
            raise ValueError("duplicate proof library inventory")
        result[relative] = data
    return result


def payloads():
    exports = approved_exports()
    manifest = MANIFEST.read_bytes()
    entries = json.loads(manifest)["libraries"]
    paths = [entry["path"] for entry in entries]
    if len(paths) != len(set(paths)) or set(paths) != set(exports):
        raise ValueError("proof library manifest inventory differs from approved exports")
    if any(not isinstance(entry["title"], str) or not entry["title"].strip() for entry in entries):
        raise ValueError("proof library titles must be nonempty")
    licenses = {"LICENSE.lean4": PROOF.CORE / "LICENSE.lean4",
                "LICENSE.openai-math": PROOF.CORPUS / "LICENSE.openai-math"}
    return {"proof-libraries.json": manifest, **exports,
            **{f"{LIBRARIES}/{name}": path.read_bytes() for name, path in licenses.items()}}


def safe_target(destination, relative):
    path = Path(relative)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError("proof asset path escapes the bundle")
    current = destination
    for part in ("", *path.parts):
        current = current / part
        if current.is_symlink():
            raise ValueError("proof asset staging refuses a symlink")
    return current


def verify(destination, files):
    for relative, data in files.items():
        target = safe_target(destination, relative)
        if not target.is_file() or target.read_bytes() != data:
            raise ValueError(f"missing or stale proof asset: {relative}")
    actual = {path.relative_to(destination).as_posix() for path in (destination / LIBRARIES).rglob("*")
              if path.is_file() or path.is_symlink()}
    if actual != {name for name in files if name.startswith(LIBRARIES + "/")}:
        raise ValueError("bundled proof asset inventory differs from approved exports")


def stage(destination, check=False):
    destination = Path(destination)
    files = payloads()
    folder = safe_target(destination, LIBRARIES)
    safe_target(destination, "proof-libraries.json")
    if not check:
        if folder.exists():
            shutil.rmtree(folder)
        for relative, data in files.items():
            target = safe_target(destination, relative)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
    verify(destination, files)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stage(args.destination, args.check)
    print(f"Workbench proof assets: {len(groups())} exports and 2 licenses PASS")


if __name__ == "__main__":
    main()
