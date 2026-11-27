"""Produce a content-addressed ZIP of a prebuilt registry Pages directory."""
from hashlib import sha256
from pathlib import Path
from tempfile import NamedTemporaryFile
from zipfile import ZipFile, ZipInfo
import sys


def archive(site: Path, destination: Path) -> Path:
    destination.mkdir(parents=True, exist_ok=True)
    with NamedTemporaryFile(dir=destination, suffix=".tmp", delete=False) as tmp:
        temporary = Path(tmp.name)
    try:
        with ZipFile(temporary, "w") as bundle:
            for source in sorted(site.rglob("*")):
                if source.is_symlink():
                    raise ValueError(f"Unexpected symbolic link: {source}")
                if source.is_file():
                    entry = ZipInfo(source.relative_to(site).as_posix())
                    entry.create_system = 3
                    entry.external_attr = 0o100644 << 16
                    bundle.writestr(entry, source.read_bytes())
        digest = sha256(temporary.read_bytes()).hexdigest()
        result = destination / f"registry-site-{digest}.zip"
        temporary.replace(result)
        return result
    finally:
        temporary.unlink(missing_ok=True)


if __name__ == "__main__":
    print(archive(Path(sys.argv[1]), Path(sys.argv[2])))
