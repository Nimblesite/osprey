#!/usr/bin/env zsh
# Run every assertion suite under one memory backend and one code generator;
# used by conformance CI so moving an assertion suite within tests never drops
# its GC, ARC or wasm coverage.
#
#   run_test_corpus.sh [default|gc|arc]     native backend, chosen allocator
#   OSPREY_TARGET=wasm32 run_test_corpus.sh wasm32 backend under Node's WASI
#
# Two independent checks run per program, and BOTH must hold:
#
#   1. Exit status — the in-language `test(...)` assertions passed.
#   2. Byte-exact stdout vs the sibling `.expectedoutput` golden.
#
# Check 2 is not redundant. An assertion can only state a property someone
# thought to write down as a value; a golden pins the ENTIRE observable output,
# including properties no value assertion can express — the ORDER prints are
# emitted in, their exact formatting, and the TAP tally that reveals an
# assertion having been silently deleted. `fiber_determinism` is the canonical
# case: its assertion sums three fiber results, and a sum is order-blind, so
# assertions alone cannot fail when the deterministic scheduler stops being
# deterministic. Only the golden can. Implements [CONCURRENCY-DETERMINISTIC].
#
# Comparing the same golden under every memory backend is also the
# [MEM-BACKENDS] [MEM-OPAQUE] oracle: swapping the allocator must not perturb
# one byte of program output. Comparing it under wasm32 is the same oracle for
# the second code generator [WASM-TARGET].
set -u

ROOT=${OSPREY_ROOT:-${0:A:h}/..}
ROOT=${ROOT:A}
SCRIPT=${0:A}
BIN=$ROOT/target/release/osprey
TESTDIR=$ROOT/tests
SMOKE=$ROOT/scripts/wasm-smoke.mjs

# Which backend executes each program: `native` runs it through the compiler's
# own `--run`, `wasm32` compiles it to a WebAssembly module and executes that
# under Node's WASI host. Same corpus, same goldens, second code generator —
# so the wasm backend is held to the byte-exact output the native one produces
# [WASM-TARGET]. Set with OSPREY_TARGET=wasm32.
TARGET=${OSPREY_TARGET:-native}

# The Node version rule is NOT restated here: `scripts/wasm-smoke.mjs`
# re-executes itself under a sound interpreter when this host's is too old
# ([WASM-TARGET-NODE]). One copy of that policy, so this harness cannot drift
# from it — an earlier copy here is exactly what let a box on Node 22 report the
# RUNNER's use-after-free as every program in the corpus failing.
NODE=node

# Status sentinel for a program the wasm32 target deliberately cannot link.
SKIP_STATUS=skip

# Reviewed, committed record of every program allowed to link-fail on wasm32 and
# the symbol it fails on. Compared EXACTLY after the run — see the check below.
WASM_MANIFEST=$TESTDIR/WASM_UNPORTABLE.txt

# Anti-regression ratchet: the number of PROGRAMS that must be golden-compared.
# Goldens were once deleted wholesale during a corpus migration and nothing
# noticed, because a missing golden silently degrades to "not compared".
# Silence is not success — if coverage ever drops below this floor the harness
# FAILS rather than quietly checking less than it used to.
#
# Natively all 203 programs are covered by 105 golden files: 98 are shared by a
# Default/ML flavor pair, 7 belong to a program with no twin. On wasm32 the 61
# programs blocked on a capability WASI does not have are skipped — each named
# in tests/WASM_UNPORTABLE.txt — leaving 142.
# Ratchet UP as goldens are added; never lower it to turn a red build green.
if [[ $TARGET == wasm32 ]]; then
  GOLDEN_MIN=${OSPREY_GOLDEN_MIN:-142}
else
  GOLDEN_MIN=${OSPREY_GOLDEN_MIN:-203}
fi

# [GPU-KERNEL-EXTRACT] differential. The extracted-kernel lowering and the
# pre-stage-3 inlined host-loop lowering are two code generators for the same
# semantics, so the GPU suites must produce byte-identical output under both.
# This re-runs ONLY tests/core/gpu under the OPPOSITE lowering and compares it
# to the stdout the main pass already captured — a second oracle for the same
# goldens at the cost of eighteen programs, not a second corpus.
GPU_SUITE_DIR=$TESTDIR/core/gpu
# Canonicalized exactly as the compiler's mode_of: surrounding whitespace is
# ignored, empty keeps extraction, anything else is an error. Selecting the
# alternate from the RAW string once let `OSPREY_GPU_KERNELS=" inline "` run
# inline against itself — the compiler trimmed, the shell did not.
GPU_KERNELS_MODE=${OSPREY_GPU_KERNELS:-extract}
GPU_KERNELS_MODE=${GPU_KERNELS_MODE//[[:space:]]/}
[[ -n $GPU_KERNELS_MODE ]] || GPU_KERNELS_MODE=extract
case $GPU_KERNELS_MODE in
  extract) GPU_ALT_MODE=inline ;;
  inline) GPU_ALT_MODE=extract ;;
  *)
    echo "OSPREY_GPU_KERNELS=$GPU_KERNELS_MODE: expected 'extract' or 'inline'" >&2
    exit 2
    ;;
