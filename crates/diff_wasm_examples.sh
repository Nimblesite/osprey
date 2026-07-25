#!/usr/bin/env zsh
# WebAssembly golden harness: compile every examples/tested/*.osp to
# wasm32, run it under Node's WASI host, and compare stdout to .expectedoutput.
# Usage: diff_wasm_examples.sh [--verbose] [name-filter]
set -u

# Repo root: derived from this script's location (crates/diff_wasm_examples.sh)
# so the harness runs unchanged on a dev box and in CI; override with
# OSPREY_ROOT.
ROOT=${OSPREY_ROOT:-${0:A:h}/..}
ROOT=${ROOT:A}
BIN=$ROOT/target/release/osprey
EXDIR=$ROOT/examples/tested
OUTDIR=${OSPREY_WASM_EXAMPLES_OUT:-$ROOT/target/wasm-examples}
SMOKE=$ROOT/scripts/wasm-smoke.mjs
VERBOSE=0
FILTER=""
for a in "$@"; do
  case "$a" in
    --verbose) VERBOSE=1 ;;
    *) FILTER="$a" ;;
  esac
done

pass=0; fail=0; noexp=0; builderr=0; runerr=0; skip=0
typeset -a FAILED
typeset -a SKIPPED
mkdir -p "$OUTDIR"

for f in $(find $EXDIR -name '*.osp' | sort); do
  rel=${f#$EXDIR/}
  [[ -n "$FILTER" && "$rel" != *"$FILTER"* ]] && continue
  # Expected-output precedence matches crates/diff_examples.sh exactly: the
  # per-file expectation first, then the OS-specific one, then the flavor-shared
  # <stem>.expectedoutput so an ML twin pair (foo.osp + foo.ospml) reuses ONE
  # golden file ([FLAVOR-IR-EQUIV]) — both flavors emit byte-identical output.
  base="${f%.*}"
  if [[ -f "$f.expectedoutput" ]]; then
    exp="$f.expectedoutput"
  elif [[ -f "$f.expectedoutput.$(uname -s)" ]]; then
    exp="$f.expectedoutput.$(uname -s)"
  elif [[ -f "$base.expectedoutput" ]]; then
    exp="$base.expectedoutput"
  else
    noexp=$((noexp+1))
    [[ $VERBOSE -eq 1 ]] && echo "NOEXP  $rel"
    continue
  fi

  wasm="$OUTDIR/${rel%.osp}.wasm"
  mkdir -p "${wasm:h}"
  rm -f "$wasm"

  compile_err="$OUTDIR/${rel%.osp}.compile.err"
  run_err="$OUTDIR/${rel%.osp}.run.err"
  mkdir -p "${compile_err:h}" "${run_err:h}"

  $BIN "$f" --target=wasm32 --compile -o "$wasm" 2>"$compile_err" >/dev/null
  rc=$?
  if [[ $rc -ne 0 ]]; then
    # The portable wasm runtime links every portable symbol, so an `undefined
    # symbol` link error means the program uses a feature that is intentionally
    # not ported (fibers, HTTP/WebSocket, SQLite/FFI, processes, file I/O,
    # random, or resumable continuations [WASM-TARGET-EFFECTS]). Classify those
    # as SKIP — a documented limitation, not a failure.
    # Any other build error is a real FAIL.
    sym=$(grep -m1 -oE 'undefined symbol: [A-Za-z0-9_]+' "$compile_err" | head -1)
    if [[ -n "$sym" ]]; then
      skip=$((skip+1))
      SKIPPED+=("$rel (${sym#undefined symbol: })")
      [[ $VERBOSE -eq 1 ]] && echo "SKIP   $rel — ${sym}"
    else
      fail=$((fail+1))
      builderr=$((builderr+1))
      FAILED+=("$rel")
      if [[ $VERBOSE -eq 1 ]]; then
        echo "BUILD  $rel (rc=$rc)"
        echo "  --- err ---"; head -3 "$compile_err" | sed 's/^/  /'
      fi
    fi
    continue
  fi

  expected=$(cat "$exp")
  actual=$(node "$SMOKE" "$wasm" 2>"$run_err")
  rc=$?
  expected_trim="${expected#"${expected%%[![:space:]]*}"}"; expected_trim="${expected_trim%"${expected_trim##*[![:space:]]}"}"
  actual_trim="${actual#"${actual%%[![:space:]]*}"}"; actual_trim="${actual_trim%"${actual_trim##*[![:space:]]}"}"

  if [[ $rc -eq 0 && "$actual_trim" == "$expected_trim" ]]; then
    pass=$((pass+1))
    [[ $VERBOSE -eq 1 ]] && echo "PASS   $rel"
  else
    fail=$((fail+1))
    FAILED+=("$rel")
    [[ $rc -ne 0 ]] && runerr=$((runerr+1))
    if [[ $VERBOSE -eq 1 ]]; then
      echo "FAIL   $rel (rc=$rc)"
      if [[ $rc -ne 0 ]]; then
        echo "  --- err ---"; head -3 "$run_err" | sed 's/^/  /'
      else
        echo "  expected: ${(qqq)expected_trim}"
        echo "  actual:   ${(qqq)actual_trim}"
      fi
    fi
  fi
done

echo "================================"
echo "PASS=$pass FAIL=$fail SKIP=$skip NOEXP=$noexp BUILDERR=$builderr RUNERR=$runerr (of $((pass+fail+skip+noexp)))"
if (( skip > 0 )); then
  echo "SKIPPED (non-portable feature — undefined symbol on wasm32):"
  for x in $SKIPPED; do echo "  $x"; done
fi
if (( fail > 0 )); then
  echo "FAILED:"
  for x in $FAILED; do echo "  $x"; done
fi
