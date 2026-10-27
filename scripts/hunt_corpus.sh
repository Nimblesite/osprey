#!/usr/bin/env bash
# Runs `osprey hunt` over every Osprey program under tests/ and examples/ and
# collects one JSON report per program [SYMEX-CORPUS]
# (docs/specs/tooling/SymbolicExecution.md). KLEE is the engine; it runs
# natively on this host against the mock C library bitcode.
#
#   scripts/hunt_corpus.sh [--all] [--out DIR] [--max-time S] [--jobs N] [--osprey PATH] [PROGRAM...]
#
# Programs listed in tests/HUNT_UNPORTABLE.txt are skipped unless --all is
# given. With no PROGRAM arguments the whole corpus is swept. Results land in
# DIR/json/<slug>.json (the report), DIR/log/<slug>.log (stderr) and
# DIR/code/<slug> (exit code); scripts/hunt_summary.py turns them into a table.
set -euo pipefail

cd "$(dirname "$0")/.."
OUT=target/hunt-corpus
MAX_TIME=20
JOBS=$(( $(sysctl -n hw.ncpu 2>/dev/null || nproc) / 2 ))
[ "$JOBS" -ge 1 ] || JOBS=1
OSPREY=${OSPREY:-target/release/osprey}
ALL=0
PROGRAMS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --all) ALL=1 ;;
    --out) OUT=$2; shift ;;
    --max-time) MAX_TIME=$2; shift ;;
    --jobs) JOBS=$2; shift ;;
    --osprey) OSPREY=$2; shift ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) PROGRAMS+=("$1") ;;
  esac
  shift
done

# Shared inventory keeps standalone files separate from project modules.
if [ ${#PROGRAMS[@]} -eq 0 ]; then
  while IFS= read -r program; do PROGRAMS+=("$program"); done < <(
    python3 scripts/hunt_programs.py tests examples | sed '\#^tests/hunt/#d'
  )
fi
if [ "$ALL" -eq 0 ] && [ -f tests/HUNT_UNPORTABLE.txt ]; then
  kept=()
  for program in "${PROGRAMS[@]}"; do
    grep -q "^$program " tests/HUNT_UNPORTABLE.txt || kept+=("$program")
  done
  PROGRAMS=("${kept[@]}")
fi

mkdir -p "$OUT/json" "$OUT/log" "$OUT/code"
echo "hunting ${#PROGRAMS[@]} programs with KLEE, ${MAX_TIME}s each, $JOBS at a time -> $OUT" >&2

hunt_one() {
  program=$1
  slug=$(printf '%s' "$program" | tr '/' '_')
  [ -d "$program" ] && slug="$slug.project"
  set +e
  "$OSPREY" hunt "$program" --json --quiet --max-time "$MAX_TIME" \
    > "$OUT/json/$slug.json" 2> "$OUT/log/$slug.log"
  echo $? > "$OUT/code/$slug"
  set -e
  printf '%s %s\n' "$(cat "$OUT/code/$slug")" "$program" >&2
}
export -f hunt_one
export OUT MAX_TIME OSPREY

printf '%s\n' "${PROGRAMS[@]}" | xargs -P "$JOBS" -I{} bash -c 'hunt_one "$1"' _ {}
echo "done: $(ls "$OUT/json" | wc -l | tr -d ' ') reports in $OUT" >&2