esac
# Nine suites x two flavors. Ratchet UP; never lower it to turn a build green.
GPU_MODE_MIN=${OSPREY_GPU_MODE_MIN:-18}

# Exit code run_wasm uses to say "not a failure, an unported feature". Distinct
# from any status the compiler or Node can return on its own.
SKIP_CODE=200

# Compile to wasm32 and execute the module under Node's WASI host. The portable
# runtime archive links every symbol it ports, so an `undefined symbol` link
# error means the program uses a feature deliberately left off the wasm target
# (fibers, HTTP/WebSocket, processes, FFI, file I/O, random, or resumable
# continuations [WASM-TARGET-EFFECTS]) — a documented limitation, reported as
# SKIP. Any OTHER build error is a real failure and must stay one.
run_wasm() {
  local file=$1 out=$2 err=$3 module=$4
  if ! $BIN "$file" --target=wasm32 --compile -o "$module" >"$err" 2>&1; then
    grep -qE 'undefined symbol: [A-Za-z0-9_]+' "$err" && return $SKIP_CODE
    return 1
  fi
  "$NODE" "$SMOKE" "$module" >"$out" 2>"$err"
}

run_worker() {
  local memory=$1 resultdir=$2 index=$3 file=$4
  local errfile=$resultdir/$index.stderr out=$resultdir/$index.stdout
  local run_code
  if [[ $TARGET == wasm32 ]]; then
    run_wasm "$file" "$out" "$errfile" "$resultdir/$index.wasm"
    run_code=$?
    [[ $run_code -eq $SKIP_CODE ]] && run_code=$SKIP_STATUS
  else
    $BIN "$file" --run --quiet --memory="$memory" >"$out" 2>"$errfile"
    run_code=$?
  fi
  print -r -- "$run_code" >"$resultdir/$index.status"
}

# Golden precedence for a program at $1, echoing the chosen path (empty if none):
#   1. <file>.expectedoutput                     — per-file golden
#   2. <file>.expectedoutput.<uname>             — OS-specific output
#   3. <stem>.test.osp.expectedoutput            — ML twin shares the Default golden
#   4. <stem>.test.osp.expectedoutput.<uname>    — ...and its OS-specific form
# Rules 3 and 4 are what make a Default/ML pair share ONE file: both flavors are
# required to run byte-identically, so one golden proves it ([FLAVOR-IR-EQUIV]).
golden_for() {
  local file=$1 stem=${1%.*} os
  os=$(uname -s)
  for candidate in \
    "$file.expectedoutput" \
    "$file.expectedoutput.$os" \
    "$stem.osp.expectedoutput" \
    "$stem.osp.expectedoutput.$os"
  do
    [[ -f "$candidate" ]] && { print -r -- "$candidate"; return 0 }
  done
  return 1
}

