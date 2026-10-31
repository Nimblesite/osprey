#!/usr/bin/env bash
# compile_runtime_source must work under the stock macOS bash 3.2, which
# rejects an empty array expansion under `set -u`. The Apple and Android
# runtime scripts source runtime-memory.sh with `set -euo pipefail`, so a
# unit outside OSPREY_RUNTIME_SHIMS killed the build before any archive was
# written, and bash 3.2 reported that fatal error to make as exit 0. Each
# call therefore runs in a subshell and the proof is the recorded argv, never
# the exit status.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/runtime-memory.sh

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
fake_cc() { printf '%s\n' "$@" > "$scratch/argv"; }
fail() { echo "FAIL: $1" >&2; exit 1; }

rm -f "$scratch/argv"
(OSPREY_RUNTIME_SHIMS="memory_arc" compile_runtime_source system_runtime fake_cc -O2) || true
[[ -s "$scratch/argv" ]] || fail "an unselected unit never reached the compiler"
grep -q -- '^$' "$scratch/argv" && fail "an unselected unit must not pass an empty argument"
grep -q -- '-include' "$scratch/argv" && fail "an unselected unit must not get the ARC shim"

rm -f "$scratch/argv"
(OSPREY_RUNTIME_SHIMS="memory_arc" compile_runtime_source memory_arc fake_cc -O2) || true
[[ -s "$scratch/argv" ]] || fail "a selected unit never reached the compiler"
grep -q -- '^compiler/runtime/osp_arc_shim.h$' "$scratch/argv" || fail "a selected unit must get the ARC shim"
echo "runtime-memory.sh: shim selection PASS (bash $BASH_VERSION)"

if [[ $(uname -s) == Darwin ]]; then
python3 - "$scratch" "$BASH" <<'PY'
import os
from pathlib import Path
import shutil
import subprocess
import sys

project = Path(sys.argv[1]) / "apple-cache"
for folder in ["scripts", "compiler/runtime", "compiler/bin"]:
    (project / folder).mkdir(parents=True, exist_ok=True)
for name in ["scripts/ios-runtime.sh", "scripts/runtime-memory.sh", "scripts/ios.mk", "Makefile"]:
    shutil.copyfile(name, project / name)
header = project / "compiler/runtime/cache_fixture.h"
header.write_text("#define CACHE_VALUE 1\n")
(project / "compiler/runtime/cache_fixture.c").write_text('#include "cache_fixture.h"\nint cache_fixture(void) { return CACHE_VALUE; }\n')
archive = project / "compiler/bin/libcache_fixture.a"
mirror = project / "compiler/lib/libcache_fixture.a"
command = [sys.argv[2], str(project / "scripts/ios-runtime.sh"), "macosx", "arm64-apple-macos12.0", str(archive), "-c", "-O0", "--", "cache_fixture"]
subprocess.run(command, check=True)
older = header.stat().st_mtime_ns - 2_000_000_000
os.utime(archive, ns=(older, older))
os.utime(mirror, ns=(older, older))
subprocess.run(command, check=True)
assert archive.stat().st_mtime_ns >= header.stat().st_mtime_ns, "matching fingerprints must rebuild an archive older than its headers"
assert mirror.stat().st_mtime_ns >= header.stat().st_mtime_ns, "the runtime mirror must also satisfy the compiler's freshness gate"
fresh = archive.stat().st_mtime_ns
fresh_mirror = mirror.stat().st_mtime_ns
subprocess.run(command, check=True)
assert archive.stat().st_mtime_ns == fresh, "a fresh archive must remain cached"
assert mirror.stat().st_mtime_ns == fresh_mirror, "a fresh mirror must remain cached"
print("ios-runtime.sh: stale-header rebuild and fresh-cache reuse PASS")
PY
fi
