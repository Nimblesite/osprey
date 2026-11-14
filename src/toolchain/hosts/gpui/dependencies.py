#!/usr/bin/env python3
"""Stage checksum-pinned upstream sources with reviewed dependency backports."""
import hashlib
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import tomllib
from urllib.request import urlopen

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
DESTINATION = ROOT / "target/gpui-dependencies"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def source_directory(destination, packages):
    """One immutable source identity for a verified set, with sibling crate paths preserved."""
    pins = sorted((package["name"], package["version"], package["treeSha256"]) for package in packages)
    return destination / digest(json.dumps(pins, separators=(",", ":")).encode())


def tree_digest(root):
    files = tuple(root.rglob("*"))
    if root.is_symlink() or any(p.is_symlink() for p in files):
        raise ValueError(f"Dependency source contains a symlink: {root}")
    rows = ((p.relative_to(root).as_posix(), digest(p.read_bytes()))
            for p in files if p.is_file())
    return digest("".join(f"{name}\0{sha}\n" for name, sha in sorted(rows)).encode())


def inside(root, relative):
    parts = relative.split("/")
    if not relative or any(p in ("", ".", "..") for p in parts) or "\\" in relative or ":" in relative:
        raise ValueError(f"Invalid dependency source path: {relative}")
    return root.joinpath(*parts)


def staged_directory(destination, package):
    for field in ("name", "version"):
        if inside(destination, package[field]).parent != destination:
            raise ValueError(f"Invalid dependency source path: {package[field]}")
    return inside(destination, f"{package['name']}-{package['version']}")


def fetch_archive(package):
    name = f"{package['name']}-{package['version']}.crate"
    cargo = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    cached = next((cargo / "registry/cache").glob(f"*/{name}"), None)
    if cached is not None:
        return cached.read_bytes()
    url = f"https://static.crates.io/crates/{package['name']}/{name}"
    with urlopen(url, timeout=30) as response:
        return response.read()


def extract(archive, root, prefix):
    with tarfile.open(fileobj=io.BytesIO(archive)) as source:
        for member in source:
            if not member.name.startswith(prefix + "/"):
                raise ValueError(f"Unexpected dependency archive entry: {member.name}")
            target = inside(root, member.name[len(prefix) + 1:])
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            elif member.isfile():
                target.parent.mkdir(parents=True, exist_ok=True)
                with source.extractfile(member) as content:
                    target.write_bytes(content.read())
                target.chmod(member.mode & 0o777)
            else:
                raise ValueError(f"Unsupported dependency archive entry: {member.name}")


def apply_edits(root, edits):
    for edit in edits:
        file = inside(root, edit["file"])
        before, after = "".join(edit["before"]).encode(), "".join(edit["after"]).encode()
        content = file.read_bytes()
        if not before or content.count(before) != 1:
            raise ValueError(f"Dependency backport preimage mismatch: {edit['file']}")
        file.write_bytes(content.replace(before, after, 1))


# Cargo rebuilds a path dependency whose sources are newer than its last build, so a source staged
# afresh on each runner would rebuild GPUI and everything above it although the host's build is
# cached. The verified source set has a content-addressed path, so changed bytes change Cargo's
# source identity. Within that immutable identity, every path uses 2000-01-01 UTC.
STAGED_MTIME = 946684800


# `extract` writes only directories and regular files, so no path here is a link; Windows cannot
# set a time without following one anyway.
def settle_times(root):
    for path in sorted(root.rglob("*"), reverse=True):
        os.utime(path, (STAGED_MTIME, STAGED_MTIME))
    os.utime(root, (STAGED_MTIME, STAGED_MTIME))


def verify_tree(root, expected):
    if tree_digest(root) != expected:
        raise ValueError(f"Dependency source integrity mismatch: {root}")


def declared_sources(manifest):
    patches = tomllib.loads(manifest.read_text())["patch"]["crates-io"]
    if any(set(spec) != {"path"} for spec in patches.values()):
        raise ValueError(f"Unsupported dependency backport inventory: {manifest}")
    return {name: (manifest.parent / spec["path"]).resolve() for name, spec in patches.items()}


def verify_inventory(packages, destination, manifests):
    if not packages:
        raise ValueError("Empty dependency backport inventory")
    expected = {p["name"]: staged_directory(destination, p).resolve() for p in packages}
    if len(expected) != len(packages):
        raise ValueError("Duplicate dependency backport inventory")
    for manifest in manifests:
        if declared_sources(manifest) != expected:
            raise ValueError(f"Dependency backport inventory mismatch: {manifest}")


def publish(root, target, expected):
    try:
        root.rename(target)
    except OSError:
        if not target.exists():
            raise
        verify_tree(target, expected)


def stage(package, destination, patches):
    target = staged_directory(destination, package)
    if target.exists():
        verify_tree(target, package["treeSha256"])
        return target
    archive = fetch_archive(package)
    if digest(archive) != package["archiveSha256"]:
        raise ValueError(f"Dependency archive checksum mismatch: {package['name']}")
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".source-", dir=destination) as temporary:
        root = Path(temporary)
        extract(archive, root, target.name)
        apply_edits(root, json.loads(inside(patches, package["edits"]).read_text()))
        verify_tree(root, package["treeSha256"])
        settle_times(root)
        publish(root, target, package["treeSha256"])
    return target


def main():
    packages = json.loads((HERE / "dependencies.json").read_text())["packages"]
    destination = source_directory(DESTINATION, packages)
    manifests = [HERE / "Cargo.toml", ROOT / "src/osprey/examples/gpui/painting/reference/Cargo.toml"]
    verify_inventory(packages, destination, manifests)
    for package in packages:
        stage(package, destination, HERE)
    print(f"GPUI dependency source integrity: {len(packages)} backports PASS")


if __name__ == "__main__":
    main()
