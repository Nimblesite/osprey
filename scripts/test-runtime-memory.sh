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
