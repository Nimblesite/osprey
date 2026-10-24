#!/usr/bin/env python3
"""Runnable programs, with each manifest app counted once [SYMEX-CORPUS]."""
from pathlib import Path
import sys
import tomllib

ROOTS = ('tests', 'examples', 'Book/examples', 'packages', 'registry', 'benchmarks')


def project(manifest):
    try:
        value = tomllib.loads(manifest.read_text()).get('project', {})
        return value if isinstance(value, dict) else None
    except (OSError, ValueError):
        return None


def is_app(manifest):
    value = project(manifest)
    return value is None or bool(value.get('entry'))


def module_roots(manifests):
    roots = set()
    for manifest in manifests:
        sources = (project(manifest) or {}).get('source_roots', ['.'])
        sources = sources if isinstance(sources, list) else ['.']
        roots.update((manifest.parent/source).resolve() for source in sources if isinstance(source, str))
    return roots


def standalone(path, root, modules=()):
    if 'failscompilation' in path.parts or path.with_suffix('.expectederror').exists():
        return False
    if any((parent/'osprey.toml').exists() for parent in path.parents if parent != root):
        return False
    if any(path.resolve().is_relative_to(source) for source in modules):
        return False
    return path.suffix in ('.osp', '.ospml') or (
        path.suffix == '.py' and '# osprey: flavor=python' in path.read_text()[:256])


def discover(root, roots=ROOTS, libraries=False):
    programs = set()
    manifests = [p for name in roots for p in (root/name).rglob('osprey.toml') if 'failscompilation' not in p.parts]
    modules = module_roots(manifests)
    for name in roots:
        directory = root/name
        for manifest in directory.rglob('osprey.toml'):
            if 'failscompilation' not in manifest.parts and (libraries or is_app(manifest)):
                programs.add(manifest.parent.relative_to(root).as_posix())
        for extension in ('*.osp', '*.ospml', '*.py'):
            programs.update(p.relative_to(root).as_posix() for p in directory.rglob(extension) if standalone(p, root, modules))
    return sorted(programs)


if __name__ == '__main__':
    print('\n'.join(discover(Path.cwd(), sys.argv[1:] or ROOTS, libraries=True)))