# The single ARC exit sentinel in a stderr transcript: echoes the live-object
# count only when EXACTLY one `[osp-arc] exit: N live objects` line is present.
# Absent or repeated sentinels echo nothing, which every caller treats as a
# failed check — a run that never printed the sentinel (crashed runtime,
# swallowed stderr) must read as unverified, never as leak-free.
arc_live_count() {
  local matches
  matches=$(sed -n 's/^\[osp-arc\] exit: \([0-9]*\) live objects.*/\1/p' "$1")
  [[ -n "$matches" && "$matches" != *$'\n'* ]] && print -r -- "$matches"
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
# The ARC leak oracle reads the runtime's exit report, which memory_arc.c
# arms only under OSPREY_ARC_DEBUG. The harness arms it ITSELF whenever it
# audits the arc backend: the sentinel check below is strict (exactly one
# line, value 0, for every passing run in BOTH kernel lowerings), and an
# oracle that depends on the caller remembering an env var is fail-open by
# construction — an unarmed run would read as leak-free.
[[ "$MEMORY" == "arc" ]] && export OSPREY_ARC_DEBUG=1
JOBS=$(configured_jobs)
jobs_code=$?
[[ $jobs_code -eq 0 ]] || exit $jobs_code
export OSPREY_TEST_CACHE_DIR=${OSPREY_TEST_CACHE_DIR:-${TMPDIR:-/tmp}/osprey-test-cache-v1}
pass=0
fail=0
leaky=0
skipped=0
golden_pass=0
golden_fail=0
golden_missing=0
typeset -a FAILED=()
typeset -a SKIPPED=()
typeset -a LEAKS=()
typeset -a GOLDEN_FAILED=()
typeset -a GOLDEN_MISSING=()
typeset -a FILES=()
RESULTDIR=$(mktemp -d -t osprey-test-corpus.XXXXXX) || exit 1
[[ -n "$RESULTDIR" && -d "$RESULTDIR" ]] || exit 1
cleanup_results() { [[ -n ${RESULTDIR:-} && -d $RESULTDIR ]] && rm -rf -- "$RESULTDIR" }
trap cleanup_results EXIT
trap 'cleanup_results; exit 130' INT TERM

while IFS= read -r -d $'\0' f; do
  FILES+=("$f")
done < <(find "$TESTDIR" \( -name '*.test.osp' -o -name '*.test.ospml' \) -print0 | LC_ALL=C sort -z)

# Programs that contend for an OS resource are run one at a time. Two kinds do:
#
#   * Socket binders. A Default/ML pair is ONE program written on two surfaces,
#     so both hard-code the SAME port (18280, 18107, 18095, 18080, 18099 — each
#     unique per program, each shared by its pair). Run concurrently, one twin
#     loses the bind and prints a connection failure instead of its transcript.
#     The port is printed, so the twins cannot just be given different ones
#     without breaking their shared golden.
#   * Subprocess spawners. These interleave a child's stdout with their own via
#     a callback, so the ORDER of the merged transcript shifts with machine load.
#
# In both cases the program still EXITS ZERO while printing the wrong thing —
# which is precisely why an exit-status-only corpus stayed green on a wrong
# transcript, and why the goldens are what surfaced it. Serializing costs a few
# seconds and removes the race rather than hiding it behind a skip list.
serialized=()
concurrent=()
for (( index = 1; index <= ${#FILES}; index++ )); do
  if grep -qE 'httpListen|websocketListen|spawnProcess' -- "$FILES[$index]" 2>/dev/null; then
    serialized+=("$index" "$FILES[$index]")
  else
    concurrent+=("$index" "$FILES[$index]")
  fi
done

# Dispatch one batch of (index, file) pairs at the given parallelism, writing
# each worker's transcript into $dir. The result directory is a parameter so the
# alternate-lowering pass below can collect its own transcripts without
# overwriting the main pass's.
dispatch_batch() {
  local jobs=$1 dir=$2; shift 2
  (( $# > 0 )) || return 0
  printf '%s\0' "$@" | xargs -0 -n 2 -P "$jobs" zsh "$SCRIPT" --worker "$MEMORY" "$dir"
}

if (( ${#FILES} > 0 )); then
  dispatch_batch "$JOBS" "$RESULTDIR" "${concurrent[@]}" \
    && dispatch_batch 1 "$RESULTDIR" "${serialized[@]}"
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
    # A wasm SKIP is neither pass nor fail, and has no output to compare: the
    # program was never built, so drop it before the numeric coercion below.
    if [[ "$rc" == "$SKIP_STATUS" ]]; then
      skipped=$((skipped + 1))
      sym=$(grep -m1 -oE 'undefined symbol: [A-Za-z0-9_]+' "$ERRFILE" | sed 's/undefined symbol: //')
      SKIPPED+=("$rel ${sym:-UNKNOWN}")
      continue
    fi
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

  # Golden comparison is independent of the exit status: a program whose
  # assertions all pass can still have stopped printing in the right order.
  # `cmp` on the raw files — byte-exact means byte-exact, including trailing
  # whitespace and the final newline; whole-string trimming once hid both.
  if GOLDEN=$(golden_for "$f"); then
    if cmp -s -- "$RESULTDIR/$index.stdout" "$GOLDEN"; then
      golden_pass=$((golden_pass + 1))
    else
      golden_fail=$((golden_fail + 1))
      GOLDEN_FAILED+=("$rel")
      echo "GOLDEN-MISMATCH $rel ($MEMORY) vs ${GOLDEN#$ROOT/}"
      diff -u "$GOLDEN" "$RESULTDIR/$index.stdout" | sed -n '1,12p' | sed 's/^/  /'
    fi
  else
    golden_missing=$((golden_missing + 1))
    GOLDEN_MISSING+=("$rel")
  fi

  # Only a passing run owes a sentinel — a program that never built has no
  # runtime to print one and is already red above. For a passing run the
  # sentinel is REQUIRED to be present and zero; parsing "no line" as "no
  # leak" is how a crashed exit path would read as leak-free.
  if [[ "$MEMORY" == "arc" && $rc -eq 0 ]]; then
    live=$(arc_live_count "$ERRFILE")
    if [[ "$live" != "0" ]]; then
      leaky=$((leaky + 1))
      LEAKS+=("$rel (${live:-no sentinel})")
    fi
  fi
done

echo "TEST_CORPUS_PASS=$pass TEST_CORPUS_FAIL=$fail MEMORY=$MEMORY TARGET=$TARGET"
for item in $FAILED; do echo "  failed: $item"; done
skips_ok=1
if [[ $TARGET == wasm32 ]]; then
  # The skip set is PINNED, not merely counted. A skip is a hole in coverage;
  # the only thing that makes one acceptable is that a human agreed to it in
  # review. tests/WASM_UNPORTABLE.txt records every program that may link-fail
  # on wasm32 AND the symbol it fails on, and this compares the actual set to it
  # EXACTLY — in both directions:
  #
  #   * a NEW skip fails the build, so nobody can quietly make a program
  #     unportable, or add one that never runs on this target;
  #   * a REMOVED skip also fails, forcing the manifest to ratchet DOWN when a
  #     feature is ported instead of leaving a stale entry as cover;
  #   * a CHANGED symbol fails, so a program cannot start skipping for a
  #     different — possibly accidental — reason under the same line.
  #
  # There is no regeneration flag on purpose. Editing this file is a deliberate,
  # reviewable act; a `--update` switch would turn every new hole into one
  # keystroke. [WASM-TARGET-EFFECTS]
  echo "TEST_CORPUS_WASM_SKIPPED=$skipped (pinned by ${WASM_MANIFEST#$ROOT/})"
  for item in $SKIPPED; do echo "  wasm skip: $item"; done
  actual=${(F)SKIPPED}
  actual=$(print -r -- "$actual" | LC_ALL=C sort)
  if [[ -f "$WASM_MANIFEST" ]]; then
    expected=$(grep -vE '^[[:space:]]*(#|$)' "$WASM_MANIFEST" | LC_ALL=C sort)
  else
    expected=""
    echo "MISSING $WASM_MANIFEST — the skip set is unpinned." >&2
  fi
  if [[ "$actual" != "$expected" ]]; then
    skips_ok=0
    echo "WASM SKIP SET CHANGED. Skips are holes; this set is reviewed, not inferred." >&2
    echo "Port the feature, or justify the change and edit ${WASM_MANIFEST#$ROOT/}:" >&2
    diff -u <(print -r -- "$expected") <(print -r -- "$actual") | sed -n '3,$p' | sed 's/^/  /' >&2
  fi
fi
echo "TEST_CORPUS_GOLDEN_PASS=$golden_pass TEST_CORPUS_GOLDEN_FAIL=$golden_fail TEST_CORPUS_GOLDEN_MISSING=$golden_missing (floor $GOLDEN_MIN)"
for item in $GOLDEN_FAILED; do echo "  golden mismatch: $item"; done
# Report what is NOT covered, and FAIL on it. A program with no golden is
# compared on exit status alone, and staying silent about that is how the
# coverage vanished the first time. This restores the deleted differential
# target's `NOEXP=0` assertion: every program that ran must have been compared,
# not merely most of them. (A wasm SKIP never reaches here — it is dropped
# before the comparison because it was never built.)
for item in $GOLDEN_MISSING; do echo "  no golden: $item"; done
if [[ $golden_missing -gt 0 ]]; then
  echo "MISSING GOLDENS: $golden_missing program(s) ran with nothing to compare against." >&2
  echo "Add a sibling .expectedoutput; an uncompared program is not a covered one." >&2
fi
golden_total=$((golden_pass + golden_fail))
golden_floor_ok=1
if [[ $golden_total -lt $GOLDEN_MIN ]]; then
  golden_floor_ok=0
  echo "GOLDEN FLOOR BREACHED: $golden_total goldens present, minimum is $GOLDEN_MIN." >&2
  echo "Goldens pin output properties assertions cannot express (print ORDER, exact" >&2
  echo "formatting, the TAP tally). Restore the missing goldens; do not lower the floor." >&2
fi
if [[ "$MEMORY" == "arc" ]]; then
  echo "TEST_CORPUS_ARC_LEAKY=$leaky"
  for item in $LEAKS; do echo "  leak: $item"; done
fi

# [GPU-KERNEL-EXTRACT]: re-run the GPU suites under the opposite kernel lowering
# and require the SAME transcript. Extraction is a lowering choice, never a
# semantic one, so a divergence here is a codegen bug and nothing else.
gpu_mode_fail=0
gpu_mode_pairs=0
typeset -a GPU_MODE_FAILED=()
alt_pairs=()
for (( index = 1; index <= ${#FILES}; index++ )); do
  [[ $FILES[$index] == $GPU_SUITE_DIR/* ]] && alt_pairs+=("$index" "$FILES[$index]")
done
if (( ${#alt_pairs} > 0 )); then
  ALTDIR=$RESULTDIR/altmode
  mkdir -p "$ALTDIR" || exit 1
  export OSPREY_GPU_KERNELS=$GPU_ALT_MODE
  dispatch_batch "$JOBS" "$ALTDIR" "${alt_pairs[@]}"
  unset OSPREY_GPU_KERNELS
  for (( i = 1; i <= ${#alt_pairs}; i += 2 )); do
    index=$alt_pairs[$i]
    rel=${alt_pairs[$((i + 1))]#$ROOT/}
    # A wasm SKIP was never built, so it has no transcript to compare.
    if [[ -r "$ALTDIR/$index.status" && "$(<$ALTDIR/$index.status)" == "$SKIP_STATUS" ]]; then
      continue
    fi
    gpu_mode_pairs=$((gpu_mode_pairs + 1))
    # The oracle is AGREEMENT on the whole observable outcome, not stdout
    # alone: exit status, raw transcript bytes, and (under ARC) the leak
    # sentinel. Comparing stdout via command substitution once let the
    # alternate lowering crash, leak, or drop a trailing line unnoticed.
    main_rc=1 alt_rc=1
    [[ -r "$RESULTDIR/$index.status" ]] && main_rc=$(<$RESULTDIR/$index.status)
    [[ -r "$ALTDIR/$index.status" ]] && alt_rc=$(<$ALTDIR/$index.status)
    mode_diverged=""
    if [[ "$alt_rc" != "$main_rc" ]]; then
      mode_diverged="exit $alt_rc vs $main_rc"
    elif ! cmp -s -- "$RESULTDIR/$index.stdout" "$ALTDIR/$index.stdout"; then
      mode_diverged="transcripts differ"
    elif [[ "$MEMORY" == "arc" && "$main_rc" == "0" ]]; then
      alt_live=$(arc_live_count "$ALTDIR/$index.stderr")
      [[ "$alt_live" == "0" ]] || mode_diverged="alternate ARC sentinel: ${alt_live:-missing}"
    fi
    if [[ -n "$mode_diverged" ]]; then
      gpu_mode_fail=$((gpu_mode_fail + 1))
      GPU_MODE_FAILED+=("$rel")
      echo "GPU-KERNEL-MODE-MISMATCH $rel ($MEMORY, $GPU_KERNELS_MODE vs $GPU_ALT_MODE): $mode_diverged"
      diff -u "$RESULTDIR/$index.stdout" "$ALTDIR/$index.stdout" | sed -n '1,12p' | sed 's/^/  /'
    fi
  done
fi
echo "TEST_CORPUS_GPU_MODE_PASS=$((gpu_mode_pairs - gpu_mode_fail)) TEST_CORPUS_GPU_MODE_FAIL=$gpu_mode_fail (alt lowering: $GPU_ALT_MODE, floor $GPU_MODE_MIN)"
for item in $GPU_MODE_FAILED; do echo "  kernel-mode mismatch: $item"; done
gpu_mode_floor_ok=1
if [[ $gpu_mode_pairs -lt $GPU_MODE_MIN ]]; then
  gpu_mode_floor_ok=0
  echo "GPU KERNEL-MODE FLOOR BREACHED: $gpu_mode_pairs compared, minimum is $GPU_MODE_MIN." >&2
  echo "Both kernel lowerings must stay exercised; do not lower the floor." >&2
fi

[[ $fail -eq 0 && $leaky -eq 0 && $golden_fail -eq 0 && $golden_missing -eq 0 \
   && $golden_floor_ok -eq 1 && $skips_ok -eq 1 \
   && $gpu_mode_fail -eq 0 && $gpu_mode_floor_ok -eq 1 ]]
