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

<<<<<<< HEAD
selection=${2:-}
if [[ $selection == --ownership-host ]]; then
  [[ $MEMORY == ownership && $TARGET != native ]] || exit 2
  FILES=("$TESTDIR/regressions/ownership/single_owner.test.osp")
  GOLDEN_MIN=1
  GPU_MODE_MIN=0
  MANIFEST= # The host contract must run; even a reviewed skip fails its golden floor.
elif [[ $selection == --accepted ]]; then
  [[ $MEMORY == ownership && $TARGET == native ]] || exit 2
=======
# Ownership runs the whole corpus [MEM-OWNERSHIP-STATIC]. The checker refuses
# every program that would need a second owner, so each program has exactly one
# right outcome, pinned by src/osprey/tests/OWNERSHIP_ACCEPTED.txt: a listed
# program runs, matches the shared golden and leaves nothing live; every other
# program is REFUSED by name, never built wrong, never crashed. FILES becomes
# the listed programs and REFUSED_FILES the rest, which the harness compiles
# after the main pass. On a cross target the host runs the listed programs its
# own manifest does not skip.
typeset -a REFUSED_FILES=()
if [[ $MEMORY == ownership ]]; then
>>>>>>> origin/fixes
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
<<<<<<< HEAD
  (( ${#accepted} >= 137 )) || { echo "Ownership acceptance floor breached" >&2; exit 1; }
  FILES=("${accepted[@]}")
  GOLDEN_MIN=${#FILES}
  GPU_MODE_MIN=${#${(M)FILES:#$GPU_SUITE_DIR/*}}
elif [[ -n $selection ]]; then
  echo "Unknown corpus selection: $selection" >&2
  exit 2
=======
  (( ${#accepted} >= 140 )) || { echo "Ownership acceptance floor breached" >&2; exit 1; }
  if [[ $TARGET == native ]]; then
    REFUSED_FILES=(${FILES:|accepted})
  elif [[ -f $MANIFEST ]]; then
    typeset -a unportable=()
    for row in ${(f)"$(grep -vE '^[[:space:]]*(#|$)' "$MANIFEST")"}; do
      unportable+=("$ROOT/${row%% *}")
    done
    accepted=(${accepted:|unportable})
  fi
  FILES=("${accepted[@]}")
  GOLDEN_MIN=${#FILES}
  GPU_MODE_MIN=${#${(M)FILES:#$GPU_SUITE_DIR/*}}
  MANIFEST= # Every listed program must run: a skip here is a failure, not a hole.
>>>>>>> origin/fixes
fi
