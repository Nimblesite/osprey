#!/usr/bin/env zsh
# The proof speed gate of [PROOF-SPEED] (spec 0049): for every case under
# benchmarks/proof/<case>/, Osprey's kernel and Lean's check the same theorem on
# the same machine in the same run, and the gate is red when Osprey's wall time
# is not below Lean's. Both wall times and peak memory are appended to
# target/proof/results.jsonl and to target/proof/reports/<date>.md, outside
# the source tree, with the hashes of both inputs and both binaries. m0_identity
# is the milestone's regression gate; m1_export is the comparison [PROOF-SPEED]
# names, both kernels re-checking one recorded Lean module — see
# benchmarks/proof/README.md.
#
# A case is a directory holding an Osprey project — `osprey.toml`, or a
# `kernel` symlink to one — and its Lean module: `<case>.lean`, or the
# directory's single `*.lean` when the case is named after its theorem rather
# than its file. M0's case points at examples/proof/kernel. Directories without
# an Osprey project (reports/, and tools/ whose lakefile.toml marks it as
# tooling) are not cases and are skipped.
#
# Three optional files change a case's command lines:
#   lean.cmd    a shell command line, run from the case directory, that replaces
#               `lean --run <module>` as the Lean side (and removes the need for
#               a module, so an export re-check needs no Lean file at all)
#   lean.setup  a shell command line run once from the case directory, untimed,
#               before either side is preflighted (building the Lean tools and,
#               for m1_export, re-exporting the module and byte-comparing it
#               with the case's id_imp.ndjson, so both sides provably read the
#               same theorem); it must exit zero
#   lean.bin    required with lean.cmd: the path, relative to the case
#               directory, of the Lean-side executable lean.cmd times, so the
#               recorded `lean_binary` hash is of what actually ran
#   args        arguments appended to the Osprey binary's command line
#
# Lean is a hard requirement: without `lean` there is no baseline and the gate
# cannot be green, so the script exits 2 rather than passing silently. It exits
# 1 when any case fails or any timed run fails, and 0 when every case passes.
#
# Usage: run.sh [case-filter]
#   PROOF_RUNS (default 10)  timed runs per kernel; the minimum wall time counts.
set -u

HERE=${0:A:h}
ROOT=${HERE:h:h}
OSP=$ROOT/target/release/osprey
FILTER=${1:-}
RUNS=${PROOF_RUNS:-10}
OUT=$ROOT/target/proof/results.jsonl
REPORTS=$ROOT/target/proof/reports

have() { command -v "$1" >/dev/null 2>&1 }
die() { print -r -- "proof bench: $1" >&2; exit 2 }

# lean_bin — the executable actually timed. ~/.elan/bin/lean is elan's
# multiplexer, not Lean, so resolve the toolchain's own binary: its hash is what
# a recorded number was produced by.
lean_bin() {
  local real
  if have elan; then
    real=$(elan which lean 2>/dev/null) && [[ -x $real ]] && { print -r -- $real; return 0 }
  fi
  real="$(lean --print-prefix 2>/dev/null)/bin/lean"
  [[ -x $real ]] && print -r -- $real || command -v lean
}

# MEASURE_PY <cwd> <cmd...> — time one run and report "<ms> <KiB>". `ru_maxrss`
# is bytes on macOS and kilobytes on Linux, so it is normalised to KiB by
# platform; wall time is ceiled to whole milliseconds so no successful run ever
# reports zero. A non-zero exit, an empty measurement or a zero measurement is
# an error: it prints why and exits non-zero, so it can never be a winning time.
MEASURE_PY='
import math, resource, subprocess, sys, time
cwd = sys.argv[1] or None
start = time.monotonic()
code = subprocess.call(
    sys.argv[2:], cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
)
ms = math.ceil((time.monotonic() - start) * 1000.0)
rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
kb = rss // 1024 if sys.platform == "darwin" else rss
if code or ms <= 0 or kb <= 0:
    sys.stderr.write(f"timed command failed: exit {code}, {ms}ms, {kb}KiB\n")
    sys.exit(code or 1)
print(ms, kb)
'

