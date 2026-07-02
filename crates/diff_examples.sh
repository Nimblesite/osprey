#!/usr/bin/env zsh
# Differential golden harness: run every examples/tested/*.osp through
# `osprey --run`, trim, and compare to the sibling .expectedoutput.
# Usage: diff_examples.sh [--verbose] [name-filter]
set -u
# Repo root: derived from this script's location (crates/diff_examples.sh) so the
# harness runs unchanged on a dev box and in CI; override with OSPREY_ROOT.
ROOT=${OSPREY_ROOT:-${0:A:h}/..}
ROOT=${ROOT:A}
BIN=$ROOT/target/release/osprey
EXDIR=$ROOT/examples/tested
VERBOSE=0
FILTER=""
for a in "$@"; do
  case "$a" in
    --verbose) VERBOSE=1 ;;
    *) FILTER="$a" ;;
  esac
done

pass=0; fail=0; noexp=0; comperr=0
typeset -a FAILED
for f in $(find $EXDIR \( -name '*.osp' -o -name '*.ospml' \) | sort); do
  rel=${f#$EXDIR/}
  [[ -n "$FILTER" && "$rel" != *"$FILTER"* ]] && continue
  # Expected-output precedence: the per-file .expectedoutput, else the
  # OS-specific .expectedoutput.<uname> (callback_stdout_demo: its subprocess
  # error text + exit code differ Darwin vs Linux), else the flavor-shared
  # <stem>.expectedoutput. The last one lets a Default/ML flavor pair
  # (foo.osp + foo.ospml) share ONE golden file ([FLAVOR-IR-EQUIV]): both
  # flavors must produce byte-identical output, so one expected file serves both.
  base="${f%.*}"
  if [[ -f "$f.expectedoutput" ]]; then
    exp="$f.expectedoutput"
  elif [[ -f "$f.expectedoutput.$(uname -s)" ]]; then
    exp="$f.expectedoutput.$(uname -s)"
  elif [[ -f "$base.osp.expectedoutput" ]]; then
    # An ML twin <stem>.ospml shares the Default twin's golden
    # <stem>.osp.expectedoutput: both flavors must run byte-identically
    # ([FLAVOR-IR-EQUIV]), so the in-place .osp golden serves the .ospml too.
    exp="$base.osp.expectedoutput"
  elif [[ -f "$base.osp.expectedoutput.$(uname -s)" ]]; then
    # Same flavor-shared rule for an OS-specific Default golden: an ML twin
    # <stem>.ospml inherits <stem>.osp.expectedoutput.<uname> when the Default
    # twin's output is OS-dependent (callback_stdout_demo's subprocess text),
    # since both flavors run byte-identically ([FLAVOR-IR-EQUIV]).
    exp="$base.osp.expectedoutput.$(uname -s)"
  elif [[ -f "$base.expectedoutput" ]]; then
    exp="$base.expectedoutput"
  else
    noexp=$((noexp+1))
    [[ $VERBOSE -eq 1 ]] && echo "NOEXP  $rel"
    continue
  fi
  # Compare whole-string-trimmed actual vs expected — a single trim on each,
  # never a per-line strip (which would drop trailing whitespace the program
  # emits).
  expected=$(cat "$exp")
  # OSPREY_RUN_FLAGS (default empty) selects a backend for conformance, e.g.
  # `OSPREY_RUN_FLAGS=--memory=gc` runs every example under the tracing GC — the
  # [MEM-BACKENDS] oracle: output must stay byte-identical. No effect when unset.
  actual=$($BIN "$f" --run ${=OSPREY_RUN_FLAGS:-} 2>/tmp/osprey_rs_err.txt)
  rc=$?
  expected_trim="${expected#"${expected%%[![:space:]]*}"}"; expected_trim="${expected_trim%"${expected_trim##*[![:space:]]}"}"
  actual_trim="${actual#"${actual%%[![:space:]]*}"}"; actual_trim="${actual_trim%"${actual_trim##*[![:space:]]}"}"
  if [[ "$actual_trim" == "$expected_trim" ]]; then
    pass=$((pass+1))
    [[ $VERBOSE -eq 1 ]] && echo "PASS   $rel"
  else
    fail=$((fail+1))
    FAILED+=("$rel")
    if [[ $VERBOSE -eq 1 ]]; then
      echo "FAIL   $rel (rc=$rc)"
      echo "  --- err ---"; head -3 /tmp/osprey_rs_err.txt | sed 's/^/  /'
    fi
  fi
done
echo "================================"
echo "PASS=$pass FAIL=$fail NOEXP=$noexp (of $((pass+fail+noexp)))"
echo "FAILED:"
for x in $FAILED; do echo "  $x"; done

# ---- must-REJECT suite: examples/failscompilation -------------------------
# Every .ospo is an ill-formed program the language defines as a compile error.
# The compiler must refuse it (nonzero exit, nothing executed). FC_EXPECTED_ESCAPES
# is a RATCHET: it counts the ill-formed programs osprey still accepts
# (validations not yet ported — effects safety, `any` rules, named-arg checks,
# print-on-record). Port a validation -> decrease the number. An INCREASE is a
# regression and fails CI. Target: 0.
# 12 -> 11: perform-argument unification ([EFFECTS-GENERIC-INSTANTIATION]) now
# rejects effect-parameter type mismatches at compile time.
FC_EXPECTED_ESCAPES=11
FCDIR=$ROOT/examples/failscompilation
fc_rej=0; fc_esc=0
typeset -a FC_ESCAPED
if [[ -z "$FILTER" && -d "$FCDIR" ]]; then
  for f in $(find $FCDIR -name '*.ospo' | sort); do
    # alarm guards an accepted program that runs (and could block on I/O).
    perl -e 'alarm 10; exec @ARGV' -- $BIN "$f" --run >/dev/null 2>&1
    if [[ $? -eq 0 ]]; then
      fc_esc=$((fc_esc+1)); FC_ESCAPED+=("${f#$FCDIR/}")
    else
      fc_rej=$((fc_rej+1))
    fi
  done
  echo "FC_REJECT=$fc_rej FC_ESCAPE=$fc_esc (of $((fc_rej+fc_esc)), ratchet allows $FC_EXPECTED_ESCAPES)"
  for x in $FC_ESCAPED; do echo "  escape: $x"; done
  if [[ $fc_esc -le $FC_EXPECTED_ESCAPES ]]; then
    echo "FC_OK"
  else
    echo "FC_REGRESSION: $fc_esc ill-formed programs accepted (ratchet: $FC_EXPECTED_ESCAPES)"
  fi
fi
