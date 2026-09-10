#!/usr/bin/env zsh
# Sourced by run_test_corpus.sh. Implements [DOC-DOCTEST-HARNESS].
# Runnable examples compare exact stdout inside the CLI; compile-only
# examples take its type-checking path and never enter the golden counter.

run_corpus_doctests() {
  case $TARGET in native|wasm32) ;; *) return 0 ;; esac
  local file summary count=0 failed=0
  if [[ $TARGET == wasm32 ]]; then
    prepare_doctest_wasi_host || return 1
  fi
  for file in "${FILES[@]}"; do
    grep -Fq '```osprey' "$file" || continue
    if summary=$("$BIN" "$file" --doctests "--memory=$MEMORY" "--target=$TARGET"); then
      if [[ $summary =~ '^doctests: ([0-9]+) passed, 0 failed$' ]]; then
        count=$((count + match[1]))
      else
        echo "Invalid doctest report for $file: $summary" >&2
        failed=$((failed + 1))
      fi
    else
      echo "Documentation examples failed: $file" >&2
      failed=$((failed + 1))
    fi
  done
  echo "TEST_CORPUS_DOCTEST_PASS=$count TEST_CORPUS_DOCTEST_FAIL=$failed (floor 6)"
  [[ $failed -eq 0 && $count -ge 6 ]]
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