# TREE_SHA <project> <case> — sha256 over everything the Osprey side reads:
# the project's `osprey.toml` and every *.ospml its `source_roots` name — a
# root may be a directory or one file and may sit outside the project, as the
# kernel modules export_check shares with examples/proof/kernel do — keyed by
# their path relative to the project root, so the hash identifies the sources
# and not the checkout; and every data file of the case directory (the
# recorded export, `args`) under `case/`. The Lean side's files (`*.lean`,
# `lean.cmd`, `lean.setup`, `lean.bin`), dotfiles and directories are left out.
# Sorted and concatenated as path\0bytes. A root that does not exist is an
# error, so a hash never silently covers less than the build reads.
TREE_SHA='
import hashlib, os, pathlib, sys, tomllib
root = pathlib.Path(sys.argv[1]).resolve()
case = pathlib.Path(sys.argv[2]).resolve()
manifest = root / "osprey.toml"
roots = tomllib.loads(manifest.read_text())["project"].get("source_roots", ["src"])
files = {"osprey.toml": manifest}
for entry in roots:
    path = (root / entry).resolve()
    if not path.exists():
        sys.exit(f"source root {entry} does not exist")
    found = [path] if path.is_file() else sorted(path.rglob("*.ospml"))
    for p in found:
        files[os.path.relpath(p, root)] = p
lean_side = {"lean.cmd", "lean.setup", "lean.bin"}
for p in case.iterdir():
    if p.is_file() and not p.name.startswith(".") and p.suffix != ".lean" and p.name not in lean_side:
        files["case/" + p.name] = p
blob = b"".join(name.encode() + b"\0" + files[name].read_bytes() for name in sorted(files))
print(hashlib.sha256(blob).hexdigest())
'

# LEAN_SHA <case> — sha256 over everything that defines the Lean side: every
# *.lean of the case directory (a symlink counts, by its target's bytes),
# `lean.cmd`, `lean.setup` and `lean.bin`, sorted and concatenated as
# path\0bytes.
LEAN_SHA='
import hashlib, pathlib, sys
case = pathlib.Path(sys.argv[1]).resolve()
files = sorted(p for p in case.iterdir() if p.suffix == ".lean" or p.name in ("lean.cmd", "lean.setup", "lean.bin"))
blob = b"".join(p.name.encode() + b"\0" + p.read_bytes() for p in files)
print(hashlib.sha256(blob).hexdigest())
'

# FILE_SHA <path> — sha256 of one file's bytes. Python, not shasum/sha256sum,
# because those two disagree about spellings across macOS and Ubuntu.
FILE_SHA='
import hashlib, sys
with open(sys.argv[1], "rb") as handle:
    print(hashlib.sha256(handle.read()).hexdigest())
'

tree_sha() { python3 -c "$TREE_SHA" "$1" "$2"; }
lean_sha() { python3 -c "$LEAN_SHA" "$1"; }
file_sha() { python3 -c "$FILE_SHA" "$1"; }

