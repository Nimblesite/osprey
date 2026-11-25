#!/usr/bin/env zsh
# Sourced by run_test_corpus.sh. Implements [DOC-DOCTEST-HARNESS].
# Runnable examples compare exact stdout inside the CLI; compile-only
# examples take its type-checking path and never enter the golden counter.
# Corpus files run one by one; each package under src/osprey/packages/ with an
# example runs as a project, so its examples see the package's own imports.

# Every accepted example label, and `output` on its own: a source whose only
# fence is an orphaned output must still reach --doctests to be rejected.
DOCTEST_FENCE='```(osprey|ospml|output)'

run_corpus_doctests() {
  case $TARGET in native|wasm32) ;; *) return 0 ;; esac
  local file package count=0 failed=0
  if [[ $TARGET == wasm32 ]]; then
    prepare_doctest_wasi_host || return 1
  fi
  for file in "${FILES[@]}"; do
    grep -Eq "$DOCTEST_FENCE" "$file" && run_doctest_target "$file"
  done
  for package in "$ROOT"/src/osprey/packages/*/osprey.toml(N:h); do
    grep -rEq "$DOCTEST_FENCE" "$package/src" 2>/dev/null && run_doctest_target "$package"
  done
  echo "TEST_CORPUS_DOCTEST_PASS=$count TEST_CORPUS_DOCTEST_FAIL=$failed (floor 28)"
  [[ $failed -eq 0 && $count -ge 28 ]]
}

# Runs the examples of one file or package, adding to the caller's count and failed.
run_doctest_target() {
  local summary
  if summary=$("$BIN" "$1" --doctests "--memory=$MEMORY" "--target=$TARGET"); then
    if [[ $summary =~ '^doctests: ([0-9]+) passed, 0 failed$' ]]; then
      count=$((count + match[1]))
    else
      echo "Invalid doctest report for $1: $summary" >&2
      failed=$((failed + 1))
    fi
  else
    echo "Documentation examples failed: $1" >&2
    failed=$((failed + 1))
  fi
}

prepare_doctest_wasi_host() {
  export OSPREY_DOCTEST_WASI_SCRIPT=$SMOKE
  export OSPREY_WASM_RUN=$RESULTDIR/doctest-wasi-host
  cat > "$OSPREY_WASM_RUN" <<'HOST'
#!/bin/sh
exec node "$OSPREY_DOCTEST_WASI_SCRIPT" "$@"
HOST
  chmod +x "$OSPREY_WASM_RUN"
}
