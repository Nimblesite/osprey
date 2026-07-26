#!/usr/bin/env zsh
# Run every assertion suite under one memory backend; used by conformance CI so
# moving a golden example into tests never drops its GC or ARC coverage.
set -u

ROOT=${OSPREY_ROOT:-${0:A:h}/..}
ROOT=${ROOT:A}
SCRIPT=${0:A}
BIN=$ROOT/target/release/osprey
TESTDIR=$ROOT/tests

run_worker() {
  local memory=$1 resultdir=$2 index=$3 file=$4
  local errfile=$resultdir/$index.stderr
  $BIN "$file" --run --quiet --memory="$memory" >/dev/null 2>"$errfile"
  local run_code=$?
  print -r -- "$run_code" >"$resultdir/$index.status"
}

if [[ ${1:-} == "--worker" ]]; then
  [[ $# -eq 5 ]] || exit 2
  run_worker "$2" "$3" "$4" "$5"
  exit 0
fi

detected_jobs() {
  local jobs=""
  command -v nproc >/dev/null && jobs=$(nproc 2>/dev/null)
  [[ "$jobs" == <-> && $jobs -gt 0 ]] || jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null)
  [[ "$jobs" == <-> && $jobs -gt 0 ]] || jobs=$(sysctl -n hw.logicalcpu 2>/dev/null)
  [[ "$jobs" == <-> && $jobs -gt 0 ]] || jobs=2
  (( jobs = jobs < 2 ? 2 : jobs ))
  print -r -- "$jobs"
}

configured_jobs() {
  local jobs=""
  if (( ${+OSPREY_TEST_JOBS} )); then
    jobs=$OSPREY_TEST_JOBS
    if [[ "$jobs" != <-> || $jobs -eq 0 ]]; then
      echo "OSPREY_TEST_JOBS must be a positive integer" >&2
      return 2
    fi
  else
    jobs=$(detected_jobs)
  fi
  print -r -- "$jobs"
}

MEMORY=${1:-default}
JOBS=$(configured_jobs)
jobs_code=$?
[[ $jobs_code -eq 0 ]] || exit $jobs_code
export OSPREY_TEST_CACHE_DIR=${OSPREY_TEST_CACHE_DIR:-${TMPDIR:-/tmp}/osprey-test-cache-v1}
pass=0
fail=0
leaky=0
typeset -a FAILED=()
typeset -a LEAKS=()
typeset -a FILES=()
RESULTDIR=$(mktemp -d -t osprey-test-corpus.XXXXXX) || exit 1
[[ -n "$RESULTDIR" && -d "$RESULTDIR" ]] || exit 1
cleanup_results() { [[ -n ${RESULTDIR:-} && -d $RESULTDIR ]] && rm -rf -- "$RESULTDIR" }
trap cleanup_results EXIT
trap 'cleanup_results; exit 130' INT TERM

while IFS= read -r -d $'\0' f; do
  FILES+=("$f")
done < <(find "$TESTDIR" \( -name '*.test.osp' -o -name '*.test.ospml' \) -print0 | LC_ALL=C sort -z)

if (( ${#FILES} > 0 )); then
  for (( index = 1; index <= ${#FILES}; index++ )); do
    printf '%s\0%s\0' "$index" "$FILES[$index]"
  done | xargs -0 -n 2 -P "$JOBS" zsh "$SCRIPT" --worker "$MEMORY" "$RESULTDIR"
  dispatch_code=$?
  if [[ $dispatch_code -ne 0 ]]; then
    echo "test corpus worker dispatch failed ($dispatch_code)" >&2
    exit 1
  fi
fi

for (( index = 1; index <= ${#FILES}; index++ )); do
  f=$FILES[$index]
  rel=${f#$ROOT/}
  ERRFILE=$RESULTDIR/$index.stderr
  STATUSFILE=$RESULTDIR/$index.status
  if [[ -r "$STATUSFILE" ]]; then
    rc=$(<$STATUSFILE)
    [[ "$rc" == <-> ]] || rc=1
  else
    rc=1
    print -r -- "missing result from test corpus worker" >"$ERRFILE"
  fi
  if [[ $rc -eq 0 ]]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    FAILED+=("$rel")
    echo "FAIL $rel ($MEMORY)"
    sed -n '1,8p' "$ERRFILE" | sed 's/^/  /'
  fi

  if [[ "$MEMORY" == "arc" ]]; then
    live=$(sed -n 's/^\[osp-arc\] exit: \([0-9]*\) live objects.*/\1/p' "$ERRFILE" | tail -1)
    if [[ -n "$live" && "$live" != 0 ]]; then
      leaky=$((leaky + 1))
      LEAKS+=("$rel ($live)")
    fi
  fi
done

echo "TEST_CORPUS_PASS=$pass TEST_CORPUS_FAIL=$fail MEMORY=$MEMORY"
for item in $FAILED; do echo "  failed: $item"; done
if [[ "$MEMORY" == "arc" ]]; then
  echo "TEST_CORPUS_ARC_LEAKY=$leaky"
  for item in $LEAKS; do echo "  leak: $item"; done
fi
[[ $fail -eq 0 && $leaky -eq 0 ]]
