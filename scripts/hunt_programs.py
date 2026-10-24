#!/usr/bin/env python3
"""Runnable programs, with each manifest app counted once [SYMEX-CORPUS]."""
from pathlib import Path
import sys
import tomllib

ROOTS = ('tests', 'examples', 'Book/examples', 'packages', 'registry', 'benchmarks')


def is_app(manifest):
    try:
        return bool(tomllib.loads(manifest.read_text()).get('project', {}).get('entry'))
    except (OSError, ValueError):
        return True  # Let the compiler report malformed manifests in the sweep.


def standalone(path, root):
    if 'failscompilation' in path.parts or path.with_suffix('.expectederror').exists():
        return False
    if any((parent/'osprey.toml').exists() for parent in path.parents if parent != root):
        return False
    return path.suffix in ('.osp', '.ospml')


def discover(root, roots=ROOTS, libraries=False):
    programs = set()
    for name in roots:
        directory = root/name
        for manifest in directory.rglob('osprey.toml'):
            if 'failscompilation' not in manifest.parts and (libraries or is_app(manifest)):
                programs.add(manifest.parent.relative_to(root).as_posix())
        for extension in ('*.osp', '*.ospml'):
            programs.update(p.relative_to(root).as_posix() for p in directory.rglob(extension) if standalone(p, root))
    return sorted(programs)


if __name__ == '__main__':
    print('\n'.join(discover(Path.cwd(), sys.argv[1:] or ROOTS, libraries=True)))
