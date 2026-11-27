#!/usr/bin/env zsh
# Sourced by run_test_corpus.sh. Implements [DOC-DOCTEST-HARNESS].
# Runnable examples compare exact stdout inside the CLI; compile-only
# examples take its type-checking path and never enter the golden counter.
# Corpus files run one by one; each package under src/osprey/packages/ with an
# example runs as a project, so its examples see the package's own imports.
# Targets run $JOBS at a time (each CLI names its scratch files by process id),
# and their reports are read back in order, so the log matches a serial run.

zmodload zsh/parameter

# Every accepted example label, and `output` on its own: a source whose only
# fence is an orphaned output must still reach --doctests to be rejected.
DOCTEST_FENCE='```(osprey|ospml|output)'

run_corpus_doctests() {
  case $TARGET in native|wasm32) ;; *) return 0 ;; esac
  local file package count=0 failed=0 index limit=${JOBS:-1}
  (( limit >= 1 )) || limit=1
  local -a targets
  if [[ $TARGET == wasm32 ]]; then
    prepare_doctest_wasi_host || return 1
  fi
  for file in "${FILES[@]}"; do
    grep -Eq "$DOCTEST_FENCE" "$file" && targets+=("$file")
  done
  for package in "$ROOT"/src/osprey/packages/*/osprey.toml(N:h); do
    grep -rEq "$DOCTEST_FENCE" "$package/src" 2>/dev/null && targets+=("$package")
  done
  for (( index = 1; index <= ${#targets}; index++ )); do
    while (( ${#${(M)jobstates:#running*}} >= limit )); do sleep 0.05; done
    run_doctest_worker "$targets[$index]" "$RESULTDIR/doctest-$index" &
  done
  wait
  for (( index = 1; index <= ${#targets}; index++ )); do
    read_doctest_report "$targets[$index]" "$RESULTDIR/doctest-$index"
  done
  echo "TEST_CORPUS_DOCTEST_PASS=$count TEST_CORPUS_DOCTEST_FAIL=$failed (floor 28)"
  [[ $failed -eq 0 && $count -ge 28 ]]
}

# Runs the examples of one file or package, keeping its report, errors and exit status.
run_doctest_worker() {
  "$BIN" "$1" --doctests "--memory=$MEMORY" "--target=$TARGET" > "$2.out" 2> "$2.err"
  print -r -- $? > "$2.rc"
}

# Adds one target's report to the caller's count and failed.
read_doctest_report() {
  local summary=$(<"$2.out") rc=$(<"$2.rc" 2>/dev/null)
  cat "$2.err" >&2
  if [[ $rc != 0 ]]; then
    echo "Documentation examples failed: $1" >&2
    failed=$((failed + 1))
  elif [[ $summary =~ '^doctests: ([0-9]+) passed, 0 failed$' ]]; then
    count=$((count + match[1]))
  else
    echo "Invalid doctest report for $1: $summary" >&2
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