# measure_one <cwd> <cmd...> — one successful timed run, "<ms> <KiB>".
measure_one() {
  local out
  out=$(python3 -c "$MEASURE_PY" "$@") || return 1
  [[ -n ${out//[[:space:]]/} ]] || { print -r -- "proof bench: empty measurement" >&2; return 1 }
  print -r -- "$out"
}

# measure <cwd> <cmd...> — "<minimum ms> <peak KiB>" over RUNS successful runs.
# Every run must succeed; one failure measures nothing and returns non-zero.
measure() {
  local out m k i min=0 max=0
  for i in $(seq 1 "$RUNS"); do
    out=$(measure_one "$@") || return 1
    m=${out%% *}
    k=${out##* }
    (( min == 0 || m < min )) && min=$m
    (( k > max )) && max=$k
  done
  print -r -- "$min $max"
}

# timed_side <name> <label> <cwd> <cmd...> — the timed measurement of one kernel.
# A run that exits non-zero is a red case with a message, never a timing.
timed_side() {
  local name=$1 label=$2 cwd=$3 out
  shift 3
  if ! out=$(measure "$cwd" "$@"); then
    print -r -- "$name: $label timed run did not exit zero, so no timing is recorded for it" >&2
    return 1
  fi
  print -r -- "$out"
}

# preflight <name> <label> <cwd> <cmd...> — the command must exit zero and print
# a `checked: ` line before it is timed, so a case that never checks anything is
# red rather than fast.
preflight() {
  local name=$1 label=$2 cwd=$3 out code
  shift 3
  out=$( cd "$cwd" && "$@" 2>&1 ); code=$?
  if (( code != 0 )) || ! print -r -- "$out" | grep -q '^checked: '; then
    print -r -- "$name: $label did not check the theorem (exit $code): ${out[1,200]}" >&2
    return 1
  fi
}

# has_lean <dir> <name> — a case has a Lean side: a `*.lean` module or
# `lean.cmd`. A directory with neither (reports/) is not a case.
has_lean() {
  local mods=($1/*.lean(N))
  [[ -f $1/lean.cmd ]] || (( $#mods > 0 ))
}

# project_of <dir> — the Osprey project a case checks its theorem with: its own
# `osprey.toml` or a `kernel/` symlink to one. That file is what makes a
# directory a benchmark case.
project_of() {
  [[ -e $1/osprey.toml ]] && { print -r -- $1; return 0 }
  [[ -e $1/kernel/osprey.toml ]] && { print -r -- $1/kernel; return 0 }
  return 1
}

# lean_module <dir> <name> — the Lean module `lean --run` elaborates:
# `<case>.lean` or, because a case's file may be named after its theorem, the
# directory's single `*.lean`. Two candidates are ambiguous and named as such.
lean_module() {
  local dir=$1 name=$2 mods=($1/*.lean(N))
  [[ -f $dir/$name.lean ]] && { print -r -- $dir/$name.lean; return 0 }
  (( $#mods == 1 )) && { print -r -- $mods[1]; return 0 }
  print -r -- "$name: expected one *.lean (or a lean.cmd) in $dir, found $#mods" >&2
  return 1
}

# lean_binary <name> <dir> — the executable the Lean side times: `lean` itself,
# or with `lean.cmd` the file `lean.bin` names; a `lean.cmd` without a readable
# `lean.bin` is a red case, so no row records a hash of a binary that did not run.
lean_binary() {
  local path
  [[ -f $2/lean.cmd ]] || { print -r -- "$LEAN"; return 0 }
  [[ -f $2/lean.bin ]] || { print -r -- "$1: lean.cmd needs lean.bin naming the executable it times" >&2; return 1 }
  path=$2/$(<$2/lean.bin)
  [[ -x $path ]] || { print -r -- "$1: lean.bin names no executable at $path" >&2; return 1 }
  print -r -- "$path"
}

# setup <name> <dir> — run the case's `lean.setup` once, untimed, from the case
# directory; a non-zero exit is a red case before anything is measured.
setup() {
  local out code
  [[ -f $2/lean.setup ]] || return 0
  out=$( cd "$2" && sh -c "$(<$2/lean.setup)" 2>&1 ); code=$?
  (( code == 0 )) && return 0
  print -r -- "$1: lean.setup exited $code: ${out[1,300]}" >&2
  return 1
}

# build_case <name> <project> <bin> — compile the case's Osprey kernel.
build_case() {
  if ! "$OSP" "$2" --compile -o "$3" >/dev/null 2>&1; then
    print -r -- "$1: osprey compile failed ($OSP $2 --compile -o $3)" >&2
    return 1
  fi
  [[ -x $3 ]] || { print -r -- "$1: osprey compile wrote no binary at $3" >&2; return 1 }
}

# kernels <name> <dir> — set `osprey_argv` and `lean_argv`: the two command
# lines that are timed. `args` is appended to the Osprey binary, split on
# whitespace, and `lean.cmd` replaces `lean --run <module>` with its own command
# line (run from the case directory, so it can create and check its own export).
kernels() {
  local dir=$2
  osprey_argv=("$dir/.bench-osprey")
  [[ -f $dir/args ]] && osprey_argv+=(${=$(<$dir/args)})
  if [[ -f $dir/lean.cmd ]]; then
    lean_argv=(sh -c "$(<$dir/lean.cmd)")
  else
    lean_argv=(lean --run "$lean_file")
  fi
  return 0
}

# report_header <date> — the header of a day's report, written once.
report_header() {
  print -r -- "# Proof speed gate — $1"
  print -r -- ""
  print -r -- "- Lean measured: $LEAN_VERSION"
  print -r -- "- Lean toolchain binary: \`$LEAN\` (sha256 $(file_sha "$LEAN")); a row's \`lean bin\` is the executable that row timed."
  print -r -- "- Osprey compiler: \`$OSP\` (sha256 $(file_sha "$OSP"))"
  print -r -- "- Runs per kernel: $RUNS, minimum wall time; peak resident set in KiB."
  print -r -- "- Generated by \`benchmarks/proof/run.sh\`; see \`benchmarks/proof/README.md\`."
  print -r -- ""
  print -r -- "| case | osprey ms | lean ms | osprey peak KiB | lean peak KiB | verdict | osprey input | osprey bin | lean input | lean bin |"
  print -r -- "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |"
}

# record <name> <dir> <verdict> <osprey ms KiB> <lean ms KiB> — append the
# results line and the dated report row for one case. Reads `project` and
# `lean_exe` from the enclosing case.
record() {
  local name=$1 dir=$2 verdict=$3 oms=${4%% *} okb=${4##* } lms=${5%% *} lkb=${5##* }
  local at input obin linput lbin lreport
  at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  input=$(tree_sha "$project" "$dir")
  obin=$(file_sha "$dir/.bench-osprey")
  linput=$(lean_sha "$dir")
  lbin=$(file_sha "$lean_exe")
  printf '{"case":"%s","osprey_ms":%s,"lean_ms":%s,"osprey_peak_kb":%s,"lean_peak_kb":%s,"lean":"%s","osprey_input":"%s","lean_input":"%s","osprey_binary":"%s","lean_binary":"%s","verdict":"%s","at":"%s"}\n' \
    "$name" "$oms" "$lms" "$okb" "$lkb" "$LEAN_VERSION" "$input" "$linput" \
    "$obin" "$lbin" "$verdict" "$at" >> "$OUT"
  lreport=$REPORTS/${at[1,10]}.md
  [[ -f $lreport ]] || report_header "${at[1,10]}" > "$lreport"
  print -r -- "| $name | $oms | $lms | $okb | $lkb | $verdict | $input | $obin | $linput | $lbin |" >> "$lreport"
  print -r -- "$name: osprey ${oms}ms/${okb}KiB vs lean ${lms}ms/${lkb}KiB -> $verdict"
}

# time_case <name> <dir> — time both kernels, record the result, return the
# verdict: 0 when Osprey is faster, 1 when it is not or a timed run failed.
time_case() {
  local name=$1 dir=$2 osprey_measure lean_measure verdict
  osprey_measure=$(timed_side "$name" osprey "$dir" "${osprey_argv[@]}") || return 1
  lean_measure=$(timed_side "$name" lean "$dir" "${lean_argv[@]}") || return 1
  verdict=pass
  (( ${osprey_measure%% *} < ${lean_measure%% *} )) || verdict=fail
  record "$name" "$dir" "$verdict" "$osprey_measure" "$lean_measure"
  [[ $verdict == pass ]]
}

# prepare <name> <dir> <project> — run the setup, build the kernel and require
# both sides to check the theorem once before anything is timed. Fills in the
# case's argv.
prepare() {
  setup "$1" "$2" || return 1
  lean_exe=$(lean_binary "$1" "$2") || return 1
  build_case "$1" "$3" "$2/.bench-osprey" || return 1
  kernels "$1" "$2"
  preflight "$1" osprey "$2" "${osprey_argv[@]}" || return 1
  preflight "$1" lean "$2" "${lean_argv[@]}"
}

# run_case <dir> — build, preflight, time and record one case: 0 pass, 1 fail.
run_case() {
  local dir=$1 name=${1:t} project ok=1 osprey_argv lean_argv lean_file lean_exe
  has_lean "$dir" "$name" || return 0
  if ! project=$(project_of "$dir"); then
    [[ -f $dir/lakefile.toml ]] && return 0
    print -r -- "$name: Lean side but no Osprey project (osprey.toml or kernel/osprey.toml)" >&2
    return 1
  fi
  [[ -f $dir/lean.cmd ]] || lean_file=$(lean_module "$dir" "$name") || return 1
  prepare "$name" "$dir" "$project" && time_case "$name" "$dir"; ok=$?
  rm -f "$dir/.bench-osprey"
  return $ok
}

have lean || die "lean is not installed (elan: https://github.com/leanprover/elan); the speed gate has no baseline"
have python3 || die "python3 is required for wall-time and peak-memory measurement"
[[ -x $OSP ]] || die "$OSP missing; run make build first"
LEAN=$(lean_bin)
LEAN_VERSION=$(lean --version | tr -d '\n')
mkdir -p "$REPORTS" "${OUT:h}"

failed=0
for dir in "$HERE"/*/; do
  [[ -n $FILTER && ${dir:t} != *$FILTER* ]] && continue
  run_case "$dir" || failed=1
done
exit $failed
