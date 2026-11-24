#!/usr/bin/env zsh
# Source and golden selection for the language corpus.
# Golden precedence for a program at $1, echoing the chosen path (empty if none):
#   1. <file>.expectedoutput                     — per-file golden
#   2. <file>.expectedoutput.<uname>             — OS-specific output
#   3. <stem>.test.osp.expectedoutput            — an ML, Python or TypeScript twin shares the Default golden
#   4. <stem>.test.osp.expectedoutput.<uname>    — ...and its OS-specific form
# Rules 3 and 4 are what make all flavor twins share ONE file:
# every flavor is required to run byte-identically, so one golden proves it
# ([FLAVOR-IR-EQUIV], [FLAVOR-TS-VERIFICATION]).
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

while IFS= read -r -d $'\0' f; do
  # Standalone Python suites explicitly opt in ([FLAVOR-PYTHON-SELECT]).
  if [[ $f == *.py ]] && ! awk '/^[[:space:]]*# ?osprey: flavor=python[[:space:]]*$/ {found=1; exit}
    /^[[:space:]]*($|#|\/\/)/ {next} {exit} END {exit !found}' "$f"; then
    continue
  fi
  FILES+=("$f")
done < <(find "$TESTDIR" \( -name '*.test.osp' -o -name '*.test.ospml' -o -name '*.test.ts' -o -name '*.test.py' \) -print0 | LC_ALL=C sort -z)

selection=${2:-}
if [[ $selection == --ownership-host ]]; then
  [[ $MEMORY == ownership && $TARGET != native ]] || exit 2
  FILES=("$TESTDIR/regressions/ownership/single_owner.test.osp")
  GOLDEN_MIN=1
  GPU_MODE_MIN=0
  MANIFEST= # The host contract must run; even a reviewed skip fails its golden floor.
elif [[ $selection == --accepted ]]; then
  [[ $MEMORY == ownership && $TARGET == native ]] || exit 2
  manifest=$TESTDIR/OWNERSHIP_ACCEPTED.txt
  LC_ALL=C sort -cu "$manifest" || exit 1
  typeset -a accepted=()
  while IFS= read -r rel || [[ -n $rel ]]; do
    f=$TESTDIR/$rel
    if (( ! ${FILES[(Ie)$f]} )); then
      echo "Accepted ownership program missing from corpus: $rel" >&2
      exit 1
    fi
    accepted+=("$f")
  done < "$manifest"
  (( ${#accepted} >= 123 )) || { echo "Ownership acceptance floor breached" >&2; exit 1; }
  FILES=("${accepted[@]}")
  GOLDEN_MIN=${#FILES}
  GPU_MODE_MIN=${#${(M)FILES:#$GPU_SUITE_DIR/*}}
elif [[ -n $selection ]]; then
  echo "Unknown corpus selection: $selection" >&2
  exit 2
fi
