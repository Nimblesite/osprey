#!/usr/bin/env zsh
# Run every assertion suite under one memory backend; used by conformance CI so
# moving a golden example into tests never drops its GC or ARC coverage.
set -u

ROOT=${OSPREY_ROOT:-${0:A:h}/..}
ROOT=${ROOT:A}
BIN=$ROOT/target/release/osprey
TESTDIR=$ROOT/tests
MEMORY=${1:-default}
pass=0
fail=0
leaky=0
typeset -a FAILED
typeset -a LEAKS
ERRFILE=$(mktemp -t osprey-test-corpus.XXXXXX)
trap 'rm -f "$ERRFILE"' EXIT INT TERM

for f in $(find $TESTDIR \( -name '*.test.osp' -o -name '*.test.ospml' \) | sort); do
  rel=${f#$ROOT/}
  $BIN "$f" --run --quiet --memory="$MEMORY" >/dev/null 2>"$ERRFILE"
  rc=$?
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
