#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd)
TEST_DIR=$(mktemp -d "${TMPDIR:-/tmp}/osprey-arc32-ownership.XXXXXX")
trap 'rm -rf -- "$TEST_DIR"' EXIT

compile_ownership() {
  local name=$1
  shift
  "${WASM_CC:-clang}" --target=wasm32-wasip1 --sysroot="${WASI_SYSROOT:?WASI_SYSROOT is required}" \
    -O2 -std=c11 -Wall -Wextra -Werror -nodefaultlibs -lc "$@" \
    -I"$ROOT/compiler/runtime" \
    "$ROOT/compiler/runtime/memory_arc_ownership_wasm_tests.c" \
    "$ROOT/compiler/runtime/memory_arc.c" -o "$TEST_DIR/$name.wasm"
}

check_ownership() {
  local name=$1
  compile_ownership "$@"
  node "$ROOT/scripts/wasm-smoke.mjs" "$TEST_DIR/$name.wasm"
}

check_fatal() {
  local name=$1 marker=$2
  shift 2
  compile_ownership "$name" "$@"
  if node "$ROOT/scripts/wasm-smoke.mjs" "$TEST_DIR/$name.wasm" > "$TEST_DIR/$name.log" 2>&1; then
    echo "Expected ARC bookkeeping failure to abort: $name" >&2
    cat "$TEST_DIR/$name.log" >&2
    return 1
  fi
  if ! grep -F "$marker" "$TEST_DIR/$name.log"; then
    cat "$TEST_DIR/$name.log" >&2
    return 1
  fi
}

check_ownership ownership
check_ownership allocation-failure -DOSP_ARC_TEST_PAGE_OOM=1 -Wl,--wrap=calloc
check_ownership late-allocation-failure -DOSP_ARC_TEST_PAGE_OOM=3 -Wl,--wrap=calloc

status=0
check_fatal registry-failure 'ARC allocation registry exhausted' -DOSP_ARC_TEST_REGISTRY_OOM -Wl,--wrap=calloc || status=1
check_fatal worklist-failure 'ARC release worklist exhausted' -DOSP_ARC_TEST_WORKLIST_OOM -Wl,--wrap=realloc || status=1
exit "$status"
