#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd)
TEST_DIR=$(mktemp -d "${TMPDIR:-/tmp}/osprey-arc32-ownership.XXXXXX")
trap 'rm -rf -- "$TEST_DIR"' EXIT

"${WASM_CC:-clang}" --target=wasm32-wasip1 --sysroot="${WASI_SYSROOT:?WASI_SYSROOT is required}" \
  -O2 -std=c11 -Wall -Wextra -Werror -nodefaultlibs -lc \
  -I"$ROOT/compiler/runtime" \
  "$ROOT/compiler/runtime/memory_arc_ownership_wasm_tests.c" \
  "$ROOT/compiler/runtime/memory_arc.c" -o "$TEST_DIR/ownership.wasm"
node "$ROOT/scripts/wasm-smoke.mjs" "$TEST_DIR/ownership.wasm"
