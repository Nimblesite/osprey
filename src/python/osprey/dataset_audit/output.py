"""Exclusive publication: failed audits never appear as completed output datasets."""

from contextlib import contextmanager
import fcntl
import os
from pathlib import Path
import shutil
import tempfile


@contextmanager
def destination(path):
    path.parent.mkdir(parents=True, exist_ok=True)
    lock = os.open(path.parent / f".{path.name}.lock", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise RuntimeError("Another audit is writing this output directory") from error
        with _stage(path) as staging:
            yield staging
    finally:
        os.close(lock)


@contextmanager
def _stage(path):
    if path.exists():
        raise FileExistsError(f"Output already exists; choose a new directory: {path}")
    staging = Path(tempfile.mkdtemp(prefix=f".{path.name}-", dir=path.parent))
    try:
        yield staging
        if path.exists():
            raise FileExistsError(f"Output was created by another process: {path}")
        staging.rename(path)
    finally:
        if staging.exists():
            shutil.rmtree(staging)


@contextmanager
def private_file(path):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8", newline="") as stream:
        yield stream
        stream.flush()
        os.fsync(stream.fileno())
