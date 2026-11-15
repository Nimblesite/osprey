"""[MATH-CODE] The modules a package generates from its Lean sources live under
its untracked `.osprey/math/`. The gate reads their text, and in CI it runs on a
fresh checkout that never generated them, so collection archives them beside the
reports and the gate unpacks that archive before reading."""
from __future__ import annotations

import tarfile
from pathlib import Path

ARCHIVE = "math-modules.tar"
STORE = ".osprey/math"


def keep(root: Path, reports: Path) -> None:
    """Archive every generated math module under `root`, relative to it."""
    with tarfile.open(reports / ARCHIVE, "w") as archive:
        for store in sorted(root.glob(f"src/osprey/**/{STORE}")):
            archive.add(store, arcname=store.relative_to(root).as_posix())


def restore(root: Path, reports: Path) -> None:
    """Unpack the archive collection kept, if any, refusing paths outside `root`."""
    kept = reports / ARCHIVE
    if kept.is_file():
        with tarfile.open(kept) as archive:
            archive.extractall(root, filter="data")
