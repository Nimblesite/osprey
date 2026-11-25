#!/usr/bin/env zsh
# The refusal half of the ownership corpus, sourced by run_test_corpus.sh after
# its main pass over the listed programs (corpus_files.sh).
# [MEM-OWNERSHIP-STATIC] Every program ownership does not list must be refused
# by the checker, by name. One that runs is an acceptance nobody reviewed; one
# that fails any other way was built wrong or crashed.
refusals_ok=1
if (( ${#REFUSED_FILES} > 0 )); then
  REFUSEDDIR=$RESULTDIR/refused
  mkdir -p "$REFUSEDDIR" || exit 1
  refused_pairs=()
  for (( index = 1; index <= ${#REFUSED_FILES}; index++ )); do
    refused_pairs+=("$index" "$REFUSED_FILES[$index]")
  done
  dispatch_batch "$JOBS" "$REFUSEDDIR" "${refused_pairs[@]}"
  refused=0
  typeset -a UNLISTED=() UNREFUSED=()
  for (( index = 1; index <= ${#REFUSED_FILES}; index++ )); do
    rel=${REFUSED_FILES[$index]#$ROOT/}
    rc=1
    [[ -r "$REFUSEDDIR/$index.status" ]] && rc=$(<$REFUSEDDIR/$index.status)
    if [[ "$rc" == 0 ]]; then
      UNLISTED+=("$rel")
    elif grep -qF '[MEM-OWNERSHIP-STATIC]' "$REFUSEDDIR/$index.stderr" 2>/dev/null; then
      refused=$((refused + 1))
    else
      UNREFUSED+=("$rel")
      echo "NOT-REFUSED $rel (ownership)"
      sed -n '1,8p' "$REFUSEDDIR/$index.stderr" | sed 's/^/  /'
    fi
  done
  echo "TEST_CORPUS_OWNERSHIP_REFUSED=$refused TEST_CORPUS_OWNERSHIP_UNLISTED=${#UNLISTED} TEST_CORPUS_OWNERSHIP_NOT_REFUSED=${#UNREFUSED}"
  for item in $UNLISTED; do echo "  runs under ownership but is not listed: $item"; done
  for item in $UNREFUSED; do echo "  neither ran nor was refused by name: $item"; done
  if (( ${#UNLISTED} > 0 )); then
    refusals_ok=0
    echo "OWNERSHIP ACCEPTS MORE THAN IT LISTS. Add these to src/osprey/tests/OWNERSHIP_ACCEPTED.txt" >&2
    echo "so their goldens and leak sentinels are held under ownership from now on." >&2
  fi
  (( ${#UNREFUSED} == 0 )) || refusals_ok=0
fi
